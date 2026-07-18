use crate::audio::RecordedAudio;
use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::providers::{build_provider, ProviderRequest, ProviderResponse, SpeechToTextProvider};
use chrono::Local;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_MAX_ATTEMPTS: usize = 3;
const DEFAULT_BACKOFF: Duration = Duration::from_millis(500);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobId(pub u64);

pub struct JobMessage {
    pub id: JobId,
    pub result: AppResult<JobResult>,
}

#[derive(Debug)]
pub struct JobResult {
    pub response: ProviderResponse,
    pub local_duration_secs: f32,
}

#[derive(Clone, Default)]
pub struct CancellationToken {
    inner: Arc<CancellationInner>,
}

#[derive(Default)]
struct CancellationInner {
    cancelled: AtomicBool,
    wake: tokio::sync::Notify,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::Release);
        self.inner.wake.notify_one();
    }

    fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        loop {
            let notified = self.inner.wake.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
            if self.is_cancelled() {
                return;
            }
        }
    }

    fn check(&self) -> AppResult<()> {
        if self.is_cancelled() {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RetryPolicy {
    max_attempts: usize,
    base_delay: Duration,
    max_delay: Duration,
}

impl RetryPolicy {
    fn new(max_attempts: usize, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            base_delay,
            max_delay,
        }
    }

    fn max_attempts(self) -> usize {
        self.max_attempts
    }

    fn delay_before_retry(self, retry_index: usize) -> Duration {
        let multiplier = 1_u32
            .checked_shl(retry_index.min(31) as u32)
            .unwrap_or(u32::MAX);
        self.base_delay
            .saturating_mul(multiplier)
            .min(self.max_delay)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ATTEMPTS, DEFAULT_BACKOFF, DEFAULT_MAX_BACKOFF)
    }
}

pub fn spawn(
    id: JobId,
    config: AppConfig,
    audio: RecordedAudio,
    tx: Sender<JobMessage>,
) -> CancellationToken {
    let cancellation = CancellationToken::new();
    let worker_cancellation = cancellation.clone();
    std::thread::spawn(move || {
        let result = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(run(config, audio, &worker_cancellation)),
            Err(error) => Err(AppError::Configuration(format!(
                "無法建立轉錄執行期：{error}"
            ))),
        };
        let _ = tx.send(JobMessage { id, result });
    });
    cancellation
}

async fn run(
    config: AppConfig,
    audio: RecordedAudio,
    cancellation: &CancellationToken,
) -> AppResult<JobResult> {
    cancellation.check()?;
    let local_duration_secs = audio.duration_secs();
    let wav = audio.wav_16khz_mono_i16()?;
    cancellation.check()?;
    if config.save_recordings {
        save_recording(&wav)?;
    }
    cancellation.check()?;
    let provider = build_provider(&config)?;
    let request = ProviderRequest {
        wav_bytes: wav,
        language: Some(config.language.clone()),
        prompt: Some(config.prompt.clone()),
    };
    let response = transcribe_with_retry(
        provider.as_ref(),
        request,
        cancellation,
        RetryPolicy::default(),
    )
    .await?;
    cancellation.check()?;
    if response.text.trim().is_empty() {
        return Err(AppError::Transcription("API 回傳空白辨識結果".to_string()));
    }
    Ok(JobResult {
        response,
        local_duration_secs,
    })
}

pub(crate) async fn transcribe_with_retry(
    provider: &dyn SpeechToTextProvider,
    request: ProviderRequest,
    cancellation: &CancellationToken,
    policy: RetryPolicy,
) -> AppResult<ProviderResponse> {
    for attempt in 0..policy.max_attempts() {
        cancellation.check()?;
        let attempt_result = tokio::select! {
            _ = cancellation.cancelled() => return Err(AppError::Cancelled),
            result = provider.transcribe(request.clone()) => result,
        };
        match attempt_result {
            Ok(response) => return Ok(response),
            Err(error) if error.is_retryable() && attempt + 1 < policy.max_attempts() => {
                tokio::select! {
                    _ = cancellation.cancelled() => return Err(AppError::Cancelled),
                    _ = tokio::time::sleep(policy.delay_before_retry(attempt)) => {}
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(AppError::Transcription("重試次數已達上限".to_string()))
}

fn save_recording(wav: &[u8]) -> AppResult<()> {
    let dir = paths::recordings_dir();
    fs::create_dir_all(&dir).map_err(|e| AppError::Io(e.to_string()))?;
    let path = dir.join(format!(
        "recording_{}.wav",
        Local::now().format("%Y%m%d_%H%M%S_%3f")
    ));
    fs::write(path, wav).map_err(|e| AppError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::SpeechToTextProvider;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    struct TestProvider {
        failures: usize,
        retryable: bool,
        calls: AtomicUsize,
    }

    struct SlowProvider {
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl SpeechToTextProvider for SlowProvider {
        async fn transcribe(&self, _request: ProviderRequest) -> AppResult<ProviderResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(5)).await;
            Err(AppError::provider("late transient failure", true))
        }
    }

    #[async_trait::async_trait]
    impl SpeechToTextProvider for TestProvider {
        async fn transcribe(&self, _request: ProviderRequest) -> AppResult<ProviderResponse> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call < self.failures {
                Err(AppError::provider("temporary failure", self.retryable))
            } else {
                Ok(ProviderResponse {
                    text: "done".to_string(),
                    duration_secs: None,
                    provider: "test".to_string(),
                    model: None,
                })
            }
        }
    }

    fn request() -> ProviderRequest {
        ProviderRequest {
            wav_bytes: b"wav".to_vec(),
            language: None,
            prompt: None,
        }
    }

    #[test]
    fn deterministic_backoff_is_bounded() {
        let policy = RetryPolicy::new(5, Duration::from_millis(100), Duration::from_millis(250));

        assert_eq!(policy.delay_before_retry(0), Duration::from_millis(100));
        assert_eq!(policy.delay_before_retry(1), Duration::from_millis(200));
        assert_eq!(policy.delay_before_retry(2), Duration::from_millis(250));
        assert_eq!(policy.max_attempts(), 5);
    }

    #[tokio::test]
    async fn only_retryable_errors_are_retried_with_a_bound() {
        let retryable = TestProvider {
            failures: 5,
            retryable: true,
            calls: AtomicUsize::new(0),
        };
        let policy = RetryPolicy::new(3, Duration::ZERO, Duration::ZERO);
        let token = CancellationToken::new();

        let _ = transcribe_with_retry(&retryable, request(), &token, policy)
            .await
            .expect_err("bounded retry must fail");
        assert_eq!(retryable.calls.load(Ordering::SeqCst), 3);

        let permanent = TestProvider {
            failures: 1,
            retryable: false,
            calls: AtomicUsize::new(0),
        };
        let _ = transcribe_with_retry(&permanent, request(), &token, policy)
            .await
            .expect_err("permanent failure must fail");
        assert_eq!(permanent.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancellation_interrupts_backoff_wait() {
        let token = CancellationToken::new();
        let started = Instant::now();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancel.cancel();
        });

        tokio::time::timeout(Duration::from_secs(1), token.cancelled())
            .await
            .expect("wait must be cancelled");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn cancellation_drops_inflight_upload_without_retrying() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        runtime.block_on(async {
            let provider = SlowProvider {
                calls: AtomicUsize::new(0),
            };
            let token = CancellationToken::new();
            let cancel = token.clone();
            let started = Instant::now();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                cancel.cancel();
            });

            let result = tokio::time::timeout(
                Duration::from_secs(1),
                transcribe_with_retry(&provider, request(), &token, RetryPolicy::default()),
            )
            .await
            .expect("cancellation must be prompt")
            .expect_err("cancelled upload must fail");

            assert!(matches!(result, AppError::Cancelled));
            assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
            assert!(started.elapsed() < Duration::from_secs(1));
        });
    }
}
