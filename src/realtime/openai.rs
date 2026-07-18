use super::{
    sanitized_protocol_error, validate_ws_endpoint, websocket_config, RealtimeCommand,
    RealtimeEvent, RealtimeSession, SessionCancellation,
};
use crate::config::OpenAiTranscriptionDelay;
use crate::error::{AppError, AppResult};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

pub const OFFICIAL_ENDPOINT: &str = "wss://api.openai.com/v1/realtime";
pub const SAMPLE_RATE: u32 = 24_000;
const MAX_FRAME_SAMPLES: usize = 2_400;
const IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub async fn connect(
    endpoint: &str,
    api_key: String,
    model: String,
    language: String,
    delay: OpenAiTranscriptionDelay,
) -> AppResult<RealtimeSession> {
    validate_ws_endpoint(endpoint)?;
    let mut request = endpoint
        .into_client_request()
        .map_err(|error| AppError::Configuration(error.to_string()))?;
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|error| AppError::Configuration(error.to_string()))?,
    );
    let (socket, _) = connect_async_with_config(request, Some(websocket_config()), false)
        .await
        .map_err(|error| AppError::provider(error.to_string(), true))?;
    let (mut writer, mut reader) = socket.split();
    let update = json!({
        "type": "session.update",
        "session": {
            "type": "transcription",
            "audio": {
                "input": {
                    "format": { "type": "audio/pcm", "rate": SAMPLE_RATE },
                    "transcription": {
                        "model": model,
                        "language": language,
                        "delay": delay.as_api_str()
                    },
                    "turn_detection": null
                }
            }
        }
    });
    tokio::time::timeout(
        IO_TIMEOUT,
        writer.send(Message::Text(update.to_string().into())),
    )
    .await
    .map_err(|_| AppError::provider("OpenAI session.update 傳送逾時", true))?
    .map_err(|error| AppError::provider(error.to_string(), true))?;

    let (command_tx, mut command_rx) = mpsc::channel(16);
    let (event_tx, event_rx) = mpsc::channel(64);
    let cancellation = SessionCancellation::default();
    let actor_cancellation = cancellation.clone();
    tokio::spawn(async move {
        let mut parser = OpenAiEventParser::default();
        loop {
            tokio::select! {
                biased;
                _ = actor_cancellation.cancelled() => {
                    let _ = tokio::time::timeout(IO_TIMEOUT, writer.send(Message::Close(None))).await;
                    let _ = event_tx.send(RealtimeEvent::Cancelled).await;
                    break;
                }
                command = command_rx.recv() => {
                    let Some(command) = command else { break };
                    let message = match command {
                        RealtimeCommand::Audio(samples) => {
                            let mut pcm = Vec::with_capacity(samples.len() * 2);
                            for sample in samples { pcm.extend_from_slice(&sample.to_le_bytes()); }
                            let audio = base64::engine::general_purpose::STANDARD.encode(pcm);
                            Message::Text(json!({
                                "type": "input_audio_buffer.append",
                                "audio": audio
                            }).to_string().into())
                        }
                        RealtimeCommand::Finalize => {
                            parser.register_commit();
                            Message::Text(
                                json!({"type":"input_audio_buffer.commit"}).to_string().into()
                            )
                        }
                    };
                    let result = tokio::select! {
                        _ = actor_cancellation.cancelled() => None,
                        result = tokio::time::timeout(IO_TIMEOUT, writer.send(message)) => Some(result),
                    };
                    let Some(result) = result else {
                        let _ = event_tx.send(RealtimeEvent::Cancelled).await;
                        break;
                    };
                    let result = result.map_err(|_| "OpenAI realtime 傳送逾時".to_string())
                        .and_then(|result| result.map_err(|error| error.to_string()));
                    if let Err(error) = result {
                        let message = sanitized_protocol_error(error, &api_key);
                        let _ = event_tx.send(RealtimeEvent::Error(message)).await;
                        break;
                    }
                }
                incoming = reader.next() => {
                    let Some(incoming) = incoming else { break };
                    match incoming {
                        Ok(Message::Text(text)) => {
                            for event in parser.parse(&text, &api_key) {
                                let _ = event_tx.send(event).await;
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(error) => {
                            let message = sanitized_protocol_error(error.to_string(), &api_key);
                            let _ = event_tx.send(RealtimeEvent::Error(message)).await;
                            break;
                        }
                    }
                }
            }
        }
    });
    Ok(RealtimeSession::new(
        command_tx,
        event_rx,
        MAX_FRAME_SAMPLES,
        cancellation,
    ))
}

#[derive(Default)]
struct CommitState {
    item_id: Option<String>,
    completed: Option<String>,
}

#[derive(Default)]
struct OpenAiEventParser {
    commits: VecDeque<CommitState>,
    deltas: HashMap<String, String>,
    completed_before_commit: HashMap<String, String>,
}

impl OpenAiEventParser {
    fn register_commit(&mut self) {
        self.commits.push_back(CommitState::default());
    }

    fn parse(&mut self, text: &str, api_key: &str) -> Vec<RealtimeEvent> {
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(error) => {
                return vec![RealtimeEvent::Error(sanitized_protocol_error(
                    format!("OpenAI realtime JSON 錯誤：{error}"),
                    api_key,
                ))]
            }
        };
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match event_type {
            "session.updated" => vec![RealtimeEvent::Created],
            "input_audio_buffer.committed" => {
                let Some(item_id) = value.get("item_id").and_then(Value::as_str) else {
                    return Vec::new();
                };
                if let Some(commit) = self
                    .commits
                    .iter_mut()
                    .find(|commit| commit.item_id.is_none())
                {
                    commit.item_id = Some(item_id.to_string());
                    commit.completed = self.completed_before_commit.remove(item_id);
                }
                self.drain_completed()
            }
            "conversation.item.input_audio_transcription.delta" => {
                let Some(item_id) = value.get("item_id").and_then(Value::as_str) else {
                    return Vec::new();
                };
                let Some(delta) = value.get("delta").and_then(Value::as_str) else {
                    return Vec::new();
                };
                let accumulated = self.deltas.entry(item_id.to_string()).or_default();
                accumulated.push_str(delta);
                vec![RealtimeEvent::Partial(accumulated.clone())]
            }
            "conversation.item.input_audio_transcription.completed" => {
                let Some(item_id) = value.get("item_id").and_then(Value::as_str) else {
                    return Vec::new();
                };
                let accumulated = self.deltas.remove(item_id);
                let transcript = value
                    .get("transcript")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string)
                    .or(accumulated)
                    .unwrap_or_default();
                if let Some(commit) = self
                    .commits
                    .iter_mut()
                    .find(|commit| commit.item_id.as_deref() == Some(item_id))
                {
                    commit.completed = Some(transcript);
                } else {
                    self.completed_before_commit
                        .insert(item_id.to_string(), transcript);
                }
                self.drain_completed()
            }
            "error" => vec![RealtimeEvent::Error(sanitized_protocol_error(
                value.to_string(),
                api_key,
            ))],
            _ => Vec::new(),
        }
    }

    fn drain_completed(&mut self) -> Vec<RealtimeEvent> {
        let mut events = Vec::new();
        while self
            .commits
            .front()
            .is_some_and(|commit| commit.completed.is_some())
        {
            let commit = self.commits.pop_front().expect("front was present");
            events.push(RealtimeEvent::Final(
                commit.completed.expect("completion was present"),
            ));
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realtime::MAX_INBOUND_MESSAGE_BYTES;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_hdr_async;

    #[tokio::test]
    #[allow(clippy::result_large_err)] // tungstenite handshake callback owns its Response error.
    async fn mock_websocket_enforces_handshake_order_events_cancel_and_limits() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let endpoint = format!(
            "ws://{}/v1/realtime",
            listener.local_addr().expect("address")
        );
        let authorization = Arc::new(Mutex::new(String::new()));
        let captured_auth = Arc::clone(&authorization);
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_hdr_async(
                stream,
                move |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                      response| {
                    *captured_auth.lock().expect("auth") = request
                        .headers()
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_string();
                    Ok(response)
                },
            )
            .await
            .expect("websocket");
            let update = socket.next().await.expect("update").expect("update frame");
            socket
                .send(Message::Text(
                    json!({"type":"session.updated"}).to_string().into(),
                ))
                .await
                .expect("session updated");
            let audio = socket.next().await.expect("audio").expect("audio frame");
            let commit = socket.next().await.expect("commit").expect("commit frame");
            socket
                .send(Message::Text(
                    json!({"type":"input_audio_buffer.committed","item_id":"item-1"})
                        .to_string()
                        .into(),
                ))
                .await
                .expect("committed");
            socket.send(Message::Text(json!({"type":"conversation.item.input_audio_transcription.delta","item_id":"item-1","delta":"hel"}).to_string().into())).await.expect("delta");
            socket.send(Message::Text(json!({"type":"conversation.item.input_audio_transcription.delta","item_id":"item-1","delta":"lo"}).to_string().into())).await.expect("delta");
            socket.send(Message::Text(json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"item-1","transcript":"hello"}).to_string().into())).await.expect("complete");
            let close = socket.next().await.expect("close").expect("close frame");
            (update, audio, commit, close)
        });

        let mut session = connect(
            &endpoint,
            "openai-test-secret".to_string(),
            "gpt-realtime-whisper".to_string(),
            "zh".to_string(),
            OpenAiTranscriptionDelay::Medium,
        )
        .await
        .expect("session");
        assert_eq!(session.next_event().await, Some(RealtimeEvent::Created));
        assert!(session.send_audio(vec![0; 2_401]).await.is_err());
        session.send_audio(vec![7; 2_400]).await.expect("audio");
        session.finalize().await.expect("commit");
        assert_eq!(
            session.next_event().await,
            Some(RealtimeEvent::Partial("hel".to_string()))
        );
        assert_eq!(
            session.next_event().await,
            Some(RealtimeEvent::Partial("hello".to_string()))
        );
        assert_eq!(
            session.next_event().await,
            Some(RealtimeEvent::Final("hello".to_string()))
        );
        session.cancel().await.expect("cancel");
        let (update, audio, commit, close) = server.await.expect("server");
        assert_eq!(
            authorization.lock().expect("auth").as_str(),
            "Bearer openai-test-secret"
        );
        let update: Value = serde_json::from_str(&update.into_text().expect("update text"))
            .expect("session.update JSON");
        assert_eq!(
            update,
            json!({
                "type": "session.update",
                "session": {
                    "type": "transcription",
                    "audio": { "input": {
                        "format": { "type": "audio/pcm", "rate": 24000 },
                        "transcription": {
                            "model": "gpt-realtime-whisper",
                            "language": "zh",
                            "delay": "medium"
                        },
                        "turn_detection": null
                    }}
                }
            })
        );
        assert!(audio
            .into_text()
            .expect("audio text")
            .contains("input_audio_buffer.append"));
        assert!(commit
            .into_text()
            .expect("commit text")
            .contains("input_audio_buffer.commit"));
        assert!(matches!(close, Message::Close(_)));
    }

    #[tokio::test]
    async fn protocol_error_never_exposes_authorization_secret() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let endpoint = format!(
            "ws://{}/v1/realtime",
            listener.local_addr().expect("address")
        );
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket");
            let _ = socket.next().await;
            socket
                .send(Message::Text(
                    json!({"type":"error","message":"denied openai-secret-redact"})
                        .to_string()
                        .into(),
                ))
                .await
                .expect("error");
        });
        let mut session = connect(
            &endpoint,
            "openai-secret-redact".to_string(),
            "gpt-realtime-whisper".to_string(),
            "zh".to_string(),
            OpenAiTranscriptionDelay::Medium,
        )
        .await
        .expect("session");
        let event = session.next_event().await.expect("error event");
        let RealtimeEvent::Error(message) = event else {
            panic!("expected error")
        };
        assert!(!message.contains("openai-secret-redact"));
        server.await.expect("server");
    }

    #[tokio::test]
    async fn oversized_inbound_message_is_rejected() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let endpoint = format!(
            "ws://{}/v1/realtime",
            listener.local_addr().expect("address")
        );
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket");
            let _ = socket.next().await;
            socket
                .send(Message::Text(
                    json!({"type":"session.updated"}).to_string().into(),
                ))
                .await
                .expect("updated");
            socket
                .send(Message::Text(
                    "x".repeat(MAX_INBOUND_MESSAGE_BYTES + 1).into(),
                ))
                .await
                .expect("oversized");
        });
        let mut session = connect(
            &endpoint,
            "secret".to_string(),
            "gpt-realtime-whisper".to_string(),
            "zh".to_string(),
            OpenAiTranscriptionDelay::Medium,
        )
        .await
        .expect("session");
        assert_eq!(session.next_event().await, Some(RealtimeEvent::Created));
        assert!(matches!(
            session.next_event().await,
            Some(RealtimeEvent::Error(_))
        ));
        server.await.expect("server");
    }

    #[test]
    fn completed_items_emit_in_commit_order_even_when_server_completes_in_reverse() {
        let mut parser = OpenAiEventParser::default();
        parser.register_commit();
        parser.register_commit();
        assert!(parser
            .parse(
                r#"{"type":"input_audio_buffer.committed","item_id":"item-1"}"#,
                "secret"
            )
            .is_empty());
        assert!(parser
            .parse(
                r#"{"type":"input_audio_buffer.committed","item_id":"item-2"}"#,
                "secret"
            )
            .is_empty());
        assert_eq!(
            parser.parse(r#"{"type":"conversation.item.input_audio_transcription.delta","item_id":"item-2","delta":"second"}"#, "secret"),
            vec![RealtimeEvent::Partial("second".to_string())]
        );
        assert!(parser.parse(r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"item-2","transcript":"second"}"#, "secret").is_empty());
        assert_eq!(
            parser.parse(r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"item-1","transcript":"first"}"#, "secret"),
            vec![
                RealtimeEvent::Final("first".to_string()),
                RealtimeEvent::Final("second".to_string()),
            ]
        );
    }

    #[test]
    fn completed_item_removes_accumulated_delta_even_with_server_transcript() {
        let mut parser = OpenAiEventParser::default();
        parser.register_commit();
        parser.parse(
            r#"{"type":"input_audio_buffer.committed","item_id":"item-1"}"#,
            "secret",
        );
        parser.parse(r#"{"type":"conversation.item.input_audio_transcription.delta","item_id":"item-1","delta":"draft"}"#, "secret");
        assert_eq!(parser.deltas.len(), 1);
        assert_eq!(
            parser.parse(r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"item-1","transcript":"final"}"#, "secret"),
            vec![RealtimeEvent::Final("final".to_string())]
        );
        assert!(parser.deltas.is_empty());
    }
}
