use crate::audio::Recorder;
use crate::config::{AppConfig, ProviderKind};
use crate::error::{AppError, AppResult};
use crate::history::{self, HistoryEntry};
use crate::hotkey::{GlobalHotkey, HotkeyEvent};
use crate::injector::{copy_text, inject_text, WindowTarget};
use crate::postprocess::clean_transcript;
use crate::transcription::{self, JobResult};
use chrono::Local;
use eframe::egui;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub struct SpeakTypeCloudApp {
    config: AppConfig,
    recorder: Recorder,
    hotkey: Option<GlobalHotkey>,
    hotkey_error: Option<String>,
    devices: Vec<String>,
    recording_started: Option<Instant>,
    targets: TargetState,
    transcription_rx: Option<Receiver<AppResult<JobResult>>>,
    busy: bool,
    status: String,
    last_text: String,
    last_error: Option<String>,
    hotkey_edit: String,
    keyterms_edit: String,
    config_load_error: Option<String>,
}

#[derive(Default)]
struct TargetState {
    last_external: Option<WindowTarget>,
    recording: Option<WindowTarget>,
    last_text: Option<WindowTarget>,
}

impl TargetState {
    fn observe(&mut self, current_external: Option<WindowTarget>) {
        if let Some(target) = current_external {
            self.last_external = Some(target);
        }
    }

    fn start_recording(
        &mut self,
        preserve_target: bool,
        current_external: Option<WindowTarget>,
    ) -> Option<WindowTarget> {
        self.observe(current_external);
        self.recording = preserve_target
            .then_some(current_external.or(self.last_external))
            .flatten();
        self.recording
    }

    fn cancel_recording(&mut self) {
        self.recording = None;
    }

    fn accept_transcript(&mut self) -> Option<WindowTarget> {
        self.last_text = self.recording.take();
        self.last_text
    }

    fn last_text(&self) -> Option<WindowTarget> {
        self.last_text
    }
}

impl SpeakTypeCloudApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (config, config_load_error) = match AppConfig::load() {
            Ok(config) => (config, None),
            Err(error) => (AppConfig::default(), Some(error.to_string())),
        };
        let recorder = Recorder::new(
            config.recording.input_device_name.clone(),
            config.recording.gain,
        );
        let devices = recorder.list_devices();
        let (hotkey, hotkey_error) = match GlobalHotkey::new(&config.hotkey) {
            Ok(hotkey) => (Some(hotkey), None),
            Err(error) => (None, Some(error)),
        };
        let has_startup_error = config_load_error.is_some() || hotkey_error.is_some();
        Self {
            hotkey_edit: config.hotkey.clone(),
            keyterms_edit: config.xai.keyterms.join("\n"),
            config,
            recorder,
            hotkey,
            hotkey_error,
            devices,
            recording_started: None,
            targets: TargetState::default(),
            transcription_rx: None,
            busy: false,
            status: if has_startup_error {
                "啟動時發生問題；請查看錯誤訊息".to_string()
            } else {
                "就緒".to_string()
            },
            last_text: String::new(),
            last_error: config_load_error.clone(),
            config_load_error,
        }
    }

    fn start_recording(&mut self) {
        if self.busy || self.recorder.is_recording() {
            return;
        }
        let target = self.targets.start_recording(
            self.config.output.preserve_target_window,
            WindowTarget::capture_external(),
        );
        self.recorder.update_config(
            self.config.recording.input_device_name.clone(),
            self.config.recording.gain,
        );
        match self.recorder.start() {
            Ok(()) => {
                self.recording_started = Some(Instant::now());
                self.status = if self.config.output.auto_inject && target.is_none() {
                    "錄音中…未找到外部目標，完成後只會複製文字".to_string()
                } else {
                    "錄音中…放開快捷鍵即可辨識".to_string()
                };
                self.last_error = None;
            }
            Err(error) => {
                self.targets.cancel_recording();
                self.fail(error);
            }
        }
    }

    fn stop_recording(&mut self) {
        if !self.recorder.is_recording() {
            return;
        }
        let audio = self.recorder.stop();
        self.recording_started = None;
        if audio.duration_secs() * 1000.0 < self.config.recording.min_duration_ms as f32 {
            self.targets.cancel_recording();
            self.fail(AppError::Audio("錄音太短，請按住久一點".to_string()));
            return;
        }
        let (tx, rx) = mpsc::channel();
        transcription::spawn(self.config.clone(), audio, tx);
        self.transcription_rx = Some(rx);
        self.busy = true;
        self.status = format!("正在使用 {} 辨識…", self.config.provider.label());
    }

    fn toggle_recording(&mut self) {
        if self.recorder.is_recording() {
            self.stop_recording();
        } else {
            self.start_recording();
        }
    }

    fn poll_hotkey(&mut self) {
        if let Some(error) = self.hotkey.as_ref().and_then(GlobalHotkey::poll_error) {
            self.hotkey = None;
            self.hotkey_error = Some(error);
            self.status = "全域快捷鍵不可用；仍可使用按鈕錄音".to_string();
        }

        let mut events = Vec::new();
        if let Some(hotkey) = &self.hotkey {
            while let Some(event) = hotkey.poll() {
                events.push(event);
            }
        }
        for event in events {
            match (self.config.hold_to_record, event) {
                (true, HotkeyEvent::Pressed) => self.start_recording(),
                (true, HotkeyEvent::Released) => self.stop_recording(),
                (false, HotkeyEvent::Pressed) => self.toggle_recording(),
                _ => {}
            }
        }
    }

    fn poll_transcription(&mut self) {
        let result = self
            .transcription_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        let Some(result) = result else {
            return;
        };
        self.transcription_rx = None;
        self.busy = false;
        match result {
            Ok(job) => self.accept(job),
            Err(error) => {
                self.targets.cancel_recording();
                self.fail(error);
            }
        }
    }

    fn refresh_external_target(&mut self) {
        self.targets.observe(WindowTarget::capture_external());
    }

    fn accept(&mut self, job: JobResult) {
        let text = clean_transcript(&job.response.text, self.config.output.append_space);
        let target = self.targets.accept_transcript();
        self.last_text = text.clone();
        self.status = "辨識完成".to_string();
        self.last_error = None;
        let entry = HistoryEntry {
            created_at: Local::now(),
            provider: job.response.provider.clone(),
            duration_secs: job.local_duration_secs,
            text: text.clone(),
        };
        if let Err(error) = history::append(&entry) {
            record_nonfatal_error(&mut self.last_error, &format!("歷史紀錄未保存：{error}"));
        }

        if self.config.output.auto_inject {
            if let Some(target) = target {
                if let Err(injection_error) =
                    inject_text(Some(target), &text, self.config.output.restore_clipboard)
                {
                    if self.config.output.copy_only_on_injection_failure {
                        let feedback =
                            fallback_delivery_feedback(injection_error, copy_text(&text));
                        self.status = feedback.status.to_string();
                        record_nonfatal_error(&mut self.last_error, &feedback.error);
                    } else {
                        self.status = "自動貼上失敗，文字保留在最近辨識文字區".to_string();
                        record_nonfatal_error(&mut self.last_error, &injection_error.to_string());
                    }
                } else {
                    self.status = "已輸入錄音開始前的外部視窗".to_string();
                }
            } else if let Err(error) = copy_text(&text) {
                self.status = "沒有外部目標且複製失敗，文字保留在最近辨識文字區".to_string();
                record_nonfatal_error(&mut self.last_error, &error.to_string());
            } else {
                self.status = "未找到外部目標，文字已複製到剪貼簿".to_string();
            }
        } else if let Err(error) = copy_text(&text) {
            self.status = "複製失敗，文字保留在最近辨識文字區".to_string();
            record_nonfatal_error(&mut self.last_error, &error.to_string());
        } else {
            self.status = "已複製到剪貼簿".to_string();
        }
    }

    fn fail(&mut self, error: AppError) {
        self.busy = false;
        self.status = "發生錯誤".to_string();
        self.last_error = Some(error.to_string());
    }

    fn save_settings(&mut self) {
        if let Some(error) = self.config_load_error.as_deref() {
            self.fail(AppError::Configuration(format!(
                "原設定檔載入失敗，為避免覆蓋已禁止儲存；請修正設定檔並重新啟動。{error}"
            )));
            return;
        }
        self.config.hotkey = self.hotkey_edit.trim().to_string();
        self.config.xai.keyterms = self
            .keyterms_edit
            .lines()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .collect();
        let result = self.config.validate().and_then(|_| {
            if let Some(hotkey) = &self.hotkey {
                hotkey
                    .update(&self.config.hotkey)
                    .map_err(AppError::Configuration)
            } else {
                let hotkey =
                    GlobalHotkey::new(&self.config.hotkey).map_err(AppError::Configuration)?;
                self.hotkey = Some(hotkey);
                Ok(())
            }
        });
        match result.and_then(|_| self.config.save()) {
            Ok(()) => {
                self.hotkey_error = None;
                self.status = "設定已儲存".to_string();
            }
            Err(error) => self.fail(error),
        }
    }
}

impl eframe::App for SpeakTypeCloudApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_external_target();
        self.poll_hotkey();
        self.poll_transcription();
        if let Some(error) = self.recorder.take_stream_error() {
            let _ = self.recorder.stop();
            self.recording_started = None;
            self.fail(error);
        }
        if let Some(started) = self.recording_started {
            if started.elapsed() >= Duration::from_secs(self.config.recording.max_duration_secs) {
                self.stop_recording();
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("SpeakType Cloud");
            ui.label("按住全域快捷鍵說話，放開後辨識並貼到原本使用中的視窗。");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("狀態：");
                if self.recorder.is_recording() {
                    ui.strong("🔴 錄音中");
                } else if self.busy {
                    ui.strong("☁ 辨識中");
                } else {
                    ui.strong(&self.status);
                }
            });
            if let Some(error) = &self.last_error {
                ui.colored_label(egui::Color32::RED, error);
            }
            if let Some(error) = &self.hotkey_error {
                ui.colored_label(egui::Color32::RED, format!("全域快捷鍵不可用：{error}"));
            }
            ui.horizontal(|ui| {
                let label = if self.recorder.is_recording() {
                    "停止錄音"
                } else {
                    "開始錄音"
                };
                if ui
                    .add_enabled(!self.busy, egui::Button::new(label))
                    .clicked()
                {
                    self.toggle_recording();
                }
                if ui.button("再次貼上").clicked() && !self.last_text.is_empty() {
                    if let Some(target) = self.targets.last_text() {
                        if let Err(injection_error) = inject_text(
                            Some(target),
                            &self.last_text,
                            self.config.output.restore_clipboard,
                        ) {
                            let feedback = fallback_delivery_feedback(
                                injection_error,
                                copy_text(&self.last_text),
                            );
                            self.status = feedback.status.to_string();
                            self.last_error = Some(feedback.error);
                        } else {
                            self.status = "已再次貼到原本的外部視窗".to_string();
                        }
                    } else {
                        match copy_text(&self.last_text) {
                            Ok(()) => {
                                self.status = "沒有原外部目標，文字已複製到剪貼簿".to_string()
                            }
                            Err(error) => {
                                self.status = "沒有原外部目標且複製失敗，文字仍保留".to_string();
                                self.last_error = Some(error.to_string());
                            }
                        }
                    }
                }
                if ui.button("複製文字").clicked() {
                    match copy_text(&self.last_text) {
                        Ok(()) => self.status = "已複製到剪貼簿".to_string(),
                        Err(error) => {
                            self.status = "複製失敗，文字仍保留".to_string();
                            self.last_error = Some(error.to_string());
                        }
                    }
                }
            });

            ui.add_space(8.0);
            ui.label("最近辨識文字");
            ui.add(
                egui::TextEdit::multiline(&mut self.last_text)
                    .desired_rows(5)
                    .desired_width(f32::INFINITY),
            );
            ui.separator();

            ui.collapsing("API 與語言", |ui| {
                egui::ComboBox::from_label("供應商")
                    .selected_text(self.config.provider.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.config.provider,
                            ProviderKind::OpenAi,
                            "OpenAI",
                        );
                        ui.selectable_value(&mut self.config.provider, ProviderKind::Xai, "xAI");
                    });
                ui.horizontal(|ui| {
                    ui.label("語言代碼");
                    ui.text_edit_singleline(&mut self.config.language);
                });
                ui.label("提示詞／詞彙背景");
                ui.text_edit_multiline(&mut self.config.prompt);
                match self.config.provider {
                    ProviderKind::OpenAi => {
                        ui.horizontal(|ui| {
                            ui.label("模型");
                            ui.text_edit_singleline(&mut self.config.openai.model);
                        });
                        ui.horizontal(|ui| {
                            ui.label("API Key 環境變數");
                            ui.text_edit_singleline(&mut self.config.openai.api_key_env);
                        });
                    }
                    ProviderKind::Xai => {
                        ui.horizontal(|ui| {
                            ui.label("API Key 環境變數");
                            ui.text_edit_singleline(&mut self.config.xai.api_key_env);
                        });
                        ui.checkbox(&mut self.config.xai.format_text, "支援的語言啟用文字格式化");
                        ui.label("xAI Keyterms（每行一個）");
                        ui.text_edit_multiline(&mut self.keyterms_edit);
                    }
                }
                ui.label(format!(
                    "目前程式只讀取環境變數 {}，不把 API Key 寫入設定檔。",
                    self.config.api_key_env()
                ));
            });

            ui.collapsing("錄音與輸出", |ui| {
                egui::ComboBox::from_label("麥克風")
                    .selected_text(
                        self.config
                            .recording
                            .input_device_name
                            .as_deref()
                            .unwrap_or("系統預設"),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.config.recording.input_device_name,
                            None,
                            "系統預設",
                        );
                        for device in &self.devices {
                            ui.selectable_value(
                                &mut self.config.recording.input_device_name,
                                Some(device.clone()),
                                device,
                            );
                        }
                    });
                ui.add(
                    egui::Slider::new(&mut self.config.recording.gain, 0.1..=4.0)
                        .text("麥克風增益"),
                );
                ui.horizontal(|ui| {
                    ui.label("全域快捷鍵");
                    ui.text_edit_singleline(&mut self.hotkey_edit);
                });
                ui.checkbox(&mut self.config.hold_to_record, "按住錄音、放開送出（PTT）");
                ui.checkbox(&mut self.config.output.auto_inject, "自動輸入原本焦點視窗");
                ui.checkbox(
                    &mut self.config.output.restore_clipboard,
                    "貼上後還原文字剪貼簿",
                );
                ui.checkbox(
                    &mut self.config.output.preserve_target_window,
                    "錄音開始時記住目標視窗",
                );
                ui.checkbox(
                    &mut self.config.save_recordings,
                    "保留 WAV 錄音（預設關閉）",
                );
            });

            ui.add_space(8.0);
            if ui.button("儲存設定").clicked() {
                self.save_settings();
            }
        });
        ctx.request_repaint_after(Duration::from_millis(35));
    }
}

struct DeliveryFeedback {
    status: &'static str,
    error: String,
}

fn fallback_delivery_feedback(
    injection_error: AppError,
    copy_result: AppResult<()>,
) -> DeliveryFeedback {
    match copy_result {
        Ok(()) => DeliveryFeedback {
            status: "自動貼上失敗，文字已複製到剪貼簿",
            error: injection_error.to_string(),
        },
        Err(copy_error) => DeliveryFeedback {
            status: "自動貼上與備援複製皆失敗，文字保留在最近辨識文字區",
            error: format!("{injection_error}\n備援複製失敗：{copy_error}"),
        },
    }
}

fn record_nonfatal_error(current: &mut Option<String>, message: &str) {
    if let Some(current) = current {
        current.push('\n');
        current.push_str(message);
    } else {
        *current = Some(message.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_copy_failure_is_reported_without_claiming_copy_succeeded() {
        let feedback = fallback_delivery_feedback(
            AppError::Injection("focus changed".to_string()),
            Err(AppError::Injection("clipboard unavailable".to_string())),
        );

        assert!(!feedback.status.contains("已複製"));
        assert!(feedback.error.contains("focus changed"));
        assert!(feedback.error.contains("clipboard unavailable"));
    }

    #[test]
    fn nonfatal_errors_are_combined_instead_of_overwritten() {
        let mut error = None;

        record_nonfatal_error(&mut error, "history unavailable");
        record_nonfatal_error(&mut error, "clipboard unavailable");

        let error = error.expect("combined error");
        assert!(error.contains("history unavailable"));
        assert!(error.contains("clipboard unavailable"));
    }

    #[test]
    fn recording_uses_last_external_target_and_text_keeps_its_original_target() {
        let first = WindowTarget::from_raw_for_test(11);
        let second = WindowTarget::from_raw_for_test(22);
        let mut targets = TargetState::default();

        targets.observe(Some(first));
        assert_eq!(targets.start_recording(true, None), Some(first));
        targets.accept_transcript();
        targets.observe(Some(second));

        assert_eq!(targets.last_text(), Some(first));
        assert_eq!(targets.start_recording(true, Some(second)), Some(second));
    }

    #[test]
    fn recording_without_external_target_is_copy_only() {
        let mut targets = TargetState::default();

        assert_eq!(targets.start_recording(true, None), None);
        targets.accept_transcript();

        assert_eq!(targets.last_text(), None);
        assert_eq!(
            targets.start_recording(false, Some(WindowTarget::from_raw_for_test(33))),
            None
        );
    }
}
