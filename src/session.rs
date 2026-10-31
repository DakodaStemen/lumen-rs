//! Lumen's player logic: queue playback, resume, per-file memory, A-B loop, tracks and sync.
//!
//! A `Session` owns everything except the window, so it runs the same with or without a UI.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::clock::clock;
use crate::engine::{Chapter, Engine, TrackKind};
use crate::keys::{Action, binding_text};
use crate::queue::{Queue, is_url};
use crate::sponsorblock::{self, Segment};
use crate::store::{FileState, Store};
use crate::streams::{Activity, Event, Plan, Streams, Tools, has_media_extension, is_web};

pub const RATES: [f64; 10] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0, 4.0];
const SAVE_INTERVAL: Duration = Duration::from_secs(5);
const FLASH_FOR: Duration = Duration::from_millis(2500);
/// Resuming is skipped for the first seconds and the last stretch of a file.
const RESUME_AFTER_MS: i64 = 5_000;
const RESUME_BEFORE_END_MS: i64 = 15_000;
/// A sponsor segment is not worth skipping when this little of it is left.
const SPONSOR_TAIL_MS: i64 = 500;
const LONG_FLASH: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Opening,
    Buffering,
    Playing,
    Paused,
    Ended,
}

/// Sponsor segments for the video that is playing, and the ones already skipped.
#[derive(Default)]
struct Sponsor {
    video_id: Option<String>,
    segments: Vec<Segment>,
    skipped: HashSet<Segment>,
}

/// What to do once a stream link has been resolved to something mpv can open.
#[derive(Default, Clone, Copy)]
struct Pending {
    start_ms: Option<i64>,
    paused: bool,
}

pub struct Session {
    pub engine: Engine,
    pub store: Store,
    pub queue: Queue,
    pub streams: Streams,
    /// True while playing through Streamlink's relay: a live stream, whatever length mpv reports.
    live_relay: bool,
    sponsor: Sponsor,
    pending: Pending,
    loop_a: Option<i64>,
    loop_b: Option<i64>,
    rate: f64,
    finished: bool,
    was_started: bool,
    /// Set from the moment a file is loaded until its saved choices are re-applied.
    restore_pending: bool,
    /// Whether mpv has reported the file from the latest `start` as loaded. Until then its
    /// properties may still describe the previous file, so nothing is restored onto it.
    file_loaded: bool,
    last_save: Instant,
    flash: Option<(String, Instant)>,
}

impl Session {
    pub fn new(engine: Engine, store: Store) -> Self {
        engine.apply_settings(&store.settings, &crate::tuning::shader_dir());
        Self {
            engine,
            store,
            queue: Queue::default(),
            streams: Streams::new(Tools::detect()),
            live_relay: false,
            sponsor: Sponsor::default(),
            pending: Pending::default(),
            loop_a: None,
            loop_b: None,
            rate: 1.0,
            finished: false,
            was_started: false,
            restore_pending: false,
            file_loaded: false,
            last_save: Instant::now(),
            flash: None,
        }
    }

    /// Pushes the current settings to mpv (after a change in Preferences or a menu) and saves them.
    pub fn apply_settings(&mut self) {
        let refused = self
            .engine
            .apply_settings(&self.store.settings, &crate::tuning::shader_dir());
        if !refused.is_empty() {
            self.flash(&format!("mpv rejected: {}", refused.join(", ")));
        }
        self.store.save_settings();
    }

    // ----- queue -----

    /// Appends sources and, when `autoplay` is set, starts the first one added.
    pub fn add(&mut self, sources: &[String], autoplay: bool) {
        if let Some(first) = self.queue.add(sources)
            && autoplay
        {
            self.play_index(first, None, false);
        }
    }

    pub fn source(&self) -> Option<String> {
        self.queue.current_source().map(str::to_owned)
    }

    pub fn remove(&mut self, index: usize) {
        let removing_current = self.queue.current == Some(index);
        self.queue.remove(index);
        if removing_current {
            self.finished = true; // nothing left to save progress for
            self.streams.stop();
            self.engine.stop();
        }
    }

    /// Plays the item `delta` places away, or does nothing at the end of the queue.
    pub fn advance(&mut self, delta: i64) {
        let folder_next = self.store.settings.auto_next_folder;
        if let Some(index) = self.queue.advance(delta, folder_next) {
            self.play_index(index, None, false);
        }
    }

    // ----- playback -----

    pub fn play_index(&mut self, index: usize, start_ms: Option<i64>, paused: bool) {
        let Some(source) = self.queue.source(index).map(str::to_owned) else {
            return;
        };
        if self.queue.current.is_some() && !self.finished {
            self.save_position(false);
        }
        self.engine.stop();
        if self.queue.current != Some(index) {
            self.streams.quality = None;
        }
        self.streams.stop();
        self.sponsor = Sponsor::default();
        self.live_relay = false;
        self.queue.current = Some(index);
        self.finished = false;
        self.was_started = false;
        self.clear_loop();
        self.store.remember_recent(&source);
        self.pending = Pending { start_ms, paused };

        match self.streams.plan(&source) {
            Plan::Direct => {
                let no_tools =
                    self.streams.tools.streamlink.is_none() && self.streams.tools.ytdlp.is_none();
                if no_tools && is_web(&source) && !has_media_extension(&source) {
                    self.flash_for(
                        "Playing this link directly. Install yt-dlp or streamlink for sites like YouTube and Twitch.",
                        LONG_FLASH,
                    );
                }
                self.start(&source, start_ms, paused, None, true);
            }
            Plan::Resolve => {
                let settings = &self.store.settings;
                self.streams
                    .begin(&source, &settings.stream_quality, settings.low_latency);
            }
        }
    }

    /// Opens `mrl` for the current queue entry. `resume` picks up where the file was left.
    fn start(
        &mut self,
        mrl: &str,
        start_ms: Option<i64>,
        paused: bool,
        audio: Option<&str>,
        resume: bool,
    ) {
        let resumed = if resume {
            self.source().and_then(|s| self.resume_point(&s))
        } else {
            None
        };
        let start = start_ms.or(resumed);
        if start_ms.is_none()
            && let Some(position) = start
        {
            let key = binding_text(&self.store.settings.keys, Action::Restart);
            self.flash(&format!(
                "Resumed at {}  ·  press {key} to start over",
                clock(position)
            ));
        }
        self.restore_pending = true;
        // Events from earlier files must not count as this one loading.
        self.engine.poll_file_loaded();
        self.file_loaded = false;
        if !self
            .engine
            .load(mrl, start.map(|ms| ms as f64 / 1000.0), paused, audio)
        {
            self.flash("Unable to open media. Check the file or stream.");
        }
        self.engine.set_speed(self.rate);
    }

    fn resume_point(&self, source: &str) -> Option<i64> {
        if !self.store.settings.resume {
            return None;
        }
        let state = self.store.file_state(source);
        let near_end = state.length > 0 && state.position >= state.length - RESUME_BEFORE_END_MS;
        (state.position > RESUME_AFTER_MS && !near_end).then_some(state.position)
    }

    /// A live stream has no length to seek within and no position worth resuming.
    pub fn is_live(&self) -> bool {
        self.live_relay
            || (self.queue.current.is_some()
                && !self.engine.idle()
                && self.engine.duration_ms().is_none())
    }

    pub fn state(&self) -> State {
        if self.engine.idle() {
            State::Idle
        } else if self.engine.at_end() {
            State::Ended
        } else if self.engine.position_ms().is_none() {
            State::Opening
        } else if self.engine.buffering() {
            State::Buffering
        } else if self.engine.paused() {
            State::Paused
        } else {
            State::Playing
        }
    }

    pub fn toggle(&mut self) {
        match (self.queue.current, self.state()) {
            (None, _) if self.queue.items.is_empty() => {}
            (None, _) => self.play_index(0, None, false),
            (Some(current), State::Idle | State::Ended) => self.play_index(current, None, false),
            _ => self.engine.set_paused(!self.engine.paused()),
        }
    }

    pub fn stop(&mut self) {
        if self.queue.current.is_some() {
            self.save_position(false);
        }
        self.streams.stop();
        self.engine.stop();
    }

    /// Call regularly (Lumen does so every 200 ms). Drives resume, autosave and auto-advance.
    pub fn tick(&mut self) {
        self.handle_stream_events();
        if let Some(loaded) = self.engine.poll_file_loaded() {
            self.file_loaded = loaded;
        }
        let state = self.state();
        if state == State::Playing {
            self.was_started = true;
        }
        if matches!(state, State::Playing | State::Paused) {
            if self.restore_pending && self.file_loaded && self.position_ms() > 0 {
                self.restore_pending = false;
                self.restore_file_settings();
            }
            if state == State::Playing {
                if !self.sponsor.segments.is_empty() {
                    self.skip_sponsors(self.position_ms());
                }
                if self.last_save.elapsed() > SAVE_INTERVAL {
                    self.save_position(false);
                }
            }
        }
        if state == State::Ended && !self.finished {
            self.finished = true;
            self.save_position(true);
            self.advance(1);
        }
    }

    /// Re-applies the choices remembered for this file once mpv has published its tracks.
    fn restore_file_settings(&mut self) {
        let Some(source) = self.source() else { return };
        let state = self.store.file_state(&source);
        let remember = self.store.settings.remember_tracks;
        if remember {
            if let Some(id) = state.audio
                && self
                    .engine
                    .tracks(TrackKind::Audio)
                    .iter()
                    .any(|t| t.id == id)
            {
                self.engine.select_track(TrackKind::Audio, Some(id));
            }
            match state.spu {
                Some(id) if id < 0 => self.engine.select_track(TrackKind::Subtitle, None),
                Some(id)
                    if self
                        .engine
                        .tracks(TrackKind::Subtitle)
                        .iter()
                        .any(|t| t.id == id) =>
                {
                    self.engine.select_track(TrackKind::Subtitle, Some(id));
                }
                _ => {}
            }
        }
        self.engine.set_delay_ms(
            TrackKind::Audio,
            if remember { state.audio_delay } else { 0 },
        );
        self.engine.set_delay_ms(
            TrackKind::Subtitle,
            if remember { state.spu_delay } else { 0 },
        );
        for subtitle in state.subtitles.iter().filter(|s| Path::new(s).is_file()) {
            self.engine.add_subtitle(subtitle);
        }
    }

    pub fn save_position(&mut self, finished: bool) {
        let Some(source) = self.source() else { return };
        let length = self.engine.duration_ms().filter(|l| *l > 0);
        let Some(length) = length.filter(|_| !self.live_relay) else {
            return; // live streams have no position worth resuming
        };
        if !self.was_started {
            return;
        }
        self.last_save = Instant::now();
        let mut state = self.store.file_state(&source);
        state.position = if finished { 0 } else { self.position_ms() };
        state.length = length;
        if self.store.settings.remember_tracks && !self.restore_pending {
            state.audio = self.engine.selected_track(TrackKind::Audio);
            state.spu = Some(
                self.engine
                    .selected_track(TrackKind::Subtitle)
                    .unwrap_or(-1),
            );
            state.audio_delay = self.engine.delay_ms(TrackKind::Audio);
            state.spu_delay = self.engine.delay_ms(TrackKind::Subtitle);
        }
        self.store.save_file_state(&source, state);
    }

    // ----- seeking -----

    pub fn position_ms(&self) -> i64 {
        self.engine.position_ms().unwrap_or(0).max(0)
    }

    pub fn seek_ms(&mut self, ms: i64) {
        if self.engine.seekable() && !self.live_relay {
            self.engine.seek_to_ms(ms.max(0));
        }
    }

    pub fn skip(&mut self, ms: i64) {
        if self.engine.seekable() {
            self.seek_ms(self.position_ms() + ms);
        }
    }

    pub fn seek_step_ms(&self) -> i64 {
        i64::from(self.store.settings.seek_step) * 1000
    }

    pub fn restart(&mut self) {
        self.seek_ms(0);
    }

    // ----- sound -----

    pub fn set_volume(&mut self, volume: u32) {
        let volume = volume.min(200);
        self.engine.set_volume(volume);
        if self.store.settings.volume != volume {
            self.store.settings.volume = volume;
            self.store.save_settings();
        }
    }

    pub fn change_volume(&mut self, delta: i32) {
        let volume = (self.store.settings.volume as i32 + delta).clamp(0, 200) as u32;
        self.set_volume(volume);
        self.flash(&format!("Volume {volume}%"));
    }

    pub fn toggle_mute(&mut self) {
        self.engine.set_muted(!self.engine.muted());
    }

    pub fn rate(&self) -> f64 {
        self.rate
    }

    pub fn set_rate(&mut self, rate: f64) {
        let nearest = RATES
            .iter()
            .copied()
            .min_by(|a, b| (a - rate).abs().total_cmp(&(b - rate).abs()))
            .unwrap_or(1.0);
        self.rate = nearest;
        self.engine.set_speed(nearest);
        self.flash(&format!("Speed {nearest}×"));
    }

    pub fn step_rate(&mut self, delta: i32) {
        let current = RATES.iter().position(|r| *r == self.rate).unwrap_or(3) as i32;
        let next = (current + delta).clamp(0, RATES.len() as i32 - 1) as usize;
        self.set_rate(RATES[next]);
    }

    /// Nudges audio or subtitle timing. Returns nothing when no file is queued.
    pub fn shift_delay(&mut self, kind: TrackKind, ms: i64) {
        if self.queue.current.is_none() {
            return;
        }
        let value = self.engine.delay_ms(kind) + ms;
        self.engine.set_delay_ms(kind, value);
        let name = if kind == TrackKind::Audio {
            "Audio"
        } else {
            "Subtitle"
        };
        self.flash(&format!("{name} delay {value:+} ms"));
        self.save_position(false);
    }

    /// Switches to the next audio track; subtitles also cycle through "off".
    pub fn cycle_track(&mut self, kind: TrackKind) {
        let tracks = self.engine.tracks(kind);
        if tracks.is_empty() {
            self.flash("No tracks to switch");
            return;
        }
        let current = self.engine.selected_track(kind);
        let position = tracks.iter().position(|t| Some(t.id) == current);
        let slots = tracks.len() + usize::from(kind == TrackKind::Subtitle);
        let next = position.map_or(0, |p| (p + 1) % slots);
        let (id, label) = match tracks.get(next) {
            Some(track) => (Some(track.id), track.label.clone()),
            None => (None, "Off".to_owned()),
        };
        self.engine.select_track(kind, id);
        let name = if kind == TrackKind::Audio {
            "Audio"
        } else {
            "Subtitles"
        };
        self.flash(&format!("{name}: {label}"));
        self.save_position(false);
    }

    // ----- navigation -----

    pub fn frame_forward(&mut self) {
        if self.queue.current.is_some() {
            self.engine.frame_step();
            self.flash("Next frame");
        }
    }

    pub fn frame_back(&mut self) {
        if self.queue.current.is_some() && self.engine.seekable() {
            self.engine.frame_back_step();
            self.flash("Previous frame");
        }
    }

    pub fn chapter(&mut self, delta: i64) {
        if self.engine.chapter_count() > 0 {
            self.engine.chapter_step(delta);
            self.flash(if delta > 0 {
                "Next chapter"
            } else {
                "Previous chapter"
            });
        } else {
            self.flash("No chapters");
        }
    }

    pub fn loop_points(&self) -> (Option<i64>, Option<i64>) {
        (self.loop_a, self.loop_b)
    }

    pub fn set_loop(&mut self, a: Option<i64>, b: Option<i64>) {
        self.loop_a = a;
        self.loop_b = b;
        self.engine.set_loop_ms(a, b);
    }

    fn clear_loop(&mut self) {
        self.set_loop(None, None);
    }

    /// Cycles through setting the loop start, the loop end, and clearing both.
    pub fn ab_loop(&mut self) {
        if self.queue.current.is_none() {
            return;
        }
        let now = self.position_ms();
        match (self.loop_a, self.loop_b) {
            (None, _) => {
                self.set_loop(Some(now), None);
                self.flash(&format!("Loop start {}", clock(now)));
            }
            (Some(a), None) if now > a => {
                self.set_loop(Some(a), Some(now));
                self.flash(&format!("Looping {} – {}", clock(a), clock(now)));
            }
            _ => {
                self.clear_loop();
                self.flash("Loop cleared");
            }
        }
    }

    pub fn add_bookmark(&mut self) {
        let Some(source) = self.source() else { return };
        let now = self.position_ms();
        let mut state: FileState = self.store.file_state(&source);
        state.bookmarks.push(now);
        state.bookmarks.sort_unstable();
        state.bookmarks.dedup();
        self.store.save_file_state(&source, state);
        self.flash(&format!("Bookmark at {}", clock(now)));
    }

    pub fn screenshot(&mut self) {
        let Some(source) = self.source() else {
            self.flash("No video to capture");
            return;
        };
        if !self.engine.has_video() {
            self.flash("No video to capture");
            return;
        }
        let folder = crate::platform::screenshot_dir();
        let _ = std::fs::create_dir_all(&folder);
        let target = folder.join(format!(
            "{}-{}.png",
            screenshot_stem(&source),
            clock(self.position_ms()).replace(':', ".")
        ));
        if self.engine.screenshot(&target.to_string_lossy()) {
            self.flash(&format!("Screenshot saved to {}", target.display()));
        } else {
            self.flash("Screenshot failed");
        }
    }

    // ----- queue and library -----

    /// Empties the queue and stops playback.
    pub fn clear_queue(&mut self) {
        self.stop();
        self.finished = true;
        self.queue.clear();
    }

    pub fn clear_recent(&mut self) {
        self.store.settings.recent.clear();
        self.store.save_settings();
    }

    /// Bookmarks of the current file, in time order.
    pub fn bookmarks(&self) -> Vec<i64> {
        self.source()
            .map_or_else(Vec::new, |s| self.store.file_state(&s).bookmarks)
    }

    pub fn remove_bookmark(&mut self, ms: i64) {
        if let Some(source) = self.source() {
            let mut state = self.store.file_state(&source);
            state.bookmarks.retain(|b| *b != ms);
            self.store.save_file_state(&source, state);
        }
    }

    pub fn clear_bookmarks(&mut self) {
        if let Some(source) = self.source() {
            let mut state = self.store.file_state(&source);
            state.bookmarks.clear();
            self.store.save_file_state(&source, state);
            self.flash("Bookmarks cleared");
        }
    }

    // ----- chapters, tracks and subtitles -----

    pub fn chapters(&self) -> Vec<Chapter> {
        self.engine.chapters()
    }

    pub fn current_chapter(&self) -> Option<usize> {
        self.engine.chapter()
    }

    pub fn go_to_chapter(&mut self, index: usize) {
        self.engine.set_chapter(index);
    }

    /// Picks an audio or subtitle track by id, or switches subtitles off with `None`.
    pub fn select_track(&mut self, kind: TrackKind, id: Option<i64>) {
        self.engine.select_track(kind, id);
        let name = if kind == TrackKind::Audio {
            "Audio"
        } else {
            "Subtitles"
        };
        let label = id
            .and_then(|id| self.engine.tracks(kind).into_iter().find(|t| t.id == id))
            .map_or("Off".to_owned(), |t| t.label);
        self.flash(&format!("{name}: {label}"));
        self.save_position(false);
    }

    /// Loads a subtitle file now, and again whenever this file is played.
    pub fn load_subtitle(&mut self, path: &str) {
        let Some(source) = self.source() else {
            self.flash("Open a video first");
            return;
        };
        self.engine.add_subtitle(path);
        let mut state = self.store.file_state(&source);
        if !state.subtitles.iter().any(|s| s == path) {
            state.subtitles.push(path.to_owned());
            self.store.save_file_state(&source, state);
        }
        self.flash("Subtitles loaded");
    }

    pub fn reset_delays(&mut self) {
        self.engine.set_delay_ms(TrackKind::Audio, 0);
        self.engine.set_delay_ms(TrackKind::Subtitle, 0);
        self.flash("Sync reset");
        self.save_position(false);
    }

    // ----- streams -----

    /// Acts on what the Streamlink, yt-dlp and SponsorBlock workers have reported.
    fn handle_stream_events(&mut self) {
        for event in self.streams.poll() {
            let current = self.source();
            let is_current = |page: &String| current.as_ref() == Some(page);
            match event {
                Event::Ready { page, url } if is_current(&page) => {
                    self.live_relay = true;
                    self.start(&url, None, false, None, false);
                }
                Event::Info { page, title, .. } if is_current(&page) => {
                    self.set_current_title(&title)
                }
                Event::Resolved {
                    page,
                    extractor,
                    id,
                    title,
                    video,
                    audio,
                } if is_current(&page) => {
                    self.set_current_title(&title);
                    if extractor == "Youtube" && self.store.settings.sponsorblock {
                        self.sponsor.video_id = Some(id.clone());
                        sponsorblock::fetch(id, self.sponsor_categories(), self.streams.sender());
                    }
                    let Pending { start_ms, paused } = self.pending;
                    self.start(&video, start_ms, paused, audio.as_deref(), true);
                }
                Event::Direct { page, notice } if is_current(&page) => {
                    if let Some(notice) = notice {
                        self.flash(&notice);
                    }
                    let Pending { start_ms, paused } = self.pending;
                    self.start(&page, start_ms, paused, None, true);
                }
                Event::Failed { page, message } if is_current(&page) => {
                    self.flash_for(&message, LONG_FLASH)
                }
                Event::Sponsor { video_id, segments }
                    if self.sponsor.video_id.as_ref() == Some(&video_id) =>
                {
                    self.flash(&format!(
                        "SponsorBlock: {} segment(s) will be skipped",
                        segments.len()
                    ));
                    self.sponsor.segments = segments;
                }
                _ => {}
            }
        }
    }

    fn set_current_title(&mut self, title: &str) {
        if let Some(item) = self.queue.current.and_then(|c| self.queue.items.get_mut(c))
            && !title.is_empty()
        {
            item.title = title.to_owned();
        }
    }

    pub fn sponsor_categories(&self) -> Vec<String> {
        let extra = if self.store.settings.sponsorblock_intros {
            &["intro", "outro"][..]
        } else {
            &[]
        };
        ["sponsor", "selfpromo", "interaction"]
            .iter()
            .chain(extra)
            .map(|c| (*c).to_owned())
            .collect()
    }

    /// Replaces the segments to skip for the current video.
    pub fn set_sponsor_segments(&mut self, segments: Vec<Segment>) {
        self.sponsor.segments = segments;
        self.sponsor.skipped.clear();
    }

    /// Skips each sponsor segment once; seeking back into a skipped one is respected.
    fn skip_sponsors(&mut self, position: i64) {
        let Some(segment) = self
            .sponsor
            .segments
            .iter()
            .find(|s| {
                !self.sponsor.skipped.contains(*s)
                    && s.start <= position
                    && position < s.end - SPONSOR_TAIL_MS
            })
            .cloned()
        else {
            return;
        };
        self.seek_ms(segment.end);
        self.flash(&format!(
            "Skipped {} ({})",
            sponsorblock::label(&segment.category),
            clock(segment.end - segment.start)
        ));
        self.sponsor.skipped.insert(segment);
    }

    /// Quality choices for the current stream, best first, once Streamlink has listed them.
    pub fn stream_qualities(&self) -> Vec<String> {
        let source = self.source().unwrap_or_default();
        let listed = self.streams.qualities_for(&source);
        if listed.is_empty() {
            return Vec::new();
        }
        std::iter::once("best".to_owned())
            .chain(listed.iter().rev().cloned())
            .collect()
    }

    /// The quality in use: the menu choice for this file, else the saved default.
    pub fn stream_quality(&self) -> String {
        self.streams
            .quality
            .clone()
            .unwrap_or_else(|| self.store.settings.stream_quality.clone())
    }

    /// Restarts the current stream at `quality`.
    pub fn set_stream_quality(&mut self, quality: &str) {
        self.streams.quality = Some(quality.to_owned());
        if let Some(index) = self.queue.current {
            self.play_index(index, None, false);
        }
    }

    // ----- status -----

    pub fn flash(&mut self, text: &str) {
        self.flash_for(text, FLASH_FOR);
    }

    pub fn flash_for(&mut self, text: &str, duration: Duration) {
        let text = if text.chars().count() < 40 {
            text.to_uppercase()
        } else {
            text.to_owned()
        };
        self.flash = Some((text, Instant::now() + duration));
    }

    /// The message from the last action while it is still fresh, such as `SPEED 1.25×`.
    pub fn flash_message(&self) -> Option<&str> {
        self.flash
            .as_ref()
            .filter(|(_, until)| Instant::now() < *until)
            .map(|(text, _)| text.as_str())
    }

    /// The status line: a recent message if there is one, otherwise the playback state.
    pub fn status(&self) -> String {
        if let Some((text, until)) = &self.flash
            && Instant::now() < *until
        {
            return text.clone();
        }
        if self.queue.current.is_none() {
            return String::new();
        }
        match self.streams.activity() {
            Some(Activity::Streamlink) => return "CONNECTING WITH STREAMLINK…".to_owned(),
            Some(Activity::Ytdlp) => return "RESOLVING LINK WITH YT-DLP…".to_owned(),
            Some(Activity::Checking) => return "OPENING LINK…".to_owned(),
            None => {}
        }
        let mut text = match self.state() {
            State::Playing => "PLAYING",
            State::Paused => "PAUSED",
            State::Opening => "OPENING MEDIA…",
            State::Buffering => "BUFFERING…",
            State::Ended => "PLAYBACK FINISHED",
            State::Idle => "READY",
        }
        .to_owned();
        if let Some(a) = self.loop_a {
            let b = self.loop_b.map_or("…".to_owned(), clock);
            text += &format!("   ·   LOOP {} – {b}", clock(a));
        }
        text + "   /   mpv"
    }
}

fn screenshot_stem(source: &str) -> String {
    let name = if is_url(source) {
        source
            .split(['?', '#'])
            .next()
            .unwrap_or(source)
            .rsplit('/')
            .next()
            .unwrap_or("")
    } else {
        Path::new(source)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
    };
    let name = name
        .rsplit_once('.')
        .filter(|_| is_url(source))
        .map_or(name, |(stem, _)| stem);
    let mut stem = String::new();
    for c in name.chars() {
        let keep = c.is_alphanumeric() || matches!(c, '_' | '.' | '-');
        if keep {
            stem.push(c);
        } else if !stem.ends_with('_') {
            stem.push('_');
        }
    }
    let stem: String = stem.chars().take(60).collect();
    if stem.is_empty() {
        "frame".to_owned()
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_names_are_filesystem_safe() {
        assert_eq!(
            screenshot_stem("/media/My Film (2020).mkv"),
            "My_Film_2020_"
        );
        assert_eq!(screenshot_stem("https://x.test/v/clip.mp4?token=1"), "clip");
        assert_eq!(
            screenshot_stem("/media/.mkv"),
            ".mkv",
            "a hidden file keeps its name"
        );
        assert_eq!(screenshot_stem("https://x.test/"), "frame");
        assert!(screenshot_stem(&"a".repeat(200)).len() <= 60);
    }
}
