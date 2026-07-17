use super::{
    nonempty_transcript, sanitized_error_body, ProviderRequest, ProviderResponse,
    SpeechToTextProvider,
};
use crate::error::{AppError, AppResult};
use reqwest::blocking::{multipart, Client};
use serde::Deserialize;
use std::time::Duration;

pub struct XaiProvider {
    endpoint: String,
    api_key: String,
    format_text: bool,
    keyterms: Vec<String>,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct XaiResponse {
    text: String,
    duration: Option<f32>,
}

impl XaiProvider {
    pub fn new(
        base_url: String,
        api_key: String,
        format_text: bool,
        keyterms: Vec<String>,
        timeout: Duration,
    ) -> AppResult<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(timeout)
            .build()
            .map_err(|e| AppError::Configuration(e.to_string()))?;
        Ok(Self {
            endpoint: format!("{}/v1/stt", base_url.trim_end_matches('/')),
            api_key,
            format_text,
            keyterms,
            client,
        })
    }
}

impl SpeechToTextProvider for XaiProvider {
    fn transcribe(&self, request: ProviderRequest) -> AppResult<ProviderResponse> {
        // xAI 要求 file 欄位置於其他 multipart 欄位之後。
        let language = request.language.filter(|v| supports_xai_formatting(v));
        let mut form = multipart::Form::new();
        if self.format_text && language.is_some() {
            form = form.text("format", "true");
        }
        if let Some(language) = language {
            form = form.text("language", language);
        }
        for keyterm in self
            .keyterms
            .iter()
            .filter(|v| !v.trim().is_empty())
            .take(100)
        {
            form = form.text("keyterm", keyterm.clone());
        }
        form = form.part(
            "file",
            multipart::Part::bytes(request.wav_bytes)
                .file_name("speech.wav")
                .mime_str("audio/wav")
                .map_err(|e| AppError::Transcription(e.to_string()))?,
        );

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
                "xAI HTTP {status}: {}",
                sanitized_error_body(&body, &self.api_key)
            )));
        }
        let parsed: XaiResponse = serde_json::from_str(&body)
            .map_err(|e| AppError::Transcription(format!("xAI 回應格式錯誤：{e}")))?;
        Ok(ProviderResponse {
            text: nonempty_transcript(parsed.text)?,
            duration_secs: parsed.duration,
            provider: "xai".to_string(),
            model: None,
        })
    }
}

fn supports_xai_formatting(language: &str) -> bool {
    matches!(
        language.to_ascii_lowercase().as_str(),
        "ar" | "cs"
            | "da"
            | "de"
            | "en"
            | "es"
            | "fa"
            | "fil"
            | "fr"
            | "hi"
            | "id"
            | "it"
            | "ja"
            | "ko"
            | "mk"
            | "ms"
            | "nl"
            | "pl"
            | "pt"
            | "ro"
            | "ru"
            | "sv"
            | "th"
            | "tr"
            | "vi"
    )
}

fn network_message(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "xAI 請求逾時".to_string()
    } else if error.is_connect() {
        "無法連線 xAI API".to_string()
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::test_support::serve_once;

    fn request(language: &str) -> ProviderRequest {
        ProviderRequest {
            wav_bytes: b"test wav".to_vec(),
            language: Some(language.to_string()),
            prompt: None,
        }
    }

    #[test]
    fn chinese_is_sent_without_formatting_language() {
        assert!(!supports_xai_formatting("zh"));
        assert!(supports_xai_formatting("ja"));
    }

    #[test]
    fn multipart_file_field_is_last() {
        let (base_url, captured) = serve_once(
            "200 OK",
            r#"{"text":"hello","duration":1.25}"#,
            Duration::ZERO,
        );
        let provider = XaiProvider::new(
            base_url,
            "test-secret".to_string(),
            true,
            vec!["alpha".to_string(), "beta".to_string()],
            Duration::from_secs(2),
        )
        .expect("provider");

        let response = provider.transcribe(request("en")).expect("transcription");
        let raw_request = captured.recv().expect("captured request");
        let format_pos = raw_request.find(r#"name="format""#).expect("format field");
        let language_pos = raw_request
            .find(r#"name="language""#)
            .expect("language field");
        let last_keyterm_pos = raw_request
            .rfind(r#"name="keyterm""#)
            .expect("keyterm field");
        let file_pos = raw_request
            .find(r#"name="file"; filename="speech.wav""#)
            .expect("file field");

        assert_eq!(response.text, "hello");
        assert!(format_pos < file_pos);
        assert!(language_pos < file_pos);
        assert!(last_keyterm_pos < file_pos);
        let last_field_pos = raw_request
            .rfind("\r\nContent-Disposition: form-data; name=\"")
            .expect("last field");
        assert_eq!(
            file_pos,
            last_field_pos + "\r\nContent-Disposition: form-data; ".len()
        );
    }

    #[test]
    fn rejects_empty_transcript() {
        let (base_url, _) = serve_once("200 OK", r#"{"text":"","duration":null}"#, Duration::ZERO);
        let provider = XaiProvider::new(
            base_url,
            "test-secret".to_string(),
            true,
            Vec::new(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let error = provider
            .transcribe(request("zh"))
            .expect_err("empty transcript must fail")
            .to_string();

        assert!(error.contains("空白"), "unexpected error: {error}");
    }

    #[test]
    fn http_error_does_not_expose_api_key() {
        let api_key = "provider-test-secret-error-redaction";
        let body = format!(r#"{{"error":"denied {api_key}"}}"#);
        let (base_url, _) = serve_once("429 Too Many Requests", &body, Duration::ZERO);
        let provider = XaiProvider::new(
            base_url,
            api_key.to_string(),
            true,
            Vec::new(),
            Duration::from_secs(2),
        )
        .expect("provider");

        let error = provider
            .transcribe(request("en"))
            .expect_err("HTTP error expected")
            .to_string();

        assert!(error.contains("429"), "unexpected error: {error}");
        assert!(!error.contains(api_key));
    }

    #[test]
    fn timeout_is_classified() {
        let (base_url, _) = serve_once(
            "200 OK",
            r#"{"text":"late","duration":null}"#,
            Duration::from_millis(200),
        );
        let provider = XaiProvider::new(
            base_url,
            "test-secret".to_string(),
            true,
            Vec::new(),
            Duration::from_millis(30),
        )
        .expect("provider");

        let error = provider
            .transcribe(request("en"))
            .expect_err("timeout expected")
            .to_string();

        assert!(error.contains("逾時"), "unexpected error: {error}");
    }
}
