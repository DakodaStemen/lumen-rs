//! The play queue: what plays next, shuffle and repeat, M3U import and folder continuation.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::clock::natural_key;

pub const MEDIA_EXT: [&str; 25] = [
    "mp4", "mkv", "webm", "avi", "mov", "m4v", "wmv", "flv", "ts", "m2ts", "mpg", "mpeg", "ogv",
    "3gp", "mp3", "flac", "wav", "ogg", "oga", "opus", "m4a", "aac", "wma", "alac", "ape",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub source: String,
    pub title: String,
}

#[derive(Debug, Default)]
pub struct Queue {
    pub items: Vec<Item>,
    pub current: Option<usize>,
    pub shuffle: bool,
    pub repeat: bool,
}

pub fn is_url(source: &str) -> bool {
    source.contains("://")
}

fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXT.contains(&e.to_lowercase().as_str()))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Turns `file:///a%20b.mkv` into `/a b.mkv`; anything else is returned unchanged.
fn local_path(source: &str) -> String {
    match source.strip_prefix("file://") {
        Some(rest) => percent_decode(rest.strip_prefix("localhost").unwrap_or(rest)),
        None => source.to_owned(),
    }
}

fn title_of(source: &str) -> String {
    let name = if is_url(source) {
        let without_query = source.split(['?', '#']).next().unwrap_or(source);
        // Only the path names the file; a bare host such as `rtsp://cam` has no file name.
        let path = without_query
            .split_once("://")
            .map_or("", |(_, rest)| rest.find('/').map_or("", |i| &rest[i..]));
        percent_decode(path.rsplit('/').next().unwrap_or(""))
    } else {
        Path::new(source)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    if name.is_empty() {
        source.to_owned()
    } else {
        name
    }
}

/// The file that naturally follows `source` in its folder, if any.
pub fn next_in_folder(source: &str) -> Option<PathBuf> {
    if is_url(source) {
        return None;
    }
    let path = Path::new(source);
    let name = natural_key(&path.file_name()?.to_string_lossy());
    fs::read_dir(path.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && is_media(p))
        .map(|p| {
            (
                natural_key(&p.file_name().unwrap_or_default().to_string_lossy()),
                p,
            )
        })
        .filter(|(key, _)| *key > name)
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, p)| p)
}

impl Queue {
    pub fn source(&self, index: usize) -> Option<&str> {
        self.items.get(index).map(|i| i.source.as_str())
    }

    pub fn current_source(&self) -> Option<&str> {
        self.source(self.current?)
    }

    /// Appends sources, expanding M3U playlists. Returns the index of the first new item.
    pub fn add(&mut self, sources: &[String]) -> Option<usize> {
        let first = self.items.len();
        self.add_all(sources, &mut HashSet::new());
        (self.items.len() > first).then_some(first)
    }

    fn add_all(&mut self, sources: &[String], visited: &mut HashSet<PathBuf>) {
        for source in sources {
            let source = local_path(source);
            let lower = source.to_lowercase();
            if (lower.ends_with(".m3u") || lower.ends_with(".m3u8")) && Path::new(&source).is_file()
            {
                let resolved = fs::canonicalize(&source).unwrap_or_else(|_| PathBuf::from(&source));
                if !visited.insert(resolved.clone()) {
                    continue;
                }
                let Ok(text) = fs::read_to_string(&resolved) else {
                    continue;
                };
                let base = resolved.parent().map(Path::to_path_buf).unwrap_or_default();
                let entries: Vec<String> = text
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(|line| {
                        if is_url(line) || Path::new(line).is_absolute() {
                            line.to_owned()
                        } else {
                            base.join(line).to_string_lossy().into_owned()
                        }
                    })
                    .collect();
                self.add_all(&entries, visited);
                continue;
            }
            self.items.push(Item {
                title: title_of(&source),
                source,
            });
        }
    }

    /// Removes an item, keeping `current` pointing at the same entry.
    pub fn remove(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.items.remove(index);
        self.current = match self.current {
            Some(c) if c == index => None,
            Some(c) if c > index => Some(c - 1),
            other => other,
        };
    }

    /// The index to play `delta` entries away from the current one, honouring shuffle and
    /// repeat. At the end of the queue the next file in the folder is appended when allowed.
    pub fn advance(&mut self, delta: i64, folder_next: bool) -> Option<usize> {
        let count = self.items.len();
        if count == 0 {
            return None;
        }
        if self.shuffle && count > 1 {
            let others: Vec<usize> = (0..count).filter(|i| Some(*i) != self.current).collect();
            return Some(others[fastrand::usize(..others.len())]);
        }
        let current = self.current.map_or(-1, |c| c as i64);
        let mut target = current + delta;
        if self.repeat {
            target = target.rem_euclid(count as i64);
        }
        if target >= count as i64 && folder_next {
            let sibling = self.current_source().and_then(next_in_folder)?;
            self.add(&[sibling.to_string_lossy().into_owned()]);
            return Some(count);
        }
        usize::try_from(target)
            .ok()
            .filter(|t| *t < self.items.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue_of(names: &[&str]) -> Queue {
        let mut queue = Queue::default();
        queue.add(&names.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        queue
    }

    #[test]
    fn titles_come_from_file_names_and_urls() {
        let queue = queue_of(&[
            "/media/a b.mkv",
            "https://x.test/v/clip%20one.mp4?t=1",
            "rtsp://cam",
        ]);
        let titles: Vec<_> = queue.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["a b.mkv", "clip one.mp4", "rtsp://cam"]);
    }

    #[test]
    fn file_urls_become_paths() {
        let queue = queue_of(&["file:///tmp/a%20b.mkv"]);
        assert_eq!(queue.items[0].source, "/tmp/a b.mkv");
    }

    #[test]
    fn advances_and_stops_at_the_ends() {
        let mut queue = queue_of(&["/a.mkv", "/b.mkv"]);
        assert_eq!(
            queue.advance(1, false),
            Some(0),
            "nothing playing yet starts at the top"
        );
        queue.current = Some(0);
        assert_eq!(queue.advance(1, false), Some(1));
        queue.current = Some(1);
        assert_eq!(queue.advance(1, false), None);
        assert_eq!(queue.advance(-1, false), Some(0));
        queue.current = Some(0);
        assert_eq!(queue.advance(-1, false), None);
    }

    #[test]
    fn repeat_wraps_in_both_directions() {
        let mut queue = queue_of(&["/a.mkv", "/b.mkv", "/c.mkv"]);
        queue.repeat = true;
        queue.current = Some(2);
        assert_eq!(queue.advance(1, false), Some(0));
        queue.current = Some(0);
        assert_eq!(queue.advance(-1, false), Some(2));
    }

    #[test]
    fn shuffle_never_repeats_the_current_item() {
        let mut queue = queue_of(&["/a", "/b", "/c"]);
        queue.shuffle = true;
        queue.current = Some(1);
        for _ in 0..50 {
            assert_ne!(queue.advance(1, false), Some(1));
        }
    }

    #[test]
    fn continues_with_the_next_file_in_natural_order() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["ep1.mp4", "ep2.mp4", "ep10.mp4", "notes.txt"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        let ep2 = dir.path().join("ep2.mp4").to_string_lossy().into_owned();
        let mut queue = queue_of(&[&ep2]);
        queue.current = Some(0);
        assert_eq!(queue.advance(1, true), Some(1));
        assert!(
            queue.items[1].source.ends_with("ep10.mp4"),
            "ep2 is followed by ep10, not ep1"
        );
        queue.current = Some(1);
        assert_eq!(queue.advance(1, true), None, "ep10 is last in its folder");
    }

    #[test]
    fn folder_continuation_can_be_disabled() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.mp4", "b.mp4"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        let a = dir.path().join("a.mp4").to_string_lossy().into_owned();
        let mut queue = queue_of(&[&a]);
        queue.current = Some(0);
        assert_eq!(queue.advance(1, false), None);
    }

    #[test]
    fn imports_m3u_relative_to_the_playlist_and_survives_cycles() {
        let dir = tempfile::tempdir().unwrap();
        let list = dir.path().join("list.m3u");
        fs::write(
            &list,
            "#EXTM3U\nsong.mp3\nhttps://x.test/live.m3u8\n\nlist.m3u\n",
        )
        .unwrap();
        let mut queue = Queue::default();
        let first = queue.add(&[list.to_string_lossy().into_owned()]);
        assert_eq!(first, Some(0));
        assert_eq!(queue.items.len(), 2);
        assert_eq!(
            queue.items[0].source,
            dir.path().join("song.mp3").to_string_lossy()
        );
        assert_eq!(queue.items[1].source, "https://x.test/live.m3u8");
    }

    #[test]
    fn removing_keeps_current_pointing_at_the_same_item() {
        let mut queue = queue_of(&["/a", "/b", "/c"]);
        queue.current = Some(2);
        queue.remove(0);
        assert_eq!(queue.current, Some(1));
        assert_eq!(queue.current_source(), Some("/c"));
        queue.remove(1);
        assert_eq!(queue.current, None);
    }
}
