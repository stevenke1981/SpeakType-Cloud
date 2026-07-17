mod openai;
mod xai;

use crate::config::{AppConfig, ProviderKind};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub wav_bytes: Vec<u8>,
    pub language: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub text: String,
    pub duration_secs: Option<f32>,
    pub provider: String,
    pub model: Option<String>,
}

pub trait SpeechToTextProvider: Send + Sync {
    fn transcribe(&self, request: ProviderRequest) -> AppResult<ProviderResponse>;
}

pub fn build_provider(config: &AppConfig) -> AppResult<Box<dyn SpeechToTextProvider>> {
    let api_key_env = config.api_key_env();
    let api_key = std::env::var(api_key_env).map_err(|_| {
        AppError::Configuration(format!(
            "找不到環境變數 {}。請先設定 API Key，再重新啟動程式。",
            api_key_env
        ))
    })?;
    if api_key.trim().is_empty() {
        return Err(AppError::Configuration(format!(
            "環境變數 {api_key_env} 未包含 API Key。請設定後再重新啟動程式。"
        )));
    }
    let timeout = Duration::from_secs(150);
    match config.provider {
        ProviderKind::OpenAi => Ok(Box::new(openai::OpenAiProvider::new(
            config.openai.base_url.clone(),
            config.openai.model.clone(),
            api_key,
            timeout,
        )?)),
        ProviderKind::Xai => Ok(Box::new(xai::XaiProvider::new(
            config.xai.base_url.clone(),
            api_key,
            config.xai.format_text,
            config.xai.keyterms.clone(),
            timeout,
        )?)),
    }
}

fn nonempty_transcript(text: String) -> AppResult<String> {
    if text.trim().is_empty() {
        Err(AppError::Transcription("API 回傳空白辨識結果".to_string()))
    } else {
        Ok(text)
    }
}

fn sanitized_error_body(body: &str, api_key: &str) -> String {
    let redacted = if api_key.is_empty() {
        body.to_string()
    } else {
        body.replace(api_key, "[REDACTED]")
    };
    redacted.chars().take(800).collect()
}

#[cfg(test)]
pub(super) mod test_support {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

    pub fn serve_once(
        status: &str,
        response_body: &str,
        response_delay: Duration,
    ) -> (String, Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let status = status.to_string();
        let response_body = response_body.to_string();
        let (request_tx, request_rx) = mpsc::channel();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept test request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            let mut expected_len = None;
            loop {
                let count = stream.read(&mut chunk).expect("read test request");
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
                if expected_len.is_none() {
                    expected_len = total_request_len(&request);
                }
                if expected_len.is_some_and(|length| request.len() >= length) {
                    break;
                }
            }
            let _ = request_tx.send(String::from_utf8_lossy(&request).into_owned());
            std::thread::sleep(response_delay);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                response_body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });

        (format!("http://{address}"), request_rx)
    }

    fn total_request_len(request: &[u8]) -> Option<usize> {
        let header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")?
            + 4;
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })?;
        Some(header_end + content_length)
    }

    #[test]
    fn empty_api_key_environment_value_is_rejected() {
        let variable = "SPEAKTYPE_TEST_EMPTY_API_KEY";
        std::env::set_var(variable, "");
        let mut config = crate::config::AppConfig::default();
        config.openai.api_key_env = variable.to_string();

        let error = super::build_provider(&config)
            .err()
            .expect("empty key must fail")
            .to_string();
        std::env::remove_var(variable);

        assert!(error.contains(variable));
    }
}
