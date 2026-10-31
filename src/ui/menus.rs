//! The header menus, the right-click menu and the queue's context actions.

use eframe::egui::{self, Button, Ui};

use super::{Frontend, PrefsTab, always_on_top_supported, geometry};
use crate::clock::clock;
use crate::engine::TrackKind;
use crate::keys::Action;
use crate::queue::title_of;
use crate::tuning;

/// A plain entry that closes the menu when clicked.
fn plain(ui: &mut Ui, label: &str) -> bool {
    let response = ui.add(Button::new(label));
    geometry::note(ui.ctx(), label, response.rect);
    let clicked = response.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Longest menu label shown in full; longer track and chapter names get an ellipsis.
const MENU_LABEL_MAX: usize = 48;

fn shorten(label: &str) -> String {
    if label.chars().count() <= MENU_LABEL_MAX {
        return label.to_owned();
    }
    let head: String = label.chars().take(MENU_LABEL_MAX - 1).collect();
    format!("{}…", head.trim_end())
}

/// An entry that shows whether it is the current choice.
fn choice(ui: &mut Ui, label: &str, selected: bool) -> bool {
    let response = ui
        .add(Button::new(shorten(label)).selected(selected))
        .on_hover_text(label);
    geometry::note(ui.ctx(), label, response.rect);
    let clicked = response.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn disabled(ui: &mut Ui, label: &str) {
    ui.add_enabled(false, Button::new(label));
}

/// A submenu button, remembering where it was drawn.
fn submenu<R>(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui) -> R) {
    let response = ui.menu_button(label, content).response;
    geometry::note(ui.ctx(), label, response.rect);
}

impl Frontend {
    /// An entry with its shortcut that runs `action` when clicked.
    fn entry(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        label: &str,
        action: Action,
        selected: bool,
    ) {
        let hint = self.key_hint(action);
        let mut button = Button::new(label).selected(selected);
        if !hint.is_empty() {
            button = button.shortcut_text(hint);
        }
        let response = ui.add(button);
        geometry::note(ui.ctx(), label, response.rect);
        if response.clicked() {
            ui.close();
            self.perform(ctx, action);
        }
    }

    pub(super) fn audio_menu(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let audio = self.session.engine.tracks(TrackKind::Audio);
        let subtitles = self.session.engine.tracks(TrackKind::Subtitle);
        ui.set_min_width(200.0);

        submenu(ui, "Audio track", |ui| {
            if audio.is_empty() {
                disabled(ui, "No tracks available");
            }
            for track in &audio {
                if choice(ui, &track.label, track.selected) {
                    self.session.select_track(TrackKind::Audio, Some(track.id));
                }
            }
            if !audio.is_empty() {
                ui.separator();
                self.entry(ui, ctx, "Next audio track", Action::CycleAudio, false);
            }
        });
        submenu(ui, "Subtitles", |ui| {
            if choice(ui, "Off", !subtitles.iter().any(|t| t.selected)) {
                self.session.select_track(TrackKind::Subtitle, None);
            }
            for track in &subtitles {
                if choice(ui, &track.label, track.selected) {
                    self.session
                        .select_track(TrackKind::Subtitle, Some(track.id));
                }
            }
            ui.separator();
            if plain(ui, "Load subtitle file…") {
                self.picker
                    .open_subtitle(ctx, &self.session.store.settings.last_dir);
            }
        });
        submenu(ui, "Sync", |ui| {
            let has_media = self.session.queue.current.is_some();
            let audio_delay = self.session.engine.delay_ms(TrackKind::Audio);
            let subtitle_delay = self.session.engine.delay_ms(TrackKind::Subtitle);
            disabled(ui, &format!("Audio delay: {audio_delay:+} ms"));
            ui.add_enabled_ui(has_media, |ui| {
                self.entry(ui, ctx, "Audio later", Action::AudioDelayUp, false);
                self.entry(ui, ctx, "Audio earlier", Action::AudioDelayDown, false);
            });
            ui.separator();
            disabled(ui, &format!("Subtitle delay: {subtitle_delay:+} ms"));
            ui.add_enabled_ui(has_media, |ui| {
                self.entry(ui, ctx, "Subtitles later", Action::SubDelayUp, false);
                self.entry(ui, ctx, "Subtitles earlier", Action::SubDelayDown, false);
                ui.separator();
                if plain(ui, "Reset both") {
                    self.session.reset_delays();
                }
            });
        });
    }

    pub(super) fn video_menu(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.set_min_width(200.0);
        submenu(ui, "Chapters", |ui| {
            let chapters = self.chapters.clone();
            let now = self.session.current_chapter();
            if chapters.is_empty() {
                disabled(ui, "No chapters");
            }
            for (index, chapter) in chapters.iter().enumerate() {
                let name = if chapter.title.is_empty() {
                    format!("Chapter {}", index + 1)
                } else {
                    chapter.title.clone()
                };
                if choice(
                    ui,
                    &format!("{name}   {}", clock(chapter.time_ms)),
                    now == Some(index),
                ) {
                    self.session.go_to_chapter(index);
                }
            }
            if !chapters.is_empty() {
                ui.separator();
                self.entry(ui, ctx, "Next chapter", Action::ChapterNext, false);
                self.entry(ui, ctx, "Previous chapter", Action::ChapterPrev, false);
            }
        });
        let qualities = self.session.stream_qualities();
        if !qualities.is_empty() {
            submenu(ui, "Stream quality", |ui| {
                let current = self.session.stream_quality();
                for quality in &qualities {
                    if choice(ui, quality, *quality == current) {
                        self.session.set_stream_quality(quality);
                    }
                }
            });
        }
        submenu(ui, "Aspect ratio", |ui| {
            let current = self.session.engine.aspect();
            for (value, label) in tuning::ASPECTS {
                if choice(ui, label, current == value) {
                    self.session.engine.set_aspect(value);
                }
            }
        });
        submenu(ui, "Rotate", |ui| {
            let current = self.session.engine.rotation();
            for degrees in [0, 90, 180, 270] {
                if choice(ui, &format!("{degrees}°"), current == degrees) {
                    self.session.engine.set_rotation(degrees);
                }
            }
        });
        submenu(ui, "Crop", |ui| {
            for (ratio, label) in tuning::CROPS {
                if choice(ui, label, false)
                    && !self.session.engine.set_crop(ratio)
                    && ratio.is_some()
                {
                    self.session
                        .flash("Crop needs a playing video and mpv 0.41 or newer");
                }
            }
        });
        submenu(ui, "Shaders", |ui| {
            let dir = tuning::shader_dir();
            let available = tuning::list_shaders(&dir);
            if available.is_empty() {
                disabled(ui, "No shaders found");
                disabled(ui, &format!("Put .glsl files in {}", dir.display()));
            }
            let mut changed = false;
            for name in &available {
                let on = self.session.store.settings.shaders.contains(name);
                // A checkbox-style entry that keeps the menu open for several picks.
                if ui.add(Button::new(name).selected(on)).clicked() {
                    let list = &mut self.session.store.settings.shaders;
                    if on {
                        list.retain(|n| n != name);
                    } else {
                        list.push(name.clone());
                    }
                    changed = true;
                }
            }
            if !self.session.store.settings.shaders.is_empty() {
                ui.separator();
                if plain(ui, "Turn all off") {
                    self.session.store.settings.shaders.clear();
                    changed = true;
                }
            }
            if changed {
                self.session.apply_settings();
            }
        });
        submenu(ui, "Frame", |ui| {
            self.entry(ui, ctx, "Next frame", Action::Frame, false);
            self.entry(ui, ctx, "Previous frame", Action::FrameBack, false);
        });
        self.entry(ui, ctx, "Take screenshot", Action::Screenshot, false);
        ui.separator();
        self.entry(ui, ctx, "Mini player", Action::Mini, self.is_mini());
        let on_top = self.session.store.settings.always_on_top;
        let hint = self.key_hint(Action::OnTop);
        let supported = always_on_top_supported();
        let button = Button::new("Always on top")
            .selected(on_top && supported)
            .shortcut_text(hint);
        let response = ui.add_enabled(supported, button);
        let response = if supported {
            response
        } else {
            response.on_disabled_hover_text("Wayland compositors decide window stacking")
        };
        if response.clicked() {
            ui.close();
            self.perform(ctx, Action::OnTop);
        }
        self.entry(ui, ctx, "Fullscreen", Action::Fullscreen, false);
    }

    pub(super) fn more_menu(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.set_min_width(220.0);
        self.entry(ui, ctx, "Stop", Action::Stop, false);
        submenu(ui, "Open recent", |ui| {
            let recent = self.session.store.settings.recent.clone();
            if recent.is_empty() {
                disabled(ui, "Nothing yet");
            }
            for source in &recent {
                if ui
                    .add(Button::new(shorten(&title_of(source))))
                    .on_hover_text(source)
                    .clicked()
                {
                    ui.close();
                    self.session.add(std::slice::from_ref(source), true);
                }
            }
            if !recent.is_empty() {
                ui.separator();
                if plain(ui, "Clear recent") {
                    self.session.clear_recent();
                }
            }
        });
        submenu(ui, "Bookmarks", |ui| {
            let has_media = self.session.queue.current.is_some();
            ui.add_enabled_ui(has_media, |ui| {
                self.entry(ui, ctx, "Add bookmark here", Action::Bookmark, false)
            });
            let marks = self.session.bookmarks();
            if !marks.is_empty() {
                ui.separator();
            }
            for ms in &marks {
                if plain(ui, &format!("Jump to {}", clock(*ms))) {
                    self.session.seek_ms(*ms);
                }
            }
            if !marks.is_empty() {
                ui.separator();
                if plain(ui, "Clear bookmarks") {
                    self.session.clear_bookmarks();
                }
            }
        });
        let label = match self.session.loop_points() {
            (None, _) => "Set loop start",
            (Some(_), None) => "Set loop end",
            _ => "Clear loop",
        };
        let has_media = self.session.queue.current.is_some();
        ui.add_enabled_ui(has_media, |ui| {
            self.entry(ui, ctx, label, Action::Loop, false)
        });
        ui.separator();
        let shown = self.session.store.settings.show_queue;
        self.entry(ui, ctx, "Show play queue", Action::ToggleQueue, shown);
        self.entry(ui, ctx, "Preferences…", Action::Preferences, false);
        if plain(ui, "Media information") {
            self.info = true;
        }
        if plain(ui, "Keyboard shortcuts") {
            self.open_prefs(PrefsTab::Keys);
        }
    }

    /// The menu on the picture: the basics first, then the same menus as the header.
    pub(super) fn context_menu(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.set_min_width(200.0);
        let playing = self.session.state() == crate::session::State::Playing;
        self.entry(
            ui,
            ctx,
            if playing { "Pause" } else { "Play" },
            Action::Play,
            false,
        );
        self.entry(ui, ctx, "Stop", Action::Stop, false);
        ui.separator();
        ui.menu_button("Audio & subtitles", |ui| self.audio_menu(ui, ctx));
        ui.menu_button("Video", |ui| self.video_menu(ui, ctx));
        ui.menu_button("More", |ui| self.more_menu(ui, ctx));
    }
}
