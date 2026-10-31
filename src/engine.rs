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
pub struct Chapter {
    pub title: String,
    pub time_ms: i64,
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

/// Options that keep mpv from acting on anything but what Lumen hands it. libmpv defaults
/// most of these already; they are set explicitly so a distribution build or a future default
/// cannot change them.
pub const LOCKDOWN: [(&str, &str); 5] = [
    // No mpv.conf, input.conf or per-file configs from the user's mpv directory.
    ("config", "no"),
    // No Lua/JS scripts from `~/.config/mpv/scripts`.
    ("load-scripts", "no"),
    // Streams are resolved by Lumen itself, never by mpv's built-in yt-dlp hook.
    ("ytdl", "no"),
    // No JSON IPC socket another local process could drive the player through.
    ("input-ipc-server", ""),
    // Playlists mpv opens itself may not reach protocols it considers unsafe.
    ("load-unsafe-playlists", "no"),
];

fn lock_down(init: &libmpv2::MpvInitializer) -> libmpv2::Result<()> {
    for (name, value) in LOCKDOWN {
        init.set_property(name, value)?;
    }
    Ok(())
}

impl Engine {
    pub fn new(output: Output, volume: u32) -> Result<Self> {
        // mpv refuses to start unless numbers are parsed with the C locale.
        static LOCALE: Once = Once::new();
        LOCALE.call_once(crate::platform::set_c_numeric_locale);
        let mpv = Mpv::with_initializer(|init| {
            match output {
                Output::Window => {
                    init.set_property("vo", "libmpv")?;
                    // Quality defaults. Older mpv builds may lack the profile; carry on without it.
                    let _ = init.set_property("profile", "gpu-hq");
                }
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
            lock_down(&init)?;
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

    /// The mpv version, such as `mpv 0.41.0`.
    pub fn version(&self) -> String {
        self.text("mpv-version").unwrap_or_else(|| "mpv".to_owned())
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

    /// Starts `source`. `audio` is a separate audio stream to play alongside a video-only one.
    pub fn load(
        &self,
        source: &str,
        start_secs: Option<f64>,
        paused: bool,
        audio: Option<&str>,
    ) -> bool {
        let mut options = Vec::new();
        if let Some(audio) = audio {
            // mpv's `%length%` prefix keeps commas and equals signs in URLs from splitting the list.
            options.push(format!("audio-file=%{}%{audio}", audio.len()));
        }
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

    /// Drains mpv's event queue. Returns `Some(true)` when the latest file event was
    /// "file loaded", `Some(false)` when a file has since started or ended, and `None`
    /// when no file event arrived.
    ///
    /// `stop` and `loadfile` are asynchronous: for a moment after them mpv still reports the
    /// previous file's state (newer builds keep it longer), so only this event says the file
    /// just asked for is the one playing. Only `Session` calls this, from one thread.
    pub fn poll_file_loaded(&self) -> Option<bool> {
        use libmpv2::events::Event;
        let mut loaded = None;
        while let Some(event) = self.mpv.wait_event(0.0) {
            match event {
                Ok(Event::FileLoaded) => loaded = Some(true),
                Ok(Event::StartFile | Event::EndFile(_)) | Err(_) => loaded = Some(false),
                _ => {}
            }
        }
        loaded
    }

    /// The location mpv is playing, which for streams differs from the queue entry.
    pub fn path(&self) -> Option<String> {
        self.text("path")
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

    // ----- settings, picture and audio output -----

    /// Applies every mpv option Lumen's settings control (decoding, tone mapping, subtitle
    /// style, colour, audio filters and output, shaders). Returns the options mpv refused.
    pub fn apply_settings(
        &self,
        settings: &crate::store::Settings,
        shader_dir: &std::path::Path,
    ) -> Vec<String> {
        crate::tuning::mpv_options(settings, shader_dir)
            .into_iter()
            .filter(|(name, value)| self.mpv.set_property(name, value.as_str()).is_err())
            .map(|(name, _)| name.to_owned())
            .collect()
    }

    /// The current value of an mpv property as text, for display and tests.
    pub fn property(&self, name: &str) -> Option<String> {
        self.text(name)
    }

    /// Audio output devices as `(mpv device name, description)`, `auto` first.
    pub fn audio_devices(&self) -> Vec<(String, String)> {
        self.text("audio-device-list")
            .map(|json| parse_audio_devices(&json))
            .unwrap_or_default()
    }

    pub fn set_aspect(&self, aspect: &str) {
        self.set("video-aspect-override", aspect);
    }

    pub fn aspect(&self) -> String {
        self.text("video-aspect-override")
            .unwrap_or_else(|| "-1".to_owned())
    }

    /// Rotation in degrees: 0, 90, 180 or 270.
    pub fn rotation(&self) -> i64 {
        self.integer("video-rotate").unwrap_or(0)
    }

    pub fn set_rotation(&self, degrees: i64) {
        let _ = self
            .mpv
            .set_property("video-rotate", degrees.rem_euclid(360));
    }

    /// Crops to a centred area with this ratio, or removes the crop. Needs mpv 0.41 or newer;
    /// returns false when there is no picture or mpv refuses.
    pub fn set_crop(&self, ratio: Option<(u32, u32)>) -> bool {
        let value = match ratio {
            None => String::new(),
            Some(ratio) => {
                let (Some(w), Some(h)) = (
                    self.integer("video-params/w"),
                    self.integer("video-params/h"),
                ) else {
                    return false;
                };
                match crate::tuning::crop_geometry(w, h, ratio) {
                    Some(value) => value,
                    None => return false,
                }
            }
        };
        self.mpv.set_property("video-crop", value.as_str()).is_ok()
    }

    pub fn crop(&self) -> String {
        self.text("video-crop").unwrap_or_default()
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

    /// Chapter titles and start times, in file order.
    pub fn chapters(&self) -> Vec<Chapter> {
        (0..self.chapter_count())
            .map(|i| Chapter {
                title: self
                    .text(&format!("chapter-list/{i}/title"))
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
                time_ms: self
                    .number(&format!("chapter-list/{i}/time"))
                    .map_or(0, |s| (s * 1000.0).round() as i64),
            })
            .collect()
    }

    /// The chapter being played, if the file has chapters.
    pub fn chapter(&self) -> Option<usize> {
        self.integer("chapter")
            .and_then(|c| usize::try_from(c).ok())
    }

    pub fn set_chapter(&self, index: usize) {
        let _ = self.mpv.set_property("chapter", index as i64);
    }

    /// Facts about the playing file as label and value pairs, leaving out what mpv does not know.
    pub fn media_info(&self) -> Vec<(String, String)> {
        let mut rows = Vec::new();
        if self.idle() {
            return rows;
        }
        let mut add = |label: &str, value: Option<String>| {
            if let Some(value) = value.filter(|v| !v.is_empty()) {
                rows.push((label.to_owned(), value));
            }
        };
        add("Container", self.text("file-format"));
        add("Duration", self.duration_ms().map(crate::clock::clock));
        let size = self
            .integer("file-size")
            .filter(|s| *s > 0)
            .map(|s| format!("{:.1} MB", s as f64 / 1_048_576.0));
        add("Size", size);
        let resolution = match (
            self.integer("video-params/w"),
            self.integer("video-params/h"),
        ) {
            (Some(w), Some(h)) => Some(format!("{w} × {h}")),
            _ => None,
        };
        add("Video", self.text("video-codec"));
        add("Resolution", resolution);
        add("Frame rate", self.fps().map(|f| format!("{f:.3} fps")));
        add(
            "Hardware decoding",
            self.text("hwdec-current")
                .filter(|h| h != "no")
                .or(Some("off".to_owned())),
        );
        add("Audio", self.text("audio-codec"));
        let channels = self.integer("audio-params/channel-count").map(|c| match c {
            1 => "Mono".to_owned(),
            2 => "Stereo".to_owned(),
            c => format!("{c} channels"),
        });
        add("Channels", channels);
        add(
            "Sample rate",
            self.integer("audio-params/samplerate")
                .map(|r| format!("{r} Hz")),
        );
        rows
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

/// Parses mpv's `audio-device-list` (a JSON array of `{name, description}`).
fn parse_audio_devices(json: &str) -> Vec<(String, String)> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(json) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_owned();
            let description = item
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or(&name)
                .to_owned();
            Some((name, description))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Engine, LOCKDOWN, Output, has_loadfile_index, parse_audio_devices};

    #[test]
    fn parses_the_audio_device_list() {
        let json = r#"[{"name":"auto","description":"Autoselect device"},{"name":"pipewire","description":"PipeWire"},{"name":"x"}]"#;
        let devices = parse_audio_devices(json);
        assert_eq!(devices[0], ("auto".into(), "Autoselect device".into()));
        assert_eq!(devices[2], ("x".into(), "x".into()));
        assert!(parse_audio_devices("nonsense").is_empty());
    }

    #[test]
    fn mpv_starts_locked_down() {
        let engine = Engine::new(Output::Headless, 100).expect("mpv starts");
        for (name, value) in LOCKDOWN {
            assert_eq!(
                engine.text(name).as_deref(),
                Some(value),
                "{name} should be {value:?}"
            );
        }
        assert_eq!(engine.text("input-default-bindings").as_deref(), Some("no"));
    }

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
