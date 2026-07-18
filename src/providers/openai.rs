use super::{
    nonempty_transcript, retryable_http_status, retryable_transport_error, sanitized_error_body,
    ProviderRequest, ProviderResponse, SpeechToTextProvider,
};
use crate::error::{AppError, AppResult};
use reqwest::{multipart, Client};
use serde::Deserialize;
use std::time::Duration;

pub struct OpenAiProvider {
    endpoint: String,
    model: String,
    api_key: String,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    text: String,
}

impl OpenAiProvider {
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
impl SpeechToTextProvider for OpenAiProvider {
    async fn transcribe(&self, request: ProviderRequest) -> AppResult<ProviderResponse> {
        let mut form = multipart::Form::new()
            .text("model", self.model.clone())
            .text("response_format", "json")
            .part(
                "file",
                multipart::Part::bytes(request.wav_bytes)
                    .file_name("speech.wav")
                    .mime_str("audio/wav")
                    .map_err(|e| AppError::Transcription(e.to_string()))?,
            );
        if let Some(language) = request.language.filter(|v| !v.trim().is_empty()) {
            form = form.text("language", language);
        }
        if let Some(prompt) = request.prompt.filter(|v| !v.trim().is_empty()) {
            form = form.text("prompt", prompt);
        }

        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|error| {
                let retryable = retryable_transport_error(&error);
                AppError::provider(network_message(&error), retryable)
            })?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| AppError::Transcription(e.to_string()))?;
        if !status.is_success() {
            return Err(AppError::provider(
                format!(
                    "OpenAI HTTP {status}: {}",
                    sanitized_error_body(&body, &self.api_key)
                ),
                retryable_http_status(status),
            ));
        }
        let parsed: OpenAiResponse = serde_json::from_str(&body)
            .map_err(|e| AppError::Transcription(format!("OpenAI 回應格式錯誤：{e}")))?;
        Ok(ProviderResponse {
            text: nonempty_transcript(parsed.text)?,
            duration_secs: None,
            provider: "openai".to_string(),
            model: Some(self.model.clone()),
        })
    }
}

fn network_message(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "OpenAI 請求逾時".to_string()
    } else if error.is_connect() {
        "無法連線 OpenAI API".to_string()
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
            wav_bytes: b"test wav".to_vec(),
            language: Some("en".to_string()),
            prompt: Some("names".to_string()),
        }
    }

    #[tokio::test]
    async fn sends_required_fields_and_accepts_response() {
        let (base_url, captured) = serve_once("200 OK", r#"{"text":"hello"}"#, Duration::ZERO);
        let provider = OpenAiProvider::new(
            base_url,
            "gpt-test".to_string(),
            "test-secret".to_string(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let response = provider.transcribe(request()).await.expect("transcription");
        let raw_request = captured.recv().expect("captured request");

        assert_eq!(response.text, "hello");
        assert!(raw_request.starts_with("POST /v1/audio/transcriptions "));
        assert!(raw_request.contains(r#"name="model""#));
        assert!(raw_request.contains("gpt-test"));
        assert!(raw_request.contains(r#"name="file"; filename="speech.wav""#));
        assert!(raw_request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-secret"));
    }

    #[tokio::test]
    async fn rejects_empty_transcript() {
        let (base_url, _) = serve_once("200 OK", r#"{"text":"  "}"#, Duration::ZERO);
        let provider = OpenAiProvider::new(
            base_url,
            "gpt-test".to_string(),
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
        let api_key = "test-secret-that-must-not-leak";
        let body = format!(r#"{{"error":"denied {api_key}"}}"#);
        let (base_url, _) = serve_once("401 Unauthorized", &body, Duration::ZERO);
        let provider = OpenAiProvider::new(
            base_url,
            "gpt-test".to_string(),
            api_key.to_string(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let error = provider
            .transcribe(request())
            .await
            .expect_err("HTTP error expected");
        assert!(!error.is_retryable());
        let error = error.to_string();

        assert!(error.contains("401"), "unexpected error: {error}");
        assert!(!error.contains(api_key));
    }

    #[tokio::test]
    async fn timeout_is_classified() {
        let (base_url, _) = serve_once("200 OK", r#"{"text":"late"}"#, Duration::from_millis(200));
        let provider = OpenAiProvider::new(
            base_url,
            "gpt-test".to_string(),
            "test-secret".to_string(),
            Duration::from_millis(30),
        )
        .expect("provider");

        let error = provider
            .transcribe(request())
            .await
            .expect_err("timeout expected");
        assert!(error.is_retryable());
        let error = error.to_string();

        assert!(error.contains("逾時"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn slow_http_upload_is_promptly_cancelled_without_retry() {
        use crate::transcription::{transcribe_with_retry, CancellationToken, RetryPolicy};
        use std::time::Instant;

        let (base_url, captured) = serve_once(
            "503 Service Unavailable",
            r#"{"error":"late"}"#,
            Duration::from_secs(5),
        );
        let provider = OpenAiProvider::new(
            base_url,
            "gpt-test".to_string(),
            "test-secret".to_string(),
            Duration::from_secs(10),
        )
        .expect("provider");
        let token = CancellationToken::new();
        let cancel = token.clone();
        let canceller = std::thread::spawn(move || {
            captured
                .recv_timeout(Duration::from_secs(1))
                .expect("slow server received exactly one upload");
            cancel.cancel();
        });
        let started = Instant::now();

        let error = tokio::time::timeout(
            Duration::from_secs(1),
            transcribe_with_retry(&provider, request(), &token, RetryPolicy::default()),
        )
        .await
        .expect("HTTP cancellation must be prompt")
        .expect_err("cancelled upload must fail");
        canceller.join().expect("canceller thread");

        assert!(matches!(error, AppError::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
