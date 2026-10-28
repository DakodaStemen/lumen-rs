//! A thin, typed wrapper over libmpv. It holds no player policy; see `session.rs` for that.

use std::sync::{Arc, Once};

use anyhow::{Result, anyhow};
use libmpv2::Mpv;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Video is rendered through libmpv's OpenGL API into the app window.
    Window,
    /// No video or audio device; used by tests and tools.
    Headless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Subtitle,
}

impl TrackKind {
    fn type_name(self) -> &'static str {
        match self {
            TrackKind::Audio => "audio",
            TrackKind::Subtitle => "sub",
        }
    }

    fn selector(self) -> &'static str {
        match self {
            TrackKind::Audio => "aid",
            TrackKind::Subtitle => "sid",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: i64,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone)]
pub struct Engine {
    mpv: Arc<Mpv>,
    /// `loadfile` gained an index argument before its options in mpv 0.38.
    loadfile_index: bool,
}

/// Whether an `mpv-version` string such as `mpv 0.41.0` is 0.38 or newer. Unknown builds count as new.
fn has_loadfile_index(version: &str) -> bool {
    let numbers = version
        .trim_start_matches("mpv")
        .trim()
        .trim_start_matches('v');
    let mut parts = numbers.split(['.', '-']).map(|p| p.parse::<u32>());
    match (parts.next(), parts.next()) {
        (Some(Ok(major)), Some(Ok(minor))) => (major, minor) >= (0, 38),
        _ => true,
    }
}

impl Engine {
    pub fn new(output: Output, volume: u32) -> Result<Self> {
        // mpv refuses to start unless numbers are parsed with the C locale.
        static LOCALE: Once = Once::new();
        LOCALE.call_once(|| unsafe {
            libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
        });
        let mpv = Mpv::with_initializer(|init| {
            match output {
                Output::Window => init.set_property("vo", "libmpv")?,
                Output::Headless => {
                    init.set_property("vo", "null")?;
                    init.set_property("ao", "null")?;
                }
            }
            init.set_property("hwdec", "auto-safe")?;
            init.set_property("idle", "yes")?;
            init.set_property("keep-open", "yes")?;
            init.set_property("volume-max", 200i64)?;
            init.set_property("volume", i64::from(volume.min(200)))?;
            init.set_property("input-default-bindings", "no")?;
            init.set_property("input-vo-keyboard", "no")?;
            // Streams are resolved by Lumen itself, never by mpv's built-in hook.
            init.set_property("ytdl", "no")?;
            Ok(())
        })
        .map_err(|e| anyhow!("could not start mpv: {e}"))?;
        let loadfile_index = mpv
            .get_property::<String>("mpv-version")
            .map_or(true, |v| has_loadfile_index(&v));
        Ok(Self {
            mpv: Arc::new(mpv),
            loadfile_index,
        })
    }

    pub fn handle(&self) -> &Arc<Mpv> {
        &self.mpv
    }

    fn flag(&self, name: &str) -> Option<bool> {
        self.mpv.get_property::<bool>(name).ok()
    }

    fn number(&self, name: &str) -> Option<f64> {
        self.mpv.get_property::<f64>(name).ok()
    }

    fn integer(&self, name: &str) -> Option<i64> {
        self.mpv.get_property::<i64>(name).ok()
    }

    fn text(&self, name: &str) -> Option<String> {
        self.mpv.get_property::<String>(name).ok()
    }

    fn command(&self, name: &str, args: &[&str]) -> bool {
        self.mpv.command(name, args).is_ok()
    }

    fn set(&self, name: &str, value: &str) {
        let _ = self.mpv.set_property(name, value);
    }

    // ----- loading and state -----

    pub fn load(&self, source: &str, start_secs: Option<f64>, paused: bool) -> bool {
        let mut options = Vec::new();
        if let Some(start) = start_secs {
            options.push(format!("start={start:.3}"));
        }
        if paused {
            options.push("pause=yes".to_owned());
        }
        // Without an explicit `pause=no`, a pause left over from the previous file would stick.
        if !paused {
            options.push("pause=no".to_owned());
        }
        let options = options.join(",");
        if self.loadfile_index {
            self.command("loadfile", &[source, "replace", "-1", &options])
        } else {
            self.command("loadfile", &[source, "replace", &options])
        }
    }

    pub fn stop(&self) {
        self.command("stop", &[]);
    }

    /// True when nothing is loaded.
    pub fn idle(&self) -> bool {
        self.flag("idle-active").unwrap_or(true)
    }

    /// True once playback reached the end of the file (mpv keeps the file open).
    pub fn at_end(&self) -> bool {
        self.flag("eof-reached").unwrap_or(false)
    }

    pub fn paused(&self) -> bool {
        self.flag("pause").unwrap_or(false)
    }

    pub fn set_paused(&self, paused: bool) {
        let _ = self.mpv.set_property("pause", paused);
    }

    pub fn buffering(&self) -> bool {
        self.flag("paused-for-cache").unwrap_or(false)
    }

    pub fn has_video(&self) -> bool {
        self.flag("vo-configured").unwrap_or(false)
    }

    // ----- time -----

    pub fn position_ms(&self) -> Option<i64> {
        self.number("time-pos").map(|s| (s * 1000.0).round() as i64)
    }

    /// `None` for live streams and while the length is still unknown.
    pub fn duration_ms(&self) -> Option<i64> {
        self.number("duration").map(|s| (s * 1000.0).round() as i64)
    }

    pub fn seekable(&self) -> bool {
        self.flag("seekable").unwrap_or(false)
    }

    pub fn seek_to_ms(&self, ms: i64) {
        self.command("seek", &[&format!("{:.3}", ms as f64 / 1000.0), "absolute"]);
    }

    pub fn fps(&self) -> Option<f64> {
        self.number("container-fps").filter(|f| *f > 0.0)
    }

    // ----- audio -----

    pub fn volume(&self) -> u32 {
        self.number("volume").unwrap_or(0.0).round() as u32
    }

    pub fn set_volume(&self, volume: u32) {
        let _ = self.mpv.set_property("volume", f64::from(volume.min(200)));
    }

    pub fn muted(&self) -> bool {
        self.flag("mute").unwrap_or(false)
    }

    pub fn set_muted(&self, muted: bool) {
        let _ = self.mpv.set_property("mute", muted);
    }

    pub fn speed(&self) -> f64 {
        self.number("speed").unwrap_or(1.0)
    }

    pub fn set_speed(&self, speed: f64) {
        let _ = self.mpv.set_property("speed", speed);
    }

    /// Milliseconds the audio (or subtitles) are shifted later; negative moves them earlier.
    pub fn delay_ms(&self, kind: TrackKind) -> i64 {
        let name = if kind == TrackKind::Audio {
            "audio-delay"
        } else {
            "sub-delay"
        };
        self.number(name).map_or(0, |s| (s * 1000.0).round() as i64)
    }

    pub fn set_delay_ms(&self, kind: TrackKind, ms: i64) {
        let name = if kind == TrackKind::Audio {
            "audio-delay"
        } else {
            "sub-delay"
        };
        let _ = self.mpv.set_property(name, ms as f64 / 1000.0);
    }

    // ----- tracks -----

    pub fn tracks(&self, kind: TrackKind) -> Vec<Track> {
        let count = self.integer("track-list/count").unwrap_or(0);
        (0..count)
            .filter(|i| {
                self.text(&format!("track-list/{i}/type")).as_deref() == Some(kind.type_name())
            })
            .filter_map(|i| {
                let id = self.integer(&format!("track-list/{i}/id"))?;
                let language = self.text(&format!("track-list/{i}/lang"));
                let title = self.text(&format!("track-list/{i}/title"));
                let label = match (language, title) {
                    (Some(l), Some(t)) => format!("{l} · {t}"),
                    (Some(x), None) | (None, Some(x)) => x,
                    (None, None) => format!("Track {id}"),
                };
                let selected = self
                    .flag(&format!("track-list/{i}/selected"))
                    .unwrap_or(false);
                Some(Track {
                    id,
                    label,
                    selected,
                })
            })
            .collect()
    }

    /// The selected track id, or `None` when the kind is switched off.
    pub fn selected_track(&self, kind: TrackKind) -> Option<i64> {
        self.text(kind.selector())?.parse().ok()
    }

    pub fn select_track(&self, kind: TrackKind, id: Option<i64>) {
        self.set(
            kind.selector(),
            &id.map_or("no".to_owned(), |id| id.to_string()),
        );
    }

    pub fn add_subtitle(&self, path: &str) {
        self.command("sub-add", &[path, "auto"]);
    }

    // ----- navigation -----

    pub fn frame_step(&self) {
        self.command("frame-step", &[]);
    }

    pub fn frame_back_step(&self) {
        self.command("frame-back-step", &[]);
    }

    pub fn chapter_count(&self) -> i64 {
        self.integer("chapter-list/count").unwrap_or(0)
    }

    pub fn chapter_step(&self, delta: i64) {
        self.command("add", &["chapter", &delta.to_string()]);
    }

    /// Loops between two points, in milliseconds. `None` clears that end.
    pub fn set_loop_ms(&self, a: Option<i64>, b: Option<i64>) {
        let point =
            |p: Option<i64>| p.map_or("no".to_owned(), |ms| format!("{:.3}", ms as f64 / 1000.0));
        self.set("ab-loop-a", &point(a));
        self.set("ab-loop-b", &point(b));
    }

    pub fn screenshot(&self, path: &str) -> bool {
        self.command("screenshot-to-file", &[path, "video"])
    }
}

#[cfg(test)]
mod tests {
    use super::has_loadfile_index;

    #[test]
    fn detects_the_loadfile_signature_from_the_mpv_version() {
        assert!(has_loadfile_index("mpv 0.41.0"));
        assert!(has_loadfile_index("mpv 0.38.0"));
        assert!(has_loadfile_index("mpv v0.39.0-12-gabcdef"));
        assert!(has_loadfile_index("mpv 1.0.0"));
        assert!(!has_loadfile_index("mpv 0.37.0"));
        assert!(!has_loadfile_index("mpv 0.34.1"));
        assert!(
            has_loadfile_index("mpv UNKNOWN"),
            "unknown builds are assumed current"
        );
    }
}
