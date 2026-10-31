//! Everything that differs between Linux, macOS and Windows lives here, so the rest of
//! the crate stays free of `cfg` and environment-variable lookups.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// mpv refuses to start unless numbers are parsed with the C locale.
pub fn set_c_numeric_locale() {
    // SAFETY: called once, before mpv is created; `c"C"` is a valid NUL-terminated string.
    unsafe {
        libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
    }
}

/// The user's home directory.
fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// Where Lumen keeps settings and resume points.
///
/// Linux: `$XDG_CONFIG_HOME` or `~/.config`. macOS: `~/Library/Application Support`.
/// Windows: `%APPDATA%`.
pub fn config_dir() -> PathBuf {
    config_base(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("APPDATA"),
        home(),
    )
    .join("lumen-rs")
}

fn config_base(xdg: Option<OsString>, appdata: Option<OsString>, home: Option<PathBuf>) -> PathBuf {
    let fallback = || PathBuf::from(".");
    if cfg!(windows) {
        appdata
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join("AppData").join("Roaming")))
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library").join("Application Support"))
    } else {
        xdg.map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".config")))
    }
    .unwrap_or_else(fallback)
}

/// Where screenshots go: `~/Pictures/Lumen` on every platform.
pub fn screenshot_dir() -> PathBuf {
    home().unwrap_or_default().join("Pictures").join("Lumen")
}

/// Finds `name` on the `PATH`, honouring `PATHEXT` on Windows.
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(
        name,
        &std::env::var_os("PATH")?,
        std::env::var_os("PATHEXT"),
    )
}

fn which_in(name: &str, path: &OsString, pathext: Option<OsString>) -> Option<PathBuf> {
    let mut names = vec![name.to_owned()];
    if cfg!(windows) {
        let exts = pathext
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned());
        names.extend(
            exts.split(';')
                .filter(|e| !e.is_empty())
                .map(|e| format!("{name}{e}")),
        );
    }
    std::env::split_paths(path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file())
}

/// Compositors, not applications, decide stacking on Wayland. Windows and macOS always allow it.
pub fn always_on_top_supported() -> bool {
    !is_wayland_session(
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
    )
}

fn is_wayland_session(wayland_display: bool, session_type: Option<&str>) -> bool {
    cfg!(all(unix, not(target_os = "macos"))) && wayland_display && session_type != Some("x11")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_follows_the_platform() {
        let home = Some(PathBuf::from("/h"));
        let base = config_base(Some("/x".into()), Some("/a".into()), home.clone());
        if cfg!(windows) {
            assert_eq!(base, PathBuf::from("/a"));
        } else if cfg!(target_os = "macos") {
            assert_eq!(base, PathBuf::from("/h/Library/Application Support"));
        } else {
            assert_eq!(base, PathBuf::from("/x"));
            assert_eq!(
                config_base(Some("rel".into()), None, home),
                PathBuf::from("/h/.config")
            );
        }
        assert_eq!(config_base(None, None, None), PathBuf::from("."));
    }

    #[test]
    fn wayland_blocks_always_on_top_only_on_linux() {
        let linux = cfg!(all(unix, not(target_os = "macos")));
        assert_eq!(is_wayland_session(true, Some("wayland")), linux);
        assert!(!is_wayland_session(true, Some("x11")));
        assert!(!is_wayland_session(false, None));
    }

    #[test]
    fn which_finds_executables_in_path_order() {
        let dir = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "tool.exe" } else { "tool" };
        let file = dir.path().join(name);
        std::fs::write(&file, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(which_in("tool", &dir.path().into(), None), None);
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(which_in("tool", &path, Some(".EXE".into())), Some(file));
        assert_eq!(which_in("missing", &path, None), None);
    }
}
