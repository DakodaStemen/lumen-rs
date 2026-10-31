//! Symbolic icons from the desktop's icon theme, drawn in the text colour like GTK draws them.
//!
//! Each icon is looked up in the user's icon theme, the themes it inherits, then Adwaita. A
//! copy of the Adwaita icon ships in `assets/icons` for systems without one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use eframe::egui::{self, Color32, Image, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Play,
    Pause,
    Next,
    Previous,
    Shuffle,
    Repeat,
    Fullscreen,
    LeaveFullscreen,
    VolumeHigh,
    VolumeMedium,
    VolumeLow,
    Muted,
    Menu,
    Open,
    Link,
    Subtitles,
    Video,
    Close,
    Check,
    Error,
    SidebarShow,
    GoNext,
    PanDown,
    Search,
    Save,
    ClearAll,
    Undo,
    VideoFile,
    AudioFile,
    Delete,
    Add,
}

macro_rules! bundled {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!("../../assets/icons/", $name, "-symbolic.svg")) as &[u8],
        )
    };
}

impl Icon {
    pub const ALL: [Icon; 31] = [
        Icon::Play,
        Icon::Pause,
        Icon::Next,
        Icon::Previous,
        Icon::Shuffle,
        Icon::Repeat,
        Icon::Fullscreen,
        Icon::LeaveFullscreen,
        Icon::VolumeHigh,
        Icon::VolumeMedium,
        Icon::VolumeLow,
        Icon::Muted,
        Icon::Menu,
        Icon::Open,
        Icon::Link,
        Icon::Subtitles,
        Icon::Video,
        Icon::Close,
        Icon::Check,
        Icon::Error,
        Icon::SidebarShow,
        Icon::GoNext,
        Icon::PanDown,
        Icon::Search,
        Icon::Save,
        Icon::ClearAll,
        Icon::Undo,
        Icon::VideoFile,
        Icon::AudioFile,
        Icon::Delete,
        Icon::Add,
    ];

    /// The freedesktop icon name, without `-symbolic`, and the bundled Adwaita copy.
    fn source(self) -> (&'static str, &'static [u8]) {
        match self {
            Icon::Play => bundled!("media-playback-start"),
            Icon::Pause => bundled!("media-playback-pause"),
            Icon::Next => bundled!("media-skip-forward"),
            Icon::Previous => bundled!("media-skip-backward"),
            Icon::Shuffle => bundled!("media-playlist-shuffle"),
            Icon::Repeat => bundled!("media-playlist-repeat"),
            Icon::Fullscreen => bundled!("view-fullscreen"),
            Icon::LeaveFullscreen => bundled!("view-restore"),
            Icon::VolumeHigh => bundled!("audio-volume-high"),
            Icon::VolumeMedium => bundled!("audio-volume-medium"),
            Icon::VolumeLow => bundled!("audio-volume-low"),
            Icon::Muted => bundled!("audio-volume-muted"),
            Icon::Menu => bundled!("open-menu"),
            Icon::Open => bundled!("document-open"),
            Icon::Link => bundled!("insert-link"),
            Icon::Subtitles => bundled!("media-view-subtitles"),
            Icon::Video => bundled!("video-display"),
            Icon::Close => bundled!("window-close"),
            Icon::Check => bundled!("object-select"),
            Icon::Error => bundled!("dialog-error"),
            Icon::SidebarShow => bundled!("sidebar-show"),
            Icon::GoNext => bundled!("go-next"),
            Icon::PanDown => bundled!("pan-down"),
            Icon::Search => bundled!("system-search"),
            Icon::Save => bundled!("document-save"),
            Icon::ClearAll => bundled!("edit-clear-all"),
            Icon::Undo => bundled!("edit-undo"),
            Icon::VideoFile => bundled!("video-x-generic"),
            Icon::AudioFile => bundled!("audio-x-generic"),
            Icon::Delete => bundled!("edit-delete"),
            Icon::Add => bundled!("list-add"),
        }
    }

    pub fn name(self) -> &'static str {
        self.source().0
    }
}

/// The icon at `size` points, tinted `color`.
pub fn image(icon: Icon, size: f32, color: Color32) -> Image<'static> {
    Image::from_bytes(format!("bytes://icon/{}.svg", icon.name()), svg(icon))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color)
}

/// Paints the icon centred in `rect`.
pub fn paint(ui: &egui::Ui, icon: Icon, rect: egui::Rect, size: f32, color: Color32) {
    let square = egui::Rect::from_center_size(rect.center(), Vec2::splat(size));
    image(icon, size, color).paint_at(ui, square);
}

/// The recoloured SVG for `icon`, loaded once.
fn svg(icon: Icon) -> &'static [u8] {
    static CACHE: OnceLock<Mutex<HashMap<Icon, &'static [u8]>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    cache.entry(icon).or_insert_with(|| {
        let (name, fallback) = icon.source();
        let from_theme = theme_index()
            .get(&format!("{name}-symbolic.svg"))
            .and_then(|path| std::fs::read_to_string(path).ok());
        let text = from_theme.unwrap_or_else(|| String::from_utf8_lossy(fallback).into_owned());
        // A handful of small icons, kept for the life of the process.
        Box::leak(whiten(&text).into_bytes().into_boxed_slice())
    })
}

/// Makes every colour in a symbolic icon white so a tint gives it any colour, keeping opacity.
pub fn whiten(svg: &str) -> String {
    const PREFIXES: [&str; 6] = [
        "fill=\"#",
        "stroke=\"#",
        "fill:#",
        "stroke:#",
        "stop-color:#",
        "stop-color=\"#",
    ];
    let mut out = String::with_capacity(svg.len() + 16);
    let mut rest = svg;
    'scan: while !rest.is_empty() {
        for prefix in PREFIXES {
            if let Some(after) = rest.strip_prefix(prefix) {
                let hex = after
                    .bytes()
                    .take_while(u8::is_ascii_hexdigit)
                    .take(8)
                    .count();
                if matches!(hex, 3 | 4 | 6 | 8) {
                    out.push_str(prefix);
                    out.push_str("ffffff");
                    rest = &after[hex..];
                    continue 'scan;
                }
            }
        }
        let next = rest.chars().next().map_or(1, char::len_utf8);
        out.push_str(&rest[..next]);
        rest = &rest[next..];
    }
    // Shapes without a fill default to black; give the root a white one they inherit.
    if let Some(start) = out.find("<svg") {
        let tag_end = out[start..].find('>').map_or(out.len(), |i| start + i);
        if !out[start..tag_end].contains(" fill=") {
            out.insert_str(start + 4, " fill=\"#ffffff\"");
        }
    }
    out
}

/// Every `*-symbolic.svg` in the user's icon theme and its fallbacks, by file name. Themes
/// earlier in the search order win.
fn theme_index() -> &'static HashMap<String, PathBuf> {
    static INDEX: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        if cfg!(any(target_os = "windows", target_os = "macos")) {
            return index;
        }
        let roots = icon_roots();
        let mut themes: Vec<String> = Vec::new();
        let mut queue: Vec<String> = current_theme().into_iter().collect();
        queue.push("Adwaita".to_owned());
        while let Some(theme) = queue.first().cloned() {
            queue.remove(0);
            if themes.contains(&theme) || theme == "hicolor" || themes.len() > 8 {
                continue;
            }
            for root in &roots {
                let dir = root.join(&theme);
                if let Ok(index_file) = std::fs::read_to_string(dir.join("index.theme")) {
                    queue.extend(inherits(&index_file));
                }
            }
            themes.push(theme);
        }
        for theme in &themes {
            for root in &roots {
                collect(&root.join(theme), 0, &mut index);
            }
        }
        index
    })
}

/// The themes named on an `index.theme`'s `Inherits=` line.
pub fn inherits(index_file: &str) -> Vec<String> {
    index_file
        .lines()
        .find_map(|line| line.trim().strip_prefix("Inherits="))
        .map(|list| {
            list.split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn collect(dir: &Path, depth: u32, index: &mut HashMap<String, PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 3 {
                collect(&path, depth + 1, index);
            }
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.ends_with("-symbolic.svg")
        {
            index.entry(name.to_owned()).or_insert(path);
        }
    }
}

/// Where icon themes live, in freedesktop search order.
fn icon_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut roots = Vec::new();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
    if let Some(dir) = data_home {
        roots.push(dir.join("icons"));
    }
    if let Some(home) = &home {
        roots.push(home.join(".icons"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    roots.extend(
        data_dirs
            .split(':')
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .map(|p| p.join("icons")),
    );
    roots
}

/// The desktop's icon theme, from GNOME's settings.
fn current_theme() -> Option<String> {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let value = String::from_utf8(out.stdout).ok()?;
    let name = value.trim().trim_matches('\'');
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_become_white_and_opacity_stays() {
        let svg = r##"<svg width="16"><path fill="#2e3436" d="M0"/><path style="fill:#222;opacity:.35" fill-opacity="0.34902"/></svg>"##;
        let out = whiten(svg);
        assert!(out.contains(r##"fill="#ffffff" d="M0""##));
        assert!(out.contains("fill:#ffffff;opacity:.35"));
        assert!(out.contains(r#"fill-opacity="0.34902""#));
        assert!(!out.contains("2e3436"));
    }

    #[test]
    fn shapes_without_a_fill_inherit_white() {
        let out = whiten(r#"<svg width="16"><path d="M0"/></svg>"#);
        assert!(out.starts_with(r##"<svg fill="#ffffff" width="16">"##));
        let already = whiten(r##"<svg fill="#000" width="16"/>"##);
        assert_eq!(already.matches("fill=").count(), 1);
    }

    #[test]
    fn element_ids_are_left_alone() {
        let out = whiten(r##"<svg fill="#fff"><use href="#abc"/><path fill="url(#abc)"/></svg>"##);
        assert!(out.contains(r##"href="#abc""##));
        assert!(out.contains("url(#abc)"));
    }

    #[test]
    fn inherited_themes_are_read_from_index_theme() {
        let file = "[Icon Theme]\nName=Papirus\nInherits=breeze-dark, hicolor\n";
        assert_eq!(inherits(file), ["breeze-dark", "hicolor"]);
        assert!(inherits("[Icon Theme]\n").is_empty());
    }

    #[test]
    fn every_bundled_icon_is_a_valid_symbolic_svg() {
        for icon in Icon::ALL {
            let text = std::str::from_utf8(icon.source().1).unwrap();
            assert!(text.contains("<svg"), "{icon:?}");
            assert!(!whiten(text).contains("2e3436"), "{icon:?}");
        }
    }
}
