use super::{
    nonempty_transcript, sanitized_error_body, ProviderRequest, ProviderResponse,
    SpeechToTextProvider,
};
use crate::error::{AppError, AppResult};
use reqwest::blocking::{multipart, Client};
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

impl SpeechToTextProvider for OpenAiProvider {
    fn transcribe(&self, request: ProviderRequest) -> AppResult<ProviderResponse> {
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
            .map_err(|e| AppError::Transcription(network_message(&e)))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|e| AppError::Transcription(e.to_string()))?;
        if !status.is_success() {
            return Err(AppError::Transcription(format!(
                "OpenAI HTTP {status}: {}",
                sanitized_error_body(&body, &self.api_key)
            )));
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

    #[test]
    fn sends_required_fields_and_accepts_response() {
        let (base_url, captured) = serve_once("200 OK", r#"{"text":"hello"}"#, Duration::ZERO);
        let provider = OpenAiProvider::new(
            base_url,
            "gpt-test".to_string(),
            "test-secret".to_string(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let response = provider.transcribe(request()).expect("transcription");
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

    #[test]
    fn rejects_empty_transcript() {
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
            .expect_err("empty transcript must fail")
            .to_string();

        assert!(error.contains("空白"), "unexpected error: {error}");
    }

    #[test]
    fn http_error_does_not_expose_api_key() {
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
            .expect_err("HTTP error expected")
            .to_string();

        assert!(error.contains("401"), "unexpected error: {error}");
        assert!(!error.contains(api_key));
    }

    #[test]
    fn timeout_is_classified() {
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
            .expect_err("timeout expected")
            .to_string();

        assert!(error.contains("逾時"), "unexpected error: {error}");
    }
}
