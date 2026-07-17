use crate::paths;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub created_at: DateTime<Local>,
    pub provider: String,
    pub duration_secs: f32,
    pub text: String,
}

pub fn append(entry: &HistoryEntry) -> Result<(), String> {
    let path = paths::history_path();
    append_to_path(&path, entry)
}

fn append_to_path(path: &Path, entry: &HistoryEntry) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn append_reports_unwritable_parent() {
        let blocker = std::env::temp_dir().join(format!(
            "speaktype-history-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::write(&blocker, b"not a directory").expect("create blocker file");
        let path = blocker.join("history.jsonl");
        let entry = HistoryEntry {
            created_at: Local.timestamp_opt(0, 0).single().expect("timestamp"),
            provider: "test".to_string(),
            duration_secs: 1.0,
            text: "preserve me".to_string(),
        };

        let result = append_to_path(&path, &entry);
        fs::remove_file(&blocker).expect("remove blocker file");

        assert!(result.is_err());
        assert_eq!(entry.text, "preserve me");
    }
}
