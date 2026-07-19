use crate::error::{AppError, AppResult};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;

pub const MAX_RECORDING_DURATION_SECS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAi,
    Xai,
    OpenRouter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionMode {
    BatchPtt,
    RealtimePtt,
    ContinuousDictation,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenAiTranscriptionDelay {
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    XHigh,
}

impl OpenAiTranscriptionDelay {
    pub fn as_api_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }

    pub fn label(self) -> &'static str {
        self.as_api_str()
    }
}

impl TranscriptionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::BatchPtt => "Batch / PTT",
            Self::RealtimePtt => "Realtime PTT",
            Self::ContinuousDictation => "Continuous Dictation",
        }
    }

    pub fn is_realtime(self) -> bool {
        !matches!(self, Self::BatchPtt)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryConfig {
    /// Number of days to keep history entries.  0 = keep forever.
    pub retention_days: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChineseVariant {
    Preserve,
    Traditional,
    Simplified,
}

impl ChineseVariant {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preserve => "保留原文",
            Self::Traditional => "台灣繁體",
            Self::Simplified => "簡體中文",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TextProcessingConfig {
    pub normalize_chinese_punctuation: bool,
    pub chinese_variant: ChineseVariant,
    pub dictionary: Vec<DictionaryEntry>,
    pub voice_commands_enabled: bool,
    pub voice_commands: Vec<VoiceCommand>,
}

impl Default for TextProcessingConfig {
    fn default() -> Self {
        Self {
            normalize_chinese_punctuation: true,
            chinese_variant: ChineseVariant::Preserve,
            dictionary: Vec::new(),
            voice_commands_enabled: false,
            voice_commands: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub source: String,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceCommand {
    pub phrase: String,
    #[serde(flatten)]
    pub action: VoiceCommandAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum VoiceCommandAction {
    InsertText { text: String },
    CopyOnly { text: String },
    Discard,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::Xai => "xAI",
            Self::OpenRouter => "OpenRouter",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com".to_string(),
            model: "gpt-4o-mini-transcribe".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct XaiConfig {
    pub base_url: String,
    pub api_key_env: String,
    pub format_text: bool,
    pub keyterms: Vec<String>,
}

impl Default for XaiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.x.ai".to_string(),
            api_key_env: "XAI_API_KEY".to_string(),
            format_text: true,
            keyterms: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenRouterConfig {
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            base_url: "https://openrouter.ai/api".to_string(),
            model: "openai/gpt-4o-mini-transcribe".to_string(),
            api_key_env: "OPENROUTER_API_KEY".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordingConfig {
    pub input_device_name: Option<String>,
    pub gain: f32,
    pub min_duration_ms: u64,
    pub max_duration_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RealtimeConfig {
    pub openai_model: String,
    pub openai_transcription_delay: OpenAiTranscriptionDelay,
    pub xai_smart_turn_enabled: bool,
    pub xai_smart_turn_threshold: f32,
    pub xai_smart_turn_timeout_ms: u64,
    pub vad_rms_threshold: f32,
    pub vad_pre_roll_ms: u64,
    pub vad_min_speech_ms: u64,
    pub vad_silence_ms: u64,
    pub max_utterance_secs: u64,
}

impl Default for RealtimeConfig {
    fn default() -> Self {
        Self {
            openai_model: "gpt-realtime-whisper".to_string(),
            openai_transcription_delay: OpenAiTranscriptionDelay::Medium,
            xai_smart_turn_enabled: false,
            xai_smart_turn_threshold: 0.7,
            xai_smart_turn_timeout_ms: 3_000,
            vad_rms_threshold: 0.025,
            vad_pre_roll_ms: 300,
            vad_min_speech_ms: 200,
            vad_silence_ms: 700,
            max_utterance_secs: 60,
        }
    }
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            input_device_name: None,
            gain: 1.0,
            min_duration_ms: 300,
            max_duration_secs: MAX_RECORDING_DURATION_SECS,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputBufferMode {
    /// 始終複製到系統剪貼簿（目前行為）
    #[default]
    Clipboard,
    /// 文字保留在 App 內部暫存區，不污染剪貼簿；自動注入時暫時使用剪貼簿再還原
    Temporary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// 轉錄文字儲存模式：剪貼簿或 App 暫存區
    pub buffer_mode: OutputBufferMode,
    pub auto_inject: bool,
    pub restore_clipboard: bool,
    pub preserve_target_window: bool,
    pub copy_only_on_injection_failure: bool,
    pub append_space: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            buffer_mode: OutputBufferMode::default(),
            auto_inject: true,
            restore_clipboard: true,
            preserve_target_window: true,
            copy_only_on_injection_failure: true,
            append_space: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub provider: ProviderKind,
    pub transcription_mode: TranscriptionMode,
    pub language: String,
    pub prompt: String,
    pub hotkey: String,
    pub hold_to_record: bool,
    pub launch_at_login: bool,
    pub save_recordings: bool,
    pub openai: OpenAiConfig,
    pub xai: XaiConfig,
    pub openrouter: OpenRouterConfig,
    pub recording: RecordingConfig,
    pub realtime: RealtimeConfig,
    pub output: OutputConfig,
    pub text_processing: TextProcessingConfig,
    pub history: HistoryConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::OpenAi,
            transcription_mode: TranscriptionMode::BatchPtt,
            language: "zh".to_string(),
            prompt: "使用繁體中文標點；保留專有名詞、英文縮寫與程式碼名稱。".to_string(),
            hotkey: "Ctrl+Shift+Space".to_string(),
            hold_to_record: true,
            launch_at_login: false,
            save_recordings: false,
            openai: OpenAiConfig::default(),
            xai: XaiConfig::default(),
            openrouter: OpenRouterConfig::default(),
            recording: RecordingConfig::default(),
            realtime: RealtimeConfig::default(),
            output: OutputConfig::default(),
            text_processing: TextProcessingConfig::default(),
            history: HistoryConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load() -> AppResult<Self> {
        let path = paths::config_path();
        match fs::read_to_string(&path) {
            Ok(text) => parse_config(&text),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(AppError::Io(format!(
                "無法讀取設定檔 {}：{error}",
                path.display()
            ))),
        }
    }

    pub fn save(&self) -> AppResult<()> {
        let path = paths::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::Io(e.to_string()))?;
        }
        let text =
            toml::to_string_pretty(self).map_err(|e| AppError::Configuration(e.to_string()))?;
        fs::write(path, text).map_err(|e| AppError::Io(e.to_string()))
    }

    pub fn api_key_env(&self) -> &str {
        match self.provider {
            ProviderKind::OpenAi => &self.openai.api_key_env,
            ProviderKind::Xai => &self.xai.api_key_env,
            ProviderKind::OpenRouter => &self.openrouter.api_key_env,
        }
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.hotkey.trim().is_empty() {
            return Err(AppError::Configuration("全域熱鍵不可為空".to_string()));
        }
        if self.recording.gain <= 0.0 {
            return Err(AppError::Configuration("麥克風增益必須大於 0".to_string()));
        }
        if self.recording.max_duration_secs == 0
            || self.recording.max_duration_secs > MAX_RECORDING_DURATION_SECS
        {
            return Err(AppError::Configuration(format!(
                "錄音時間上限必須介於 1 與 {MAX_RECORDING_DURATION_SECS} 秒"
            )));
        }
        if self.realtime.openai_model.trim().is_empty() {
            return Err(AppError::Configuration(
                "OpenAI realtime 模型不可為空".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.realtime.xai_smart_turn_threshold) {
            return Err(AppError::Configuration(
                "xAI Smart Turn threshold 必須介於 0 與 1".to_string(),
            ));
        }
        if !(1..=5_000).contains(&self.realtime.xai_smart_turn_timeout_ms) {
            return Err(AppError::Configuration(
                "xAI Smart Turn timeout 必須介於 1 與 5000 ms".to_string(),
            ));
        }
        if !(0.001..=1.0).contains(&self.realtime.vad_rms_threshold)
            || self.realtime.vad_pre_roll_ms > 2_000
            || self.realtime.vad_min_speech_ms < 100
            || self.realtime.vad_silence_ms < 100
            || self.realtime.max_utterance_secs == 0
        {
            return Err(AppError::Configuration(
                "Realtime VAD 設定超出安全範圍".to_string(),
            ));
        }
        if !(0..=36_500).contains(&self.history.retention_days) {
            return Err(AppError::Configuration(
                "歷史紀錄保留天數必須介於 0 與 36500 天".to_string(),
            ));
        }
        if self.provider == ProviderKind::OpenAi && self.openai.model.trim().is_empty() {
            return Err(AppError::Configuration("OpenAI 模型不可為空".to_string()));
        }
        if self.provider == ProviderKind::OpenRouter && self.openrouter.model.trim().is_empty() {
            return Err(AppError::Configuration(
                "OpenRouter 模型不可為空".to_string(),
            ));
        }
        if self.provider == ProviderKind::OpenRouter && self.transcription_mode.is_realtime() {
            return Err(AppError::Configuration(
                "OpenRouter 僅支援 Batch / PTT 模式，不支援 Realtime 或 Continuous Dictation"
                    .to_string(),
            ));
        }
        if !is_environment_variable_name(&self.openai.api_key_env)
            || !is_environment_variable_name(&self.xai.api_key_env)
            || !is_environment_variable_name(&self.openrouter.api_key_env)
        {
            return Err(AppError::Configuration(
                "API Key 欄位必須是有效的環境變數名稱，不可填入 API Key 本身".to_string(),
            ));
        }
        let mut dictionary_sources = std::collections::HashSet::new();
        for entry in &self.text_processing.dictionary {
            if entry.source.trim().is_empty() || entry.source != entry.source.trim() {
                return Err(AppError::Configuration(
                    "自訂詞典來源不可為空，且前後不可包含空白".to_string(),
                ));
            }
            if !dictionary_sources.insert(&entry.source) {
                return Err(AppError::Configuration(format!(
                    "自訂詞典來源重複：{}",
                    entry.source
                )));
            }
        }
        let mut command_phrases = std::collections::HashSet::new();
        for command in &self.text_processing.voice_commands {
            if command.phrase.trim().is_empty() || command.phrase != command.phrase.trim() {
                return Err(AppError::Configuration(
                    "語音命令片語不可為空，且前後不可包含空白".to_string(),
                ));
            }
            if !command_phrases.insert(&command.phrase) {
                return Err(AppError::Configuration(format!(
                    "語音命令片語重複：{}",
                    command.phrase
                )));
            }
            match &command.action {
                VoiceCommandAction::InsertText { text } | VoiceCommandAction::CopyOnly { text }
                    if text.is_empty() =>
                {
                    return Err(AppError::Configuration(
                        "insert_text/copy_only 語音命令的文字不可為空".to_string(),
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn parse_config(text: &str) -> AppResult<AppConfig> {
    toml::from_str(text)
        .map_err(|error| AppError::Configuration(format!("設定檔格式錯誤：{error}")))
}

pub fn is_environment_variable_name(value: &str) -> bool {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_realtime_delay_uses_only_official_enum_values() {
        for (delay, encoded) in [
            (OpenAiTranscriptionDelay::Minimal, "minimal"),
            (OpenAiTranscriptionDelay::Low, "low"),
            (OpenAiTranscriptionDelay::Medium, "medium"),
            (OpenAiTranscriptionDelay::High, "high"),
            (OpenAiTranscriptionDelay::XHigh, "xhigh"),
        ] {
            assert_eq!(
                toml::Value::try_from(delay).expect("serialize").as_str(),
                Some(encoded)
            );
        }
    }

    #[test]
    fn default_config_is_valid() {
        assert!(AppConfig::default().validate().is_ok());
        assert!(!AppConfig::default().launch_at_login);
        assert_eq!(
            AppConfig::default().transcription_mode,
            TranscriptionMode::BatchPtt
        );
    }

    #[test]
    fn realtime_settings_are_bounded_and_opt_in() {
        let mut config = AppConfig {
            transcription_mode: TranscriptionMode::ContinuousDictation,
            ..AppConfig::default()
        };
        config.realtime.xai_smart_turn_threshold = 1.1;
        assert!(config.validate().is_err());
        config.realtime.xai_smart_turn_threshold = 0.7;
        config.realtime.xai_smart_turn_timeout_ms = 5_001;
        assert!(config.validate().is_err());
        config.realtime.xai_smart_turn_timeout_ms = 5_000;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn recording_duration_cannot_exceed_capture_ring_capacity() {
        let mut config = AppConfig::default();
        config.recording.max_duration_secs = 121;
        assert!(config.validate().is_err());
        config.recording.max_duration_secs = 120;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn api_key_env_switches_with_provider() {
        let mut config = AppConfig::default();
        assert_eq!(config.api_key_env(), "OPENAI_API_KEY");
        config.provider = ProviderKind::Xai;
        assert_eq!(config.api_key_env(), "XAI_API_KEY");
        config.provider = ProviderKind::OpenRouter;
        assert_eq!(config.api_key_env(), "OPENROUTER_API_KEY");
    }

    #[test]
    fn serialization_and_debug_never_read_api_key_value() {
        let api_key = "provider-test-secret-config-serialization";
        let variable = "SPEAKTYPE_TEST_API_KEY_SERIALIZATION";
        std::env::set_var(variable, api_key);
        let mut config = AppConfig::default();
        config.openai.api_key_env = variable.to_string();

        let serialized = toml::to_string(&config).expect("serialize config");
        let debug = format!("{config:?}");
        std::env::remove_var(variable);

        assert!(serialized.contains(variable));
        assert!(!serialized.contains(api_key));
        assert!(!debug.contains(api_key));
    }

    #[test]
    fn openrouter_defaults_are_reasonable() {
        let config = OpenRouterConfig::default();
        assert_eq!(config.base_url, "https://openrouter.ai/api");
        assert_eq!(config.model, "openai/gpt-4o-mini-transcribe");
        assert_eq!(config.api_key_env, "OPENROUTER_API_KEY");
    }

    #[test]
    fn openrouter_realtime_is_rejected() {
        let mut config = AppConfig {
            provider: ProviderKind::OpenRouter,
            transcription_mode: TranscriptionMode::RealtimePtt,
            ..AppConfig::default()
        };
        assert!(config.validate().is_err());
        config.transcription_mode = TranscriptionMode::ContinuousDictation;
        assert!(config.validate().is_err());
        config.transcription_mode = TranscriptionMode::BatchPtt;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn openrouter_empty_model_is_rejected() {
        let config = AppConfig {
            provider: ProviderKind::OpenRouter,
            openrouter: OpenRouterConfig {
                model: "".to_string(),
                ..OpenRouterConfig::default()
            },
            ..AppConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn api_key_fields_must_be_environment_variable_names() {
        let mut config = AppConfig::default();
        config.openai.api_key_env = "provider-test-secret-invalid-one".to_string();
        assert!(config.validate().is_err());

        config.openai.api_key_env = "OPENAI_API_KEY".to_string();
        config.xai.api_key_env = "provider-test-secret-invalid-two".to_string();
        assert!(config.validate().is_err());

        config.xai.api_key_env = "XAI_API_KEY".to_string();
        config.openrouter.api_key_env = "provider-test-secret-invalid-three".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn malformed_toml_is_reported_instead_of_using_defaults() {
        let error = parse_config("provider = [")
            .expect_err("malformed TOML must fail")
            .to_string();

        assert!(error.contains("設定檔"));
    }

    #[test]
    fn example_config_parses_and_validates() {
        let config = parse_config(include_str!("../config.example.toml")).expect("example config");
        config.validate().expect("valid example config");
    }

    #[test]
    fn dictionary_rejects_empty_source() {
        let mut config = AppConfig::default();
        config.text_processing.dictionary.push(DictionaryEntry {
            source: "   ".to_string(),
            replacement: "ignored".to_string(),
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn voice_commands_are_disabled_by_default_and_reject_empty_phrases() {
        let mut config = AppConfig::default();
        assert!(!config.text_processing.voice_commands_enabled);
        config.text_processing.voice_commands.push(VoiceCommand {
            phrase: "".to_string(),
            action: VoiceCommandAction::Discard,
        });

        assert!(config.validate().is_err());
    }
}
