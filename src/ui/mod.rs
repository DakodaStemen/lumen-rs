//! The window: Lumen's layout, menus, dialogs and input handling on top of a `Session`.
//!
//! Everything here draws through a [`Surface`], so the whole interface can be built and
//! clicked through in tests without a GPU or a video.

mod bar;
mod dialogs;
pub mod geometry;
mod menus;
pub mod picker;
pub mod theme;

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Margin, Order,
    RichText, Sense, Stroke, StrokeKind, UiBuilder, Vec2,
};

use crate::clock::clock;
use crate::engine::Chapter;
use crate::keys::{self, Action, Binding, Keymap, binding_text};
use crate::session::{RATES, Session, State};
use bar::Bar;
use picker::{Picked, Picker};

const TICK: Duration = Duration::from_millis(200);
/// Controls hide in fullscreen and the mini player after the pointer has been still this long.
const CHROME_HIDE: Duration = Duration::from_secs(2);
/// A single click waits this long for a second one before it toggles pause.
const DOUBLE_CLICK: Duration = Duration::from_millis(280);
/// Height of the status line, seek bar and transport row under the video.
const CONTROLS_HEIGHT: f32 = 132.0;
const SIDEBAR_WIDTH: f32 = 248.0;
/// The one spacing unit between groups of controls.
const GAP: f32 = 12.0;
const MINI_SIZE: [f32; 2] = [480.0, 270.0];
/// Below this width the transport row drops its extras.
const NARROW: f32 = 640.0;

/// Where the video is drawn.
pub trait Surface {
    fn paint(&mut self, ui: &mut egui::Ui, rect: egui::Rect);
}

/// A black rectangle, for tests and for when there is no GPU video.
pub struct NullSurface;

impl Surface for NullSurface {
    fn paint(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
    }
}

pub struct UrlDialog {
    pub text: String,
    pub rejected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefsTab {
    Playback,
    Picture,
    Streams,
    Keys,
}

pub struct Prefs {
    pub tab: PrefsTab,
    /// The action waiting for its new key to be pressed.
    pub capture: Option<Action>,
    pub message: Option<String>,
}

pub struct Frontend {
    pub session: Session,
    pub picker: Picker,
    keymap: Keymap,
    pub selected: Option<usize>,
    pub filter: String,
    /// Seek-bar value (0..=1) while the bar is being dragged.
    scrubbing: Option<f32>,
    pub url: Option<UrlDialog>,
    pub prefs: Option<Prefs>,
    pub info: bool,
    /// The window size to return to when the mini player closes; `Some` while it is open.
    mini_restore: Option<Vec2>,
    chapters: Vec<Chapter>,
    chapters_for: Option<String>,
    last_tick: Instant,
    last_pointer_move: Instant,
    title: String,
    pending_click: Option<Instant>,
    scroll: f32,
    dropping: bool,
    brand: Option<egui::TextureHandle>,
}

/// Compositors, not applications, decide stacking on Wayland.
pub fn always_on_top_supported() -> bool {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("x11");
    !wayland
}

impl Frontend {
    pub fn new(session: Session, ctx: &egui::Context) -> Self {
        theme::apply(ctx);
        let keymap = Keymap::from_settings(&session.store.settings.keys);
        let brand = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/mark.png"))
            .ok()
            .map(|icon| {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                );
                ctx.load_texture("brand", image, egui::TextureOptions::LINEAR)
            });
        Self {
            session,
            picker: Picker::default(),
            keymap,
            selected: None,
            filter: String::new(),
            scrubbing: None,
            url: None,
            prefs: None,
            info: false,
            mini_restore: None,
            chapters: Vec::new(),
            chapters_for: None,
            last_tick: Instant::now(),
            last_pointer_move: Instant::now(),
            title: String::new(),
            pending_click: None,
            scroll: 0.0,
            dropping: false,
            brand,
        }
    }

    pub fn is_mini(&self) -> bool {
        self.mini_restore.is_some()
    }

    fn rebuild_keymap(&mut self) {
        self.keymap = Keymap::from_settings(&self.session.store.settings.keys);
    }

    fn key_hint(&self, action: Action) -> String {
        binding_text(&self.session.store.settings.keys, action)
    }

    /// Draws one frame.
    pub fn show(&mut self, ctx: &egui::Context, surface: &mut dyn Surface) {
        self.pump(ctx);
        let fullscreen = is_fullscreen(ctx);
        if fullscreen || self.is_mini() {
            self.compact_view(ctx, surface, fullscreen);
        } else {
            if self.session.store.settings.show_queue {
                egui::SidePanel::left("sidebar")
                    .exact_width(SIDEBAR_WIDTH)
                    .resizable(false)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::SIDEBAR)
                            .inner_margin(Margin::same(GAP as i8)),
                    )
                    .show(ctx, |ui| self.sidebar(ui));
            }
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(theme::BG)
                        .inner_margin(Margin::same(GAP as i8 * 2)),
                )
                .show(ctx, |ui| self.main_view(ui, surface));
        }
        self.dialogs(ctx);
        ctx.request_repaint_after(TICK);
    }

    // ----- input and timing -----

    fn pump(&mut self, ctx: &egui::Context) {
        for picked in self.picker.poll() {
            self.handle_picked(picked);
        }
        let (dropped, hovering) = ctx.input(|i| {
            let paths: Vec<String> = i
                .raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.as_ref().map(|p| p.to_string_lossy().into_owned()))
                .collect();
            (paths, !i.raw.hovered_files.is_empty())
        });
        self.dropping = hovering;
        if !dropped.is_empty() {
            self.session.add(&dropped, true);
        }

        self.handle_keys(ctx);

        if self.last_tick.elapsed() >= TICK {
            self.last_tick = Instant::now();
            self.session.tick();
            self.refresh_chapters();
        }
        if ctx.input(|i| i.pointer.delta() != Vec2::ZERO || i.pointer.any_down()) {
            self.last_pointer_move = Instant::now();
        }
        if self
            .pending_click
            .is_some_and(|t| t.elapsed() >= DOUBLE_CLICK)
        {
            self.pending_click = None;
            self.session.toggle();
        }

        let title = self
            .session
            .queue
            .current
            .and_then(|c| self.session.queue.items.get(c))
            .map(|i| i.title.clone());
        let title = title.map_or("Lumen".to_owned(), |t| format!("{t} — Lumen"));
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn handle_picked(&mut self, picked: Picked) {
        match picked {
            Picked::Media(paths) => {
                if let Some(dir) = paths.first().and_then(|p| std::path::Path::new(p).parent()) {
                    self.session.store.settings.last_dir = dir.to_string_lossy().into_owned();
                    self.session.store.save_settings();
                }
                self.session.add(&paths, true);
            }
            Picked::Subtitle(path) => self.session.load_subtitle(&path),
            Picked::SaveQueue(path) => match std::fs::write(&path, self.session.queue.to_m3u()) {
                Ok(()) => self
                    .session
                    .flash(&format!("Saved queue to {}", path.display())),
                Err(e) => self
                    .session
                    .flash(&format!("Could not save the queue ({e})")),
            },
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if self.prefs.as_ref().is_some_and(|p| p.capture.is_some()) {
            self.capture_key(ctx);
            return;
        }
        // Escape closes the top-most dialog before it can act as a shortcut.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && (self.url.take().is_some()
                || self.prefs.take().is_some()
                || std::mem::take(&mut self.info))
        {
            return;
        }
        if ctx.wants_keyboard_input() {
            return; // typing in a text field
        }
        let mut pressed = Vec::new();
        ctx.input_mut(|input| {
            for (action, binding) in self.keymap.bindings() {
                if input.consume_key(binding.modifiers, binding.key) {
                    pressed.push(*action);
                }
            }
        });
        for action in pressed {
            self.perform(ctx, action);
        }
    }

    /// While the key table waits for a new key, the next key press becomes the binding.
    fn capture_key(&mut self, ctx: &egui::Context) {
        let Some(action) = self.prefs.as_ref().and_then(|p| p.capture) else {
            return;
        };
        let pressed = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    repeat: false,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        let Some((key, modifiers)) = pressed else {
            return;
        };
        let Some(prefs) = self.prefs.as_mut() else {
            return;
        };
        if key == egui::Key::Escape && !modifiers.any() {
            prefs.capture = None;
            prefs.message = None;
            return;
        }
        let Some(text) = Binding::format(key, modifiers) else {
            prefs.message = Some("That key cannot be used.".to_owned());
            return;
        };
        match keys::rebind(&mut self.session.store.settings.keys, action, &text) {
            Ok(()) => {
                prefs.capture = None;
                prefs.message = None;
                self.session.store.save_settings();
                self.rebuild_keymap();
            }
            Err(other) => {
                prefs.message = Some(format!("{text} is already used by “{}”.", other.label()))
            }
        }
    }

    fn refresh_chapters(&mut self) {
        let source = self.session.source();
        let count = self.session.engine.chapter_count() as usize;
        if source != self.chapters_for || count != self.chapters.len() {
            self.chapters = if count == 0 {
                Vec::new()
            } else {
                self.session.chapters()
            };
            self.chapters_for = source;
        }
    }

    fn perform(&mut self, ctx: &egui::Context, action: Action) {
        let step = self.session.seek_step_ms();
        let s = &mut self.session;
        match action {
            Action::Play => s.toggle(),
            Action::Stop => s.stop(),
            Action::Forward => s.skip(step),
            Action::Back => s.skip(-step),
            Action::ForwardLong => s.skip(60_000),
            Action::BackLong => s.skip(-60_000),
            Action::Restart => s.restart(),
            Action::Next => s.advance(1),
            Action::Previous => s.advance(-1),
            Action::Frame => s.frame_forward(),
            Action::FrameBack => s.frame_back(),
            Action::ChapterNext => s.chapter(1),
            Action::ChapterPrev => s.chapter(-1),
            Action::Faster => s.step_rate(1),
            Action::Slower => s.step_rate(-1),
            Action::Normal => s.set_rate(1.0),
            Action::VolumeUp => s.change_volume(5),
            Action::VolumeDown => s.change_volume(-5),
            Action::Mute => s.toggle_mute(),
            Action::AudioDelayUp => s.shift_delay(crate::engine::TrackKind::Audio, 50),
            Action::AudioDelayDown => s.shift_delay(crate::engine::TrackKind::Audio, -50),
            Action::SubDelayUp => s.shift_delay(crate::engine::TrackKind::Subtitle, 50),
            Action::SubDelayDown => s.shift_delay(crate::engine::TrackKind::Subtitle, -50),
            Action::CycleAudio => s.cycle_track(crate::engine::TrackKind::Audio),
            Action::CycleSub => s.cycle_track(crate::engine::TrackKind::Subtitle),
            Action::Screenshot => s.screenshot(),
            Action::Loop => s.ab_loop(),
            Action::Bookmark => s.add_bookmark(),
            Action::Open => self
                .picker
                .open_media(ctx, &self.session.store.settings.last_dir),
            Action::OpenUrl => {
                self.url = Some(UrlDialog {
                    text: String::new(),
                    rejected: false,
                })
            }
            Action::Preferences => self.open_prefs(PrefsTab::Playback),
            Action::Fullscreen => {
                if self.is_mini() {
                    self.toggle_mini(ctx);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen(ctx)));
            }
            Action::ExitFullscreen => {
                if self.is_mini() {
                    self.toggle_mini(ctx);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                }
            }
            Action::Mini => self.toggle_mini(ctx),
            Action::OnTop => self.toggle_on_top(ctx),
            Action::ToggleQueue => {
                let settings = &mut self.session.store.settings;
                settings.show_queue = !settings.show_queue;
                self.session.store.save_settings();
            }
            Action::Remove => {
                if let Some(index) = self.selected.take() {
                    self.session.remove(index);
                }
            }
        }
    }

    pub fn open_prefs(&mut self, tab: PrefsTab) {
        self.prefs = Some(Prefs {
            tab,
            capture: None,
            message: None,
        });
    }

    fn toggle_mini(&mut self, ctx: &egui::Context) {
        match self.mini_restore.take() {
            Some(size) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                if !self.session.store.settings.always_on_top {
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::Normal,
                    ));
                }
            }
            None => {
                self.mini_restore = Some(ctx.input(|i| i.content_rect().size()));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::from(MINI_SIZE)));
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::AlwaysOnTop,
                ));
            }
        }
    }

    fn toggle_on_top(&mut self, ctx: &egui::Context) {
        if !always_on_top_supported() {
            self.session
                .flash("Always on top is not available on Wayland");
            return;
        }
        let on = !self.session.store.settings.always_on_top;
        self.session.store.settings.always_on_top = on;
        self.session.store.save_settings();
        let level = if on {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
    }

    // ----- sidebar -----

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.spacing_mut().item_spacing.y = GAP;
        if theme::primary(ui, "+   Open media", Some(ui.available_width())).clicked() {
            self.perform(&ctx, Action::Open);
        }
        if theme::wide(ui, "↗   Network stream").clicked() {
            self.perform(&ctx, Action::OpenUrl);
        }

        egui::TopBottomPanel::bottom("sidebar_footer")
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.add_space(GAP);
                ui.horizontal(|ui| {
                    let half = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                    let save = ui.add_enabled(
                        !self.session.queue.items.is_empty(),
                        egui::Button::new("Save queue")
                            .min_size(Vec2::new(half, theme::BUTTON_HEIGHT)),
                    );
                    if save.clicked() {
                        self.picker
                            .save_queue(&ctx, &self.session.store.settings.last_dir);
                    }
                    let clear = ui.add_enabled(
                        !self.session.queue.items.is_empty(),
                        egui::Button::new("Clear").min_size(Vec2::new(half, theme::BUTTON_HEIGHT)),
                    );
                    if clear.clicked() {
                        self.session.clear_queue();
                        self.selected = None;
                    }
                });
                let hint = format!(
                    "{}  Play / pause   ·   {}  Fullscreen",
                    self.key_hint(Action::Play),
                    self.key_hint(Action::Fullscreen)
                );
                ui.add(egui::Label::new(theme::muted(hint)).truncate());
            });

        ui.label(theme::muted(format!(
            "PLAY QUEUE  ·  {}",
            self.session.queue.items.len()
        )));
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("Filter queue…")
                .desired_width(f32::INFINITY)
                .margin(Margin::symmetric(10, 8)),
        );
        ui.spacing_mut().item_spacing.y = 4.0;
        self.queue_list(ui);
    }

    fn queue_list(&mut self, ui: &mut egui::Ui) {
        let needle = self.filter.trim().to_lowercase();
        let mut play = None;
        let mut remove = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let mut shown = 0;
                for (index, item) in self.session.queue.items.iter().enumerate() {
                    if !needle.is_empty()
                        && !item.title.to_lowercase().contains(&needle)
                        && !item.source.to_lowercase().contains(&needle)
                    {
                        continue;
                    }
                    shown += 1;
                    let current = self.session.queue.current == Some(index);
                    let selected = self.selected == Some(index);
                    let (rect, row) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
                    geometry::note(ui.ctx(), &item.title, rect);
                    let fill = if current || selected {
                        theme::SELECTED_BG
                    } else if row.hovered() {
                        theme::HOVER
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
                    let color = if current {
                        theme::SELECTED_TEXT
                    } else {
                        theme::TEXT
                    };
                    let text_rect = rect.shrink2(Vec2::new(GAP, 0.0));
                    // Left-aligned, unlike `ui.put`, which centres its widget.
                    let mut text_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(text_rect)
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    text_ui.add(
                        egui::Label::new(RichText::new(&item.title).color(color))
                            .truncate()
                            .selectable(false),
                    );
                    row.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            item.title.as_str(),
                        )
                    });
                    if row.clicked() {
                        self.selected = Some(index);
                    }
                    if row.double_clicked() {
                        play = Some(index);
                    }
                    row.on_hover_text(&item.source).context_menu(|ui| {
                        if ui.button("Play").clicked() {
                            play = Some(index);
                            ui.close();
                        }
                        if ui.button("Copy location").clicked() {
                            ui.ctx().copy_text(item.source.clone());
                            ui.close();
                        }
                        if ui.button("Remove from queue").clicked() {
                            remove = Some(index);
                            ui.close();
                        }
                    });
                }
                if self.session.queue.items.is_empty() {
                    ui.add_space(8.0);
                    ui.label(theme::muted("Nothing queued yet."));
                } else if shown == 0 {
                    ui.add_space(8.0);
                    ui.label(theme::muted("No matches."));
                }
            });
        if let Some(index) = play {
            self.session.play_index(index, None, false);
        }
        if let Some(index) = remove {
            if self.selected == Some(index) {
                self.selected = None;
            }
            self.session.remove(index);
        }
    }

    // ----- main view -----

    fn main_view(&mut self, ui: &mut egui::Ui, surface: &mut dyn Surface) {
        self.header(ui);
        ui.add_space(GAP);
        let video_height = (ui.available_height() - CONTROLS_HEIGHT).max(120.0);
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), video_height),
            Sense::hover(),
        );
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), Color32::BLACK);
        let inner = rect.shrink(1.0);
        geometry::note(ui.ctx(), "Video area", inner);
        surface.paint(ui, inner);
        self.video_interaction(ui, inner, false);
        self.video_overlays(ui, inner);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.0_f32, theme::BORDER),
            StrokeKind::Inside,
        );

        ui.add_space(GAP / 2.0);
        ui.label(theme::muted(self.session.status()));
        self.seek_row(ui);
        self.transport_row(ui);
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let playing = self
            .session
            .queue
            .current
            .and_then(|c| self.session.queue.items.get(c))
            .map(|i| i.title.clone());
        ui.horizontal(|ui| {
            let menus_width = 220.0;
            let text_width = (ui.available_width() - menus_width).max(120.0);
            ui.allocate_ui(Vec2::new(text_width, 40.0), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let heading = if playing.is_some() {
                        "Now playing"
                    } else {
                        "Welcome"
                    };
                    ui.label(theme::muted(heading));
                    let title = playing.as_deref().unwrap_or("Open something to begin");
                    ui.add(egui::Label::new(RichText::new(title).size(18.0).strong()).truncate());
                });
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Quiet header menus: no resting fill or outline until hovered.
                let widgets = &mut ui.visuals_mut().widgets;
                widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                widgets.inactive.bg_stroke = Stroke::NONE;
                ui.spacing_mut().item_spacing.x = 4.0;
                let more =
                    ui.menu_button(RichText::new("…").strong(), |ui| self.more_menu(ui, &ctx));
                geometry::note(&ctx, "…", more.response.rect);
                let video = ui.menu_button(RichText::new("Video").color(theme::MUTED), |ui| {
                    self.video_menu(ui, &ctx)
                });
                geometry::note(&ctx, "Video", video.response.rect);
                let audio = ui.menu_button(
                    RichText::new("Audio & subtitles").color(theme::MUTED),
                    |ui| self.audio_menu(ui, &ctx),
                );
                geometry::note(&ctx, "Audio & subtitles", audio.response.rect);
            });
        });
    }

    /// Click, double-click, scroll and right-click on the picture.
    fn video_interaction(&mut self, ui: &mut egui::Ui, rect: egui::Rect, compact: bool) {
        let ctx = ui.ctx().clone();
        let response = ui.interact(rect, ui.id().with("video"), Sense::click());
        if response.double_clicked() {
            self.pending_click = None;
            self.perform(&ctx, Action::Fullscreen);
        } else if response.clicked() {
            self.pending_click = Some(Instant::now());
        }
        if response.hovered() {
            self.scroll += ui.input(|i| i.raw_scroll_delta.y);
            while self.scroll.abs() >= 20.0 {
                let up = self.scroll > 0.0;
                self.scroll -= if up { 20.0 } else { -20.0 };
                self.session.change_volume(if up { 5 } else { -5 });
            }
            if compact
                && self.last_pointer_move.elapsed() >= CHROME_HIDE
                && self.session.state() == State::Playing
            {
                ctx.set_cursor_icon(CursorIcon::None);
            }
        }
        response.context_menu(|ui| self.context_menu(ui, &ctx));
    }

    /// Everything drawn over the picture: the empty state, spinner, messages and the drop hint.
    fn video_overlays(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let current = self.session.queue.current.is_some();
        let state = self.session.state();
        if !current && !self.dropping {
            self.empty_state(ui, rect);
        } else if self.session.streams.activity().is_some()
            || matches!(state, State::Opening | State::Buffering)
        {
            let spinner = egui::Spinner::new().size(40.0).color(theme::ACCENT);
            ui.put(
                egui::Rect::from_center_size(rect.center(), Vec2::splat(40.0)),
                spinner,
            );
        }

        if let Some(message) = self.session.flash_message() {
            let painter = ui.painter();
            let galley = painter.layout_no_wrap(
                message.to_owned(),
                FontId::proportional(15.0),
                Color32::WHITE,
            );
            let size = galley.size() + Vec2::new(28.0, 16.0);
            let pill = egui::Rect::from_center_size(
                rect.center_top() + Vec2::new(0.0, 14.0 + size.y / 2.0),
                size,
            );
            painter.rect_filled(pill, CornerRadius::same(12), Color32::from_black_alpha(190));
            painter.galley(pill.min + Vec2::new(14.0, 8.0), galley, Color32::WHITE);
        }

        if self.dropping {
            let painter = ui.painter();
            painter.rect_filled(rect, CornerRadius::same(6), Color32::from_black_alpha(170));
            painter.rect_stroke(
                rect.shrink(10.0),
                CornerRadius::same(10),
                Stroke::new(2.0_f32, theme::ACCENT),
                StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Drop to play",
                FontId::proportional(26.0),
                theme::ACCENT,
            );
        }
    }

    fn empty_state(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let ctx = ui.ctx().clone();
        let queued = !self.session.queue.items.is_empty();
        let recent: Vec<String> = if rect.height() > 380.0 {
            self.session
                .store
                .settings
                .recent
                .iter()
                .take(4)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(24.0, 0.0)))
                .layout(Layout::top_down(Align::Center)),
        );
        let content = 230.0
            + if recent.is_empty() {
                0.0
            } else {
                48.0 + 32.0 * recent.len() as f32
            };
        child.spacing_mut().item_spacing.y = GAP;
        child.add_space(((rect.height() - content) / 2.0).max(8.0));
        if let Some(brand) = &self.brand {
            child.image((brand.id(), Vec2::splat(48.0)));
        }
        child.label(RichText::new("Lumen").size(32.0).strong());
        child.label(theme::muted("Every format. Every frame."));
        child.add_space(GAP / 2.0);
        let heading = if queued {
            "Ready when you are"
        } else {
            "Drop media here"
        };
        child.label(RichText::new(heading).size(16.0).color(theme::TEXT));
        if queued {
            if theme::primary(&mut child, "▶   Play", Some(180.0)).clicked() {
                self.session.toggle();
            }
        } else if theme::primary(&mut child, "+   Open media", Some(180.0)).clicked() {
            self.perform(&ctx, Action::Open);
        }
        child.label(theme::muted(format!(
            "or press {} to paste a link",
            self.key_hint(Action::OpenUrl)
        )));
        if !recent.is_empty() {
            child.add_space(GAP / 2.0);
            child.label(theme::muted("RECENT"));
            child.spacing_mut().item_spacing.y = 0.0;
            for source in recent {
                let name = crate::queue::title_of(&source);
                let button = egui::Button::new(RichText::new(name).color(theme::TEXT))
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::NONE)
                    .min_size(Vec2::new(300.0, 32.0));
                if child.add(button).on_hover_text(&source).clicked() {
                    self.session.add(&[source], true);
                }
            }
        }
    }

    fn seek_row(&mut self, ui: &mut egui::Ui) {
        let position = self.session.position_ms();
        let duration = self.session.engine.duration_ms().filter(|d| *d > 0);
        let live = self.session.is_live();
        let seekable = self.session.engine.seekable() && !live && duration.is_some();
        let shown = self
            .scrubbing
            .zip(duration)
            .map_or(position, |(v, d)| (v as f64 * d as f64) as i64);

        ui.horizontal(|ui| {
            ui.add_sized(
                [46.0, 24.0],
                egui::Label::new(RichText::new(clock(shown)).monospace().color(theme::MUTED)),
            );
            let width = (ui.available_width() - 62.0).max(40.0);
            let value = match (self.scrubbing, duration) {
                (Some(v), _) => v,
                (None, _) if live => 1.0,
                (None, Some(d)) => (position as f64 / d as f64).clamp(0.0, 1.0) as f32,
                (None, None) => 0.0,
            };
            let markers: Vec<f32> = match duration {
                Some(d) => self
                    .chapters
                    .iter()
                    .filter(|c| c.time_ms > 0)
                    .map(|c| (c.time_ms as f64 / d as f64) as f32)
                    .collect(),
                None => Vec::new(),
            };
            let band = match (self.session.loop_points(), duration) {
                ((Some(a), b), Some(d)) => {
                    let end = b.unwrap_or(position.max(a));
                    Some(((a as f64 / d as f64) as f32, (end as f64 / d as f64) as f32))
                }
                _ => None,
            };
            let output = bar::show(
                ui,
                Vec2::new(width, 24.0),
                &Bar {
                    value,
                    enabled: seekable,
                    markers: &markers,
                    band,
                },
            );
            geometry::note(ui.ctx(), "Seek bar", output.response.rect);
            if output.pressed {
                self.scrubbing = output.pointer;
            }
            if output.released {
                if let (Some(value), Some(d)) = (output.pointer.or(self.scrubbing), duration) {
                    self.session.seek_ms((value as f64 * d as f64) as i64);
                }
                self.scrubbing = None;
            }
            if let (Some(value), Some(d)) = (output.pointer, duration) {
                let at = (value as f64 * d as f64) as i64;
                let chapter = self
                    .chapters
                    .iter()
                    .rev()
                    .find(|c| c.time_ms <= at)
                    .filter(|c| !c.title.is_empty());
                let text = match chapter {
                    Some(c) => format!("{}  ·  {}", clock(at), c.title),
                    None => clock(at),
                };
                output.response.on_hover_text_at_pointer(text);
            }
            let end = if live {
                "● LIVE".to_owned()
            } else {
                clock(duration.unwrap_or(0))
            };
            ui.add_sized(
                [56.0, 24.0],
                egui::Label::new(RichText::new(end).monospace().color(theme::MUTED)),
            );
        });
    }

    fn transport_row(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let narrow = ui.available_width() < NARROW;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let queue = &mut self.session.queue;
            if theme::toggle(ui, "🔀", "Shuffle", queue.shuffle).clicked() {
                queue.shuffle = !queue.shuffle;
            }
            if theme::icon(ui, "⏮", "Previous").clicked() {
                self.session.advance(-1);
            }
            let playing = self.session.state() == State::Playing;
            let play = theme::primary(ui, if playing { "⏸" } else { "▶" }, Some(64.0));
            if theme::named(play, if playing { "Pause" } else { "Play" }).clicked() {
                self.session.toggle();
            }
            if theme::icon(ui, "⏭", "Next").clicked() {
                self.session.advance(1);
            }
            let queue = &mut self.session.queue;
            if theme::toggle(ui, "🔁", "Repeat", queue.repeat).clicked() {
                queue.repeat = !queue.repeat;
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let fullscreen = is_fullscreen(&ctx);
                if theme::icon(
                    ui,
                    "⛶",
                    if fullscreen {
                        "Leave fullscreen"
                    } else {
                        "Fullscreen"
                    },
                )
                .clicked()
                {
                    self.perform(&ctx, Action::Fullscreen);
                }
                if theme::toggle(ui, "🗖", "Mini player", self.is_mini()).clicked() {
                    self.perform(&ctx, Action::Mini);
                }
                if narrow {
                    let muted = self.session.engine.muted();
                    if theme::icon(ui, if muted { "🔇" } else { "🔊" }, "Mute").clicked() {
                        self.session.toggle_mute();
                    }
                    return;
                }
                let volume = self.session.store.settings.volume;
                let boosted = volume > 100;
                ui.add_sized(
                    [44.0, 24.0],
                    egui::Label::new(
                        RichText::new(format!("{volume}%"))
                            .monospace()
                            .color(if boosted { theme::ACCENT } else { theme::MUTED }),
                    ),
                );
                let output = bar::show(
                    ui,
                    Vec2::new(120.0, 24.0),
                    &Bar {
                        value: volume as f32 / 200.0,
                        enabled: true,
                        markers: &[0.5],
                        band: None,
                    },
                );
                geometry::note(ui.ctx(), "Volume bar", output.response.rect);
                if let (true, Some(value)) = (output.pressed || output.released, output.pointer) {
                    self.session
                        .set_volume((value * 200.0 / 5.0).round() as u32 * 5);
                }
                let muted = self.session.engine.muted();
                if theme::icon(ui, if muted { "🔇" } else { "🔊" }, "Mute").clicked() {
                    self.session.toggle_mute();
                }
                self.speed_menu(ui);
                self.quality_menu(ui);
            });
        });
    }

    fn speed_menu(&mut self, ui: &mut egui::Ui) {
        let rate = self.session.rate();
        let mut chosen = None;
        let combo = egui::ComboBox::from_id_salt("speed")
            .width(70.0)
            .selected_text(format!("{rate}×"))
            .show_ui(ui, |ui| {
                for candidate in RATES {
                    if ui
                        .selectable_label(candidate == rate, format!("{candidate}×"))
                        .clicked()
                    {
                        chosen = Some(candidate);
                    }
                }
            });
        geometry::note(ui.ctx(), "Speed", combo.response.rect);
        if let Some(rate) = chosen {
            self.session.set_rate(rate);
        }
    }

    /// The stream quality choice, shown only once Streamlink has listed qualities.
    fn quality_menu(&mut self, ui: &mut egui::Ui) {
        let qualities = self.session.stream_qualities();
        if qualities.is_empty() {
            return;
        }
        let current = self.session.stream_quality();
        let mut chosen = None;
        egui::ComboBox::from_id_salt("quality")
            .width(96.0)
            .selected_text(current.clone())
            .show_ui(ui, |ui| {
                for quality in &qualities {
                    if ui.selectable_label(*quality == current, quality).clicked() {
                        chosen = Some(quality.clone());
                    }
                }
            });
        if let Some(quality) = chosen {
            self.session.set_stream_quality(&quality);
        }
    }

    // ----- fullscreen and mini player -----

    fn compact_view(&mut self, ctx: &egui::Context, surface: &mut dyn Surface, fullscreen: bool) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                surface.paint(ui, rect);
                self.video_interaction(ui, rect, true);
                self.video_overlays(ui, rect);
            });
        let recent = self.last_pointer_move.elapsed() < CHROME_HIDE;
        let paused = self.session.state() == State::Paused;
        if recent || paused || self.session.queue.current.is_none() {
            let width = ctx.content_rect().width();
            egui::Area::new(egui::Id::new("overlay_controls"))
                .anchor(Align2::LEFT_BOTTOM, Vec2::ZERO)
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(
                            theme::BG.r(),
                            theme::BG.g(),
                            theme::BG.b(),
                            225,
                        ))
                        .inner_margin(Margin::symmetric(16, 10))
                        .show(ui, |ui| {
                            ui.set_width(width - 32.0);
                            self.seek_row(ui);
                            self.transport_row(ui);
                            if fullscreen {
                                ui.add_space(0.0);
                            }
                        });
                });
        }
    }
}

pub fn is_fullscreen(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}
