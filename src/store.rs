//! Settings and per-file memory (resume point, tracks, sync offsets, bookmarks).
//!
//! Lumen Rust keeps its own profile and never touches the Python version's settings.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const RECENT_LIMIT: usize = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: u32,
    /// Seconds moved by the seek keys.
    pub seek_step: u32,
    pub resume: bool,
    pub remember_tracks: bool,
    pub auto_next_folder: bool,
    /// Streamlink quality: `best`, or a name such as `720p60`.
    pub stream_quality: String,
    pub low_latency: bool,
    pub sponsorblock: bool,
    /// Also skip intros and outros, not just sponsor reads.
    pub sponsorblock_intros: bool,
    /// Key bindings by action id. An empty string unbinds the action.
    pub keys: BTreeMap<String, String>,
    pub recent: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 80,
            seek_step: 10,
            resume: true,
            remember_tracks: true,
            auto_next_folder: true,
            stream_quality: "best".to_owned(),
            low_latency: true,
            sponsorblock: true,
            sponsorblock_intros: false,
            keys: BTreeMap::new(),
            recent: Vec::new(),
        }
    }
}

/// What Lumen remembers about one file. Times are milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileState {
    pub position: i64,
    pub length: i64,
    pub audio: Option<i64>,
    /// Subtitle track id, or -1 when subtitles were switched off.
    pub spu: Option<i64>,
    pub audio_delay: i64,
    pub spu_delay: i64,
    pub bookmarks: Vec<i64>,
    pub subtitles: Vec<String>,
}

pub struct Store {
    dir: Option<PathBuf>,
    pub settings: Settings,
    files: BTreeMap<String, FileState>,
}

impl Store {
    /// A store that is never written to disk.
    pub fn in_memory() -> Self {
        Self {
            dir: None,
            settings: Settings::default(),
            files: BTreeMap::new(),
        }
    }

    /// Opens the profile in `dir`, starting empty when files are missing or unreadable.
    pub fn open(dir: PathBuf) -> Self {
        let settings = read_json(&dir.join("settings.json")).unwrap_or_default();
        let files = read_json(&dir.join("files.json")).unwrap_or_default();
        Self {
            dir: Some(dir),
            settings,
            files,
        }
    }

    pub fn default_dir() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("lumen-rs")
    }

    fn file_key(source: &str) -> String {
        let digest = Sha256::digest(source.as_bytes());
        digest.iter().take(10).map(|b| format!("{b:02x}")).collect()
    }

    pub fn file_state(&self, source: &str) -> FileState {
        self.files
            .get(&Self::file_key(source))
            .cloned()
            .unwrap_or_default()
    }

    pub fn save_file_state(&mut self, source: &str, state: FileState) {
        self.files.insert(Self::file_key(source), state);
        if let Some(dir) = &self.dir {
            write_json(&dir.join("files.json"), &self.files);
        }
    }

    pub fn save_settings(&self) {
        if let Some(dir) = &self.dir {
            write_json(&dir.join("settings.json"), &self.settings);
        }
    }

    pub fn remember_recent(&mut self, source: &str) {
        self.settings.recent.retain(|s| s != source);
        self.settings.recent.insert(0, source.to_owned());
        self.settings.recent.truncate(RECENT_LIMIT);
        self.save_settings();
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Writes through a temporary file so a crash never leaves half a profile.
fn write_json<T: Serialize>(path: &Path, value: &T) {
    let Ok(bytes) = serde_json::to_vec_pretty(value) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let temp = path.with_extension("json.tmp");
    if fs::write(&temp, bytes).is_ok() && fs::rename(&temp, path).is_err() {
        let _ = fs::remove_file(&temp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_state_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().to_path_buf());
        let state = FileState {
            position: 12_000,
            length: 30_000,
            spu: Some(-1),
            bookmarks: vec![5_000],
            ..Default::default()
        };
        store.save_file_state("/a.mkv", state.clone());
        store.settings.volume = 120;
        store.remember_recent("/a.mkv");

        let reopened = Store::open(dir.path().to_path_buf());
        assert_eq!(reopened.file_state("/a.mkv"), state);
        assert_eq!(reopened.file_state("/other.mkv"), FileState::default());
        assert_eq!(reopened.settings.volume, 120);
        assert_eq!(reopened.settings.recent, ["/a.mkv"]);
    }

    #[test]
    fn recent_list_is_deduplicated_and_capped() {
        let mut store = Store::in_memory();
        for n in 0..20 {
            store.remember_recent(&format!("/{n}"));
        }
        store.remember_recent("/10");
        assert_eq!(store.settings.recent.len(), RECENT_LIMIT);
        assert_eq!(store.settings.recent[0], "/10");
        assert_eq!(
            store.settings.recent.iter().filter(|s| *s == "/10").count(),
            1
        );
    }

    #[test]
    fn corrupt_profile_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.json"), "{ not json").unwrap();
        fs::write(dir.path().join("files.json"), "[]").unwrap();
        let store = Store::open(dir.path().to_path_buf());
        assert_eq!(store.settings.volume, 80);
    }
}
