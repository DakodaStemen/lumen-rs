//! Settings and per-file memory (resume point, tracks, sync offsets, bookmarks).
//!
//! Lumen Rust keeps its own profile and never touches the Python version's settings.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const RECENT_LIMIT: usize = 15;
/// Most remembered files kept; the least recently saved are evicted first.
const FILES_LIMIT: usize = 2000;

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
    pub show_queue: bool,
    pub always_on_top: bool,
    /// The folder the file picker opens in.
    pub last_dir: String,
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
            show_queue: true,
            always_on_top: false,
            last_dir: String::new(),
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
    /// Save sequence number used for eviction. Old files.json lacks it (0 = oldest).
    pub saved: u64,
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
        let mut state = self
            .files
            .get(&Self::file_key(source))
            .cloned()
            .unwrap_or_default();
        state.saved = 0;
        state
    }

    pub fn save_file_state(&mut self, source: &str, mut state: FileState) {
        state.saved = self.files.values().map(|f| f.saved).max().unwrap_or(0) + 1;
        self.files.insert(Self::file_key(source), state);
        evict_oldest(&mut self.files, FILES_LIMIT);
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
        let source = redact_source(source);
        self.settings.recent.retain(|s| *s != source);
        self.settings.recent.insert(0, source);
        self.settings.recent.truncate(RECENT_LIMIT);
        self.save_settings();
    }
}

/// Drops userinfo, query and fragment from network URLs so secrets never reach
/// settings.json. Local paths and other strings are returned unchanged.
fn redact_source(source: &str) -> String {
    let lower = source.to_ascii_lowercase();
    let network = [
        "http://", "https://", "rtsp://", "rtsps://", "rtmp://", "rtmps://",
    ];
    let Some(scheme) = network.iter().find(|s| lower.starts_with(**s)) else {
        return source.to_owned();
    };
    let (head, rest) = source.split_at(scheme.len());
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let rest = &rest[..end];
    let (authority, path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    format!("{head}{host}{path}")
}

/// Keeps at most `limit` entries, removing those with the lowest `saved`.
fn evict_oldest(files: &mut BTreeMap<String, FileState>, limit: usize) {
    if files.len() <= limit {
        return;
    }
    let mut order: Vec<(u64, String)> = files.iter().map(|(k, v)| (v.saved, k.clone())).collect();
    order.sort();
    for (_, key) in order.into_iter().take(files.len() - limit) {
        files.remove(&key);
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn create_private_dir(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir);
    }
    #[cfg(not(unix))]
    let _ = fs::create_dir_all(dir);
}

/// Writes through a private (0600) temporary file, fsynced before the rename,
/// so a crash never leaves half a profile.
fn write_json<T: Serialize>(path: &Path, value: &T) {
    use std::io::Write;
    let Ok(bytes) = serde_json::to_vec_pretty(value) else {
        return;
    };
    if let Some(parent) = path.parent() {
        create_private_dir(parent);
    }
    let temp = path.with_extension("json.tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options.open(&temp).and_then(|mut f| {
        f.write_all(&bytes)?;
        f.sync_all()
    });
    if written.is_err() || fs::rename(&temp, path).is_err() {
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

    #[test]
    fn recent_redacts_network_secrets() {
        let mut store = Store::in_memory();
        store.remember_recent("rtsp://user:pass@cam.local:554/live?token=abc#x");
        store.remember_recent("https://u:p@example.com/v.mp4?sig=SECRET");
        store.remember_recent("/home/me/a.mkv?keep");
        assert_eq!(
            store.settings.recent,
            [
                "/home/me/a.mkv?keep",
                "https://example.com/v.mp4",
                "rtsp://cam.local:554/live"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn profile_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("profile");
        let mut store = Store::open(dir.clone());
        store.save_file_state("/a", FileState::default());
        store.save_settings();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir.join("settings.json")), 0o600);
        assert_eq!(mode(&dir.join("files.json")), 0o600);
        assert_eq!(mode(&dir), 0o700);
    }

    #[test]
    fn remembered_files_are_capped_evicting_oldest() {
        let mut store = Store::in_memory();
        for n in 0..FILES_LIMIT + 5 {
            store.save_file_state(
                &format!("/{n}"),
                FileState {
                    position: n as i64 + 1,
                    ..Default::default()
                },
            );
        }
        assert_eq!(store.files.len(), FILES_LIMIT);
        assert_eq!(store.file_state("/0"), FileState::default());
        assert_eq!(store.file_state("/4"), FileState::default());
        assert_eq!(store.file_state("/5").position, 6);
        // Re-saving refreshes recency.
        store.save_file_state("/5", FileState::default());
        store.save_file_state("/new", FileState::default());
        assert!(store.files.contains_key(&Store::file_key("/5")));
        assert!(!store.files.contains_key(&Store::file_key("/6")));
    }

    #[test]
    fn old_files_json_without_saved_field_loads() {
        let dir = tempfile::tempdir().unwrap();
        let key = Store::file_key("/old.mkv");
        fs::write(
            dir.path().join("files.json"),
            format!(r#"{{"{key}": {{"position": 7, "length": 9}}}}"#),
        )
        .unwrap();
        let store = Store::open(dir.path().to_path_buf());
        let state = store.file_state("/old.mkv");
        assert_eq!((state.position, state.length), (7, 9));
    }
}
