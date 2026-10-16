//! Original egui view retained behind legacy-ui during migration.
use super::*;
use eframe::egui;

impl eframe::App for SpeakTypeCloudApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_external_target();
        self.poll_persistence();
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
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
            // MARK: - Mode selector
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("辨識模式")
                        .size(14.0)
                        .color(crate::theme::colors::TEXT_PRIMARY)
                        .strong(),
                );
                    // AcmeUIKit-inspired segmented controls remain in normal
                    // reading order and wrap safely at the minimum width.
                    if crate::theme::mode_pill(
                        ui,
                        self.config.transcription_mode == TranscriptionMode::BatchPtt,
                        "Batch / PTT",
                    )
                    .clicked()
                    {
                        if self.recorder.is_recording() || self.busy {
                            self.status = "請先停止目前錄音或等待辨識完成後再切換模式。".to_string();
                        } else {
                            self.config.transcription_mode = TranscriptionMode::BatchPtt;
                        }
                    }
                    if crate::theme::mode_pill(
                        ui,
                        self.config.transcription_mode == TranscriptionMode::RealtimePtt,
                        "Realtime PTT",
                    )
                    .clicked()
                    {
                        if self.recorder.is_recording() || self.busy {
                            self.status = "請先停止目前錄音或等待辨識完成後再切換模式。".to_string();
                        } else {
                            self.config.transcription_mode = TranscriptionMode::RealtimePtt;
                        }
                    }
                    if crate::theme::mode_pill(
                        ui,
                        self.config.transcription_mode == TranscriptionMode::ContinuousDictation,
                        "Continuous",
                    )
                    .clicked()
                    {
                        if self.recorder.is_recording() || self.busy {
                            self.status = "請先停止目前錄音或等待辨識完成後再切換模式。".to_string();
                        } else {
                            self.config.transcription_mode = TranscriptionMode::ContinuousDictation;
                        }
                    }
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

            // MARK: - Status & Action toolbar (AcmeUIKit-inspired single compact card)
            crate::theme::card(
                ui,
                Some("狀態"),
                Some("目前系統狀態與主要控制按鈕。"),
                |ui| {
            // Status line
            ui.horizontal_wrapped(|ui| {
                if self.recorder.is_recording() {
                    crate::theme::recording_dot(ui, true);
                    ui.strong(
                        egui::RichText::new("錄音中")
                            .color(crate::theme::colors::RED_RECORDING),
                    );
                } else if self.busy {
                    crate::theme::processing_dot(ui);
                    ui.strong(
                        egui::RichText::new("辨識中")
                            .color(crate::theme::colors::ACCENT_BLUE),
                    );
                } else {
                    let msg = &self.status;
                    ui.strong(
                        egui::RichText::new(msg)
                            .color(crate::theme::colors::TEXT_PRIMARY),
                    );
                }
            });
                let record_label = if self.recorder.is_recording() {
                    if self.config.transcription_mode == TranscriptionMode::ContinuousDictation {
                        "停止連續聽寫"
                    } else {
                        "停止錄音"
                    }
                } else if self.config.transcription_mode == TranscriptionMode::ContinuousDictation {
                    "開始連續聽寫"
                } else {
                    "開始錄音"
                };
                if crate::theme::primary_button_enabled(ui, !self.busy, record_label).clicked() {
                    self.toggle_recording();
                }
            // Cancel buttons line (when applicable)
            if self.busy || self.realtime_worker.is_some() {
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
            }
            // Error / warning messages (§9: color + icon + text)
            if let Some(error) = &self.last_error {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.colored_label(crate::theme::colors::RED_ERROR, "✗");
                    ui.colored_label(crate::theme::colors::RED_ERROR, error);
                });
            }
            if let Some(error) = &self.hotkey_error {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.colored_label(crate::theme::colors::ORANGE_WARNING, "⚠");
                    ui.colored_label(
                        crate::theme::colors::ORANGE_WARNING,
                        format!("全域快捷鍵不可用：{error}"),
                    );
                });
            }
            if let Some(stats) = &self.live_audio_stats {
                let dropped = stats.dropped_chunks();
                let capture_dropped = stats.dropped_capture_samples();
                if dropped > 0 {
                    ui.horizontal(|ui| {
                        ui.colored_label(crate::theme::colors::YELLOW_CAUTION, "⚡");
                        ui.colored_label(
                            crate::theme::colors::YELLOW_CAUTION,
                            format!("Realtime 音訊背壓：已丟棄 {dropped} 個 callback chunks"),
                        );
                    });
                }
                if capture_dropped > 0 {
                    ui.horizontal(|ui| {
                        ui.colored_label(crate::theme::colors::YELLOW_CAUTION, "⚡");
                        ui.colored_label(
                            crate::theme::colors::YELLOW_CAUTION,
                            format!("錄音保留 ring 已覆寫／略過 {capture_dropped} 個 samples"),
                        );
                    });
                }
            }
            });
            ui.add_space(12.0);

            // Batch fallback warning
            if self.batch_fallback_audio.is_some() {
                crate::theme::card(ui, None, None, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(crate::theme::colors::ORANGE_WARNING, "⚠");
                    ui.colored_label(
                        crate::theme::colors::ORANGE_WARNING,
                        "Realtime 失敗後不會自動重傳。此段音訊可能包含已顯示的 partial；請確認後才改用 Batch。",
                    );
                });
                if crate::theme::primary_button(ui, "確認改用 Batch 上傳此段音訊").clicked() {
                    self.confirm_batch_fallback();
                }
                });
                ui.add_space(12.0);
            }

            // MARK: - Latest transcript card with inline actions (AcmeUIKit pattern)
            crate::theme::card(
                ui,
                Some("最近辨識文字"),
                Some("點選「複製」或「再次注入」使用本次辨識結果。"),
                |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.last_text)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            ui.horizontal_wrapped(|ui| {
                if crate::theme::ghost_button(ui, "📋 複製文字").clicked() {
                    match copy_text(&self.last_text) {
                        Ok(()) => self.status = "已複製到剪貼簿".to_string(),
                        Err(error) => {
                            self.status = "複製失敗，文字仍保留".to_string();
                            self.last_error = Some(error.to_string());
                        }
                    }
                }
                if crate::theme::ghost_button(ui, "↩ 再次貼上").clicked() && !self.last_text.is_empty() {
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
            });
            });
            ui.add_space(12.0);

            // MARK: - Realtime partial (conditional)
            if !self.partial_text.is_empty() {
                crate::theme::card(ui, Some("Realtime partial 字幕（尚未寫入 history／注入）"), None, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.partial_text)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .interactive(false)
                        .font(egui::TextStyle::Monospace),
                );
                });
            }
            }); // ScrollArea
        });
        if self.realtime_worker.is_some()
            && realtime_settings_fingerprint(&self.config) != realtime_settings_before
        {
            self.cancel_realtime("Realtime session 已因 provider／模式／設定變更而停止");
        }
        // Smart repaint scheduling — only repaint when state requires it:
        // - Recording active: poll at 8 Hz for timer/duration display
        // Run history cleanup once per day (or on first update).
        let now = Instant::now();
        let should_cleanup = self
            .last_history_cleanup
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(86400));
        if should_cleanup {
            self.last_history_cleanup = Some(now);
            if self.config.history.retention_days > 0 {
                match history::cleanup_older_than(self.config.history.retention_days) {
                    Ok(count) if count > 0 => {
                        record_nonfatal_error(
                            &mut self.last_error,
                            &format!("已清理 {count} 筆過期歷史紀錄"),
                        );
                    }
                    Err(error) => {
                        record_nonfatal_error(
                            &mut self.last_error,
                            &format!("歷史紀錄清理失敗：{error}"),
                        );
                    }
                    _ => {}
                }
            }
        }

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
