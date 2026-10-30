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

/// Schemes a playlist file may point at. Anything else with `://` (smb, edl, lavfi, ftp, ...) is
/// dropped, since mpv would open it as given. `file://` entries are decoded to a local path first.
const PLAYLIST_SCHEMES: [&str; 6] = ["http", "https", "rtsp", "rtmp", "udp", "rtp"];

/// What a line read from a playlist file queues as, or `None` when it is refused.
///
/// Playlists are untrusted input, so unlike sources the user opens directly they may only name
/// local paths (relative ones resolve against `base`, the playlist's folder) or network links
/// with an allowed scheme to a host that is not this machine or its local network. The check
/// runs on the final value: `file://` is decoded first, so `file://lavfi%3A%2F%2Fsine` cannot
/// smuggle a protocol past it.
fn playlist_entry(line: &str, base: &Path) -> Option<String> {
    let is_file_url = line
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("file://"));
    let value = local_path(line);
    if is_url(&value) {
        // A decoded `file://` value must be a plain path, never another link.
        if is_file_url || !network_entry_allowed(&value) {
            return None;
        }
        return Some(value);
    }
    if is_share_path(&value) {
        return None;
    }
    let path = Path::new(&value);
    let resolved = if path.is_absolute() {
        value
    } else {
        base.join(path).to_string_lossy().into_owned()
    };
    (!is_url(&resolved) && !is_share_path(&resolved)).then_some(resolved)
}

/// `\\host\share`, `//host/share` and Windows device paths (`\\?\`, `\\.\`) reach other
/// machines or devices, so a playlist may not name them.
fn is_share_path(path: &str) -> bool {
    let mut chars = path.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some('/' | '\\'), Some('/' | '\\'))
    )
}

/// An allowed scheme, and a host that is not loopback, link-local or a private address.
fn network_entry_allowed(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let scheme = scheme.to_ascii_lowercase();
    PLAYLIST_SCHEMES.contains(&scheme.as_str()) && !is_local_host(&host_of(rest))
}

/// The host part of everything after `scheme://`, lower-cased and without user info or port.
fn host_of(rest: &str) -> String {
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = match host_port.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => host_port.split(':').next().unwrap_or(""),
    };
    let host = percent_decode(host).to_ascii_lowercase();
    // Drop an IPv6 zone such as `%eth0`, and the trailing dot of a fully qualified name.
    let host = host.split('%').next().unwrap_or("");
    host.trim_end_matches('.').to_owned()
}

/// Whether a playlist host names this machine or its local network.
///
/// Only names that need no lookup are caught (`localhost`, literal addresses in any form
/// `inet_aton` accepts). A public name that resolves to a private address is not; that needs
/// a resolver and is out of scope here. An empty host (`udp://@:1234`) listens locally rather
/// than connecting anywhere, so it is allowed.
fn is_local_host(host: &str) -> bool {
    use std::net::{IpAddr, Ipv6Addr};
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    if let Ok(ip) = host.parse::<Ipv6Addr>() {
        return is_local_ip(IpAddr::V6(ip));
    }
    parse_ipv4_loose(host).is_some_and(|ip| is_local_ip(IpAddr::V4(ip)))
}

fn is_local_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback()
                || v4.is_unspecified()
                || v4.is_private()
                || v4.is_link_local()
                || a == 0
                || (a == 100 && (64..128).contains(&b)) // carrier-grade NAT, 100.64.0.0/10
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped `::ffff:a.b.c.d` and IPv4-compatible `::a.b.c.d` (which covers `::`
            // and `::1`, as 0.0.0.0 and 0.0.0.1) are judged as the IPv4 address they carry.
            if let Some(v4) = v6.to_ipv4() {
                return is_local_ip(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00 // unique local, fc00::/7
                || (first & 0xffc0) == 0xfe80 // link-local, fe80::/10
                || (first & 0xffc0) == 0xfec0 // deprecated site-local, fec0::/10
        }
    }
}

/// Parses an IPv4 address the way `inet_aton` (and so the network stack mpv uses) does:
/// `127.0.0.1`, `127.1`, `2130706433`, `0x7f.0.0.1` and `0177.0.0.1` all name loopback.
fn parse_ipv4_loose(host: &str) -> Option<std::net::Ipv4Addr> {
    let parts: Vec<u32> = host
        .split('.')
        .map(|part| {
            let (digits, radix) = match part.strip_prefix("0x").or_else(|| part.strip_prefix("0X"))
            {
                Some(hex) => (hex, 16),
                None if part.len() > 1 && part.starts_with('0') => (&part[1..], 8),
                None => (part, 10),
            };
            if digits.is_empty() && radix != 16 {
                return None;
            }
            if digits.is_empty() {
                return Some(0);
            }
            u32::from_str_radix(digits, radix).ok()
        })
        .collect::<Option<_>>()?;
    let (last, head) = parts.split_last()?;
    if head.len() > 3 || head.iter().any(|p| *p > 255) {
        return None;
    }
    let rest_bits = 8 * (4 - head.len() as u32);
    if rest_bits < 32 && *last >> rest_bits != 0 {
        return None;
    }
    let value = head
        .iter()
        .fold(0u32, |acc, p| (acc << 8) | p)
        .checked_shl(rest_bits)
        .unwrap_or(0)
        | last;
    Some(std::net::Ipv4Addr::from(value))
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
    match source
        .get(..7)
        .filter(|p| p.eq_ignore_ascii_case("file://"))
        .map(|_| &source[7..])
    {
        Some(rest) => percent_decode(rest.strip_prefix("localhost").unwrap_or(rest)),
        None => source.to_owned(),
    }
}

pub fn title_of(source: &str) -> String {
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
                let resolved = fs::canonicalize(&source)
                    .map(strip_verbatim)
                    .unwrap_or_else(|_| PathBuf::from(&source));
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
                    .filter_map(|line| playlist_entry(line, &base))
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

    pub fn clear(&mut self) {
        self.items.clear();
        self.current = None;
    }

    /// The queue as an M3U playlist that `add` can read back.
    pub fn to_m3u(&self) -> String {
        let mut text = String::from("#EXTM3U\n");
        for item in &self.items {
            text.push_str(&item.source);
            text.push('\n');
        }
        text
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

/// Windows `canonicalize` yields a `\\?\C:\..` verbatim path; drop the prefix so paths stay plain.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => path,
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
        // macOS temp dirs sit behind a /var -> /private/var symlink
        let dir_path = strip_verbatim(fs::canonicalize(dir.path()).unwrap());
        let list = dir_path.join("list.m3u");
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
            dir_path.join("song.mp3").to_string_lossy()
        );
        assert_eq!(queue.items[1].source, "https://x.test/live.m3u8");
    }

    #[test]
    fn playlist_files_only_queue_allowed_schemes() {
        let dir = tempfile::tempdir().unwrap();
        // macOS temp dirs sit behind a /var -> /private/var symlink
        let dir_path = strip_verbatim(fs::canonicalize(dir.path()).unwrap());
        let list = dir_path.join("evil.m3u");
        let abs = dir_path
            .join("abs")
            .join("c.mp3")
            .to_string_lossy()
            .into_owned();
        fs::write(
            &list,
            format!(
                "#EXTM3U\nsmb://host/share/x.mkv\nedl://%5%http://a;!new_stream\nlavfi://sine\n\
             ftp://h/x.mp3\nFILE:///tmp/ok.mp3\n  HTTP://x.test/a.mp3  \nrtsp://cam/s\n\
             Rtmp://l/app\nudp://@:1\nrtp://239.0.0.1\nhttps://x.test/b.mp3\nrel.mp3\n{abs}\n\
             fd://3\nmemory://x\ncdda://\ndvd://1\n"
            ),
        )
        .unwrap();
        let mut queue = Queue::default();
        queue.add(&[list.to_string_lossy().into_owned()]);
        let sources: Vec<&str> = queue.items.iter().map(|i| i.source.as_str()).collect();
        let rel = dir_path.join("rel.mp3").to_string_lossy().into_owned();
        assert_eq!(
            sources,
            [
                "/tmp/ok.mp3",
                "HTTP://x.test/a.mp3",
                "rtsp://cam/s",
                "Rtmp://l/app",
                "udp://@:1",
                "rtp://239.0.0.1",
                "https://x.test/b.mp3",
                rel.as_str(),
                abs.as_str(),
            ]
        );
    }

    #[test]
    fn decoded_file_urls_cannot_smuggle_other_protocols() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = strip_verbatim(fs::canonicalize(dir.path()).unwrap());
        let list = dir_path.join("smuggle.m3u");
        fs::write(
            &list,
            "#EXTM3U\nfile://lavfi%3A%2F%2Fsine\nfile://smb%3A%2F%2Fhost%2Fshare%2Fx.mkv\n\
             file://edl%3A%2F%2F%255%25http%3A%2F%2Fa\nFILE://ftp%3A%2F%2Fh%2Fx.mp3\n\
             file:///lavfi%3A%2F%2Fsine\nfile://file%3A%2F%2Flavfi%253A%252F%252Fsine\n\
             fIlE://localhost/smb%3A%2F%2Fh%2Fx\nfile:////host/share/x.mkv\n\
             file://%5C%5Chost%5Cshare%5Cx.mkv\nfile:///tmp/fine%20one.mp3\n",
        )
        .unwrap();
        let mut queue = Queue::default();
        queue.add(&[list.to_string_lossy().into_owned()]);
        let sources: Vec<&str> = queue.items.iter().map(|i| i.source.as_str()).collect();
        assert_eq!(sources, ["/tmp/fine one.mp3"]);
    }

    #[test]
    fn playlist_files_cannot_name_network_shares() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = strip_verbatim(fs::canonicalize(dir.path()).unwrap());
        let list = dir_path.join("unc.m3u");
        fs::write(
            &list,
            "\\\\host\\share\\x.mkv\n//host/share/x.mkv\n\\\\?\\UNC\\host\\s\\x.mkv\n\
             \\\\.\\pipe\\x\n/\\host\\share\\x.mkv\n\\/host/x.mkv\nok.mp3\n",
        )
        .unwrap();
        let mut queue = Queue::default();
        queue.add(&[list.to_string_lossy().into_owned()]);
        let sources: Vec<&str> = queue.items.iter().map(|i| i.source.as_str()).collect();
        let ok = dir_path.join("ok.mp3").to_string_lossy().into_owned();
        assert_eq!(sources, [ok.as_str()]);
    }

    #[test]
    fn playlist_links_cannot_reach_this_machine_or_its_network() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = strip_verbatim(fs::canonicalize(dir.path()).unwrap());
        let list = dir_path.join("lan.m3u");
        let refused = [
            "http://localhost/a.mp3",
            "http://LOCALHOST.:8080/a.mp3",
            "http://api.localhost/a.mp3",
            "http://127.0.0.1/a.mp3",
            "http://127.1/a.mp3",
            "http://2130706433/a.mp3",
            "http://0x7f.0.0.1/a.mp3",
            "http://0177.0.0.1/a.mp3",
            "http://user:pw@127.0.0.1:9/a.mp3",
            "http://%31%32%37.0.0.1/a.mp3",
            "http://0.0.0.0:8000/",
            "https://10.0.0.5/a.mp3",
            "rtsp://172.16.3.4/cam",
            "rtsp://192.168.1.10:554/cam",
            "http://169.254.169.254/latest/meta-data",
            "http://100.64.0.1/a.mp3",
            "http://[::1]/a.mp3",
            "http://[::]/a.mp3",
            "http://[::ffff:127.0.0.1]/a.mp3",
            "http://[::ffff:192.168.0.1]/a.mp3",
            "http://[fd00::1]/a.mp3",
            "http://[fe80::1%25eth0]/a.mp3",
            "udp://127.0.0.1:1234",
            "rtmp://192.168.0.2/app",
        ];
        let allowed = [
            "https://x.test/a.mp3",
            "http://8.8.8.8/a.mp3",
            "http://[2001:db8::1]/a.mp3",
            "http://172.32.0.1/a.mp3",
            "rtp://239.0.0.1",
            "udp://@:1234",
            "https://127.0.0.1.example.com/a.mp3",
        ];
        let text: String = refused
            .iter()
            .chain(allowed.iter())
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(&list, text).unwrap();
        let mut queue = Queue::default();
        queue.add(&[list.to_string_lossy().into_owned()]);
        let sources: Vec<&str> = queue.items.iter().map(|i| i.source.as_str()).collect();
        assert_eq!(sources, allowed);
    }

    #[test]
    fn direct_links_to_local_hosts_stay_allowed() {
        let queue = queue_of(&["http://127.0.0.1:8080/live.ts", "file:///tmp/a.mkv"]);
        assert_eq!(
            queue.items.len(),
            2,
            "a link the user typed is theirs to open"
        );
    }

    #[test]
    fn loose_ipv4_forms_parse_like_inet_aton() {
        let ip = |s: &str| parse_ipv4_loose(s).map(|a| a.to_string());
        assert_eq!(ip("127.0.0.1").as_deref(), Some("127.0.0.1"));
        assert_eq!(ip("127.1").as_deref(), Some("127.0.0.1"));
        assert_eq!(ip("10.1.2").as_deref(), Some("10.1.0.2"));
        assert_eq!(ip("3232235521").as_deref(), Some("192.168.0.1"));
        assert_eq!(ip("0xc0.0250.1").as_deref(), Some("192.168.0.1"));
        assert_eq!(ip("1.2.3.4.5"), None);
        assert_eq!(ip("256.1.1.1"), None);
        assert_eq!(ip("1.2.3.256"), None);
        assert_eq!(ip("08.1.1.1"), None);
        assert_eq!(ip("x.test"), None);
        assert_eq!(ip(""), None);
    }

    #[test]
    fn direct_sources_are_not_filtered() {
        let queue = queue_of(&["smb://h/s/x.mkv", "/a.mkv"]);
        assert_eq!(queue.items.len(), 2, "command-line arguments stay as given");
    }

    #[test]
    fn nested_playlists_are_filtered_too() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner.m3u");
        fs::write(&inner, "lavfi://x\nok.mp3\n").unwrap();
        let outer = dir.path().join("outer.m3u");
        fs::write(&outer, "inner.m3u\nsmb://h/x\n").unwrap();
        let mut queue = Queue::default();
        queue.add(&[outer.to_string_lossy().into_owned()]);
        assert_eq!(queue.items.len(), 1);
        assert!(queue.items[0].source.ends_with("ok.mp3"));
    }

    #[test]
    fn saved_queues_read_back_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a b.mkv").to_string_lossy().into_owned();
        let c = dir.path().join("c.mp3").to_string_lossy().into_owned();
        let queue = queue_of(&[&a, "https://x.test/live.m3u8", &c]);
        let list = dir.path().join("saved.m3u");
        fs::write(&list, queue.to_m3u()).unwrap();
        let mut restored = Queue::default();
        restored.add(&[list.to_string_lossy().into_owned()]);
        assert_eq!(restored.items, queue.items);
    }

    #[test]
    fn clearing_empties_the_queue() {
        let mut queue = queue_of(&["/a", "/b"]);
        queue.current = Some(1);
        queue.clear();
        assert!(queue.items.is_empty());
        assert_eq!(queue.current, None);
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
