//! Settings and per-file memory (resume point, tracks, sync offsets, bookmarks).
//!
//! Lumen keeps its own profile in its own settings directory.

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
        crate::platform::config_dir()
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

/// Creates `dir` (0700 on Unix) and tightens it to 0700 if it already exists with looser
/// permissions. A symlink is left alone rather than changing whatever it points at.
fn create_private_dir(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let _ = fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir);
        if let Ok(meta) = fs::symlink_metadata(dir)
            && meta.is_dir()
            && meta.permissions().mode() & 0o077 != 0
        {
            let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
        }
    }
    #[cfg(not(unix))]
    let _ = fs::create_dir_all(dir);
}

/// Writes through a private (0600), freshly created temporary file with a random name,
/// fsynced before the rename, so a crash never leaves half a profile and a planted
/// `settings.json.tmp` symlink cannot redirect the write. The rename replaces `path` itself,
/// never a file a symlink there points at.
fn write_json<T: Serialize>(path: &Path, value: &T) {
    use std::io::Write;
    let Ok(bytes) = serde_json::to_vec_pretty(value) else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    create_private_dir(parent);
    let written = tempfile::Builder::new()
        .prefix(".lumen-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .and_then(|mut temp| {
            temp.write_all(&bytes)?;
            temp.as_file().sync_all()?;
            temp.persist(path).map_err(|e| e.error)
        });
    if let Err(e) = written {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        eprintln!("lumen: could not save {name}: {e}");
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

    #[cfg(unix)]
    #[test]
    fn saving_never_writes_through_planted_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        fs::write(&victim, "untouched").unwrap();
        let profile = dir.path().join("profile");
        fs::create_dir(&profile).unwrap();
        // The old fixed temporary name, and the target itself, both pointing elsewhere.
        std::os::unix::fs::symlink(&victim, profile.join("settings.json.tmp")).unwrap();
        std::os::unix::fs::symlink(&victim, profile.join("settings.json")).unwrap();
        let mut store = Store::open(profile.clone());
        store.settings.volume = 42;
        store.save_settings();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
        let saved = fs::symlink_metadata(profile.join("settings.json")).unwrap();
        assert!(saved.is_file(), "the link is replaced by a real file");
        assert_eq!(Store::open(profile).settings.volume, 42);
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_profile_folder_is_made_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("profile");
        fs::create_dir(&profile).unwrap();
        fs::set_permissions(&profile, fs::Permissions::from_mode(0o755)).unwrap();
        Store::open(profile.clone()).save_settings();
        let mode = fs::metadata(&profile).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let leftovers: Vec<_> = fs::read_dir(&profile)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "no temporary files are left behind");
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
