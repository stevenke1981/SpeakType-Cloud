use crate::audio::{LiveAudioStats, RecordedAudio, Recorder};
use crate::config::{AppConfig, OutputBufferMode, ProviderKind, TranscriptionMode};
use crate::error::{AppError, AppResult};
use crate::history::{self, HistoryEntry};
use crate::hotkey::{GlobalHotkey, HotkeyEvent};
use crate::injector::{copy_text, inject_text, WindowTarget};
use crate::postprocess::{format_transcript, process_transcript, DeliveryMode};
use crate::providers::ProviderResponse;
use crate::realtime::RealtimeEvent;
use crate::realtime_worker::{self, RealtimeWorkerHandle, WorkerMessage};
use crate::runtime::RuntimeMode;
use crate::startup;
use crate::transcription::{self, CancellationToken, JobId, JobMessage, JobResult};
use chrono::Local;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub struct SpeakTypeCloudApp {
    mode: RuntimeMode,
    closing: bool,
    demo_recording: bool,
    raw_text: String,
    preview_revision: u64,
    pub dictionary_suggestions: Vec<crate::config::DictionaryEntry>,
    pub style_suggestion: Option<String>,
    retry_audio: Option<RecordedAudio>,
    pub config: AppConfig,
    recorder: Recorder,
    hotkey: Option<GlobalHotkey>,
    hotkey_error: Option<String>,
    pub devices: Vec<String>,
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
    realtime_cancelled: bool,
    realtime_stopped: bool,
    session_config: Option<AppConfig>,
    realtime_shutdown_reason: Option<String>,
    active_job: Option<ActiveJob>,
    next_job_id: u64,
    busy: bool,
    status: String,
    last_text: String,
    pub(crate) last_error: Option<String>,
    pub hotkey_edit: String,
    pub(crate) keyterms_edit: String,
    config_load_error: Option<String>,
    next_hotkey_retry: Option<Instant>,
    persistence_rx: Vec<Receiver<Result<(), String>>>,
    history_store: Option<history::HistoryStore>,
    last_history_cleanup: Option<Instant>,
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
    config: AppConfig,
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
    pub fn new(mode: RuntimeMode) -> Self {
        let (mut config, config_load_error) = match if mode.is_demo() {
            Ok(AppConfig::default())
        } else {
            AppConfig::load()
        } {
            Ok(config) => (config, None),
            Err(error) => (AppConfig::default(), Some(error.to_string())),
        };
        if mode.is_demo() {
            config.output.auto_inject = false;
            config.output.buffer_mode = OutputBufferMode::Temporary;
            config.save_recordings = false;
        }
        let recorder = Recorder::new(
            config.recording.input_device_name.clone(),
            config.recording.gain,
        );
        let devices = if mode.is_demo() {
            Vec::new()
        } else {
            recorder.list_devices()
        };
        let (hotkey, hotkey_error) = if mode.is_demo() {
            (None, None)
        } else {
            match GlobalHotkey::new(&config.hotkey) {
                Ok(hotkey) => (Some(hotkey), None),
                Err(error) => (None, Some(error)),
            }
        };
        let has_startup_error = config_load_error.is_some() || hotkey_error.is_some();
        let hotkey_failed_at_startup = hotkey_error.is_some();
        Self {
            mode,
            demo_recording: false,
            closing: false,
            raw_text: String::new(),
            preview_revision: 0,
            dictionary_suggestions: Vec::new(),
            style_suggestion: None,
            retry_audio: None,
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
            realtime_cancelled: false,
            realtime_stopped: false,
            session_config: None,
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
            persistence_rx: Vec::new(),
            history_store: if mode.is_demo() {
                None
            } else {
                Some(history::HistoryStore::production())
            },
            last_history_cleanup: None,
            next_hotkey_retry: if hotkey_failed_at_startup {
                Some(Instant::now() + Duration::from_secs(4))
            } else {
                None
            },
        }
    }

    fn start_recording(&mut self) {
        if self.closing
            || self.busy
            || self.active_job.is_some()
            || self.transcription_rx.is_some()
            || self.is_recording()
            || self.realtime_worker.is_some()
            || self.realtime_rx.is_some()
        {
            return;
        }
        if self.mode.is_demo() {
            if !self.busy && !self.demo_recording {
                self.demo_recording = true;
                self.recording_started = Some(Instant::now());
                self.last_error = None;
                self.status = "示範：合成音訊擷取中（未開啟麥克風）".into();
            }
            return;
        }
        if self.busy || self.recorder.is_recording() {
            return;
        }
        if let Err(error) = self.config.validate() {
            self.fail(error);
            return;
        }
        if (self.config.provider == ProviderKind::Gemini || self.config.cleanup.enabled)
            && !self.config.gemini.cloud_consent
        {
            self.fail(AppError::Configuration(
                "請先在設定同意 Google 資料傳送".into(),
            ));
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
                    self.session_config = Some(self.config.clone());
                    self.realtime_cancelled = false;
                    self.realtime_stopped = false;
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
                    match self.config.output.buffer_mode {
                        OutputBufferMode::Temporary => {
                            "錄音中…未找到外部目標，完成後文字保留在 App 暫存區".to_string()
                        }
                        OutputBufferMode::Clipboard => {
                            "錄音中…未找到外部目標，完成後只會複製文字".to_string()
                        }
                    }
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
        if self.mode.is_demo() {
            if self.demo_recording {
                self.demo_recording = false;
                self.recording_started = None;
                self.spawn_batch_transcription(crate::transcription::synthetic_audio());
            }
            return;
        }
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
        if self.closing
            || self.busy
            || self.active_job.is_some()
            || self.transcription_rx.is_some()
            || self.realtime_worker.is_some()
            || self.realtime_rx.is_some()
        {
            return;
        }
        self.retry_audio = Some(audio.clone());

        let (tx, rx) = mpsc::channel();
        let id = JobId(self.next_job_id);
        self.next_job_id = self.next_job_id.wrapping_add(1).max(1);
        let cancellation = if self.mode.is_demo() {
            transcription::spawn_demo(id, audio, tx)
        } else {
            transcription::spawn(id, self.config.clone(), audio, tx)
        };
        self.transcription_rx = Some(rx);
        self.active_job = Some(ActiveJob {
            id,
            cancellation,
            cancelling: false,
            config: self.config.clone(),
        });
        self.busy = true;
        self.status = if self.mode.is_demo() {
            "正在處理離線合成音訊…".into()
        } else {
            format!("正在使用 {} 辨識…", self.config.provider.label())
        };
    }

    fn toggle_recording(&mut self) {
        if self.is_recording() {
            self.stop_recording();
        } else {
            self.start_recording();
        }
    }

    fn poll_hotkey(&mut self) {
        if self.mode.is_demo() {
            return;
        }
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
        let received = self.transcription_rx.as_ref().map(|rx| rx.try_recv());
        let message = match received {
            Some(Ok(message)) => message,
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.transcription_rx = None;
                self.active_job = None;
                self.targets.cancel_recording();
                self.fail(AppError::Transcription("辨識工作已意外結束，請重試".into()));
                return;
            }
            _ => return,
        };
        if !is_current_job(self.active_job.as_ref().map(|active| active.id), message.id) {
            return;
        }
        let Some(active) = self.active_job.take() else {
            return;
        };
        self.transcription_rx = None;
        self.busy = false;
        // Cancel wins even if success was queued before the user cancelled.
        // The terminal channel is per-job, and no next job is admitted before
        // this completion (or a disconnected worker) is observed.
        if active.cancelling {
            self.targets.cancel_recording();
            self.status = "辨識已取消".into();
            self.last_error = None;
            return;
        }
        match message.result {
            Ok(job) => self.accept(job, false, &active.config),
            Err(AppError::Cancelled) => {
                self.targets.cancel_recording();
                self.status = "辨識已取消".into();
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
        if !self.realtime_cancelled
            && self.session_config.as_ref().is_some_and(|accepted| {
                realtime_settings_fingerprint(accepted)
                    != realtime_settings_fingerprint(&self.config)
            })
        {
            self.cancel_realtime("設定已變更；先前的即時聽寫已取消");
        }
        let mut messages = Vec::new();
        if let Some(rx) = &self.realtime_rx {
            loop {
                match rx.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        // A panic can skip Stopped. Still recover without admitting
                        // a replacement while the actual worker is alive.
                        if !self.realtime_stopped
                            && !messages.iter().any(|m| matches!(m, WorkerMessage::Stopped))
                        {
                            self.realtime_cancelled = true;
                            self.last_error = Some("即時工作連線已結束".into());
                        }
                        self.realtime_stopped = true;
                        break;
                    }
                }
            }
        }
        for message in messages {
            if self.realtime_cancelled && !matches!(message, WorkerMessage::Stopped) {
                continue;
            }
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
                    let accepted_config = self
                        .session_config
                        .clone()
                        .unwrap_or_else(|| self.config.clone());
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
                            cleanup: None,
                            cleanup_error: None,
                        },
                        preserve_target,
                        &accepted_config,
                    );
                    if !preserve_target {
                        self.realtime_cancelled = true;
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
                    self.realtime_stopped = true;
                }
            }
        }
        if self.realtime_stopped
            && self
                .realtime_worker
                .as_ref()
                .is_none_or(RealtimeWorkerHandle::is_finished)
        {
            if let Some(mut worker) = self.realtime_worker.take() {
                if !worker.join_if_finished() {
                    self.last_error = Some("即時工作未正常結束".into());
                }
            }
            self.realtime_rx = None;
            self.live_audio_stats = None;
            self.pending_realtime_audio = None;
            self.realtime_stop_requested = false;
            self.session_config = None;
            self.realtime_stopped = false;
            if self.recorder.is_recording() {
                let _ = self.recorder.stop();
            }
            self.recording_started = None;
            if let Some(reason) = self.realtime_shutdown_reason.take() {
                self.status = reason;
            }
        }
    }

    fn fail_realtime(&mut self, error: String) {
        self.realtime_cancelled = true;
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
        self.realtime_cancelled = true;
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
        } else if self.realtime_rx.is_none() {
            self.live_audio_stats = None;
            self.realtime_shutdown_reason = None;
            self.status = reason.to_string();
        }
    }

    pub fn confirm_batch_fallback(&mut self) {
        if self.busy || self.realtime_worker.is_some() || self.realtime_rx.is_some() {
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
        if self.mode.is_demo() {
            return;
        }
        self.targets.observe(WindowTarget::capture_external());
    }

    fn accept(
        &mut self,
        job: JobResult,
        preserve_recording_target: bool,
        accepted_config: &AppConfig,
    ) {
        self.raw_text = job.response.text.clone();
        self.preview_revision = self.preview_revision.wrapping_add(1);
        let source = job
            .cleanup
            .as_ref()
            .map(|result| result.text.as_str())
            .unwrap_or(&job.response.text);
        self.dictionary_suggestions = job
            .cleanup
            .as_ref()
            .map(|result| result.dictionary_suggestions.clone())
            .unwrap_or_default();
        self.style_suggestion = job
            .cleanup
            .as_ref()
            .and_then(|result| result.style_suggestion.clone());
        let (text, delivery, processing_error) = match process_transcript(
            source,
            &accepted_config.text_processing,
            accepted_config.output.append_space,
        ) {
            Ok(processed) => (processed.text, processed.delivery, None),
            Err(error) => (
                format_transcript(
                    &job.response.text,
                    &accepted_config.text_processing,
                    accepted_config.output.append_space,
                ),
                DeliveryMode::Normal,
                Some(format!("文字後處理失敗，已保留原辨識文字：{error}")),
            ),
        };
        let target = self.targets.accept_transcript(preserve_recording_target);
        self.last_text = text.clone();
        self.status = "辨識完成".to_string();
        self.last_error = processing_error.or(job.cleanup_error);
        if delivery == DeliveryMode::Discard {
            self.status = "語音命令已執行：不輸出文字".to_string();
            return;
        }
        if self.mode.is_demo() {
            self.status = "示範結果已就緒；未傳送、未儲存、未寫入剪貼簿".into();
            return;
        }
        // Only an accepted, non-cancelled result may reach persistent storage.
        if accepted_config.history.save_text || accepted_config.save_recordings {
            let Some(store) = self.history_store.clone() else {
                return;
            };
            let entry = HistoryEntry {
                id: history::next_id(),
                created_at: Local::now(),
                provider: job.response.provider.clone(),
                duration_secs: job.local_duration_secs,
                text: text.clone(),
                audio: None,
            };
            let audio = if accepted_config.transcription_mode.is_realtime() {
                self.pending_realtime_audio.clone()
            } else {
                self.retry_audio.clone()
            };
            let save_text = accepted_config.history.save_text;
            let save_audio = accepted_config.save_recordings;
            let (tx, rx) = mpsc::channel();
            self.persistence_rx.push(rx);
            std::thread::spawn(move || {
                let _ =
                    tx.send(store.commit_accepted(entry, audio.as_ref(), save_text, save_audio));
            });
        }

        if accepted_config.cleanup.enabled {
            self.status = "整理預覽已就緒；請比較原文後明確複製或貼上".into();
            return;
        }
        match accepted_config.output.buffer_mode {
            OutputBufferMode::Temporary => {
                // App 暫存區模式：不污染系統剪貼簿
                if accepted_config.output.auto_inject && delivery == DeliveryMode::Normal {
                    if let Some(target) = target {
                        if let Err(injection_error) = inject_text(
                            Some(target),
                            &text,
                            accepted_config.output.restore_clipboard,
                        ) {
                            if accepted_config.output.copy_only_on_injection_failure {
                                let feedback =
                                    fallback_delivery_feedback(injection_error, copy_text(&text));
                                self.status = feedback.status.to_string();
                                record_nonfatal_error(&mut self.last_error, &feedback.error);
                            } else {
                                self.status = "自動貼上失敗，文字保留在 App 暫存區".to_string();
                                record_nonfatal_error(
                                    &mut self.last_error,
                                    &injection_error.to_string(),
                                );
                            }
                        } else {
                            self.status = "已輸入錄音開始前的外部視窗".to_string();
                        }
                    } else {
                        // Temporary mode + no target = keep in last_text only
                        self.status = "沒有外部目標，文字保留在 App 暫存區".to_string();
                    }
                } else if delivery == DeliveryMode::CopyOnly {
                    // CopyOnly 命令在 Temporary 模式下仍需複製
                    if let Err(error) = copy_text(&text) {
                        self.status = "複製失敗，文字保留在 App 暫存區".to_string();
                        record_nonfatal_error(&mut self.last_error, &error.to_string());
                    } else {
                        self.status = "語音命令已執行：文字已複製到剪貼簿".to_string();
                    }
                } else {
                    // 純暫存 — 不碰剪貼簿
                    self.status = "文字保留在 App 暫存區".to_string();
                }
            }
            OutputBufferMode::Clipboard => {
                // 剪貼簿模式：始終複製到系統剪貼簿（保留原行為）
                if accepted_config.output.auto_inject && delivery == DeliveryMode::Normal {
                    if let Some(target) = target {
                        if let Err(injection_error) = inject_text(
                            Some(target),
                            &text,
                            accepted_config.output.restore_clipboard,
                        ) {
                            if accepted_config.output.copy_only_on_injection_failure {
                                let feedback =
                                    fallback_delivery_feedback(injection_error, copy_text(&text));
                                self.status = feedback.status.to_string();
                                record_nonfatal_error(&mut self.last_error, &feedback.error);
                            } else {
                                self.status = "自動貼上失敗，文字保留在最近辨識文字區".to_string();
                                record_nonfatal_error(
                                    &mut self.last_error,
                                    &injection_error.to_string(),
                                );
                            }
                        } else {
                            self.status = "已輸入錄音開始前的外部視窗".to_string();
                        }
                    } else if let Err(error) = copy_text(&text) {
                        self.status =
                            "沒有外部目標且複製失敗，文字保留在最近辨識文字區".to_string();
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
        }
    }

    pub fn poll_persistence(&mut self) {
        self.persistence_rx.retain(|rx| match rx.try_recv() {
            Ok(result) => {
                if let Err(error) = result {
                    record_nonfatal_error(
                        &mut self.last_error,
                        &format!("歷史紀錄未保存：{error}"),
                    );
                }
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                record_nonfatal_error(&mut self.last_error, "歷史儲存工作中斷");
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
        });
    }
    pub fn fallback_available(&self) -> bool {
        self.batch_fallback_audio.is_some()
    }
    pub fn open_history_preview(&mut self, text: String) {
        self.raw_text = text.clone();
        self.last_text = text;
        self.targets.last_text = None;
        self.preview_revision = self.preview_revision.wrapping_add(1);
        self.status = "歷史文字已開啟；貼上需重新取得安全目標".into();
    }
    pub fn restore_original(&mut self) {
        self.last_text = self.raw_text.clone();
        self.preview_revision = self.preview_revision.wrapping_add(1);
        self.status = "已還原轉錄原文".into();
    }
    pub fn accept_dictionary_suggestion(&mut self, index: usize) {
        if let Some(entry) = self.dictionary_suggestions.get(index).cloned() {
            if let Some(existing) = self
                .config
                .text_processing
                .dictionary
                .iter_mut()
                .find(|item| item.source == entry.source)
            {
                *existing = entry;
            } else {
                self.config.text_processing.dictionary.push(entry);
            }
            self.dictionary_suggestions.remove(index);
            self.status = "已接受詞典建議；請儲存設定以保留".into();
        }
    }
    pub fn reject_dictionary_suggestion(&mut self, index: usize) {
        if index < self.dictionary_suggestions.len() {
            self.dictionary_suggestions.remove(index);
        }
    }
    pub fn accept_style_suggestion(&mut self) {
        if let Some(style) = self.style_suggestion.take() {
            self.config.cleanup.custom_style = style;
            self.status = "已接受風格建議；請儲存設定以保留".into();
        }
    }
    pub fn tick(&mut self) {
        self.poll_persistence();
        if self.closing {
            self.poll_transcription();
            self.poll_realtime();
            self.status = "正在結束背景工作，請稍候…".into();
            return;
        }
        if !self.mode.is_demo()
            && self.config.history.retention_days > 0
            && self
                .last_history_cleanup
                .is_none_or(|time| time.elapsed() > Duration::from_secs(3600))
        {
            self.last_history_cleanup = Some(Instant::now());
            let days = self.config.history.retention_days;
            let (tx, rx) = mpsc::channel();
            self.persistence_rx.push(rx);
            std::thread::spawn(move || {
                let _ = tx.send(history::cleanup_older_than(days).map(|_| ()));
            });
        }
        self.refresh_external_target();
        self.poll_hotkey();
        self.poll_transcription();
        self.poll_realtime();
        if let Some(error) = self.recorder.take_stream_error() {
            if self.config.transcription_mode.is_realtime() {
                self.fail_realtime(error.to_string());
            } else {
                let _ = self.recorder.stop();
                self.recording_started = None;
                self.fail(error);
            }
        }
        if self.recording_started.is_some_and(|started| {
            should_stop_for_recording_limit(
                self.config.transcription_mode,
                started.elapsed(),
                self.config.recording.max_duration_secs,
            )
        }) {
            self.stop_recording();
        }
    }

    pub fn is_recording(&self) -> bool {
        self.demo_recording || self.recorder.is_recording()
    }
    pub fn is_busy(&self) -> bool {
        self.busy
    }
    pub fn capture_health(&self) -> String {
        self.live_audio_stats
            .as_ref()
            .map(|stats| {
                format!(
                    "音訊狀態：遺失 {} 區塊／{} 樣本",
                    stats.dropped_chunks(),
                    stats.dropped_capture_samples()
                )
            })
            .unwrap_or_default()
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn raw_text(&self) -> &str {
        &self.raw_text
    }
    pub fn text(&self) -> &str {
        &self.last_text
    }
    pub fn partial_text(&self) -> &str {
        &self.partial_text
    }
    pub fn toggle(&mut self) {
        self.toggle_recording();
    }
    pub fn cancel(&mut self) {
        if self.demo_recording {
            self.demo_recording = false;
            self.recording_started = None;
            self.status = "示範擷取已取消".into();
        } else if self.realtime_worker.is_some() {
            self.cancel_realtime("已取消即時聽寫");
        } else if self.recorder.is_recording() {
            let _ = self.recorder.stop();
            self.recording_started = None;
            self.targets.cancel_recording();
            self.status = "錄音已取消".into();
        } else {
            self.cancel_transcription();
        }
    }
    pub fn retry(&mut self) {
        if !self.busy && !self.is_recording() {
            if let Some(audio) = self.retry_audio.clone() {
                self.spawn_batch_transcription(audio);
            }
        }
    }
    pub fn copy_preview(&mut self, text: String) {
        self.last_text = text;
        if self.mode.is_demo() {
            self.status = "示範：已模擬複製，系統剪貼簿未變更".into();
        } else if let Err(error) = copy_text(&self.last_text) {
            self.fail(error);
        } else {
            self.status = "已複製預覽文字".into();
        }
    }
    pub fn preview_revision(&self) -> u64 {
        self.preview_revision
    }
    pub fn paste_preview(&mut self, text: String) {
        self.last_text = text;
        if self.mode.is_demo() {
            self.status = "示範：已模擬貼上，未操作任何其他程式".into();
        } else if let Err(error) = inject_text(
            self.targets.last_text(),
            &self.last_text,
            self.config.output.restore_clipboard,
        ) {
            self.fail(error);
        } else {
            self.status = "已送出貼上；請確認目標程式".into();
        }
    }
    pub fn shutdown(&mut self) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.cancel();
        if let Some(active) = &self.active_job {
            active.cancellation.cancel();
        }
    }

    pub fn is_closing(&self) -> bool {
        self.closing
    }

    pub fn shutdown_ready(&self) -> bool {
        self.closing
            && self.active_job.is_none()
            && self.transcription_rx.is_none()
            && self.realtime_worker.is_none()
            && self.realtime_rx.is_none()
            && self.persistence_rx.is_empty()
            && !self.is_recording()
    }

    fn fail(&mut self, error: AppError) {
        self.busy = self.active_job.is_some()
            || self.transcription_rx.is_some()
            || self.realtime_worker.is_some()
            || self.realtime_rx.is_some();
        self.status = "發生錯誤".to_string();
        self.last_error = Some(error.to_string());
    }

    pub(crate) fn save_settings(&mut self) {
        if self.is_recording() {
            self.cancel();
        }
        self.cancel_transcription();
        if self.realtime_worker.is_some() || self.realtime_rx.is_some() {
            self.cancel_realtime("設定變更已取消先前工作");
        }
        if self.mode.is_demo() {
            self.status = "示範設定僅保留於記憶體".into();
            return;
        }
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

#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
#[path = "legacy_view.rs"]
mod legacy_view;

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

fn realtime_settings_fingerprint(config: &AppConfig) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    // Config never contains key values. Cover every delivery/persistence setting.
    serde_json::to_string(config)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
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

    fn queue_batch_success(app: &mut SpeakTypeCloudApp) {
        let (tx, rx) = mpsc::channel();
        let id = JobId(99);
        app.active_job = Some(ActiveJob {
            id,
            cancellation: CancellationToken::new(),
            cancelling: false,
            config: app.config.clone(),
        });
        app.transcription_rx = Some(rx);
        app.busy = true;
        tx.send(JobMessage {
            id,
            result: Ok(JobResult {
                response: ProviderResponse {
                    text: "must not deliver".into(),
                    provider: "mock".into(),
                    duration_secs: None,
                    model: None,
                },
                local_duration_secs: 1.0,
                cleanup: None,
                cleanup_error: None,
            }),
        })
        .expect("queue success");
    }

    #[test]
    fn cancelled_job_cannot_persist_but_accepted_job_uses_injected_store() {
        let root = std::env::current_dir()
            .expect("workspace")
            .join("target")
            .join(format!("app-history-fixture-{}", history::next_id()));
        let store = history::HistoryStore::new(root.clone());
        let mut app = SpeakTypeCloudApp::new(RuntimeMode::Demo);
        // Reuse the isolated constructor: no private config, microphone, key or hotkey.
        // Only the accept/storage branch is switched on, with an owned fixture root.
        app.mode = RuntimeMode::Live;
        app.history_store = Some(store.clone());
        app.config.history.save_text = true;
        app.config.save_recordings = true;
        app.retry_audio = Some(transcription::synthetic_audio());
        queue_batch_success(&mut app);
        app.cancel_transcription();
        app.poll_transcription();
        assert!(!root.exists());
        assert!(app.persistence_rx.is_empty());
        queue_batch_success(&mut app);
        app.poll_transcription();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !app.persistence_rx.is_empty() && Instant::now() < deadline {
            app.poll_persistence();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.persistence_rx.is_empty());
        let entries = store.load().expect("fixture history");
        assert_eq!(entries.len(), 1);
        let name = entries[0].audio.as_ref().expect("opted-in audio");
        assert_eq!(
            &std::fs::read(root.join("audio").join(name)).expect("fixture WAV")[..4],
            b"RIFF"
        );
        std::fs::remove_dir_all(root).expect("remove owned fixture");
    }

    #[test]
    fn settings_failure_keeps_batch_ownership_until_disconnect() {
        let mut app = SpeakTypeCloudApp::new(RuntimeMode::Demo);
        let (tx, rx) = mpsc::channel();
        app.transcription_rx = Some(rx);
        app.active_job = Some(ActiveJob {
            id: JobId(99),
            cancellation: CancellationToken::new(),
            cancelling: false,
            config: app.config.clone(),
        });
        app.busy = true;
        app.retry_audio = Some(transcription::synthetic_audio());
        // Inject precisely the settings error path, without accessing any profile.
        app.cancel_transcription();
        app.fail(AppError::Configuration(
            "injected settings write failure".into(),
        ));
        app.toggle();
        assert!(!app.is_recording());
        app.retry();
        assert_eq!(app.active_job.as_ref().expect("owned job").id, JobId(99));
        app.spawn_batch_transcription(transcription::synthetic_audio());
        assert_eq!(app.active_job.as_ref().expect("owned job").id, JobId(99));
        drop(tx);
        app.poll_transcription();
        app.toggle();
        assert!(app.is_recording());
    }

    #[test]
    fn queued_success_is_rejected_after_cancel_and_start_waits_for_ack() {
        let mut app = SpeakTypeCloudApp::new(RuntimeMode::Demo);
        queue_batch_success(&mut app);
        app.cancel();
        app.toggle();
        assert!(!app.is_recording());
        assert!(app.busy);
        app.poll_transcription();
        assert!(!app.busy);
        assert!(app.raw_text.is_empty());
        assert!(app.last_text.is_empty());
        assert!(app.persistence_rx.is_empty());
        app.toggle();
        assert!(app.is_recording());
    }

    #[test]
    fn queued_realtime_final_is_rejected_after_cancel_until_stopped() {
        let mut app = SpeakTypeCloudApp::new(RuntimeMode::Demo);
        let (tx, rx) = mpsc::channel();
        app.realtime_rx = Some(rx);
        tx.send(WorkerMessage::Event(RealtimeEvent::Final(
            "must not deliver".into(),
        )))
        .expect("send final");
        app.cancel_realtime("cancel");
        app.toggle();
        assert!(!app.is_recording());
        app.poll_realtime();
        assert!(app.raw_text.is_empty());
        assert!(app.last_text.is_empty());
        assert!(app.realtime_rx.is_some());
        tx.send(WorkerMessage::Stopped).expect("send stop");
        app.poll_realtime();
        assert!(app.realtime_rx.is_none());
        app.toggle();
        assert!(app.is_recording());
    }

    #[test]
    fn changing_settings_invalidates_queued_final_and_batch_success() {
        let mut app = SpeakTypeCloudApp::new(RuntimeMode::Demo);
        let (tx, rx) = mpsc::channel();
        app.realtime_rx = Some(rx);
        app.session_config = Some(app.config.clone());
        tx.send(WorkerMessage::Event(RealtimeEvent::Final(
            "old settings".into(),
        )))
        .expect("send final");
        app.config.provider = ProviderKind::Xai;
        app.poll_realtime();
        assert!(app.raw_text.is_empty());
        assert!(app.realtime_cancelled);
        tx.send(WorkerMessage::Stopped).expect("stop");
        app.poll_realtime();
        queue_batch_success(&mut app);
        app.save_settings();
        app.poll_transcription();
        assert!(app.raw_text.is_empty());
    }

    #[test]
    fn disconnected_workers_recover_instead_of_remaining_busy() {
        let mut app = SpeakTypeCloudApp::new(RuntimeMode::Demo);
        let (tx, rx) = mpsc::channel();
        app.transcription_rx = Some(rx);
        app.busy = true;
        drop(tx);
        app.poll_transcription();
        assert!(!app.busy);
        assert!(app.last_error.is_some());
        let (tx, rx) = mpsc::channel();
        app.realtime_rx = Some(rx);
        drop(tx);
        app.poll_realtime();
        assert!(app.realtime_rx.is_none());
    }

    #[test]
    fn demo_bootstrap_and_commands_have_no_live_handles() {
        let mut app = SpeakTypeCloudApp::new(RuntimeMode::Demo);
        assert!(app.hotkey.is_none());
        assert!(app.devices.is_empty());
        app.tick();
        app.save_settings();
        app.copy_preview("synthetic".into());
        app.paste_preview("synthetic".into());
        assert!(app.hotkey.is_none());
        assert!(app.targets.last_external.is_none());
        assert!(app.targets.last_text.is_none());
        assert!(app.persistence_rx.is_empty());
        app.toggle();
        assert!(app.is_recording());
        assert!(!app.recorder.is_recording());
        app.cancel();
        assert!(!app.is_recording());
    }

    #[test]
    fn shutdown_drains_owned_work_and_rejects_new_recording() {
        let mut app = SpeakTypeCloudApp::new(RuntimeMode::Demo);
        queue_batch_success(&mut app);
        let (tx, rx) = mpsc::channel();
        app.persistence_rx.push(rx);
        app.shutdown();
        assert!(!app.shutdown_ready());
        app.tick();
        assert!(app.raw_text.is_empty());
        assert!(!app.shutdown_ready(), "pending disk commit must finish");
        app.toggle();
        assert!(!app.is_recording());
        tx.send(Ok(())).expect("complete synthetic persistence");
        app.tick();
        assert!(app.shutdown_ready());
    }

    #[test]
    fn demo_uses_real_worker_wav_and_cancellation_without_a_microphone() {
        let mut app = SpeakTypeCloudApp::new(RuntimeMode::Demo);
        app.toggle();
        app.toggle();
        assert!(app.busy);
        app.cancel();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.busy && Instant::now() < deadline {
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!app.busy);
        assert!(app.last_text.is_empty());
        app.retry();
        while app.busy && Instant::now() < deadline {
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!app.busy);
        assert!(app.last_text.contains("下午 3 點"));
        assert!(app.persistence_rx.is_empty());
        assert!(!app.recorder.is_recording());
    }

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
