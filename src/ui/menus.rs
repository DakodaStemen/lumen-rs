//! The header menus, the right-click menu and the queue's context actions.

use eframe::egui::{self, Atom, Button, Color32, Id, IntoAtoms, Ui};

use super::icons::{self, Icon};
use super::{Frontend, PrefsTab, always_on_top_supported, geometry, theme};
use crate::clock::clock;
use crate::engine::TrackKind;
use crate::keys::Action;
use crate::queue::title_of;
use crate::tuning;

fn gutter_id(ui: &Ui) -> Id {
    Id::new(("lumen-menu-gutter", ui.layer_id()))
}

/// Starts a menu's content. Menus with a check item leave room for the check on every row,
/// as GTK does, so their labels line up.
pub(super) fn begin(ui: &mut Ui, width: f32, gutter: bool) {
    theme::menu_rows(ui, width);
    let id = gutter_id(ui);
    ui.data_mut(|d| d.insert_temp(id, gutter));
}

fn has_gutter(ui: &Ui) -> bool {
    ui.data(|d| d.get_temp(gutter_id(ui))).unwrap_or(false)
}

/// A row's label, with a check mark (or the room for one) in front when the menu has a gutter.
fn row<'a>(ui: &Ui, label: impl Into<Atom<'a>>, checked: bool) -> Button<'a> {
    if has_gutter(ui) || checked {
        let color = if checked {
            theme::ink(ui)
        } else {
            Color32::TRANSPARENT
        };
        Button::new((icons::image(Icon::Check, theme::ICON, color), label.into()).into_atoms())
    } else {
        Button::new(label.into())
    }
}

/// A plain entry that closes the menu when clicked.
fn plain(ui: &mut Ui, label: &str) -> bool {
    let response = ui.add(row(ui, label, false));
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
pub(super) fn choice(ui: &mut Ui, label: &str, selected: bool) -> bool {
    let response = ui
        .add(row(ui, shorten(label), selected))
        .on_hover_text(label);
    geometry::note(ui.ctx(), label, response.rect);
    let clicked = response.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn disabled(ui: &mut Ui, label: &str) {
    let button = row(ui, label, false);
    ui.add_enabled(false, button);
}

fn separator(ui: &mut Ui) {
    theme::menu_separator(ui);
}

/// Shows `dir` in the file manager, creating it first.
fn open_folder(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let opener = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut child = std::process::Command::new(opener).arg(dir).spawn()?;
    // Reaped in the background so it does not linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// A submenu button with GTK's arrow, remembering where it was drawn. `gutter` says whether
/// the submenu has check items.
fn submenu<R>(ui: &mut Ui, label: &str, gutter: bool, content: impl FnOnce(&mut Ui) -> R) {
    let arrow = icons::image(Icon::GoNext, theme::ICON, theme::palette(ui.ctx()).dim);
    let button = row(ui, label, false).right_text(arrow);
    let response = egui::containers::menu::SubMenuButton::from_button(button)
        .ui(ui, |ui| {
            begin(ui, 200.0, gutter);
            // Room from where the submenu starts down to just above the window's bottom
            // edge, so it opens beside its row instead of being pushed over the menu.
            let max = ui.ctx().content_rect().bottom() - ui.max_rect().top() - 6.0 - 12.0;
            theme::scroll_menu(ui, max, content)
        })
        .0;
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
        let mut button = row(ui, label, selected);
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
        begin(ui, 200.0, false);

        submenu(ui, "Audio track", true, |ui| {
            if audio.is_empty() {
                disabled(ui, "No tracks available");
            }
            for track in &audio {
                if choice(ui, &track.label, track.selected) {
                    self.session.select_track(TrackKind::Audio, Some(track.id));
                }
            }
            if !audio.is_empty() {
                separator(ui);
                self.entry(ui, ctx, "Next audio track", Action::CycleAudio, false);
            }
        });
        submenu(ui, "Subtitles", true, |ui| {
            if choice(ui, "Off", !subtitles.iter().any(|t| t.selected)) {
                self.session.select_track(TrackKind::Subtitle, None);
            }
            for track in &subtitles {
                if choice(ui, &track.label, track.selected) {
                    self.session
                        .select_track(TrackKind::Subtitle, Some(track.id));
                }
            }
            separator(ui);
            if plain(ui, "Load subtitle file…") {
                self.picker
                    .open_subtitle(ctx, &self.session.store.settings.last_dir);
            }
        });
        submenu(ui, "Sync", false, |ui| {
            let has_media = self.session.queue.current.is_some();
            let audio_delay = self.session.engine.delay_ms(TrackKind::Audio);
            let subtitle_delay = self.session.engine.delay_ms(TrackKind::Subtitle);
            disabled(ui, &format!("Audio delay: {audio_delay:+} ms"));
            ui.add_enabled_ui(has_media, |ui| {
                self.entry(ui, ctx, "Audio later", Action::AudioDelayUp, false);
                self.entry(ui, ctx, "Audio earlier", Action::AudioDelayDown, false);
            });
            separator(ui);
            disabled(ui, &format!("Subtitle delay: {subtitle_delay:+} ms"));
            ui.add_enabled_ui(has_media, |ui| {
                self.entry(ui, ctx, "Subtitles later", Action::SubDelayUp, false);
                self.entry(ui, ctx, "Subtitles earlier", Action::SubDelayDown, false);
                separator(ui);
                if plain(ui, "Reset both") {
                    self.session.reset_delays();
                }
            });
        });
    }

    pub(super) fn video_menu(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        begin(ui, 260.0, true);
        submenu(ui, "Chapters", true, |ui| {
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
                // The start time sits on the right like a shortcut, so a long title is cut
                // short rather than the time.
                let response = ui
                    .add(
                        row(ui, shorten(&name), now == Some(index))
                            .shortcut_text(clock(chapter.time_ms)),
                    )
                    .on_hover_text(&name);
                // Screen readers hear the full title and its time.
                let spoken = format!("{name}   {}", clock(chapter.time_ms));
                let (enabled, current) = (response.enabled(), now == Some(index));
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, current, &spoken)
                });
                geometry::note(ui.ctx(), &name, response.rect);
                if response.clicked() {
                    ui.close();
                    self.session.go_to_chapter(index);
                }
            }
            if !chapters.is_empty() {
                separator(ui);
                self.entry(ui, ctx, "Next chapter", Action::ChapterNext, false);
                self.entry(ui, ctx, "Previous chapter", Action::ChapterPrev, false);
            }
        });
        let qualities = self.session.stream_qualities();
        if !qualities.is_empty() {
            submenu(ui, "Stream quality", true, |ui| {
                let current = self.session.stream_quality();
                for quality in &qualities {
                    if choice(ui, quality, *quality == current) {
                        self.session.set_stream_quality(quality);
                    }
                }
            });
        }
        submenu(ui, "Playback speed", true, |ui| {
            let rate = self.session.rate();
            for candidate in crate::session::RATES {
                if choice(ui, &format!("{candidate}×"), candidate == rate) {
                    self.session.set_rate(candidate);
                }
            }
        });
        submenu(ui, "Aspect ratio", true, |ui| {
            let current = self.session.engine.aspect();
            for (value, label) in tuning::ASPECTS {
                if choice(ui, label, current == value) {
                    self.session.engine.set_aspect(value);
                }
            }
        });
        submenu(ui, "Rotate", true, |ui| {
            let current = self.session.engine.rotation();
            for degrees in [0, 90, 180, 270] {
                if choice(ui, &format!("{degrees}°"), current == degrees) {
                    self.session.engine.set_rotation(degrees);
                }
            }
        });
        submenu(ui, "Crop", false, |ui| {
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
        submenu(ui, "Shaders", true, |ui| {
            let dir = tuning::shader_dir();
            let available = tuning::list_shaders(&dir);
            if available.is_empty() {
                disabled(ui, "No shaders found");
            }
            let mut changed = false;
            for name in &available {
                let on = self.session.store.settings.shaders.contains(name);
                // A checkbox-style entry that keeps the menu open for several picks.
                if ui.add(row(ui, name.as_str(), on)).clicked() {
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
                separator(ui);
                if plain(ui, "Turn all off") {
                    self.session.store.settings.shaders.clear();
                    changed = true;
                }
            }
            separator(ui);
            let response = ui
                .add(row(ui, "Open shaders folder", false))
                .on_hover_text(format!("Put .glsl files in {}", dir.display()));
            if response.clicked() {
                ui.close();
                if let Err(e) = open_folder(&dir) {
                    self.session
                        .flash(&format!("Could not open {} ({e})", dir.display()));
                }
            }
            if changed {
                self.session.apply_settings();
            }
        });
        submenu(ui, "Frame", false, |ui| {
            self.entry(ui, ctx, "Next frame", Action::Frame, false);
            self.entry(ui, ctx, "Previous frame", Action::FrameBack, false);
        });
        self.entry(ui, ctx, "Take screenshot", Action::Screenshot, false);
        separator(ui);
        self.entry(ui, ctx, "Mini player", Action::Mini, self.is_mini());
        let on_top = self.session.store.settings.always_on_top;
        let hint = self.key_hint(Action::OnTop);
        let supported = always_on_top_supported();
        let button = row(ui, "Always on top", on_top && supported).shortcut_text(hint);
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
        begin(ui, 240.0, true);
        self.entry(ui, ctx, "Open…", Action::Open, false);
        self.entry(ui, ctx, "Open link…", Action::OpenUrl, false);
        separator(ui);
        self.entry(ui, ctx, "Stop", Action::Stop, false);
        submenu(ui, "Open recent", false, |ui| {
            let recent = self.session.store.settings.recent.clone();
            if recent.is_empty() {
                disabled(ui, "Nothing yet");
            }
            for source in &recent {
                if ui
                    .add(row(ui, shorten(&title_of(source)), false))
                    .on_hover_text(source)
                    .clicked()
                {
                    ui.close();
                    self.session.add(std::slice::from_ref(source), true);
                }
            }
            if !recent.is_empty() {
                separator(ui);
                if plain(ui, "Clear recent") {
                    self.session.clear_recent();
                }
            }
        });
        submenu(ui, "Bookmarks", false, |ui| {
            let has_media = self.session.queue.current.is_some();
            ui.add_enabled_ui(has_media, |ui| {
                self.entry(ui, ctx, "Add bookmark here", Action::Bookmark, false)
            });
            let marks = self.session.bookmarks();
            if !marks.is_empty() {
                separator(ui);
            }
            for ms in &marks {
                if plain(ui, &format!("Jump to {}", clock(*ms))) {
                    self.session.seek_ms(*ms);
                }
            }
            if !marks.is_empty() {
                separator(ui);
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
        separator(ui);
        let queue = &mut self.session.queue;
        if ui.add(row(ui, "Shuffle", queue.shuffle)).clicked() {
            queue.shuffle = !queue.shuffle;
            ui.close();
        }
        if ui.add(row(ui, "Repeat", queue.repeat)).clicked() {
            queue.repeat = !queue.repeat;
            ui.close();
        }
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
        begin(ui, 200.0, false);
        let playing = self.session.state() == crate::session::State::Playing;
        self.entry(
            ui,
            ctx,
            if playing { "Pause" } else { "Play" },
            Action::Play,
            false,
        );
        self.entry(ui, ctx, "Stop", Action::Stop, false);
        separator(ui);
        submenu(ui, "Audio & subtitles", false, |ui| {
            self.audio_menu(ui, ctx)
        });
        submenu(ui, "Video", true, |ui| self.video_menu(ui, ctx));
        submenu(ui, "More", true, |ui| self.more_menu(ui, ctx));
    }
}
