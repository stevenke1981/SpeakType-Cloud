use crate::config::{AppConfig, ProviderKind};
use crate::history::{HistoryEntry, HistoryStore};
use crate::runtime::RuntimeMode;
use crate::updater::{self, StagedUpdate, UpdateManifest};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc,
};
use std::time::Duration;

enum ResultMessage {
    History(Result<Vec<HistoryEntry>, String>),
    Models(ProviderKind, Result<Vec<String>, String>),
    Notice(Result<String, String>),
    Checked(Result<Option<UpdateManifest>, String>),
    Staged(Result<StagedUpdate, String>),
}
pub struct Services {
    mode: RuntimeMode,
    closing: bool,
    rx: Option<Receiver<ResultMessage>>,
    pub history: Vec<HistoryEntry>,
    pub models: Vec<String>,
    pub models_provider: Option<ProviderKind>,
    pub notice: String,
    pub revision: u64,
    pub update_status: String,
    manifest: Option<UpdateManifest>,
    staged: Option<StagedUpdate>,
    playback_cancel: Arc<AtomicBool>,
}
impl Services {
    pub fn new(mode: RuntimeMode) -> Self {
        Self {
            mode,
            closing: false,
            rx: None,
            history: Vec::new(),
            models: Vec::new(),
            models_provider: None,
            notice: String::new(),
            revision: 0,
            update_status: if mode.is_demo() {
                "離線示範，不連線檢查更新".into()
            } else {
                updater::configured_signer_cert_sha256()
                    .err()
                    .unwrap_or_else(|| "尚未檢查更新".into())
            },
            manifest: None,
            staged: None,
            playback_cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn busy(&self) -> bool {
        self.rx.is_some()
    }
    pub fn can_download(&self) -> bool {
        self.manifest.is_some()
    }
    pub fn can_install(&self) -> bool {
        self.staged.is_some()
    }
    fn start(&mut self, task: impl FnOnce() -> ResultMessage + Send + 'static) {
        if self.closing || self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.notice = "處理中…".into();
        std::thread::spawn(move || {
            let _ = tx.send(task());
        });
    }
    pub fn poll(&mut self) {
        let result = match self.rx.as_ref().map(Receiver::try_recv) {
            Some(Ok(result)) => result,
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                ResultMessage::Notice(Err("背景工作中斷，請重試".into()))
            }
            _ => return,
        };
        self.rx = None;
        self.revision = self.revision.wrapping_add(1);
        match result {
            ResultMessage::History(Ok(entries)) => {
                self.history = entries;
                self.notice = format!("已載入 {} 筆紀錄", self.history.len());
            }
            ResultMessage::Models(provider, Ok(models)) => {
                self.models = models;
                self.models_provider = Some(provider);
                self.notice = format!("已載入 {} 個模型", self.models.len());
            }
            ResultMessage::Notice(Ok(message)) => self.notice = message,
            ResultMessage::History(Err(error))
            | ResultMessage::Models(_, Err(error))
            | ResultMessage::Notice(Err(error)) => self.notice = error,
            ResultMessage::Checked(result) => match result {
                Ok(Some(manifest)) => {
                    self.update_status = format!("可更新至 {}，請確認下載", manifest.version);
                    self.manifest = Some(manifest);
                    self.staged = None;
                    self.notice.clear();
                }
                Ok(None) => {
                    self.update_status = "目前已是最新版本".into();
                    self.manifest = None;
                    self.staged = None;
                    self.notice.clear();
                }
                Err(error) => {
                    self.update_status = error;
                    self.notice.clear();
                }
            },
            ResultMessage::Staged(result) => match result {
                Ok(staged) => {
                    self.update_status =
                        format!("{} 已驗證雜湊與簽章；請確認啟動安裝精靈", staged.version);
                    self.staged = Some(staged);
                    self.notice.clear();
                }
                Err(error) => {
                    self.update_status = error;
                    self.notice.clear();
                }
            },
        }
    }
    pub fn load_history(&mut self) {
        if self.mode.is_demo() {
            self.notice = "示範模式不讀取私人歷史".into();
            self.history = vec![HistoryEntry {
                id: "demo-fixture".into(),
                created_at: chrono::Local::now(),
                provider: "offline mock".into(),
                duration_secs: 1.0,
                text: "這是可刪除的記憶體示範紀錄。".into(),
                audio: None,
            }];
            self.revision = self.revision.wrapping_add(1);
            return;
        }
        self.start(|| ResultMessage::History(crate::history::load()));
    }
    pub fn delete_history(&mut self, id: String) {
        if self.mode.is_demo() {
            self.history.retain(|e| e.id != id);
            self.revision = self.revision.wrapping_add(1);
            self.notice = "已刪除記憶體示範紀錄".into();
            return;
        }
        self.start(move || {
            ResultMessage::History(
                HistoryStore::production()
                    .delete(&id)
                    .and_then(|()| crate::history::load()),
            )
        });
    }
    pub fn cleanup_history(&mut self, days: u64) {
        if self.mode.is_demo() {
            self.notice = "示範模式未刪除任何磁碟資料".into();
            return;
        }
        if days == 0 {
            self.notice = "保留天數為 0（永久），沒有執行刪除".into();
            return;
        }
        self.start(move || {
            ResultMessage::History(
                HistoryStore::production()
                    .cleanup(days)
                    .and_then(|_| crate::history::load()),
            )
        });
    }
    pub fn key_action(&mut self, config: AppConfig, provider: ProviderKind, key: Option<String>) {
        if self.mode.is_demo() {
            self.notice = "示範模式未讀取或寫入任何金鑰".into();
            return;
        }
        self.start(move || {
            let mut config = config;
            config.provider = provider;
            let result = match key {
                Some(key) => crate::secrets::save_api_key(config.api_key_env(), &key),
                None => crate::secrets::clear_api_key(config.api_key_env()),
            };
            ResultMessage::Notice(
                result
                    .map(|()| "金鑰設定已更新；欄位已清空".into())
                    .map_err(|e| e.to_string()),
            )
        });
    }
    pub fn fetch_models(&mut self, mut config: AppConfig, provider: ProviderKind) {
        config.provider = provider;
        if self.mode.is_demo() || provider == ProviderKind::Gemini {
            self.models = match provider {
                ProviderKind::Gemini => vec!["gemini-3.8-flash".into()],
                ProviderKind::OpenAi => vec![config.openai.model],
                ProviderKind::OpenRouter => vec![config.openrouter.model],
                ProviderKind::Xai => vec!["grok".into()],
            };
            self.models_provider = Some(provider);
            self.revision = self.revision.wrapping_add(1);
            self.notice = if self.mode.is_demo() {
                "離線模型示範，未連線".into()
            } else {
                "Gemini 官方相容模型預設，可手動輸入其他模型".into()
            };
            return;
        }
        self.start(move || {
            let result = (|| {
                let key = crate::providers::load_api_key(&config).map_err(|e| e.to_string())?;
                let url = match provider {
                    ProviderKind::OpenAi => &config.openai.base_url,
                    ProviderKind::Xai => &config.xai.base_url,
                    _ => &config.openrouter.base_url,
                };
                runtime()?
                    .block_on(crate::providers::fetch_available_models(url, &key))
                    .map_err(|e| e.to_string())
            })();
            ResultMessage::Models(provider, result)
        });
    }
    pub fn check_update(&mut self) {
        if self.mode.is_demo() {
            self.update_status = "離線示範，不連線檢查更新".into();
            return;
        }
        self.start(|| {
            ResultMessage::Checked(runtime().and_then(|r| r.block_on(updater::check_for_update())))
        });
    }
    pub fn download_update(&mut self) {
        if self.mode.is_demo() {
            return;
        }
        if let Some(manifest) = self.manifest.clone() {
            self.start(move || {
                ResultMessage::Staged(
                    runtime().and_then(|r| r.block_on(updater::stage_update(&manifest))),
                )
            });
        }
    }
    pub fn install_update(&mut self) {
        if self.mode.is_demo() {
            return;
        }
        if let Some(staged) = self.staged.clone() {
            self.start(move || {
                ResultMessage::Notice(
                    updater::launch_installer(&staged)
                        .map(|()| "安裝精靈已啟動，請依畫面完成；本程式可從系統匣退出".into()),
                )
            });
        }
    }
    pub fn stop_playback(&self) {
        self.playback_cancel.store(true, Ordering::Release);
    }
    pub fn shutdown(&mut self) {
        self.closing = true;
        self.stop_playback();
    }
    pub fn play_history(&mut self, id: &str) {
        if self.mode.is_demo() {
            self.notice = "示範紀錄沒有私人音檔".into();
            return;
        }
        if self.busy() {
            return;
        }
        let Some(path) = self
            .history
            .iter()
            .find(|e| e.id == id)
            .and_then(HistoryEntry::audio_path)
        else {
            return;
        };
        self.playback_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.playback_cancel.clone();
        self.start(move || {
            ResultMessage::Notice((|| {
                let (_stream, handle) =
                    rodio::OutputStream::try_default().map_err(|e| e.to_string())?;
                let sink = rodio::Sink::try_new(&handle).map_err(|e| e.to_string())?;
                let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                let source = rodio::Decoder::new(std::io::BufReader::new(file))
                    .map_err(|e| e.to_string())?;
                sink.append(source);
                while !sink.empty() && !cancel.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(40));
                }
                sink.stop();
                Ok("播放已停止".into())
            })())
        });
    }
}
impl Drop for Services {
    fn drop(&mut self) {
        self.stop_playback();
    }
}
fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
}
