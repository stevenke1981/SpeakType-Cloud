use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("錄音錯誤：{0}")]
    Audio(String),
    #[error("API 設定錯誤：{0}")]
    Configuration(String),
    #[error("語音辨識失敗：{0}")]
    Transcription(String),
    #[error("文字注入失敗：{0}")]
    Injection(String),
    #[error("檔案錯誤：{0}")]
    Io(String),
}

pub type AppResult<T> = Result<T, AppError>;
