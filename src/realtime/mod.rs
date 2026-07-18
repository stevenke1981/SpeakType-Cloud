pub mod openai;
pub mod xai;

use crate::error::{AppError, AppResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

pub(crate) const MAX_INBOUND_MESSAGE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RealtimeEvent {
    Created,
    Partial(String),
    Final(String),
    Done,
    Error(String),
    Cancelled,
}

#[derive(Debug)]
pub(crate) enum RealtimeCommand {
    Audio(Vec<i16>),
    Finalize,
}

#[derive(Clone, Default)]
pub(crate) struct SessionCancellation {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl SessionCancellation {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub(crate) async fn cancelled(&self) {
        loop {
            let notified = self.notify.notified();
            if self.cancelled.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}

pub struct RealtimeSession {
    commands: mpsc::Sender<RealtimeCommand>,
    events: mpsc::Receiver<RealtimeEvent>,
    max_audio_samples: usize,
    cancellation: SessionCancellation,
}

impl RealtimeSession {
    pub(crate) fn new(
        commands: mpsc::Sender<RealtimeCommand>,
        events: mpsc::Receiver<RealtimeEvent>,
        max_audio_samples: usize,
        cancellation: SessionCancellation,
    ) -> Self {
        Self {
            commands,
            events,
            max_audio_samples,
            cancellation,
        }
    }

    pub async fn send_audio(&self, samples: Vec<i16>) -> AppResult<()> {
        if samples.is_empty() || samples.len() > self.max_audio_samples {
            return Err(AppError::Audio(format!(
                "Realtime 音訊 frame 必須介於 1 與 {} samples",
                self.max_audio_samples
            )));
        }
        self.commands
            .send(RealtimeCommand::Audio(samples))
            .await
            .map_err(|_| AppError::Transcription("Realtime session 已關閉".to_string()))
    }

    pub async fn finalize(&self) -> AppResult<()> {
        self.commands
            .send(RealtimeCommand::Finalize)
            .await
            .map_err(|_| AppError::Transcription("Realtime session 已關閉".to_string()))
    }

    pub async fn cancel(&self) -> AppResult<()> {
        self.cancellation.cancel();
        Ok(())
    }

    #[cfg(test)]
    pub async fn next_event(&mut self) -> Option<RealtimeEvent> {
        self.events.recv().await
    }

    pub fn try_next_event(&mut self) -> AppResult<Option<RealtimeEvent>> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(AppError::Transcription(
                "Realtime event channel 已關閉".to_string(),
            )),
        }
    }

    #[cfg(test)]
    fn new_with_cancellation_for_test(
        commands: mpsc::Sender<RealtimeCommand>,
        events: mpsc::Receiver<RealtimeEvent>,
        max_audio_samples: usize,
    ) -> Self {
        Self::new(
            commands,
            events,
            max_audio_samples,
            SessionCancellation::default(),
        )
    }
}

impl Drop for RealtimeSession {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

pub(crate) fn websocket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_INBOUND_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_INBOUND_MESSAGE_BYTES))
}

pub(crate) fn sanitized_protocol_error(message: impl Into<String>, api_key: &str) -> String {
    let message = message.into();
    if api_key.is_empty() {
        message
    } else {
        message.replace(api_key, "[REDACTED]")
    }
}

pub(crate) fn validate_ws_endpoint(endpoint: &str) -> AppResult<()> {
    let parsed = reqwest::Url::parse(endpoint)
        .map_err(|error| AppError::Configuration(format!("Realtime URL 無效：{error}")))?;
    let secure = parsed.scheme() == "wss";
    let local_test = parsed.scheme() == "ws"
        && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if !secure && !local_test {
        return Err(AppError::Configuration(
            "Realtime 正式端點必須使用 wss://".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_is_bounded_even_when_command_channel_is_full() {
        let (commands, _receiver) = mpsc::channel(1);
        commands
            .send(RealtimeCommand::Audio(vec![1]))
            .await
            .expect("fill command channel");
        let (_events_tx, events) = mpsc::channel(1);
        let session = RealtimeSession::new_with_cancellation_for_test(commands, events, 10);
        tokio::time::timeout(std::time::Duration::from_millis(100), session.cancel())
            .await
            .expect("cancel must not block")
            .expect("cancel");
    }
}
