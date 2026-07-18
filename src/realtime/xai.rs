use super::{
    sanitized_protocol_error, validate_ws_endpoint, websocket_config, RealtimeCommand,
    RealtimeEvent, RealtimeSession, SessionCancellation,
};
use crate::error::{AppError, AppResult};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

pub const OFFICIAL_ENDPOINT: &str = "wss://api.x.ai/v1/stt";
pub const SAMPLE_RATE: u32 = 16_000;
const MAX_FRAME_SAMPLES: usize = 1_600;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn connect(
    endpoint: &str,
    api_key: String,
    smart_turn: bool,
    smart_turn_threshold: f32,
    smart_turn_timeout_ms: u64,
) -> AppResult<RealtimeSession> {
    validate_ws_endpoint(endpoint)?;
    let mut url = reqwest::Url::parse(endpoint)
        .map_err(|error| AppError::Configuration(error.to_string()))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("sample_rate", "16000")
            .append_pair("encoding", "pcm")
            .append_pair("interim_results", "true");
        if smart_turn {
            query
                .append_pair("smart_turn", &smart_turn_threshold.to_string())
                .append_pair("smart_turn_timeout", &smart_turn_timeout_ms.to_string());
        }
    }
    let mut request = url
        .as_str()
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

    let created = tokio::time::timeout(Duration::from_secs(5), reader.next())
        .await
        .map_err(|_| AppError::provider("xAI transcript.created 等待逾時", true))?
        .ok_or_else(|| AppError::provider("xAI realtime 在建立 transcript 前關閉", true))?
        .map_err(|error| AppError::provider(error.to_string(), true))?;
    let Message::Text(created) = created else {
        return Err(AppError::Transcription(
            "xAI 必須先傳回 transcript.created".to_string(),
        ));
    };
    let created_value: Value = serde_json::from_str(&created)
        .map_err(|error| AppError::Transcription(format!("xAI realtime JSON 錯誤：{error}")))?;
    if created_value.get("type").and_then(Value::as_str) != Some("transcript.created") {
        return Err(AppError::Transcription(
            "xAI 必須先傳回 transcript.created".to_string(),
        ));
    }

    let (command_tx, mut command_rx) = mpsc::channel(16);
    let (event_tx, event_rx) = mpsc::channel(64);
    let _ = event_tx.send(RealtimeEvent::Created).await;
    let cancellation = SessionCancellation::default();
    let actor_cancellation = cancellation.clone();
    tokio::spawn(async move {
        let mut parser = XaiEventParser::new(smart_turn);
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
                            Message::Binary(pcm.into())
                        }
                        RealtimeCommand::Finalize => Message::Text(
                            json!({"type":"Finalize"}).to_string().into()
                        ),
                    };
                    let result = tokio::select! {
                        _ = actor_cancellation.cancelled() => None,
                        result = tokio::time::timeout(IO_TIMEOUT, writer.send(message)) => Some(result),
                    };
                    let Some(result) = result else {
                        let _ = event_tx.send(RealtimeEvent::Cancelled).await;
                        break;
                    };
                    let result = result.map_err(|_| "xAI realtime 傳送逾時".to_string())
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
                            if let Some(event) = parser.parse(&text, &api_key) {
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

struct XaiEventParser {
    locked: String,
}

impl XaiEventParser {
    fn new(_smart_turn: bool) -> Self {
        Self {
            locked: String::new(),
        }
    }

    #[cfg(test)]
    fn locked_text(&self) -> &str {
        &self.locked
    }

    fn parse(&mut self, text: &str, api_key: &str) -> Option<RealtimeEvent> {
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(error) => {
                return Some(RealtimeEvent::Error(sanitized_protocol_error(
                    format!("xAI realtime JSON 錯誤：{error}"),
                    api_key,
                )))
            }
        };
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if event_type == "audio.done" {
            return Some(RealtimeEvent::Done);
        }
        if event_type == "error" {
            return Some(RealtimeEvent::Error(sanitized_protocol_error(
                value.to_string(),
                api_key,
            )));
        }
        let text = value.get("text").and_then(Value::as_str)?;
        let is_final = value
            .get("is_final")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let speech_final = value
            .get("speech_final")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let combined = reconcile_locked(&self.locked, text);
        if speech_final {
            self.locked.clear();
            Some(RealtimeEvent::Final(combined))
        } else if is_final {
            self.locked = combined.clone();
            Some(RealtimeEvent::Partial(combined))
        } else {
            Some(RealtimeEvent::Partial(combined))
        }
    }
}

fn reconcile_locked(locked: &str, text: &str) -> String {
    if locked.is_empty() || text.starts_with(locked) {
        text.to_string()
    } else {
        format!("{locked}{text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_hdr_async;

    #[tokio::test]
    #[allow(clippy::result_large_err)] // tungstenite handshake callback owns its Response error.
    async fn mock_websocket_waits_created_and_uses_raw_pcm_finalize_and_done() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let endpoint = format!("ws://{}/v1/stt", listener.local_addr().expect("address"));
        let request_line = Arc::new(Mutex::new(String::new()));
        let captured = Arc::clone(&request_line);
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_hdr_async(
                stream,
                move |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                      response| {
                    let auth = request
                        .headers()
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default();
                    *captured.lock().expect("request") = format!("{} {auth}", request.uri());
                    Ok(response)
                },
            )
            .await
            .expect("websocket");
            socket
                .send(Message::Text(
                    json!({"type":"transcript.created"}).to_string().into(),
                ))
                .await
                .expect("created");
            let audio = socket.next().await.expect("audio").expect("audio frame");
            let finalize = socket
                .next()
                .await
                .expect("finalize")
                .expect("finalize frame");
            socket.send(Message::Text(json!({"type":"transcript.partial","text":"你","is_final":false,"speech_final":false}).to_string().into())).await.expect("partial");
            socket.send(Message::Text(json!({"type":"transcript.partial","text":"你好","is_final":true,"speech_final":true}).to_string().into())).await.expect("final");
            socket
                .send(Message::Text(
                    json!({"type":"audio.done"}).to_string().into(),
                ))
                .await
                .expect("done");
            let close = socket.next().await.expect("close").expect("close frame");
            (audio, finalize, close)
        });

        let mut session = connect(&endpoint, "xai-test-secret".to_string(), true, 0.7, 3_000)
            .await
            .expect("session");
        assert_eq!(session.next_event().await, Some(RealtimeEvent::Created));
        assert!(session.send_audio(vec![0; 1_601]).await.is_err());
        session.send_audio(vec![1; 1_600]).await.expect("audio");
        session.finalize().await.expect("finalize");
        assert_eq!(
            session.next_event().await,
            Some(RealtimeEvent::Partial("你".to_string()))
        );
        assert_eq!(
            session.next_event().await,
            Some(RealtimeEvent::Final("你好".to_string()))
        );
        assert_eq!(session.next_event().await, Some(RealtimeEvent::Done));
        session.cancel().await.expect("cancel");
        let (audio, finalize, close) = server.await.expect("server");
        assert_eq!(audio.into_data().len(), 3_200);
        assert!(finalize
            .into_text()
            .expect("finalize text")
            .contains("Finalize"));
        assert!(matches!(close, Message::Close(_)));
        let request = request_line.lock().expect("request");
        assert_eq!(
            request.as_str(),
            "/v1/stt?sample_rate=16000&encoding=pcm&interim_results=true&smart_turn=0.7&smart_turn_timeout=3000 Bearer xai-test-secret"
        );
        assert!(!request.contains("turn_detection"));
        assert!(!request.contains("smart_turn_threshold"));
        assert!(!request.contains("smart_turn_timeout_ms"));
    }

    #[test]
    fn smart_turn_locks_chunks_until_speech_final_without_duplicate_final() {
        let mut parser = XaiEventParser::new(true);
        assert_eq!(
            parser.parse(
                r#"{"type":"transcript","text":"hello ","is_final":true,"speech_final":false}"#,
                "secret"
            ),
            Some(RealtimeEvent::Partial("hello ".to_string()))
        );
        assert_eq!(
            parser.parse(
                r#"{"type":"transcript","text":"world","is_final":false,"speech_final":false}"#,
                "secret"
            ),
            Some(RealtimeEvent::Partial("hello world".to_string()))
        );
        assert_eq!(
            parser.parse(
                r#"{"type":"transcript","text":"world","is_final":true,"speech_final":true}"#,
                "secret"
            ),
            Some(RealtimeEvent::Final("hello world".to_string()))
        );
        assert_eq!(parser.locked_text(), "");
    }

    #[test]
    fn non_smart_turn_also_locks_is_final_until_speech_final() {
        let mut parser = XaiEventParser::new(false);
        assert_eq!(
            parser.parse(
                r#"{"type":"transcript","text":"locked ","is_final":true,"speech_final":false}"#,
                "secret"
            ),
            Some(RealtimeEvent::Partial("locked ".to_string()))
        );
        assert_eq!(
            parser.parse(
                r#"{"type":"transcript","text":"utterance","is_final":true,"speech_final":true}"#,
                "secret"
            ),
            Some(RealtimeEvent::Final("locked utterance".to_string()))
        );
    }

    #[tokio::test]
    async fn refuses_audio_before_transcript_created() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let endpoint = format!("ws://{}/v1/stt", listener.local_addr().expect("address"));
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket");
            socket
                .send(Message::Text(
                    json!({"type":"transcript.partial","text":"too early"})
                        .to_string()
                        .into(),
                ))
                .await
                .expect("early");
        });
        let error = connect(&endpoint, "xai-secret".to_string(), false, 0.7, 3_000)
            .await
            .err()
            .expect("created ordering error")
            .to_string();
        assert!(error.contains("transcript.created"));
        assert!(!error.contains("xai-secret"));
        server.await.expect("server");
    }
}
