use directories::ProjectDirs;
use std::path::PathBuf;

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "SpeakType", "SpeakTypeCloud")
}

pub fn data_dir() -> PathBuf {
    project_dirs()
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("data"))
}

pub fn config_path() -> PathBuf {
    project_dirs()
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

pub fn history_path() -> PathBuf {
    data_dir().join("history").join("entries.jsonl")
}

pub fn history_audio_dir() -> PathBuf {
    data_dir().join("history").join("audio")
}

pub fn recordings_dir() -> PathBuf {
    data_dir().join("recordings")
}
