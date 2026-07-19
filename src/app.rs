use crate::audio::{LiveAudioStats, RecordedAudio, Recorder};
use crate::config::{AppConfig, ProviderKind, TranscriptionMode, MAX_RECORDING_DURATION_SECS};
use crate::error::{AppError, AppResult};
use crate::history::{self, HistoryEntry};
use crate::hotkey::{GlobalHotkey, HotkeyEvent};
use crate::injector::{copy_text, inject_text, WindowTarget};
use crate::postprocess::{format_transcript, process_transcript, DeliveryMode};
use crate::providers::ProviderResponse;
use crate::realtime::RealtimeEvent;
use crate::realtime_worker::{self, RealtimeWorkerHandle, WorkerMessage};
use crate::startup;
use crate::transcription::{self, CancellationToken, JobId, JobMessage, JobResult};
use chrono::Local;
use eframe::egui;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub struct SpeakTypeCloudApp {
    pub config: AppConfig,
    recorder: Recorder,
    hotkey: Option<GlobalHotkey>,
    hotkey_error: Option<String>,
    devices: Vec<String>,
    recording_started: Option<Instant>,
    targets: TargetState,
    transcription_rx: Option<Receiver<JobMessage>>,
    realtime_rx: Option<Receiver<WorkerMessage>>,
    realtime_worker: Option<RealtimeWorkerHandle>,
    live_audio_stats: Option<LiveAudioStats>,
    partial_text: String,
    pending_realtime_audio: Option<RecordedAudio>,
    batch_fallback_audio: Option<RecordedAudio>,
    realtime_stop_requested: bool,
    realtime_shutdown_reason: Option<String>,
    active_job: Option<ActiveJob>,
    next_job_id: u64,
    busy: bool,
    status: String,
    last_text: String,
    last_error: Option<String>,
    hotkey_edit: String,
    pub(crate) keyterms_edit: String,
    config_load_error: Option<String>,
    next_hotkey_retry: Option<Instant>,
}

#[derive(Default)]
struct TargetState {
    last_external: Option<WindowTarget>,
    recording: Option<WindowTarget>,
    last_text: Option<WindowTarget>,
}

struct ActiveJob {
    id: JobId,
    cancellation: CancellationToken,
    cancelling: bool,
}

impl Drop for ActiveJob {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
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

    fn accept_transcript(&mut self, preserve_recording: bool) -> Option<WindowTarget> {
        self.last_text = if preserve_recording {
            self.recording
        } else {
            self.recording.take()
        };
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
        let hotkey_failed_at_startup = hotkey_error.is_some();
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
            realtime_rx: None,
            realtime_worker: None,
            live_audio_stats: None,
            partial_text: String::new(),
            pending_realtime_audio: None,
            batch_fallback_audio: None,
            realtime_stop_requested: false,
            realtime_shutdown_reason: None,
            active_job: None,
            next_job_id: 1,
            busy: false,
            status: if has_startup_error {
                "啟動時發生問題；請查看錯誤訊息".to_string()
            } else {
                "就緒".to_string()
            },
            last_text: String::new(),
            last_error: config_load_error.clone(),
            config_load_error,
            next_hotkey_retry: if hotkey_failed_at_startup {
                Some(Instant::now() + Duration::from_secs(4))
            } else {
                None
            },
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
        if self.config.transcription_mode.is_realtime() {
            match self.recorder.start_live(8) {
                Ok((receiver, stats)) => {
                    let (tx, rx) = mpsc::channel();
                    self.realtime_worker =
                        Some(realtime_worker::spawn(self.config.clone(), receiver, tx));
                    self.realtime_rx = Some(rx);
                    self.live_audio_stats = Some(stats);
                    self.partial_text.clear();
                    self.pending_realtime_audio = None;
                    self.batch_fallback_audio = None;
                    self.realtime_stop_requested = false;
                    self.realtime_shutdown_reason = None;
                    self.recording_started = Some(Instant::now());
                    self.status = match self.config.transcription_mode {
                        TranscriptionMode::RealtimePtt => "Realtime PTT 錄音中…".to_string(),
                        TranscriptionMode::ContinuousDictation => {
                            "Continuous Dictation 已開始；本地 VAD 監測中…".to_string()
                        }
                        TranscriptionMode::BatchPtt => unreachable!(),
                    };
                    self.last_error = None;
                }
                Err(error) => {
                    self.targets.cancel_recording();
                    self.fail(error);
                }
            }
            return;
        }
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
        if self.config.transcription_mode.is_realtime() {
            self.pending_realtime_audio = Some(audio);
            self.realtime_stop_requested = true;
            if let Some(worker) = &self.realtime_worker {
                worker.finalize();
                self.status = "正在完成 realtime utterance…".to_string();
            } else {
                self.fail_realtime("Realtime session 不可用".to_string());
            }
            return;
        }
        self.spawn_batch_transcription(audio);
    }

    fn spawn_batch_transcription(&mut self, audio: RecordedAudio) {
        let (tx, rx) = mpsc::channel();
        let id = JobId(self.next_job_id);
        self.next_job_id = self.next_job_id.wrapping_add(1).max(1);
        let cancellation = transcription::spawn(id, self.config.clone(), audio, tx);
        self.transcription_rx = Some(rx);
        self.active_job = Some(ActiveJob {
            id,
            cancellation,
            cancelling: false,
        });
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
        // If the hotkey is missing (initial creation failed or runtime death),
        // periodically try to recreate it.  This handles transient system
        // conditions (antivirus delay, UIPI, thread timing) that later resolve.
        if self.hotkey.is_none() {
            // Stagger retries: first attempt after ~4 seconds, then every
            // ~4 seconds.  This avoids a tight restart loop at 250 Hz.
            let should_retry = self
                .next_hotkey_retry
                .is_none_or(|deadline| Instant::now() >= deadline);
            if should_retry {
                match GlobalHotkey::new(&self.config.hotkey) {
                    Ok(hotkey) => {
                        self.hotkey = Some(hotkey);
                        self.hotkey_error = None;
                        self.status = "快捷鍵已啟動".to_string();
                        self.next_hotkey_retry = None;
                    }
                    Err(error) => {
                        self.hotkey_error = Some(error);
                        self.next_hotkey_retry = Some(Instant::now() + Duration::from_secs(4));
                    }
                }
            }
        }

        // Runtime death — the old listener thread has already exited and its
        // hook is gone.  Recreate immediately (no staggering needed).
        if let Some(error) = self.hotkey.as_ref().and_then(GlobalHotkey::poll_error) {
            self.hotkey_error = Some(error.clone());
            match GlobalHotkey::new(&self.config.hotkey) {
                Ok(hotkey) => {
                    self.hotkey = Some(hotkey);
                    self.hotkey_error = None;
                    self.status = "快捷鍵已重新啟動".to_string();
                }
                Err(new_error) => {
                    self.hotkey = None;
                    self.hotkey_error = Some(new_error);
                    self.status = "全域快捷鍵不可用；仍可使用按鈕錄音".to_string();
                }
            }
        }

        let mut events = Vec::new();
        if let Some(hotkey) = &mut self.hotkey {
            while let Some(event) = hotkey.poll_event() {
                events.push(event);
            }
        }
        for event in events {
            if self.config.transcription_mode == TranscriptionMode::ContinuousDictation {
                if event == HotkeyEvent::Pressed {
                    self.toggle_recording();
                }
                continue;
            }
            match (self.config.hold_to_record, event) {
                (true, HotkeyEvent::Pressed) => self.start_recording(),
                (true, HotkeyEvent::Released) => self.stop_recording(),
                (false, HotkeyEvent::Pressed) => self.toggle_recording(),
                _ => {}
            }
        }
    }

    fn poll_transcription(&mut self) {
        let message = self
            .transcription_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        let Some(message) = message else {
            return;
        };
        if !is_current_job(self.active_job.as_ref().map(|active| active.id), message.id) {
            return;
        }
        self.transcription_rx = None;
        self.active_job = None;
        self.busy = false;
        match message.result {
            Ok(job) => self.accept(job, false),
            Err(AppError::Cancelled) => {
                self.targets.cancel_recording();
                self.status = "辨識已取消".to_string();
                self.last_error = None;
            }
            Err(error) => {
                self.targets.cancel_recording();
                self.fail(error);
            }
        }
    }

    fn cancel_transcription(&mut self) {
        let Some(active) = self.active_job.as_mut() else {
            return;
        };
        active.cancellation.cancel();
        active.cancelling = true;
        self.status = "正在取消辨識…".to_string();
        self.last_error = None;
    }

    fn poll_realtime(&mut self) {
        let mut messages = Vec::new();
        if let Some(rx) = &self.realtime_rx {
            while let Ok(message) = rx.try_recv() {
                messages.push(message);
            }
        }
        for message in messages {
            match message {
                WorkerMessage::Event(RealtimeEvent::Created) => {
                    self.status = "Realtime session 已連線；正在接收音訊".to_string();
                }
                WorkerMessage::Event(RealtimeEvent::Partial(text)) => {
                    self.partial_text = text;
                    self.status = "Realtime partial 字幕更新中…".to_string();
                }
                WorkerMessage::Event(RealtimeEvent::Final(text)) => {
                    self.partial_text.clear();
                    let duration = self
                        .pending_realtime_audio
                        .as_ref()
                        .map_or(0.0, RecordedAudio::duration_secs);
                    let preserve_target = self.config.transcription_mode
                        == TranscriptionMode::ContinuousDictation
                        && !self.realtime_stop_requested;
                    self.accept(
                        JobResult {
                            response: ProviderResponse {
                                text,
                                duration_secs: None,
                                provider: format!("{}-realtime", self.config.provider.label()),
                                model: (self.config.provider == ProviderKind::OpenAi)
                                    .then(|| self.config.realtime.openai_model.clone()),
                            },
                            local_duration_secs: duration,
                        },
                        preserve_target,
                    );
                    if !preserve_target {
                        self.pending_realtime_audio = None;
                        if let Some(worker) = &self.realtime_worker {
                            worker.cancel();
                        }
                        self.realtime_shutdown_reason = Some(self.status.clone());
                        self.realtime_stop_requested = false;
                    }
                }
                WorkerMessage::Event(RealtimeEvent::Done) => {
                    if self.realtime_stop_requested {
                        if let Some(worker) = &self.realtime_worker {
                            worker.cancel();
                        }
                        self.pending_realtime_audio = None;
                        self.realtime_stop_requested = false;
                        self.targets.cancel_recording();
                        self.status =
                            "Realtime session 已停止，provider 未回傳 final 文字".to_string();
                        self.realtime_shutdown_reason = Some(self.status.clone());
                    } else {
                        self.status = "Realtime provider 已完成目前音訊".to_string();
                    }
                }
                WorkerMessage::Event(RealtimeEvent::Cancelled) => {
                    self.status = "Realtime session 已取消".to_string();
                }
                WorkerMessage::Event(RealtimeEvent::Error(error))
                | WorkerMessage::Failed(error) => self.fail_realtime(error),
                WorkerMessage::Stopped => {
                    if let Some(mut worker) = self.realtime_worker.take() {
                        worker.join_after_ack();
                    }
                    self.realtime_rx = None;
                    self.live_audio_stats = None;
                    self.pending_realtime_audio = None;
                    self.realtime_stop_requested = false;
                    if let Some(reason) = self.realtime_shutdown_reason.take() {
                        self.status = reason;
                    }
                }
            }
        }
    }

    fn fail_realtime(&mut self, error: String) {
        let audio = if self.recorder.is_recording() {
            self.recording_started = None;
            Some(self.recorder.stop())
        } else {
            self.pending_realtime_audio.take()
        };
        self.batch_fallback_audio = audio.filter(|audio| {
            audio.duration_secs() * 1_000.0 >= self.config.recording.min_duration_ms as f32
        });
        if let Some(worker) = &self.realtime_worker {
            worker.cancel();
        }
        self.partial_text.clear();
        self.realtime_stop_requested = false;
        self.status = if self.batch_fallback_audio.is_some() {
            "Realtime 失敗；可明確確認後改用 Batch（可能重複已出現的 partial）".to_string()
        } else {
            "Realtime 失敗，沒有可供 Batch fallback 的完整音訊".to_string()
        };
        self.realtime_shutdown_reason = Some(self.status.clone());
        self.last_error = Some(error);
    }

    fn cancel_realtime(&mut self, reason: &str) {
        if self.recorder.is_recording() {
            let _ = self.recorder.stop();
        }
        self.recording_started = None;
        self.pending_realtime_audio = None;
        self.batch_fallback_audio = None;
        self.partial_text.clear();
        self.realtime_stop_requested = false;
        self.targets.cancel_recording();
        self.last_error = None;
        if let Some(worker) = &self.realtime_worker {
            worker.cancel();
            self.realtime_shutdown_reason = Some(reason.to_string());
            self.status = "正在取消 Realtime session…".to_string();
        } else {
            self.realtime_rx = None;
            self.live_audio_stats = None;
            self.realtime_shutdown_reason = None;
            self.status = reason.to_string();
        }
    }

    fn confirm_batch_fallback(&mut self) {
        if self.busy {
            return;
        }
        let Some(audio) = take_confirmed_batch_fallback(&mut self.batch_fallback_audio, true)
        else {
            return;
        };
        self.status = "已由使用者確認：改用 Batch 上傳此段音訊".to_string();
        self.spawn_batch_transcription(audio);
    }

    fn refresh_external_target(&mut self) {
        self.targets.observe(WindowTarget::capture_external());
    }

    fn accept(&mut self, job: JobResult, preserve_recording_target: bool) {
        let (text, delivery, processing_error) = match process_transcript(
            &job.response.text,
            &self.config.text_processing,
            self.config.output.append_space,
        ) {
            Ok(processed) => (processed.text, processed.delivery, None),
            Err(error) => (
                format_transcript(
                    &job.response.text,
                    &self.config.text_processing,
                    self.config.output.append_space,
                ),
                DeliveryMode::Normal,
                Some(format!("文字後處理失敗，已保留原辨識文字：{error}")),
            ),
        };
        let target = self.targets.accept_transcript(preserve_recording_target);
        self.last_text = text.clone();
        self.status = "辨識完成".to_string();
        self.last_error = processing_error;
        if delivery == DeliveryMode::Discard {
            self.status = "語音命令已執行：不輸出文字".to_string();
            return;
        }
        let entry = HistoryEntry {
            created_at: Local::now(),
            provider: job.response.provider.clone(),
            duration_secs: job.local_duration_secs,
            text: text.clone(),
        };
        if let Err(error) = history::append(&entry) {
            record_nonfatal_error(&mut self.last_error, &format!("歷史紀錄未保存：{error}"));
        }

        if self.config.output.auto_inject && delivery == DeliveryMode::Normal {
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
            self.status = if delivery == DeliveryMode::CopyOnly {
                "語音命令已執行：文字已複製到剪貼簿".to_string()
            } else {
                "已複製到剪貼簿".to_string()
            };
        }
    }

    fn fail(&mut self, error: AppError) {
        self.busy = false;
        self.status = "發生錯誤".to_string();
        self.last_error = Some(error.to_string());
    }

    pub(crate) fn save_settings(&mut self) {
        if self.realtime_worker.is_some() {
            self.cancel_realtime("Realtime session 已因儲存設定而停止");
        }
        if let Some(error) = self.config_load_error.as_deref() {
            self.fail(AppError::Configuration(format!(
                "原設定檔載入失敗，為避免覆蓋已禁止儲存；請修正設定檔並重新啟動。{error}"
            )));
            return;
        }
        let previous_config = match AppConfig::load() {
            Ok(config) => config,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        let mut next_config = self.config.clone();
        next_config.hotkey = self.hotkey_edit.trim().to_string();
        next_config.xai.keyterms = self
            .keyterms_edit
            .lines()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .collect();
        if let Err(error) = next_config.validate() {
            self.config = previous_config.clone();
            self.hotkey_edit = previous_config.hotkey.clone();
            self.keyterms_edit = previous_config.xai.keyterms.join("\n");
            self.fail(error);
            return;
        }
        let hotkey = &mut self.hotkey;
        let result = apply_settings_transaction(
            &mut self.config,
            &previous_config,
            &next_config,
            startup::persist_config,
            |config| {
                if let Some(hotkey) = hotkey.as_ref() {
                    hotkey
                        .update(&config.hotkey)
                        .map_err(AppError::Configuration)
                } else {
                    let created =
                        GlobalHotkey::new(&config.hotkey).map_err(AppError::Configuration)?;
                    *hotkey = Some(created);
                    Ok(())
                }
            },
        );
        match result {
            Ok(()) => {
                self.hotkey_error = None;
                self.status = "設定已儲存".to_string();
            }
            Err(error) => {
                self.hotkey_edit = previous_config.hotkey.clone();
                self.keyterms_edit = previous_config.xai.keyterms.join("\n");
                self.fail(error);
            }
        }
    }
}

impl eframe::App for SpeakTypeCloudApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_external_target();
        self.poll_hotkey();
        self.poll_transcription();
        self.poll_realtime();
        let realtime_settings_before = realtime_settings_fingerprint(&self.config);
        if let Some(error) = self.recorder.take_stream_error() {
            if self.config.transcription_mode.is_realtime() {
                self.fail_realtime(error.to_string());
            } else {
                let _ = self.recorder.stop();
                self.recording_started = None;
                self.fail(error);
            }
        }
        if let Some(started) = self.recording_started {
            if should_stop_for_recording_limit(
                self.config.transcription_mode,
                started.elapsed(),
                self.config.recording.max_duration_secs,
            ) {
                self.stop_recording();
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // MARK: - Header
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("SpeakType Cloud")
                        .size(22.0)
                        .color(crate::theme::colors::TEXT_PRIMARY)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    crate::theme::caption(ui, match self.config.transcription_mode {
                        TranscriptionMode::BatchPtt => "Batch / PTT 模式",
                        TranscriptionMode::RealtimePtt => "Realtime PTT 模式",
                        TranscriptionMode::ContinuousDictation => "Continuous 模式",
                    });
                });
            });
            ui.add_space(2.0);
            crate::theme::caption(ui, match self.config.transcription_mode {
                TranscriptionMode::BatchPtt => {
                    "按住全域快捷鍵說話，放開後才上傳辨識。"
                }
                TranscriptionMode::RealtimePtt => {
                    "按住時開始串流，放開時手動 commit。"
                }
                TranscriptionMode::ContinuousDictation => {
                    "按下開始後由本地 VAD 分段；再次按下停止。"
                }
            });
            ui.add_space(12.0);

            // MARK: - Status card
            crate::theme::card_begin(ui, None);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("狀態").size(13.0).color(crate::theme::colors::TEXT_SECONDARY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.recorder.is_recording() {
                        crate::theme::recording_dot(ui, true);
                        ui.strong(
                            egui::RichText::new("錄音中")
                                .size(15.0)
                                .color(crate::theme::colors::RED_RECORDING),
                        );
                    } else if self.busy {
                        crate::theme::processing_dot(ui);
                        ui.strong(
                            egui::RichText::new("辨識中")
                                .size(15.0)
                                .color(crate::theme::colors::ACCENT_BLUE),
                        );
                    } else {
                        let msg = &self.status;
                        ui.strong(
                            egui::RichText::new(msg)
                                .size(15.0)
                                .color(crate::theme::colors::TEXT_PRIMARY),
                        );
                    }
                });
            });
            // Error / warning messages
            if let Some(error) = &self.last_error {
                ui.add_space(4.0);
                ui.colored_label(crate::theme::colors::RED_ERROR, error);
            }
            if let Some(error) = &self.hotkey_error {
                ui.add_space(4.0);
                ui.colored_label(
                    crate::theme::colors::ORANGE_WARNING,
                    format!("全域快捷鍵不可用：{error}"),
                );
            }
            if let Some(stats) = &self.live_audio_stats {
                let dropped = stats.dropped_chunks();
                let capture_dropped = stats.dropped_capture_samples();
                if dropped > 0 {
                    ui.colored_label(
                        crate::theme::colors::YELLOW_CAUTION,
                        format!("Realtime 音訊背壓：已丟棄 {dropped} 個 callback chunks"),
                    );
                }
                if capture_dropped > 0 {
                    ui.colored_label(
                        crate::theme::colors::YELLOW_CAUTION,
                        format!("錄音保留 ring 已覆寫／略過 {capture_dropped} 個 samples"),
                    );
                }
            }
            crate::theme::card_end(ui);

            // MARK: - Action buttons
            crate::theme::card_begin(ui, None);
            ui.horizontal(|ui| {
                let label = if self.recorder.is_recording() {
                    if self.config.transcription_mode == TranscriptionMode::ContinuousDictation {
                        "停止連續聽寫"
                    } else {
                        "停止錄音"
                    }
                } else {
                    if self.config.transcription_mode == TranscriptionMode::ContinuousDictation {
                        "開始連續聽寫"
                    } else {
                        "開始錄音"
                    }
                };
                if crate::theme::primary_button_enabled(ui, !self.busy, label).clicked() {
                    self.toggle_recording();
                }
                if crate::theme::secondary_button(ui, "再次貼上").clicked() && !self.last_text.is_empty() {
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
                if crate::theme::secondary_button(ui, "複製文字").clicked() {
                    match copy_text(&self.last_text) {
                        Ok(()) => self.status = "已複製到剪貼簿".to_string(),
                        Err(error) => {
                            self.status = "複製失敗，文字仍保留".to_string();
                            self.last_error = Some(error.to_string());
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                if crate::theme::primary_button_enabled(
                    ui,
                    self.busy
                        && !self
                            .active_job
                            .as_ref()
                            .is_some_and(|active| active.cancelling),
                    "取消辨識",
                )
                .on_disabled_hover_text("目前沒有進行中的辨識工作")
                .clicked()
                {
                    self.cancel_transcription();
                }
                if crate::theme::primary_button_enabled(
                    ui,
                    self.realtime_worker.is_some(),
                    "取消 Realtime",
                )
                .on_disabled_hover_text("沒有進行中的 Realtime session")
                .clicked()
                {
                    self.cancel_realtime("Realtime session 已由使用者取消");
                }
            });
            crate::theme::card_end(ui);

            // Batch fallback warning
            if self.batch_fallback_audio.is_some() {
                crate::theme::card_begin(ui, None);
                ui.colored_label(
                    crate::theme::colors::ORANGE_WARNING,
                    "Realtime 失敗後不會自動重傳。此段音訊可能包含已顯示的 partial；請確認後才改用 Batch。",
                );
                if crate::theme::primary_button(ui, "確認改用 Batch 上傳此段音訊").clicked() {
                    self.confirm_batch_fallback();
                }
                crate::theme::card_end(ui);
            }

            // MARK: - Realtime partial
            if !self.partial_text.is_empty() {
                crate::theme::card_begin(ui, Some("Realtime partial 字幕（尚未寫入 history／注入）"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.partial_text)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .interactive(false)
                        .font(egui::TextStyle::Monospace),
                );
                crate::theme::card_end(ui);
            }

            // MARK: - Latest transcript
            crate::theme::card_begin(ui, Some("最近辨識文字"));
            ui.add(
                egui::TextEdit::multiline(&mut self.last_text)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            crate::theme::card_end(ui);

            // MARK: - Recording & Output settings
            crate::theme::section_header(ui, "錄音與輸出設定");
            crate::theme::card_begin(ui, None);
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
            ui.add(
                egui::Slider::new(
                    &mut self.config.recording.max_duration_secs,
                    1..=MAX_RECORDING_DURATION_SECS,
                )
                .text("Batch／Realtime PTT 錄音上限（秒）"),
            );
            ui.horizontal(|ui| {
                ui.label("全域快捷鍵");
                ui.text_edit_singleline(&mut self.hotkey_edit);
            });
            ui.checkbox(&mut self.config.hold_to_record, "按住錄音、放開送出（PTT）");
            ui.checkbox(&mut self.config.launch_at_login, "Windows 登入時自動啟動");
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
            crate::theme::card_end(ui);

            // MARK: - Save button
            ui.add_space(12.0);
            if crate::theme::primary_button(ui, "儲存設定").clicked() {
                self.save_settings();
            }
        });
        if self.realtime_worker.is_some()
            && realtime_settings_fingerprint(&self.config) != realtime_settings_before
        {
            self.cancel_realtime("Realtime session 已因 provider／模式／設定變更而停止");
        }
        // Smart repaint scheduling — only repaint when state requires it:
        // - Recording active: poll at 8 Hz for timer/duration display
        // - Realtime session active: poll at 28 Hz for partial subtitles
        // - Batch transcription busy: poll at 8 Hz for completion
        // - Otherwise: poll at 4 Hz for tray/hotkey events (idle mode)
        let repaint_interval = if self.recorder.is_recording() {
            if self.config.transcription_mode.is_realtime() {
                Duration::from_millis(35) // ~28 Hz for realtime partial updates
            } else {
                Duration::from_millis(125) // ~8 Hz for timer/recording limit
            }
        } else if self.busy || self.realtime_worker.is_some() {
            Duration::from_millis(125) // ~8 Hz, waiting for completion
        } else {
            Duration::from_millis(250) // ~4 Hz idle — tray/hotkey polling only
        };
        ctx.request_repaint_after(repaint_interval);
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

fn is_current_job(active: Option<JobId>, received: JobId) -> bool {
    active == Some(received)
}

fn should_stop_for_recording_limit(
    mode: TranscriptionMode,
    elapsed: Duration,
    max_duration_secs: u64,
) -> bool {
    mode != TranscriptionMode::ContinuousDictation
        && elapsed >= Duration::from_secs(max_duration_secs)
}

fn realtime_settings_fingerprint(config: &AppConfig) -> String {
    format!(
        "{:?}|{:?}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        config.provider,
        config.transcription_mode,
        config.language,
        config.openai.api_key_env,
        config.xai.api_key_env,
        config.openrouter.api_key_env,
        config.openai.model,
        config.openrouter.model,
        config.realtime.openai_model,
        config.realtime.openai_transcription_delay.as_api_str(),
        config.realtime.xai_smart_turn_enabled,
        config.realtime.xai_smart_turn_threshold,
        config.realtime.xai_smart_turn_timeout_ms,
        config.realtime.vad_rms_threshold,
        config.realtime.vad_pre_roll_ms,
        config.realtime.vad_min_speech_ms,
        config.realtime.vad_silence_ms,
        config.realtime.max_utterance_secs,
    )
}

fn take_confirmed_batch_fallback(
    audio: &mut Option<RecordedAudio>,
    user_confirmed: bool,
) -> Option<RecordedAudio> {
    user_confirmed.then(|| audio.take()).flatten()
}

fn apply_settings_transaction<P, H>(
    current: &mut AppConfig,
    previous: &AppConfig,
    next: &AppConfig,
    mut persist: P,
    mut apply_runtime_hotkey: H,
) -> AppResult<()>
where
    P: FnMut(&AppConfig, &AppConfig) -> AppResult<()>,
    H: FnMut(&AppConfig) -> AppResult<()>,
{
    let result = match persist(previous, next) {
        Err(error) => Err(error),
        Ok(()) => match apply_runtime_hotkey(next) {
            Ok(()) => Ok(()),
            Err(runtime_error) => match persist(next, previous) {
                Ok(()) => Err(runtime_error),
                Err(rollback_error) => Err(AppError::Io(format!(
                    "{runtime_error}；設定與登入自啟回滾失敗：{rollback_error}"
                ))),
            },
        },
    };
    *current = if result.is_ok() {
        next.clone()
    } else {
        previous.clone()
    };
    result
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
        targets.accept_transcript(false);
        targets.observe(Some(second));

        assert_eq!(targets.last_text(), Some(first));
        assert_eq!(targets.start_recording(true, Some(second)), Some(second));
    }

    #[test]
    fn recording_without_external_target_is_copy_only() {
        let mut targets = TargetState::default();

        assert_eq!(targets.start_recording(true, None), None);
        targets.accept_transcript(false);

        assert_eq!(targets.last_text(), None);
        assert_eq!(
            targets.start_recording(false, Some(WindowTarget::from_raw_for_test(33))),
            None
        );
    }

    #[test]
    fn stale_transcription_result_is_not_current() {
        assert!(!is_current_job(Some(JobId(7)), JobId(6)));
        assert!(is_current_job(Some(JobId(7)), JobId(7)));
        assert!(!is_current_job(None, JobId(7)));
    }

    #[test]
    fn runtime_hotkey_failure_rolls_back_durable_state_and_current_config() {
        let previous = AppConfig::default();
        let mut next = previous.clone();
        next.launch_at_login = true;
        next.hotkey = "Ctrl+Alt+Space".to_string();
        let mut current = next.clone();
        let mut persist_calls = Vec::new();

        let result = apply_settings_transaction(
            &mut current,
            &previous,
            &next,
            |from, to| {
                persist_calls.push((from.launch_at_login, to.launch_at_login));
                Ok(())
            },
            |_| {
                Err(AppError::Configuration(
                    "injected hotkey failure".to_string(),
                ))
            },
        );

        assert!(result.is_err());
        assert_eq!(current.hotkey, previous.hotkey);
        assert_eq!(current.launch_at_login, previous.launch_at_login);
        assert_eq!(persist_calls, vec![(false, true), (true, false)]);
    }

    #[test]
    fn durable_failure_does_not_apply_runtime_hotkey() {
        let previous = AppConfig::default();
        let mut next = previous.clone();
        next.hotkey = "Ctrl+Alt+Space".to_string();
        let mut current = next.clone();
        let runtime_called = std::cell::Cell::new(false);

        let result = apply_settings_transaction(
            &mut current,
            &previous,
            &next,
            |_, _| Err(AppError::Io("injected persist failure".to_string())),
            |_| {
                runtime_called.set(true);
                Ok(())
            },
        );

        assert!(result.is_err());
        assert!(!runtime_called.get());
        assert_eq!(current.hotkey, previous.hotkey);
    }

    #[test]
    fn batch_fallback_requires_explicit_confirmation() {
        let mut audio = Some(RecordedAudio {
            samples: vec![0.1; 160],
            sample_rate: 16_000,
            channels: 1,
        });
        assert!(take_confirmed_batch_fallback(&mut audio, false).is_none());
        assert!(audio.is_some());
        assert!(take_confirmed_batch_fallback(&mut audio, true).is_some());
        assert!(audio.is_none());
    }

    #[test]
    fn continuous_final_preserves_target_for_next_utterance() {
        let target = WindowTarget::from_raw_for_test(44);
        let mut targets = TargetState::default();
        targets.start_recording(true, Some(target));
        assert_eq!(targets.accept_transcript(true), Some(target));
        assert_eq!(targets.accept_transcript(true), Some(target));
        assert_eq!(targets.accept_transcript(false), Some(target));
    }

    #[test]
    fn continuous_session_is_not_stopped_by_batch_recording_limit() {
        assert!(!should_stop_for_recording_limit(
            TranscriptionMode::ContinuousDictation,
            Duration::from_secs(121),
            120,
        ));
        assert!(should_stop_for_recording_limit(
            TranscriptionMode::RealtimePtt,
            Duration::from_secs(121),
            120,
        ));
    }
}
