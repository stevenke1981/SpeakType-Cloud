use super::generated::SettingsData;
use crate::config::*;
use crate::error::{AppError, AppResult};

pub fn provider(index: i32) -> ProviderKind {
    match index {
        1 => ProviderKind::Xai,
        2 => ProviderKind::OpenRouter,
        3 => ProviderKind::Gemini,
        _ => ProviderKind::OpenAi,
    }
}
pub fn from_config(c: &AppConfig, devices: &[String]) -> SettingsData {
    SettingsData {
        provider: match c.provider {
            ProviderKind::OpenAi => 0,
            ProviderKind::Xai => 1,
            ProviderKind::OpenRouter => 2,
            ProviderKind::Gemini => 3,
        },
        mode: match c.transcription_mode {
            TranscriptionMode::BatchPtt => 0,
            TranscriptionMode::RealtimePtt => 1,
            TranscriptionMode::ContinuousDictation => 2,
        },
        language: c.language.as_str().into(),
        prompt: c.prompt.as_str().into(),
        openai_model: c.openai.model.as_str().into(),
        openai_url: c.openai.base_url.as_str().into(),
        openrouter_model: c.openrouter.model.as_str().into(),
        openrouter_url: c.openrouter.base_url.as_str().into(),
        xai_url: c.xai.base_url.as_str().into(),
        gemini_model: c.gemini.model.as_str().into(),
        openai_key_env: c.openai.api_key_env.as_str().into(),
        xai_key_env: c.xai.api_key_env.as_str().into(),
        openrouter_key_env: c.openrouter.api_key_env.as_str().into(),
        gemini_key_env: c.gemini.api_key_env.as_str().into(),
        hotkey: c.hotkey.as_str().into(),
        hold: c.hold_to_record,
        launch: c.launch_at_login,
        device: c
            .recording
            .input_device_name
            .as_ref()
            .and_then(|name| devices.iter().position(|d| d == name))
            .map(|i| i as i32 + 1)
            .unwrap_or(0),
        gain: (c.recording.gain * 100.0).round() as i32,
        min_ms: c.recording.min_duration_ms as i32,
        max_secs: c.recording.max_duration_secs as i32,
        save_text: c.history.save_text,
        save_audio: c.save_recordings,
        retention: c.history.retention_days as i32,
        consent: c.gemini.cloud_consent,
        buffer: if c.output.buffer_mode == OutputBufferMode::Temporary {
            1
        } else {
            0
        },
        auto_paste: c.output.auto_inject,
        restore_clipboard: c.output.restore_clipboard,
        preserve_target: c.output.preserve_target_window,
        copy_fallback: c.output.copy_only_on_injection_failure,
        append_space: c.output.append_space,
        normalize: c.text_processing.normalize_chinese_punctuation,
        variant: match c.text_processing.chinese_variant {
            ChineseVariant::Preserve => 0,
            ChineseVariant::Traditional => 1,
            ChineseVariant::Simplified => 2,
        },
        dictionary: c
            .text_processing
            .dictionary
            .iter()
            .map(|e| format!("{} → {}", e.source, e.replacement))
            .collect::<Vec<_>>()
            .join("\n")
            .into(),
        commands_enabled: c.text_processing.voice_commands_enabled,
        commands: c
            .text_processing
            .voice_commands
            .iter()
            .map(|e| {
                format!(
                    "{} → {}",
                    e.phrase,
                    match &e.action {
                        VoiceCommandAction::InsertText { text } => format!("insert:{text}"),
                        VoiceCommandAction::CopyOnly { text } => format!("copy:{text}"),
                        VoiceCommandAction::Discard => "discard".into(),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
            .into(),
        realtime_model: c.realtime.openai_model.as_str().into(),
        delay: match c.realtime.openai_transcription_delay {
            OpenAiTranscriptionDelay::Minimal => 0,
            OpenAiTranscriptionDelay::Low => 1,
            OpenAiTranscriptionDelay::Medium => 2,
            OpenAiTranscriptionDelay::High => 3,
            OpenAiTranscriptionDelay::XHigh => 4,
        },
        xai_format: c.xai.format_text,
        keyterms: c.xai.keyterms.join("\n").into(),
        smart_turn: c.realtime.xai_smart_turn_enabled,
        smart_threshold: (c.realtime.xai_smart_turn_threshold * 100.0).round() as i32,
        smart_timeout: c.realtime.xai_smart_turn_timeout_ms as i32,
        vad_threshold: (c.realtime.vad_rms_threshold * 1000.0).round() as i32,
        vad_pre_roll: c.realtime.vad_pre_roll_ms as i32,
        vad_speech: c.realtime.vad_min_speech_ms as i32,
        vad_silence: c.realtime.vad_silence_ms as i32,
        utterance: c.realtime.max_utterance_secs as i32,
        cleanup: c.cleanup.enabled,
        fillers: c.cleanup.fillers,
        repetitions: c.cleanup.repetitions,
        corrections: c.cleanup.self_corrections,
        formatting: c.cleanup.formatting,
        polish: c.cleanup.polish,
        tone: match c.cleanup.tone.as_str() {
            "neutral" => 1,
            "friendly" => 2,
            "formal" => 3,
            _ => 0,
        },
        custom_style: c.cleanup.custom_style.as_str().into(),
        suggest_terms: c.cleanup.suggest_terms,
        suggest_style: c.cleanup.suggest_style,
    }
}
pub fn apply(d: &SettingsData, previous: &AppConfig, devices: &[String]) -> AppResult<AppConfig> {
    let mut c = previous.clone();
    c.provider = provider(d.provider);
    c.transcription_mode = match d.mode {
        1 => TranscriptionMode::RealtimePtt,
        2 => TranscriptionMode::ContinuousDictation,
        _ => TranscriptionMode::BatchPtt,
    };
    c.language = d.language.trim().into();
    c.prompt = d.prompt.to_string();
    c.hotkey = d.hotkey.trim().into();
    c.hold_to_record = d.hold;
    c.launch_at_login = d.launch;
    c.openai.model = d.openai_model.trim().into();
    c.openai.base_url = d.openai_url.trim().into();
    c.openrouter.model = d.openrouter_model.trim().into();
    c.openrouter.base_url = d.openrouter_url.trim().into();
    c.xai.base_url = d.xai_url.trim().into();
    c.gemini.model = d.gemini_model.trim().into();
    c.openai.api_key_env = d.openai_key_env.trim().into();
    c.xai.api_key_env = d.xai_key_env.trim().into();
    c.openrouter.api_key_env = d.openrouter_key_env.trim().into();
    c.gemini.api_key_env = d.gemini_key_env.trim().into();
    c.gemini.cloud_consent = d.consent;
    c.recording.input_device_name = if d.device > 0 {
        Some(
            devices
                .get((d.device - 1) as usize)
                .ok_or_else(|| AppError::Configuration("麥克風選項已失效，請重新選擇".into()))?
                .clone(),
        )
    } else {
        None
    };
    c.recording.gain = d.gain as f32 / 100.0;
    c.recording.min_duration_ms = d.min_ms.max(0) as u64;
    c.recording.max_duration_secs = d.max_secs.max(0) as u64;
    c.history.save_text = d.save_text;
    c.save_recordings = d.save_audio;
    c.history.retention_days = d.retention.max(0) as u64;
    c.output.buffer_mode = if d.buffer == 1 {
        OutputBufferMode::Temporary
    } else {
        OutputBufferMode::Clipboard
    };
    c.output.auto_inject = d.auto_paste;
    c.output.restore_clipboard = d.restore_clipboard;
    c.output.preserve_target_window = d.preserve_target;
    c.output.copy_only_on_injection_failure = d.copy_fallback;
    c.output.append_space = d.append_space;
    c.text_processing.normalize_chinese_punctuation = d.normalize;
    c.text_processing.chinese_variant = match d.variant {
        1 => ChineseVariant::Traditional,
        2 => ChineseVariant::Simplified,
        _ => ChineseVariant::Preserve,
    };
    c.text_processing.dictionary = pairs(&d.dictionary)?
        .into_iter()
        .map(|(source, replacement)| DictionaryEntry {
            source,
            replacement,
        })
        .collect();
    c.text_processing.voice_commands_enabled = d.commands_enabled;
    c.text_processing.voice_commands = pairs(&d.commands)?
        .into_iter()
        .map(|(phrase, value)| {
            let action = if let Some(text) = value.strip_prefix("insert:") {
                VoiceCommandAction::InsertText { text: text.into() }
            } else if let Some(text) = value.strip_prefix("copy:") {
                VoiceCommandAction::CopyOnly { text: text.into() }
            } else if value == "discard" {
                VoiceCommandAction::Discard
            } else {
                return Err(AppError::Configuration(
                    "語音命令請使用 insert:、copy: 或 discard".into(),
                ));
            };
            Ok(VoiceCommand { phrase, action })
        })
        .collect::<AppResult<Vec<_>>>()?;
    c.realtime.openai_model = d.realtime_model.trim().into();
    c.realtime.openai_transcription_delay = match d.delay {
        0 => OpenAiTranscriptionDelay::Minimal,
        1 => OpenAiTranscriptionDelay::Low,
        3 => OpenAiTranscriptionDelay::High,
        4 => OpenAiTranscriptionDelay::XHigh,
        _ => OpenAiTranscriptionDelay::Medium,
    };
    c.xai.format_text = d.xai_format;
    c.xai.keyterms = d
        .keyterms
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    c.realtime.xai_smart_turn_enabled = d.smart_turn;
    c.realtime.xai_smart_turn_threshold = d.smart_threshold as f32 / 100.0;
    c.realtime.xai_smart_turn_timeout_ms = d.smart_timeout.max(0) as u64;
    c.realtime.vad_rms_threshold = d.vad_threshold as f32 / 1000.0;
    c.realtime.vad_pre_roll_ms = d.vad_pre_roll.max(0) as u64;
    c.realtime.vad_min_speech_ms = d.vad_speech.max(0) as u64;
    c.realtime.vad_silence_ms = d.vad_silence.max(0) as u64;
    c.realtime.max_utterance_secs = d.utterance.max(0) as u64;
    c.cleanup.enabled = d.cleanup;
    c.cleanup.fillers = d.fillers;
    c.cleanup.repetitions = d.repetitions;
    c.cleanup.self_corrections = d.corrections;
    c.cleanup.formatting = d.formatting;
    c.cleanup.polish = d.polish;
    c.cleanup.tone = match d.tone {
        1 => "neutral",
        2 => "friendly",
        3 => "formal",
        _ => "preserve",
    }
    .into();
    c.cleanup.custom_style = d.custom_style.to_string();
    c.cleanup.suggest_terms = d.suggest_terms;
    c.cleanup.suggest_style = d.suggest_style;
    c.validate()?;
    Ok(c)
}
fn pairs(text: &str) -> AppResult<Vec<(String, String)>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (a, b) = line.split_once('→').ok_or_else(|| {
                AppError::Configuration("詞典／語音命令每行需有 → 分隔符號".into())
            })?;
            Ok((a.trim().into(), b.trim().into()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advanced_key_environment_names_can_be_edited_and_validated() {
        let c = AppConfig::default();
        let mut d = from_config(&c, &[]);
        d.openai_key_env = "CUSTOM_OPENAI_KEY".into();
        d.xai_key_env = "CUSTOM_XAI_KEY".into();
        d.openrouter_key_env = "CUSTOM_ROUTER_KEY".into();
        d.gemini_key_env = "CUSTOM_GEMINI_KEY".into();
        let updated = apply(&d, &c, &[]).expect("valid environment names");
        assert_eq!(updated.openai.api_key_env, "CUSTOM_OPENAI_KEY");
        assert_eq!(updated.xai.api_key_env, "CUSTOM_XAI_KEY");
        assert_eq!(updated.openrouter.api_key_env, "CUSTOM_ROUTER_KEY");
        assert_eq!(updated.gemini.api_key_env, "CUSTOM_GEMINI_KEY");
        assert_eq!(from_config(&updated, &[]).gemini_key_env, d.gemini_key_env);
        d.gemini_key_env = "invalid-name".into();
        assert!(apply(&d, &c, &[]).is_err());
    }
    #[test]
    fn all_default_settings_roundtrip_without_loss() {
        let c = AppConfig::default();
        let d = from_config(&c, &[]);
        let restored = apply(&d, &c, &[]).expect("valid");
        assert_eq!(
            toml::to_string(&c).expect("config"),
            toml::to_string(&restored).expect("config")
        );
    }
    #[test]
    fn dictionary_commands_and_google_privacy_roundtrip() {
        let mut c = AppConfig {
            provider: ProviderKind::Gemini,
            ..AppConfig::default()
        };
        c.gemini.cloud_consent = true;
        c.history.save_text = true;
        c.save_recordings = true;
        c.text_processing.dictionary.push(DictionaryEntry {
            source: "speak type".into(),
            replacement: "SpeakType".into(),
        });
        c.text_processing.voice_commands.push(VoiceCommand {
            phrase: "取消輸出".into(),
            action: VoiceCommandAction::Discard,
        });
        let d = from_config(&c, &[]);
        assert_eq!(
            toml::to_string(&c).expect("serialize"),
            toml::to_string(&apply(&d, &c, &[]).expect("restore")).expect("serialize")
        );
    }
}
