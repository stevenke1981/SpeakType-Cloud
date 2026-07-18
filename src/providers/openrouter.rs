use super::{
    nonempty_transcript, retryable_http_status, retryable_transport_error, sanitized_error_body,
    ProviderRequest, ProviderResponse, SpeechToTextProvider,
};
use crate::error::{AppError, AppResult};
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct OpenRouterProvider {
    endpoint: String,
    model: String,
    api_key: String,
    client: Client,
}

#[derive(Debug, Serialize)]
struct OpenRouterRequest {
    model: String,
    #[serde(rename = "input_audio")]
    input_audio: OpenRouterInputAudio,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenRouterInputAudio {
    data: String,
    format: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    text: String,
}

impl OpenRouterProvider {
    pub fn new(
        base_url: String,
        model: String,
        api_key: String,
        timeout: Duration,
    ) -> AppResult<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(timeout)
            .build()
            .map_err(|e| AppError::Configuration(e.to_string()))?;
        Ok(Self {
            endpoint: format!("{}/v1/audio/transcriptions", base_url.trim_end_matches('/')),
            model,
            api_key,
            client,
        })
    }
}

#[async_trait::async_trait]
impl SpeechToTextProvider for OpenRouterProvider {
    async fn transcribe(&self, request: ProviderRequest) -> AppResult<ProviderResponse> {
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&request.wav_bytes);

        let body = OpenRouterRequest {
            model: self.model.clone(),
            input_audio: OpenRouterInputAudio {
                data: audio_b64,
                format: "wav".to_string(),
            },
            language: request.language.filter(|v| !v.trim().is_empty()),
            prompt: request.prompt.filter(|v| !v.trim().is_empty()),
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                let retryable = retryable_transport_error(&error);
                AppError::provider(network_message(&error), retryable)
            })?;
        let status = response.status();
        let raw_body = response
            .text()
            .await
            .map_err(|e| AppError::Transcription(e.to_string()))?;
        if !status.is_success() {
            return Err(AppError::provider(
                format!(
                    "OpenRouter HTTP {status}: {}",
                    sanitized_error_body(&raw_body, &self.api_key)
                ),
                retryable_http_status(status),
            ));
        }
        let parsed: OpenRouterResponse = serde_json::from_str(&raw_body)
            .map_err(|e| AppError::Transcription(format!("OpenRouter 回應格式錯誤：{e}")))?;
        Ok(ProviderResponse {
            text: nonempty_transcript(parsed.text)?,
            duration_secs: None,
            provider: "openrouter".to_string(),
            model: Some(self.model.clone()),
        })
    }
}

fn network_message(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "OpenRouter 請求逾時".to_string()
    } else if error.is_connect() {
        "無法連線 OpenRouter API".to_string()
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::test_support::serve_once;

    fn request() -> ProviderRequest {
        ProviderRequest {
            wav_bytes: b"test wav content".to_vec(),
            language: Some("zh".to_string()),
            prompt: Some("使用繁體中文".to_string()),
        }
    }

    #[tokio::test]
    async fn sends_json_base64_and_receives_text() {
        let (base_url, captured) = serve_once("200 OK", r#"{"text":"你好世界"}"#, Duration::ZERO);
        let provider = OpenRouterProvider::new(
            base_url,
            "openai/gpt-4o-mini-transcribe".to_string(),
            "test-secret-or".to_string(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let response = provider.transcribe(request()).await.expect("transcription");
        let raw_request = captured.recv().expect("captured request");

        assert_eq!(response.text, "你好世界");
        assert_eq!(response.provider, "openrouter");
        assert_eq!(
            response.model.as_deref(),
            Some("openai/gpt-4o-mini-transcribe")
        );

        assert!(raw_request.starts_with("POST /v1/audio/transcriptions "));

        // Verify JSON body
        assert!(raw_request.contains(r#""model":"openai/gpt-4o-mini-transcribe""#));
        assert!(raw_request.contains(r#""input_audio""#));
        assert!(raw_request.contains(r#""format":"wav""#));
        assert!(raw_request.contains(r#""language":"zh""#));
        assert!(raw_request.contains(r#""prompt":"使用繁體中文""#));
        let expected_audio = base64::engine::general_purpose::STANDARD.encode(b"test wav content");
        assert!(raw_request.contains(&expected_audio));

        // Verify Authorization header
        assert!(raw_request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-secret-or"));

        // Verify Content-Type is JSON
        let lower = raw_request.to_ascii_lowercase();
        assert!(lower.contains("content-type: application/json"));
        // Should not contain multipart
        assert!(!lower.contains("multipart/form-data"));
    }

    #[tokio::test]
    async fn rejects_empty_transcript() {
        let (base_url, _) = serve_once("200 OK", r#"{"text":""}"#, Duration::ZERO);
        let provider = OpenRouterProvider::new(
            base_url,
            "openai/gpt-4o-mini-transcribe".to_string(),
            "test-secret".to_string(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let error = provider
            .transcribe(request())
            .await
            .expect_err("empty transcript must fail")
            .to_string();

        assert!(error.contains("空白"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn http_error_does_not_expose_api_key() {
        let api_key = "test-secret-or-redaction";
        let body = format!(r#"{{"error":"denied {api_key}"}}"#);
        let (base_url, _) = serve_once("401 Unauthorized", &body, Duration::ZERO);
        let provider = OpenRouterProvider::new(
            base_url,
            "openai/gpt-4o-mini-transcribe".to_string(),
            api_key.to_string(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let error = provider
            .transcribe(request())
            .await
            .expect_err("HTTP error expected");
        assert!(!error.is_retryable());
        let error_str = error.to_string();

        assert!(error_str.contains("401"), "unexpected error: {error_str}");
        assert!(!error_str.contains(api_key));
    }

    #[tokio::test]
    async fn http_429_is_retryable() {
        let api_key = "test-secret-or-429";
        let (base_url, _) = serve_once(
            "429 Too Many Requests",
            r#"{"error":"rate limited"}"#,
            Duration::ZERO,
        );
        let provider = OpenRouterProvider::new(
            base_url,
            "openai/gpt-4o-mini-transcribe".to_string(),
            api_key.to_string(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let error = provider
            .transcribe(request())
            .await
            .expect_err("429 expected");
        assert!(error.is_retryable());
        let error_str = error.to_string();
        assert!(!error_str.contains(api_key));
    }

    #[tokio::test]
    async fn timeout_is_classified() {
        let (base_url, _) = serve_once("200 OK", r#"{"text":"late"}"#, Duration::from_millis(200));
        let provider = OpenRouterProvider::new(
            base_url,
            "openai/gpt-4o-mini-transcribe".to_string(),
            "test-secret".to_string(),
            Duration::from_millis(30),
        )
        .expect("provider");

        let error = provider
            .transcribe(request())
            .await
            .expect_err("timeout expected");
        assert!(error.is_retryable());
        let error_str = error.to_string();
        assert!(error_str.contains("逾時"), "unexpected error: {error_str}");
    }

    #[tokio::test]
    async fn language_and_prompt_can_be_omitted() {
        let (base_url, captured) = serve_once("200 OK", r#"{"text":"hello"}"#, Duration::ZERO);
        let provider = OpenRouterProvider::new(
            base_url,
            "openai/gpt-4o-mini-transcribe".to_string(),
            "test-secret".to_string(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let req = ProviderRequest {
            wav_bytes: b"test".to_vec(),
            language: None,
            prompt: None,
        };
        let _ = provider.transcribe(req).await.expect("transcription");
        let raw_request = captured.recv().expect("captured request");

        assert!(!raw_request.contains(r#""language""#));
        assert!(!raw_request.contains(r#""prompt""#));
    }
}
