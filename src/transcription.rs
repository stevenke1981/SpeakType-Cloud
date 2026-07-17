use crate::audio::RecordedAudio;
use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::providers::{build_provider, ProviderRequest, ProviderResponse};
use chrono::Local;
use std::fs;
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub struct JobResult {
    pub response: ProviderResponse,
    pub local_duration_secs: f32,
}

pub fn spawn(config: AppConfig, audio: RecordedAudio, tx: Sender<AppResult<JobResult>>) {
    std::thread::spawn(move || {
        let result = run(config, audio);
        let _ = tx.send(result);
    });
}

fn run(config: AppConfig, audio: RecordedAudio) -> AppResult<JobResult> {
    let local_duration_secs = audio.duration_secs();
    let wav = audio.wav_16khz_mono_i16()?;
    if config.save_recordings {
        save_recording(&wav)?;
    }
    let provider = build_provider(&config)?;
    let response = provider.transcribe(ProviderRequest {
        wav_bytes: wav,
        language: Some(config.language.clone()),
        prompt: Some(config.prompt.clone()),
    })?;
    if response.text.trim().is_empty() {
        return Err(AppError::Transcription("API 回傳空白辨識結果".to_string()));
    }
    Ok(JobResult {
        response,
        local_duration_secs,
    })
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
