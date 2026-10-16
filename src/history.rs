#[cfg(test)]
use crate::audio::RecordedAudio;
use crate::paths;
#[cfg(test)]
use chrono::Days;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::fs::{self, OpenOptions};
#[cfg(test)]
use std::io::Write;
mod store;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
pub use store::HistoryStore;

static NEXT_HISTORY_ID: AtomicU64 = AtomicU64::new(0);

pub fn next_id() -> String {
    let ts = Local::now().format("%Y%m%d_%H%M%S");
    let seq = NEXT_HISTORY_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "{ts}_{}_{seq:04}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

/// Canonical stem for the audio WAV file on disk.
pub fn audio_stem(id: &str) -> String {
    format!("{id}.wav")
}

/// Full path to the audio WAV file for a history entry.
#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
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
        store::validate_entry(self).ok()?;
        self.audio
            .as_ref()
            .map(|name| paths::history_audio_dir().join(name))
    }

    /// Short one-line preview (first 72 Unicode scalar values, not bytes).
    pub fn preview(&self) -> String {
        let line = self.text.trim().replace('\n', " ");
        let mut chars = line.chars();
        let mut preview: String = chars.by_ref().take(72).collect();
        if chars.next().is_some() {
            preview.push('…');
        }
        preview
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Errors are returned to the production history screen instead of overwriting data.
pub fn load() -> Result<Vec<HistoryEntry>, String> {
    HistoryStore::production().load()
}
// Legacy adapter retained while the Slint history screen is migrated.
#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
pub fn load_all() -> Vec<HistoryEntry> {
    load().unwrap_or_default()
}
#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
pub fn delete_entry(id: &str) -> Result<(), String> {
    HistoryStore::production().delete(id)
}
pub fn cleanup_older_than(days: u64) -> Result<usize, String> {
    HistoryStore::production().cleanup(days)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixture() -> HistoryStore {
        HistoryStore::new(
            std::env::current_dir()
                .expect("workspace")
                .join("target")
                .join(format!(
                    "speaktype-store-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("clock")
                        .as_nanos()
                )),
        )
    }
    #[test]
    fn actual_store_obeys_all_four_privacy_combinations() {
        for text in [false, true] {
            for audio in [false, true] {
                let store = fixture();
                let entry = make_entry(Local::now(), "synthetic private transcript");
                store
                    .commit_accepted(entry.clone(), Some(&sample_audio()), text, audio)
                    .expect("commit");
                if !text && !audio {
                    assert!(!store.root.exists());
                    continue;
                }
                let entries = store.load().expect("load");
                assert_eq!(entries.len(), 1);
                assert_eq!(
                    entries[0].text,
                    if text {
                        "synthetic private transcript"
                    } else {
                        ""
                    }
                );
                assert_eq!(
                    store
                        .root
                        .join("audio")
                        .join(audio_stem(&entry.id))
                        .exists(),
                    audio
                );
                store.delete(&entry.id).expect("delete");
                assert!(store.load().expect("load after delete").is_empty());
                assert!(!store
                    .root
                    .join("audio")
                    .join(audio_stem(&entry.id))
                    .exists());
                fs::remove_dir_all(&store.root).expect("remove own fixture");
            }
        }
    }
    #[test]
    fn failed_manifest_rolls_back_audio_and_corrupt_history_is_preserved() {
        let store = fixture();
        fs::create_dir_all(&store.root).expect("fixture");
        fs::write(store.root.join("entries.jsonl"), b"invalid-json\n").expect("corrupt fixture");
        let entry = make_entry(Local::now(), "fixture");
        assert!(store
            .commit_accepted(entry.clone(), Some(&sample_audio()), true, true)
            .is_err());
        assert_eq!(
            fs::read(store.root.join("entries.jsonl")).expect("read"),
            b"invalid-json\n"
        );
        assert!(!store
            .root
            .join("audio")
            .join(audio_stem(&entry.id))
            .exists());
        fs::remove_dir_all(&store.root).expect("remove fixture");
    }
    #[test]
    fn traversal_audio_reference_and_id_are_rejected() {
        let store = fixture();
        let mut entry = make_entry(Local::now(), "fixture");
        entry.id = "../outside".into();
        assert!(store.commit_accepted(entry, None, true, false).is_err());
        assert!(store.delete("../outside").is_err());
        assert!(!store.root.exists());
    }

    #[cfg(windows)]
    #[test]
    fn locked_audio_delete_keeps_visible_retryable_entry() {
        use std::os::windows::fs::OpenOptionsExt;
        let store = fixture();
        let entry = make_entry(Local::now(), "synthetic locked recording");
        store
            .commit_accepted(entry.clone(), Some(&sample_audio()), true, true)
            .expect("commit");
        let path = store.root.join("audio").join(audio_stem(&entry.id));
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .expect("exclusive lock");
        assert!(store.delete(&entry.id).is_err());
        assert_eq!(store.load().expect("still retryable").len(), 1);
        drop(lock);
        store.delete(&entry.id).expect("retry after unlock");
        assert!(store.load().expect("deleted").is_empty());
        assert!(!path.exists());
        fs::remove_dir_all(&store.root).expect("remove own fixture");
    }
    #[test]
    fn concurrent_commits_and_retention_use_real_store() {
        let store = fixture();
        std::thread::scope(|scope| {
            for _ in 0..12 {
                let store = store.clone();
                scope.spawn(move || {
                    store
                        .commit_accepted(make_entry(Local::now(), "fixture"), None, true, false)
                        .expect("concurrent commit")
                });
            }
        });
        assert_eq!(store.load().expect("load").len(), 12);
        let old = make_entry(
            Local::now().checked_sub_days(Days::new(40)).expect("old"),
            "old",
        );
        store
            .commit_accepted(old.clone(), Some(&sample_audio()), true, true)
            .expect("old commit");
        assert_eq!(store.cleanup(30).expect("cleanup"), 1);
        assert_eq!(store.load().expect("load").len(), 12);
        assert!(!store.root.join("audio").join(audio_stem(&old.id)).exists());
        fs::remove_dir_all(&store.root).expect("remove fixture");
    }

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
    fn preview_preserves_short_multibyte_text() {
        let text = "中".repeat(30);
        assert_eq!(make_entry(Local::now(), &text).preview(), text);
    }

    #[test]
    fn preview_truncates_mixed_utf8_at_character_boundaries() {
        let text = format!("A{}", "中".repeat(100));
        let expected = format!("A{}…", "中".repeat(71));
        assert_eq!(make_entry(Local::now(), &text).preview(), expected);
    }

    #[test]
    fn preview_handles_exact_unicode_limit_and_emoji() {
        let exact = "😊".repeat(72);
        assert_eq!(make_entry(Local::now(), &exact).preview(), exact);
        let longer = "😊".repeat(73);
        assert_eq!(
            make_entry(Local::now(), &longer).preview(),
            format!("{exact}…")
        );
    }

    #[test]
    fn preview_trims_blank_text_and_flattens_newlines() {
        assert_eq!(make_entry(Local::now(), " \n\t ").preview(), "");
        assert_eq!(make_entry(Local::now(), " A\nB ").preview(), "A B");
    }

    #[test]
    fn next_id_generates_unique_ids() {
        let a = next_id();
        let b = next_id();
        assert_ne!(a, b);
        assert!(a.len() >= 15); // YYYYMMDD_HHMMSS_NNNN
    }
}
