use super::{audio_stem, next_id, HistoryEntry};
use crate::audio::RecordedAudio;
use chrono::{Days, Local};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::sync::Mutex;

// All production wrappers and fixture stores share the serialization boundary.
// File work is performed by the controller's worker, never the audio callback.
static STORE_LOCK: Mutex<()> = Mutex::new(());
const MAX_HISTORY_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct HistoryStore {
    pub(super) root: PathBuf,
}
impl HistoryStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn production() -> Self {
        Self::new(crate::paths::data_dir().join("history"))
    }
    fn load_inner(&self) -> Result<Vec<HistoryEntry>, String> {
        let path = self.root.join("entries.jsonl");
        match fs::metadata(&path) {
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.to_string()),
            Ok(meta) if meta.len() > MAX_HISTORY_BYTES => {
                return Err("歷史檔案超過 32 MiB，請先匯出或整理".into())
            }
            _ => {}
        }
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            let entry: HistoryEntry = serde_json::from_str(line)
                .map_err(|_| "歷史檔案格式錯誤；已保留原檔，未覆寫".to_string())?;
            validate_entry(&entry)?;
            entries.push(entry);
        }
        Ok(entries)
    }
    pub fn load(&self) -> Result<Vec<HistoryEntry>, String> {
        let _guard = STORE_LOCK
            .lock()
            .map_err(|_| "歷史儲存鎖定失敗".to_string())?;
        self.load_inner()
    }
    fn save_inner(&self, entries: &[HistoryEntry]) -> Result<(), String> {
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        for entry in entries {
            serde_json::to_writer(&mut bytes, entry).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
        }
        if bytes.len() as u64 > MAX_HISTORY_BYTES {
            return Err("歷史檔案已達大小限制，未寫入新紀錄".into());
        }
        let temporary = self.root.join(format!(".{}.tmp", next_id()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| e.to_string())?;
            drop(file);
            // std::fs::rename uses replace-existing semantics on Windows.
            fs::rename(&temporary, self.root.join("entries.jsonl")).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
    pub fn commit_accepted(
        &self,
        mut entry: HistoryEntry,
        audio: Option<&RecordedAudio>,
        save_text: bool,
        save_audio: bool,
    ) -> Result<(), String> {
        if !save_text && !save_audio {
            return Ok(());
        }
        validate_id(&entry.id)?;
        let _guard = STORE_LOCK
            .lock()
            .map_err(|_| "歷史儲存鎖定失敗".to_string())?;
        let mut entries = self.load_inner()?;
        if entries.iter().any(|e| e.id == entry.id) {
            return Err("歷史紀錄識別碼重複，未覆寫".into());
        }
        entry.audio = None;
        if !save_text {
            entry.text.clear();
        }
        let mut written_audio = None;
        if save_audio {
            if let Some(audio) = audio {
                let bytes = audio.wav_16khz_mono_i16().map_err(|e| e.to_string())?;
                let directory = self.root.join("audio");
                fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                let path = directory.join(audio_stem(&entry.id));
                let mut file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(error.to_string());
                }
                entry.audio = Some(audio_stem(&entry.id));
                written_audio = Some(path);
            }
        }
        entries.push(entry);
        if let Err(error) = self.save_inner(&entries) {
            if let Some(path) = written_audio {
                if let Err(rollback) = fs::remove_file(path) {
                    return Err(format!("{error}；新增音訊回復失敗：{rollback}"));
                }
            }
            return Err(error);
        }
        Ok(())
    }
    pub fn delete(&self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        self.remove_matching(|entry| entry.id == id).map(|_| ())
    }
    pub fn cleanup(&self, days: u64) -> Result<usize, String> {
        if days == 0 {
            return Ok(0);
        }
        let cutoff = Local::now()
            .checked_sub_days(Days::new(days))
            .ok_or("保留天數無效")?;
        self.remove_matching(|entry| entry.created_at < cutoff)
    }
    fn remove_matching(&self, predicate: impl Fn(&HistoryEntry) -> bool) -> Result<usize, String> {
        let _guard = STORE_LOCK
            .lock()
            .map_err(|_| "歷史儲存鎖定失敗".to_string())?;
        let entries = self.load_inner()?;
        let (removed, keep): (Vec<_>, Vec<_>) = entries.into_iter().partition(predicate);
        if removed.is_empty() {
            return Ok(0);
        }
        // A failed manifest write never removes audio belonging to existing entries.
        self.save_inner(&keep)?;
        for entry in &removed {
            if let Some(name) = &entry.audio {
                match fs::remove_file(self.root.join("audio").join(name)) {
                    Ok(()) => {}
                    Err(e) if e.kind() == ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("文字索引已更新，但音訊刪除失敗：{e}")),
                }
            }
        }
        Ok(removed.len())
    }
}
pub(super) fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err("歷史紀錄識別碼無效".into());
    }
    Ok(())
}
pub(super) fn validate_entry(entry: &HistoryEntry) -> Result<(), String> {
    validate_id(&entry.id)?;
    if entry
        .audio
        .as_ref()
        .is_some_and(|name| name != &audio_stem(&entry.id))
    {
        return Err("歷史音訊路徑無效；已保留原檔".into());
    }
    Ok(())
}
