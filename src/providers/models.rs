use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

/// Response from OpenAI-compatible `/v1/models` endpoint.
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

/// Fetch available model IDs from a provider's `/v1/models` endpoint (30s timeout).
///
/// Works with OpenAI, xAI, and OpenRouter (all follow the same OpenAI-compatible
/// format).  Returns a sorted list of model IDs.
pub async fn fetch_available_models(base_url: &str, api_key: &str) -> AppResult<Vec<String>> {
    fetch_available_models_with_timeout(base_url, api_key, Duration::from_secs(30)).await
}

/// Like `fetch_available_models` but with a configurable request timeout.
/// Use the default wrapper above in production; pass a shorter timeout in tests.
pub async fn fetch_available_models_with_timeout(
    base_url: &str,
    api_key: &str,
    request_timeout: Duration,
) -> AppResult<Vec<String>> {
    let base = base_url.trim_end_matches('/');
    let url = format!("{base}/v1/models");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(request_timeout)
        .no_proxy()
        .build()
        .map_err(|e| AppError::Configuration(e.to_string()))?;

    let response = client
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                AppError::provider("取得模型列表請求逾時".to_string(), true)
            } else if e.is_connect() {
                AppError::provider("無法連線 API 以取得模型列表".to_string(), true)
            } else {
                AppError::provider(format!("取得模型列表失敗：{e}"), true)
            }
        })?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| AppError::Transcription(format!("讀取模型列表回應失敗：{e}")))?;

    if !status.is_success() {
        let redacted = if api_key.is_empty() {
            body.clone()
        } else {
            body.replace(api_key, "[REDACTED]")
        };
        return Err(AppError::provider(
            format!(
                "API HTTP {status}：{}",
                redacted.chars().take(400).collect::<String>()
            ),
            status.as_u16() >= 500,
        ));
    }

    let parsed: ModelsResponse = serde_json::from_str(&body)
        .map_err(|e| AppError::Transcription(format!("模型列表回應格式錯誤：{e}")))?;

    let mut models: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
    models.sort();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::test_support::serve_once;
    use std::time::Duration;

    #[tokio::test]
    async fn parses_openai_models_response() {
        let (base_url, _) = serve_once(
            "200 OK",
            r#"{"object":"list","data":[{"id":"gpt-4o-mini-transcribe","object":"model"},{"id":"whisper-1","object":"model"}]}"#,
            Duration::ZERO,
        );
        let models = fetch_available_models(&base_url, "test-key")
            .await
            .expect("fetch models");

        assert_eq!(models.len(), 2);
        assert_eq!(models[0], "gpt-4o-mini-transcribe");
        assert_eq!(models[1], "whisper-1");
    }

    #[tokio::test]
    async fn returns_sorted_list() {
        let (base_url, _) = serve_once(
            "200 OK",
            r#"{"object":"list","data":[{"id":"z-model","object":"model"},{"id":"a-model","object":"model"},{"id":"m-model","object":"model"}]}"#,
            Duration::ZERO,
        );
        let models = fetch_available_models(&base_url, "test-key")
            .await
            .expect("fetch models");

        assert_eq!(models, vec!["a-model", "m-model", "z-model"]);
    }

    #[tokio::test]
    async fn http_error_is_reported() {
        let (base_url, _) = serve_once(
            "401 Unauthorized",
            r#"{"error":"invalid credentials"}"#,
            Duration::ZERO,
        );
        let error = fetch_available_models(&base_url, "test-secret")
            .await
            .expect_err("HTTP error expected");

        assert!(error.to_string().contains("401"));
    }

    #[tokio::test]
    async fn api_key_is_redacted_in_error() {
        let api_key = "super-secret-key-not-to-leak";
        let (base_url, _) = serve_once(
            "403 Forbidden",
            &format!(r#"{{"error":"denied {api_key}"}}"#),
            Duration::ZERO,
        );
        let error = fetch_available_models(&base_url, api_key)
            .await
            .expect_err("HTTP error expected");

        let error_str = error.to_string();
        assert!(!error_str.contains(api_key));
        assert!(error_str.contains("403"));
    }

    #[tokio::test]
    async fn empty_response_is_ok() {
        let (base_url, _) = serve_once("200 OK", r#"{"object":"list","data":[]}"#, Duration::ZERO);
        let models = fetch_available_models(&base_url, "test-key")
            .await
            .expect("fetch models");

        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn timeout_is_classified_as_retryable() {
        let (base_url, _) = serve_once(
            "200 OK",
            r#"{"data":[{"id":"slow"}]}"#,
            Duration::from_millis(500),
        );
        let error =
            fetch_available_models_with_timeout(&base_url, "test-key", Duration::from_millis(50))
                .await
                .expect_err("timeout expected");

        assert!(error.is_retryable());
        assert!(error.to_string().contains("逾時"));
    }
}
