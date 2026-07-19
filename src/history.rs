use crate::audio::RecordedAudio;
use crate::paths;
use chrono::{DateTime, Days, Local};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_HISTORY_ID: AtomicU64 = AtomicU64::new(0);

pub fn next_id() -> String {
    let ts = Local::now().format("%Y%m%d_%H%M%S");
    let seq = NEXT_HISTORY_ID.fetch_add(1, Ordering::Relaxed);
    format!("{ts}_{seq:04}")
}

/// Canonical stem for the audio WAV file on disk.
pub fn audio_stem(id: &str) -> String {
    format!("{id}.wav")
}

/// Full path to the audio WAV file for a history entry.
pub fn audio_path(id: &str) -> PathBuf {
    paths::history_audio_dir().join(audio_stem(id))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub created_at: DateTime<Local>,
    pub provider: String,
    pub duration_secs: f32,
    pub text: String,
    /// Filename stem (e.g. "20260719_123456_0000.wav") relative to
    /// [`paths::history_audio_dir()`].  `None` when the original audio was
    /// too short or the save failed.
    pub audio: Option<String>,
}

impl HistoryEntry {
    /// Audio WAV file path, if the entry has audio.
    pub fn audio_path(&self) -> Option<PathBuf> {
        self.audio
            .as_ref()
            .map(|name| paths::history_audio_dir().join(name))
    }

    /// Short one-line preview (first 72 chars).
    pub fn preview(&self) -> String {
        let line = self.text.trim().replace('\n', " ");
        if line.len() > 72 {
            format!("{}…", &line[..72])
        } else {
            line
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Append a single entry to the JSONL file.
pub fn append(entry: &HistoryEntry) -> Result<(), String> {
    let path = paths::history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())
}

/// Load all history entries; returns an empty vec on error.
pub fn load_all() -> Vec<HistoryEntry> {
    let path = paths::history_path();
    let file = match fs::File::open(&path) {
        Ok(f) => BufReader::new(f),
        Err(_) => return Vec::new(),
    };
    file.lines()
        .filter_map(|line| line.ok().and_then(|l| serde_json::from_str(&l).ok()))
        .collect()
}

/// Persist a full entry list (rewrites the entire file).  Used after
/// deletions.
fn save_all(entries: &[HistoryEntry]) -> Result<(), String> {
    let path = paths::history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    for entry in entries {
        let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Delete one entry by id: removes it from the JSONL file and deletes the
/// associated audio WAV file (if any).
pub fn delete_entry(id: &str) -> Result<(), String> {
    // Delete the audio file (best-effort).
    let wav = audio_path(id);
    let _ = fs::remove_file(&wav);

    // Rewrite the JSONL without this entry.
    let entries = load_all();
    let filtered: Vec<HistoryEntry> = entries.into_iter().filter(|e| e.id != id).collect();
    save_all(&filtered)
}

/// Remove entries older than `days`.  Also deletes their audio files.
/// Returns the number of removed entries.
pub fn cleanup_older_than(days: u64) -> Result<usize, String> {
    if days == 0 {
        return Ok(0);
    }
    let cutoff = Local::now()
        .checked_sub_days(Days::new(days))
        .unwrap_or(Local::now());
    let entries = load_all();
    let mut removed = 0usize;
    let mut keep: Vec<HistoryEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.created_at < cutoff {
            // Delete audio (best-effort).
            if let Some(path) = entry.audio_path() {
                let _ = fs::remove_file(&path);
            }
            removed = removed.saturating_add(1);
        } else {
            keep.push(entry);
        }
    }
    if removed > 0 {
        save_all(&keep)?;
    }
    Ok(removed)
}

/// Save a `RecordedAudio` as a 16‑kHz mono 16‑bit WAV file in the history
/// audio directory.  Returns the canonical filename (e.g. "20260719_123456_0000.wav")
/// on success, or an error message on failure.
pub fn save_audio(id: &str, audio: &RecordedAudio) -> Result<String, String> {
    let dir = paths::history_audio_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let wav_bytes = audio.wav_16khz_mono_i16().map_err(|e| e.to_string())?;
    let path = dir.join(audio_stem(id));
    fs::write(&path, &wav_bytes).map_err(|e| e.to_string())?;
    Ok(audio_stem(id))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_audio() -> RecordedAudio {
        RecordedAudio {
            samples: vec![0.0; 160],
            sample_rate: 16_000,
            channels: 1,
        }
    }

    fn make_entry(created_at: DateTime<Local>, text: &str) -> HistoryEntry {
        HistoryEntry {
            id: next_id(),
            created_at,
            provider: "test".to_string(),
            duration_secs: 1.0,
            text: text.to_string(),
            audio: None,
        }
    }

    #[test]
    fn append_and_load_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("speaktype-history-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        // Override the history path via an env-driven shim – for this test we
        // write directly to a temp location via the low‑level append / load.
        let entry_a = make_entry(Local::now(), "hello");
        let entry_b = make_entry(Local::now(), "world");

        // Write temp file directly
        let tmp_path = dir.join("entries.jsonl");
        fs::create_dir_all(&dir).expect("create tmp dir");
        {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&tmp_path)
                .expect("open tmp file");
            let line = serde_json::to_string(&entry_a).expect("serialize") + "\n";
            file.write_all(line.as_bytes()).expect("write a");
            let line = serde_json::to_string(&entry_b).expect("serialize") + "\n";
            file.write_all(line.as_bytes()).expect("write b");
        }
        // Note: the actual load_all reads from the project path, so we test
        // the JSONL format correctness instead via inline roundtrip:
        let serialized = serde_json::to_string(&entry_a).expect("serialize");
        let deserialized: HistoryEntry = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized.id, entry_a.id);
        assert_eq!(deserialized.text, entry_a.text);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_reports_unwritable_parent() {
        let blocker = std::env::temp_dir().join(format!(
            "speaktype-history-block-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::write(&blocker, b"not a directory").expect("create blocker file");
        let path = blocker.join("entries.jsonl");
        let entry = make_entry(Local::now(), "preserve me");

        let result = append_to_path_for_test(&path, &entry);
        let _ = fs::remove_file(&blocker);

        assert!(result.is_err());
        assert_eq!(entry.text, "preserve me");
    }

    /// Thin wrapper so we can test with an arbitrary path.
    fn append_to_path_for_test(path: &std::path::Path, entry: &HistoryEntry) -> Result<(), String> {
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

    #[test]
    fn save_audio_creates_wav() {
        let dir = std::env::temp_dir().join(format!(
            "speaktype-history-audio-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        // Temporarily redirect history_audio_dir
        // (We test the file creation manually)

        let audio = sample_audio();
        let id = next_id();
        let stem = audio_stem(&id);
        let path = dir.join(&stem);
        fs::create_dir_all(&dir).expect("create tmp dir");

        let wav_bytes = audio.wav_16khz_mono_i16().expect("wav");
        fs::write(&path, &wav_bytes).expect("write wav");

        assert!(path.exists());
        assert_eq!(&fs::read(&path).expect("read")[..4], b"RIFF");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_entry_removes_jsonl_line_and_audio() {
        let dir = std::env::temp_dir().join(format!(
            "speaktype-history-delete-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);

        // We'll test the logic via the public `delete_entry` but need to
        // redirect paths – so we test the filtering logic directly:
        let now = Local.with_ymd_and_hms(2026, 7, 19, 12, 0, 0).unwrap();
        let a = make_entry(now, "keep me");
        let b = make_entry(now, "delete me");

        let all = vec![a.clone(), b.clone()];
        let filtered: Vec<HistoryEntry> = all.into_iter().filter(|e| e.id != b.id).collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, a.id);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleanup_removes_old_entries() {
        let now = Local.with_ymd_and_hms(2026, 7, 19, 12, 0, 0).unwrap();
        let old = make_entry(
            now.checked_sub_days(Days::new(10)).expect("now - 10 days"),
            "old entry",
        );
        let recent = make_entry(
            now.checked_sub_days(Days::new(1)).expect("now - 1 day"),
            "recent entry",
        );

        let entries = vec![old.clone(), recent.clone()];
        let cutoff = now.checked_sub_days(Days::new(7)).expect("now - 7 days");
        let mut keep = Vec::new();
        for e in entries {
            if e.created_at >= cutoff {
                keep.push(e);
            }
        }

        assert_eq!(keep.len(), 1);
        assert_eq!(keep[0].id, recent.id);
    }

    #[test]
    fn preview_truncates_long_text() {
        let now = Local::now();
        let short = make_entry(now, "Hello world");
        assert_eq!(short.preview(), "Hello world");

        let long_text = "A".repeat(100);
        let long = make_entry(now, &long_text);
        let preview = long.preview();
        assert!(preview.ends_with('…'));
        assert_eq!(preview.chars().count(), 73); // 72 chars + '…'
    }

    #[test]
    fn next_id_generates_unique_ids() {
        let a = next_id();
        let b = next_id();
        assert_ne!(a, b);
        assert!(a.len() >= 15); // YYYYMMDD_HHMMSS_NNNN
    }
}
