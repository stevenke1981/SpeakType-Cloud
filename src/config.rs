use crate::error::{AppError, AppResult};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAi,
    Xai,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::Xai => "xAI",
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
pub struct RecordingConfig {
    pub input_device_name: Option<String>,
    pub gain: f32,
    pub min_duration_ms: u64,
    pub max_duration_secs: u64,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            input_device_name: None,
            gain: 1.0,
            min_duration_ms: 300,
            max_duration_secs: 120,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub auto_inject: bool,
    pub restore_clipboard: bool,
    pub preserve_target_window: bool,
    pub copy_only_on_injection_failure: bool,
    pub append_space: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
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
    pub language: String,
    pub prompt: String,
    pub hotkey: String,
    pub hold_to_record: bool,
    pub save_recordings: bool,
    pub openai: OpenAiConfig,
    pub xai: XaiConfig,
    pub recording: RecordingConfig,
    pub output: OutputConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::OpenAi,
            language: "zh".to_string(),
            prompt: "使用繁體中文標點；保留專有名詞、英文縮寫與程式碼名稱。".to_string(),
            hotkey: "Ctrl+Shift+Space".to_string(),
            hold_to_record: true,
            save_recordings: false,
            openai: OpenAiConfig::default(),
            xai: XaiConfig::default(),
            recording: RecordingConfig::default(),
            output: OutputConfig::default(),
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
        }
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.hotkey.trim().is_empty() {
            return Err(AppError::Configuration("全域熱鍵不可為空".to_string()));
        }
        if self.recording.gain <= 0.0 {
            return Err(AppError::Configuration("麥克風增益必須大於 0".to_string()));
        }
        if self.provider == ProviderKind::OpenAi && self.openai.model.trim().is_empty() {
            return Err(AppError::Configuration("OpenAI 模型不可為空".to_string()));
        }
        if !is_environment_variable_name(&self.openai.api_key_env)
            || !is_environment_variable_name(&self.xai.api_key_env)
        {
            return Err(AppError::Configuration(
                "API Key 欄位必須是有效的環境變數名稱，不可填入 API Key 本身".to_string(),
            ));
        }
        Ok(())
    }
}

fn parse_config(text: &str) -> AppResult<AppConfig> {
    toml::from_str(text)
        .map_err(|error| AppError::Configuration(format!("設定檔格式錯誤：{error}")))
}

fn is_environment_variable_name(value: &str) -> bool {
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
    fn default_config_is_valid() {
        assert!(AppConfig::default().validate().is_ok());
    }

    #[test]
    fn api_key_env_switches_with_provider() {
        let mut config = AppConfig::default();
        assert_eq!(config.api_key_env(), "OPENAI_API_KEY");
        config.provider = ProviderKind::Xai;
        assert_eq!(config.api_key_env(), "XAI_API_KEY");
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
    fn api_key_fields_must_be_environment_variable_names() {
        let mut config = AppConfig::default();
        config.openai.api_key_env = "provider-test-secret-invalid-one".to_string();
        assert!(config.validate().is_err());

        config.openai.api_key_env = "OPENAI_API_KEY".to_string();
        config.xai.api_key_env = "provider-test-secret-invalid-two".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn malformed_toml_is_reported_instead_of_using_defaults() {
        let error = parse_config("provider = [")
            .expect_err("malformed TOML must fail")
            .to_string();

        assert!(error.contains("設定檔"));
    }
}
