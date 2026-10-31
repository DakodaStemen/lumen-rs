//! What the desktop says Lumen should look like: light or dark, the accent colour and the UI font.
//!
//! On Linux and the BSDs the freedesktop portal (`org.freedesktop.appearance`) is asked first,
//! with `gsettings` as a fallback, and the portal is watched so a change in the desktop's
//! settings shows up right away. Elsewhere, egui's own report of the system theme is followed
//! and the accent is the Adwaita default. Every lookup is best effort: anything missing simply
//! leaves the default in place.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use eframe::egui::{self, Color32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Dark,
    Light,
}

/// The desktop's stated preferences; `None` means it has none.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Appearance {
    pub scheme: Option<Scheme>,
    pub accent: Option<Color32>,
}

/// GNOME's default accent, used when the desktop names none.
pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0x35, 0x84, 0xe4);

impl Appearance {
    /// The light or dark choice to draw with: the desktop's, else the window system's, else dark.
    pub fn resolve(&self, ctx: &egui::Context) -> Scheme {
        scheme_override()
            .or(self.scheme)
            .or_else(|| {
                ctx.system_theme().map(|t| match t {
                    egui::Theme::Dark => Scheme::Dark,
                    egui::Theme::Light => Scheme::Light,
                })
            })
            .unwrap_or(Scheme::Dark)
    }

    pub fn accent(&self) -> Color32 {
        self.accent.unwrap_or(DEFAULT_ACCENT)
    }

    /// Asks the desktop. Never blocks for long: a missing or slow portal falls through.
    pub fn detect() -> Self {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let portal = portal::read().unwrap_or_default();
            let mut found = portal;
            if found.scheme.is_none() || found.accent.is_none() {
                let gs = gsettings_appearance();
                found.scheme = found.scheme.or(gs.scheme);
                found.accent = found.accent.or(gs.accent);
            }
            found
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            Self::default()
        }
    }
}

/// Watches the desktop for appearance changes. Each change arrives as a whole new
/// [`Appearance`] and wakes the interface. `None` where there is nothing to watch.
pub fn watch(ctx: &egui::Context) -> Option<Receiver<Appearance>> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        portal::watch(ctx.clone())
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = ctx;
        None
    }
}

/// `LUMEN_COLOR_SCHEME=light` or `dark` overrides the desktop, for trying the other look
/// without changing the desktop's settings.
pub fn scheme_override() -> Option<Scheme> {
    parse_scheme_override(std::env::var("LUMEN_COLOR_SCHEME").ok()?.as_str())
}

pub fn parse_scheme_override(value: &str) -> Option<Scheme> {
    match value.trim().to_ascii_lowercase().as_str() {
        "dark" => Some(Scheme::Dark),
        "light" => Some(Scheme::Light),
        _ => None,
    }
}

/// The portal's `color-scheme`: 1 prefers dark, 2 prefers light, 0 (or anything else) has no say.
pub fn scheme_from_portal(value: u32) -> Option<Scheme> {
    match value {
        1 => Some(Scheme::Dark),
        2 => Some(Scheme::Light),
        _ => None,
    }
}

/// The portal's `accent-color`, an sRGB triple from 0 to 1; out-of-range values mean "unset".
pub fn accent_from_portal((r, g, b): (f64, f64, f64)) -> Option<Color32> {
    let in_range = |v: f64| (0.0..=1.0).contains(&v);
    if !(in_range(r) && in_range(g) && in_range(b)) {
        return None;
    }
    let byte = |v: f64| (v * 255.0).round() as u8;
    Some(Color32::from_rgb(byte(r), byte(g), byte(b)))
}

/// GNOME's `color-scheme` key: `'prefer-dark'`, `'prefer-light'` or `'default'` (light).
pub fn scheme_from_gsettings(value: &str) -> Option<Scheme> {
    match unquote(value) {
        "prefer-dark" => Some(Scheme::Dark),
        "prefer-light" | "default" => Some(Scheme::Light),
        _ => None,
    }
}

/// GNOME's named accents (`accent-color` key), as GNOME draws them.
pub fn accent_from_name(value: &str) -> Option<Color32> {
    let rgb = match unquote(value) {
        "blue" => 0x3584e4,
        "teal" => 0x2190a4,
        "green" => 0x3a944a,
        "yellow" => 0xc88800,
        "orange" => 0xed5b00,
        "red" => 0xe62d42,
        "pink" => 0xd56199,
        "purple" => 0x9141ac,
        "slate" => 0x6f8396,
        _ => return None,
    };
    Some(Color32::from_rgb(
        (rgb >> 16) as u8,
        (rgb >> 8) as u8,
        rgb as u8,
    ))
}

fn unquote(value: &str) -> &str {
    value.trim().trim_matches(|c| c == '\'' || c == '"').trim()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn gsettings(schema: &str, key: &str) -> Option<String> {
    let out = std::process::Command::new("gsettings")
        .args(["get", schema, key])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn gsettings_appearance() -> Appearance {
    const SCHEMA: &str = "org.gnome.desktop.interface";
    Appearance {
        scheme: gsettings(SCHEMA, "color-scheme").and_then(|v| scheme_from_gsettings(&v)),
        accent: gsettings(SCHEMA, "accent-color").and_then(|v| accent_from_name(&v)),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod portal {
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

    use eframe::egui;
    use zbus::blocking::{Connection, Proxy, connection};
    use zbus::zvariant::{OwnedValue, Value};

    use super::{Appearance, accent_from_portal, scheme_from_portal};

    const NAMESPACE: &str = "org.freedesktop.appearance";

    fn connect() -> zbus::Result<Connection> {
        connection::Builder::session()?
            .method_timeout(Duration::from_millis(400))
            .build()
    }

    fn proxy(conn: &Connection) -> zbus::Result<Proxy<'static>> {
        Proxy::new(
            conn,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )
    }

    /// `ReadOne` returns the value itself; older portals only have `Read`, which wraps it once more.
    fn read_key(proxy: &Proxy<'_>, key: &str) -> Option<OwnedValue> {
        let value: OwnedValue = proxy
            .call("ReadOne", &(NAMESPACE, key))
            .or_else(|_| proxy.call("Read", &(NAMESPACE, key)))
            .ok()?;
        Some(unwrap_variant(value))
    }

    fn unwrap_variant(value: OwnedValue) -> OwnedValue {
        match &*value {
            Value::Value(inner) => inner.try_to_owned().map_or(value, unwrap_variant),
            _ => value,
        }
    }

    fn scheme(value: OwnedValue) -> Option<super::Scheme> {
        u32::try_from(value).ok().and_then(scheme_from_portal)
    }

    fn accent(value: OwnedValue) -> Option<eframe::egui::Color32> {
        <(f64, f64, f64)>::try_from(Value::from(value))
            .ok()
            .and_then(accent_from_portal)
    }

    fn read_with(proxy: &Proxy<'_>) -> Appearance {
        Appearance {
            scheme: read_key(proxy, "color-scheme").and_then(scheme),
            accent: read_key(proxy, "accent-color").and_then(accent),
        }
    }

    pub fn read() -> Option<Appearance> {
        let conn = connect().ok()?;
        Some(read_with(&proxy(&conn).ok()?))
    }

    pub fn watch(ctx: egui::Context) -> Option<Receiver<Appearance>> {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("appearance".to_owned())
            .spawn(move || {
                let Ok(conn) = connect() else { return };
                let Ok(proxy) = proxy(&conn) else { return };
                let Ok(changes) = proxy.receive_signal("SettingChanged") else {
                    return;
                };
                for message in changes {
                    let Ok((namespace, _key, _value)) =
                        message.body().deserialize::<(String, String, OwnedValue)>()
                    else {
                        continue;
                    };
                    if namespace != NAMESPACE {
                        continue;
                    }
                    // Re-read both keys, so one update always carries the whole picture.
                    let mut now = read_with(&proxy);
                    if now.scheme.is_none() || now.accent.is_none() {
                        let gs = super::gsettings_appearance();
                        now.scheme = now.scheme.or(gs.scheme);
                        now.accent = now.accent.or(gs.accent);
                    }
                    if tx.send(now).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            })
            .ok()?;
        Some(rx)
    }
}

/// The desktop's interface font, loaded so egui can draw with it.
#[derive(Debug, Clone)]
pub struct SystemFont {
    pub path: PathBuf,
    /// The face within a collection file.
    pub index: u32,
    /// The size the desktop asks for, in points, when it says.
    pub size_pt: Option<f32>,
}

/// Splits a GNOME font name such as `'Adwaita Sans 11'` into family and size.
pub fn parse_font_name(value: &str) -> Option<(String, Option<f32>)> {
    let value = unquote(value);
    if value.is_empty() {
        return None;
    }
    match value.rsplit_once(char::is_whitespace) {
        Some((family, size)) if size.parse::<f32>().is_ok() => {
            let size = size.parse::<f32>().ok().filter(|s| *s > 0.0);
            Some((family.trim().to_owned(), size))
        }
        _ => Some((value.to_owned(), None)),
    }
}

/// Font files egui can read.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
fn loadable(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "ttf" | "otf" | "ttc" | "otc"
        )
    })
}

/// The desktop's interface and monospace fonts, each when it can be found.
#[derive(Debug, Clone, Default)]
pub struct SystemFonts {
    pub ui: Option<SystemFont>,
    pub mono: Option<SystemFont>,
    /// Fonts for scripts the interface font lacks (CJK, Arabic, Hebrew, Thai, Devanagari),
    /// so names in those scripts draw as text rather than empty boxes.
    pub fallbacks: Vec<SystemFont>,
}

/// Languages whose scripts get a fallback font, as fontconfig language tags.
#[cfg(all(unix, not(target_os = "macos")))]
const FALLBACK_LANGS: [&str; 6] = ["ja", "ko", "ar", "he", "th", "hi"];

/// Finds the interface fonts through GNOME's settings and fontconfig, or the Windows UI font.
pub fn system_fonts() -> SystemFonts {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let lookup = |key: &str, generic: &str| {
            let named =
                gsettings("org.gnome.desktop.interface", key).and_then(|v| parse_font_name(&v));
            let (family, size_pt) = named.unwrap_or_else(|| (generic.to_owned(), None));
            fc_match(&family).map(|(path, index)| SystemFont {
                path,
                index,
                size_pt,
            })
        };
        let ui = lookup("font-name", "sans-serif");
        let mut fallbacks: Vec<SystemFont> = Vec::new();
        for lang in FALLBACK_LANGS {
            let Some((path, index)) = fc_match(&format!("sans-serif:lang={lang}")) else {
                continue;
            };
            let known = |f: &SystemFont| f.path == path && f.index == index;
            if ui.as_ref().is_some_and(known) || fallbacks.iter().any(known) {
                continue;
            }
            fallbacks.push(SystemFont {
                path,
                index,
                size_pt: None,
            });
        }
        SystemFonts {
            ui,
            mono: lookup("monospace-font-name", "monospace"),
            fallbacks,
        }
    }
    #[cfg(target_os = "windows")]
    {
        let root = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
        let font = |name: &str| {
            let path = PathBuf::from(&root).join("Fonts").join(name);
            path.is_file().then_some(SystemFont {
                path,
                index: 0,
                size_pt: None,
            })
        };
        SystemFonts {
            ui: font("segoeui.ttf"),
            mono: font("consola.ttf"),
            // Segoe UI's own fallbacks for East Asian and other scripts.
            fallbacks: ["YuGothM.ttc", "malgun.ttf", "msyh.ttc"]
                .into_iter()
                .filter_map(font)
                .collect(),
        }
    }
    #[cfg(not(any(all(unix, not(target_os = "macos")), target_os = "windows")))]
    {
        SystemFonts::default()
    }
}

/// The file and face fontconfig picks for `family`, if egui can load it.
#[cfg(all(unix, not(target_os = "macos")))]
fn fc_match(family: &str) -> Option<(PathBuf, u32)> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}\n%{index}", family])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let path = PathBuf::from(lines.next()?.trim());
    // fontconfig puts a variable font's named instance in the high bits; egui wants the face.
    let index = lines
        .next()
        .and_then(|i| i.trim().parse::<u32>().ok())
        .unwrap_or(0)
        & 0xffff;
    (loadable(&path) && path.is_file()).then_some((path, index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_values_map_to_schemes_and_accents() {
        assert_eq!(scheme_from_portal(0), None);
        assert_eq!(scheme_from_portal(1), Some(Scheme::Dark));
        assert_eq!(scheme_from_portal(2), Some(Scheme::Light));
        assert_eq!(scheme_from_portal(7), None);
        assert_eq!(
            accent_from_portal((0.902, 0.176, 0.259)),
            Some(Color32::from_rgb(230, 45, 66))
        );
        assert_eq!(
            accent_from_portal((-1.0, 0.5, 0.5)),
            None,
            "out of range means unset"
        );
    }

    #[test]
    fn the_scheme_can_be_forced_from_the_environment() {
        assert_eq!(parse_scheme_override("light"), Some(Scheme::Light));
        assert_eq!(parse_scheme_override(" Dark\n"), Some(Scheme::Dark));
        assert_eq!(parse_scheme_override(""), None);
        assert_eq!(parse_scheme_override("auto"), None);
    }

    #[test]
    fn gsettings_values_map_to_schemes_and_accents() {
        assert_eq!(scheme_from_gsettings("'prefer-dark'"), Some(Scheme::Dark));
        assert_eq!(scheme_from_gsettings("'default'\n"), Some(Scheme::Light));
        assert_eq!(scheme_from_gsettings("'prefer-light'"), Some(Scheme::Light));
        assert_eq!(scheme_from_gsettings(""), None);
        assert_eq!(
            accent_from_name("'red'"),
            Some(Color32::from_rgb(0xe6, 0x2d, 0x42))
        );
        assert_eq!(accent_from_name("'magenta'"), None);
    }

    #[test]
    fn font_names_split_into_family_and_size() {
        assert_eq!(
            parse_font_name("'Adwaita Sans 11'"),
            Some(("Adwaita Sans".to_owned(), Some(11.0)))
        );
        assert_eq!(
            parse_font_name("'Cantarell 10.5'\n"),
            Some(("Cantarell".to_owned(), Some(10.5)))
        );
        assert_eq!(parse_font_name("'Inter'"), Some(("Inter".to_owned(), None)));
        assert_eq!(parse_font_name("''"), None);
    }

    #[test]
    fn only_outline_fonts_are_loaded() {
        assert!(loadable(std::path::Path::new("/f/AdwaitaSans-Regular.ttf")));
        assert!(loadable(std::path::Path::new("/f/Noto.TTC")));
        assert!(!loadable(std::path::Path::new("/f/font.pcf.gz")));
        assert!(!loadable(std::path::Path::new("/f/font.woff2")));
    }
}
