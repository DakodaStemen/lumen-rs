//! The window: Lumen's layout, menus, dialogs and input handling on top of a `Session`.
//!
//! Everything here draws through a [`Surface`], so the whole interface can be built and
//! clicked through in tests without a GPU or a video.

pub mod appearance;
mod bar;
mod dialogs;
pub mod geometry;
mod icons;
mod menus;
pub mod picker;
pub mod theme;
pub mod window;

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Margin, Order,
    RichText, Sense, Stroke, StrokeKind, UiBuilder, Vec2,
};

use crate::clock::clock;
use crate::engine::Chapter;
use crate::keys::{self, Action, Binding, Keymap, binding_text};
use crate::session::{RATES, Session, State};
use crate::streams::Activity;
use appearance::Appearance;
use bar::Bar;
use icons::Icon;
use picker::{Picked, Picker};

const TICK: Duration = Duration::from_millis(200);
/// Controls hide in fullscreen and the mini player after the pointer has been still this long.
const CHROME_HIDE: Duration = Duration::from_secs(2);
/// A single click waits this long for a second one before it toggles pause.
const DOUBLE_CLICK: Duration = Duration::from_millis(280);
const SIDEBAR_WIDTH: f32 = 248.0;
/// The one spacing unit between groups of controls.
const GAP: f32 = 12.0;
const MINI_SIZE: [f32; 2] = [480.0, 270.0];
/// Width of the round play button.
const PLAY_SIZE: f32 = 42.0;
/// Below this width the transport row drops its extras.
const NARROW: f32 = 640.0;
/// Below this window width the queue no longer sits beside the picture unless asked for.
const SIDEBAR_COLLAPSE: f32 = 760.0;

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
    /// Set until the field has been focused once, so the cursor can start after the prefix.
    pub fresh: bool,
}

impl UrlDialog {
    /// A dialog whose field starts with `https://`, so a bare `twitch.tv/name` needs nothing more.
    pub fn new() -> Self {
        Self {
            text: crate::streams::URL_PREFIX.to_owned(),
            rejected: false,
            fresh: true,
        }
    }
}

impl Default for UrlDialog {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// What the desktop asks for, and where its changes arrive.
    appearance: Appearance,
    appearance_changes: Option<Receiver<Appearance>>,
    /// The palette last applied, so the theme is only rebuilt when something changed.
    applied: Option<theme::Palette>,
    body_size: f32,
    /// In a narrow window the queue is hidden until shown for the moment with its button.
    sidebar_peek: bool,
    /// Whether Lumen draws its own title bar (GNOME on Wayland), and its window buttons.
    csd: bool,
    window_buttons: window::ButtonLayout,
    /// The window edge under the pointer while it can resize the undecorated window.
    edge: Option<egui::ResizeDirection>,
}

pub use crate::platform::always_on_top_supported;

impl Frontend {
    /// A frontend with the default dark look and egui's fonts; see [`Frontend::follow_desktop`].
    pub fn new(session: Session, ctx: &egui::Context) -> Self {
        egui_extras::install_image_loaders(ctx);
        let keymap = Keymap::from_settings(&session.store.settings.keys);
        let mut frontend = Self {
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
            appearance: Appearance::default(),
            appearance_changes: None,
            applied: None,
            body_size: theme::body_size(None),
            sidebar_peek: false,
            csd: window::client_side_decorations(),
            window_buttons: window::button_layout(),
            edge: None,
        };
        frontend.sync_theme(ctx);
        frontend
    }

    /// Takes the desktop's fonts, light or dark preference and accent colour, and keeps
    /// following the preference and accent as they change.
    pub fn follow_desktop(&mut self, ctx: &egui::Context) {
        self.body_size = theme::install_fonts(ctx, &appearance::system_fonts());
        self.appearance = Appearance::detect();
        self.appearance_changes = appearance::watch(ctx);
        self.applied = None;
        self.sync_theme(ctx);
    }

    /// Uses `appearance` instead of what the desktop says, until it next changes.
    pub fn set_appearance(&mut self, ctx: &egui::Context, appearance: Appearance) {
        self.appearance = appearance;
        self.sync_theme(ctx);
    }

    /// Rebuilds the theme when the scheme or accent changed.
    fn sync_theme(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.appearance_changes
            && let Some(latest) = rx.try_iter().last()
        {
            self.appearance = latest;
        }
        let palette = theme::Palette::new(self.appearance.resolve(ctx), self.appearance.accent());
        if self.applied != Some(palette) {
            theme::apply(ctx, palette, self.body_size);
            self.applied = Some(palette);
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
        self.sync_theme(ctx);
        self.pump(ctx);
        self.edge = if self.csd { window::edges(ctx) } else { None };
        let fullscreen = is_fullscreen(ctx);
        if fullscreen || self.is_mini() {
            self.compact_view(ctx, surface, fullscreen);
        } else {
            let p = theme::palette(ctx);
            let collapsed = ctx.content_rect().width() < SIDEBAR_COLLAPSE;
            if !collapsed && self.session.store.settings.show_queue {
                egui::SidePanel::left("sidebar")
                    .exact_width(SIDEBAR_WIDTH)
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(egui::Frame::new().fill(p.sidebar))
                    .show(ctx, |ui| self.sidebar(ui));
            }
            egui::TopBottomPanel::top("toolbar")
                .show_separator_line(false)
                .frame(
                    egui::Frame::new()
                        .fill(p.window)
                        .inner_margin(Margin::symmetric(6, 6)),
                )
                .show(ctx, |ui| self.toolbar(ui));
            // With nothing to play the welcome page stands alone, as GNOME's player does.
            let idle = self.session.queue.items.is_empty();
            if !idle {
                egui::TopBottomPanel::bottom("controls")
                    .show_separator_line(false)
                    .frame(egui::Frame::new().fill(p.window).inner_margin(Margin {
                        left: 12,
                        right: 12,
                        top: 6,
                        bottom: 8,
                    }))
                    .show(ctx, |ui| {
                        self.seek_row(ui);
                        self.transport_row(ui);
                    });
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(Color32::BLACK))
                .show(ctx, |ui| self.video_view(ui, surface));
            if collapsed && self.sidebar_peek {
                self.sidebar_overlay(ctx);
            }
        }
        self.dialogs(ctx);
        if self.csd {
            window::frame(ctx, self.edge);
        }
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
        // Escape closes an open menu first, submenus and all, then the top-most dialog, before
        // it can act as a shortcut.
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape && egui::Popup::is_any_open(ctx) {
            egui::Popup::close_all(ctx);
            return;
        }
        if escape
            && (self.url.take().is_some()
                || self.prefs.take().is_some()
                || std::mem::take(&mut self.info)
                || std::mem::take(&mut self.sidebar_peek))
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
            Action::OpenUrl => self.url = Some(UrlDialog::new()),
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
                if ctx.content_rect().width() < SIDEBAR_COLLAPSE {
                    self.sidebar_peek = !self.sidebar_visible(ctx);
                } else {
                    let settings = &mut self.session.store.settings;
                    settings.show_queue = !settings.show_queue;
                    self.session.store.save_settings();
                }
            }
            Action::Remove => {
                if let Some(index) = self.selected.take() {
                    self.session.remove(index);
                    // The selection moves on to the row that took its place, as in GTK lists.
                    let left = self.session.queue.items.len();
                    self.selected = (left > 0).then(|| index.min(left - 1));
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

    /// Whether the queue sits beside the picture: as set in a wide window, and only while
    /// asked for in a narrow one.
    fn sidebar_visible(&self, ctx: &egui::Context) -> bool {
        if ctx.content_rect().width() < SIDEBAR_COLLAPSE {
            self.sidebar_peek
        } else {
            self.session.store.settings.show_queue
        }
    }

    /// In a narrow window the queue slides over the picture, as GNOME's overlay split views do;
    /// a click beside it puts it away.
    fn sidebar_overlay(&mut self, ctx: &egui::Context) {
        let p = theme::palette(ctx);
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("sidebar_scrim"))
            .fixed_pos(screen.min)
            .order(Order::Middle)
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(90));
                if response.clicked() {
                    self.sidebar_peek = false;
                }
            });
        egui::Area::new(egui::Id::new("sidebar_overlay"))
            .fixed_pos(screen.min)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let rect = egui::Rect::from_min_size(
                    screen.min,
                    Vec2::new(SIDEBAR_WIDTH, screen.height()),
                );
                ui.painter()
                    .add(ctx.style().visuals.window_shadow.as_shape(rect, 0));
                ui.painter().rect_filled(rect, 0.0, p.sidebar);
                ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
                    ui.set_min_size(rect.size());
                    self.sidebar(ui);
                });
            });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let count = self.session.queue.items.len();
        // A header the height of the toolbar beside it, titled like a GNOME sidebar.
        egui::TopBottomPanel::top("sidebar_header")
            .frame(egui::Frame::NONE.inner_margin(Margin::symmetric(6, 6)))
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), theme::BUTTON_HEIGHT),
                    Sense::hover(),
                );
                if self.csd {
                    window::drag_area(
                        ui,
                        ui.max_rect().expand(6.0),
                        "sidebar_move",
                        self.edge.is_some(),
                    );
                }
                let mut header = ui.new_child(
                    UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::right_to_left(Align::Center)),
                );
                if theme::icon_button(
                    &mut header,
                    Icon::Add,
                    "Add to queue",
                    &format!("Add files to the queue ({})", self.key_hint(Action::Open)),
                )
                .clicked()
                {
                    self.picker
                        .open_media(&ctx, &self.session.store.settings.last_dir);
                }
                let title_size = self.body_size;
                let galley_width = ui
                    .painter()
                    .layout_no_wrap(
                        "Queue".to_owned(),
                        FontId::proportional(title_size),
                        Color32::WHITE,
                    )
                    .size()
                    .x;
                let title_rect = egui::Rect::from_center_size(
                    rect.center(),
                    Vec2::new(galley_width + 1.0, rect.height()),
                );
                let mut title = ui.new_child(
                    UiBuilder::new()
                        .max_rect(title_rect)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                theme::heading(&mut title, "Queue", Some(title_size));
            });
        egui::TopBottomPanel::bottom("sidebar_footer")
            .frame(egui::Frame::NONE.inner_margin(Margin::symmetric(6, 6)))
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let items = match count {
                        1 => "1 item".to_owned(),
                        n => format!("{n} items"),
                    };
                    ui.add_space(6.0);
                    ui.label(theme::muted(items));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.add_enabled_ui(count > 0, |ui| {
                            if theme::icon_button(ui, Icon::ClearAll, "Clear", "Clear the queue")
                                .clicked()
                            {
                                self.session.clear_queue();
                                self.selected = None;
                            }
                            if theme::icon_button(
                                ui,
                                Icon::Save,
                                "Save queue",
                                "Save the queue as a playlist",
                            )
                            .clicked()
                            {
                                self.picker
                                    .save_queue(&ctx, &self.session.store.settings.last_dir);
                            }
                        });
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(Margin::symmetric(6, 0)))
            .show_inside(ui, |ui| {
                let p = theme::palette(&ctx);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Filter queue")
                        .desired_width(f32::INFINITY)
                        .margin(Margin {
                            left: 34,
                            right: 10,
                            top: 8,
                            bottom: 8,
                        }),
                );
                let glass = egui::Rect::from_min_size(
                    egui::pos2(
                        field.rect.left() + 10.0,
                        field.rect.center().y - theme::ICON / 2.0,
                    ),
                    Vec2::splat(theme::ICON),
                );
                icons::paint(ui, Icon::Search, glass, theme::ICON, p.dim);
                ui.add_space(6.0);
                self.queue_list(ui);
            });
    }

    fn queue_list(&mut self, ui: &mut egui::Ui) {
        let needle = self.filter.trim().to_lowercase();
        let p = theme::palette(ui.ctx());
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
                        .allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
                    geometry::note(ui.ctx(), &item.title, rect);
                    let fill = if selected {
                        p.selected
                    } else if row.hovered() {
                        p.hover
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(theme::RADIUS), fill);
                    let (icon, tint) = if current {
                        (Icon::Play, p.accent_text)
                    } else if is_audio(&item.source) {
                        (Icon::AudioFile, p.dim)
                    } else {
                        (Icon::VideoFile, p.dim)
                    };
                    let glyph = egui::Rect::from_min_size(
                        egui::pos2(rect.left() + 10.0, rect.center().y - theme::ICON / 2.0),
                        Vec2::splat(theme::ICON),
                    );
                    icons::paint(ui, icon, glyph, theme::ICON, tint);
                    let text_rect = egui::Rect::from_min_max(
                        egui::pos2(glyph.right() + 10.0, rect.top()),
                        egui::pos2(rect.right() - 10.0, rect.bottom()),
                    );
                    // Left-aligned, unlike `ui.put`, which centres its widget.
                    let mut text_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(text_rect)
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    // The row's own tooltip names the file, so the label adds none of its own.
                    text_ui.add(
                        egui::Label::new(RichText::new(&item.title).color(p.text))
                            .truncate()
                            .show_tooltip_when_elided(false)
                            .selectable(false),
                    );
                    if row.has_focus() {
                        ui.painter().rect_stroke(
                            rect.shrink(1.0),
                            CornerRadius::same(theme::RADIUS),
                            Stroke::new(2.0_f32, p.accent.gamma_multiply(0.6)),
                            StrokeKind::Inside,
                        );
                    }
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
                    if row.double_clicked()
                        || (row.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        play = Some(index);
                    }
                    row.on_hover_text(&item.source).context_menu(|ui| {
                        menus::begin(ui, 200.0, false);
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
                    ui.add_space(GAP);
                    ui.vertical_centered(|ui| ui.label(theme::muted("Nothing queued yet.")));
                } else if shown == 0 {
                    ui.add_space(GAP);
                    ui.vertical_centered(|ui| ui.label(theme::muted("No matches.")));
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

    /// The picture, or a welcome page while nothing is loaded.
    fn video_view(&mut self, ui: &mut egui::Ui, surface: &mut dyn Surface) {
        let rect = ui.max_rect();
        geometry::note(ui.ctx(), "Video area", rect);
        if self.session.queue.current.is_some() {
            ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
            surface.paint(ui, rect);
        }
        self.video_interaction(ui, rect, false);
        self.video_overlays(ui, rect);
    }

    /// The header bar above the picture: the queue and open buttons on the left, the menus on
    /// the right, all as symbolic icons like a GNOME header bar.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), theme::BUTTON_HEIGHT),
            Sense::hover(),
        );
        if self.csd {
            // The header bar is the title bar: registered first so its buttons win the pointer.
            window::drag_area(
                ui,
                ui.max_rect().expand(6.0),
                "toolbar_move",
                self.edge.is_some(),
            );
        }
        let mut left = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        left.spacing_mut().item_spacing.x = 4.0;
        if self.csd && !self.window_buttons.left.is_empty() {
            window::buttons(&mut left, &self.window_buttons.left);
            left.add_space(4.0);
        }
        let shown = self.sidebar_visible(&ctx);
        if theme::icon_toggle(
            &mut left,
            Icon::SidebarShow,
            "Queue",
            &format!("Show the queue ({})", self.key_hint(Action::ToggleQueue)),
            shown,
        )
        .clicked()
        {
            self.perform(&ctx, Action::ToggleQueue);
        }
        if theme::icon_button(
            &mut left,
            Icon::Open,
            "Open…",
            &format!("Open files ({})", self.key_hint(Action::Open)),
        )
        .clicked()
        {
            self.perform(&ctx, Action::Open);
        }
        if theme::icon_button(
            &mut left,
            Icon::Link,
            "Open link…",
            &format!(
                "Open a stream or video page ({})",
                self.key_hint(Action::OpenUrl)
            ),
        )
        .clicked()
        {
            self.perform(&ctx, Action::OpenUrl);
        }

        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::right_to_left(Align::Center)),
        );
        right.spacing_mut().item_spacing.x = 4.0;
        if self.csd && !self.window_buttons.right.is_empty() {
            let outermost_first: Vec<_> = self.window_buttons.right.iter().rev().copied().collect();
            window::buttons(&mut right, &outermost_first);
            right.add_space(4.0);
        }
        let more = theme::menu_icon(
            &mut right,
            Icon::Menu,
            "Main menu",
            "Main menu",
            240.0,
            |ui| self.more_menu(ui, &ctx),
        );
        geometry::note(&ctx, "…", more.rect);
        theme::menu_icon(&mut right, Icon::Video, "Video", "Video", 220.0, |ui| {
            self.video_menu(ui, &ctx)
        });
        theme::menu_icon(
            &mut right,
            Icon::Subtitles,
            "Audio & subtitles",
            "Audio and subtitles",
            200.0,
            |ui| self.audio_menu(ui, &ctx),
        );
    }

    /// Click, double-click, scroll and right-click on the picture.
    fn video_interaction(&mut self, ui: &mut egui::Ui, rect: egui::Rect, compact: bool) {
        let ctx = ui.ctx().clone();
        // The mini player has no header bar, so dragging the picture moves the window.
        let movable = compact && !is_fullscreen(&ctx);
        let sense = if movable {
            Sense::click_and_drag()
        } else {
            Sense::click()
        };
        let response = ui.interact(rect, ui.id().with("video"), sense);
        if movable && response.drag_started_by(egui::PointerButton::Primary) && self.edge.is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
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
        let p = theme::palette(ui.ctx());
        let current = self.session.queue.current.is_some();
        let state = self.session.state();
        if !current && !self.dropping {
            self.empty_state(ui, rect);
        } else if let Some(error) = self.session.error().map(str::to_owned) {
            self.error_state(ui, rect, &error);
        } else if self.session.streams.activity().is_some()
            || matches!(state, State::Opening | State::Buffering)
        {
            let spinner = egui::Spinner::new()
                .size(32.0)
                .color(Color32::from_white_alpha(200));
            ui.put(
                egui::Rect::from_center_size(rect.center(), Vec2::splat(32.0)),
                spinner,
            );
        }

        // Messages and what is going on (connecting, a loop) sit in a small dark pill.
        let message = self
            .session
            .flash_message()
            .map(str::to_owned)
            .or_else(|| self.notice());
        if let Some(message) = message {
            let painter = ui.painter();
            let font = FontId::proportional(self.body_size.round());
            let galley = painter.layout(
                message,
                font,
                Color32::WHITE,
                (rect.width() - 48.0).max(80.0),
            );
            let size = galley.size() + Vec2::new(24.0, 12.0);
            let pill = egui::Rect::from_center_size(
                rect.center_top() + Vec2::new(0.0, GAP + size.y / 2.0),
                size,
            );
            painter.rect_filled(
                pill,
                CornerRadius::same(theme::WINDOW_RADIUS),
                Color32::from_black_alpha(180),
            );
            painter.galley(pill.min + Vec2::new(12.0, 6.0), galley, Color32::WHITE);
        }

        if self.dropping {
            let painter = ui.painter();
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(150));
            painter.rect_stroke(
                rect.shrink(GAP),
                CornerRadius::same(theme::WINDOW_RADIUS),
                Stroke::new(2.0_f32, p.accent),
                StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Drop to play",
                FontId::proportional(20.0),
                Color32::WHITE,
            );
        }
    }

    /// What went wrong with the current entry, laid out like a GNOME status page.
    fn error_state(&self, ui: &mut egui::Ui, rect: egui::Rect, error: &str) {
        let title = self
            .session
            .queue
            .current
            .and_then(|c| self.session.queue.items.get(c))
            .map(|item| item.title.clone())
            .unwrap_or_default();
        let width = (rect.width() - 48.0).clamp(80.0, 420.0);
        let tall = rect.height() > 260.0;
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(egui::Rect::from_center_size(
                    rect.center(),
                    Vec2::new(width, rect.height()),
                ))
                .layout(Layout::top_down(Align::Center)),
        );
        child.spacing_mut().item_spacing.y = 0.0;
        // Centred on the height the page took last frame.
        let height_id = egui::Id::new("error-state-height");
        let content: f32 = ui.ctx().data(|d| d.get_temp(height_id)).unwrap_or(160.0);
        let top = ((rect.height() - content) / 2.0).max(8.0);
        child.add_space(top);
        if tall {
            let (at, _) = child.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
            icons::paint(&child, Icon::Error, at, 64.0, Color32::from_white_alpha(90));
            child.add_space(20.0);
        }
        // Over the black picture, so drawn light whatever the desktop scheme.
        let heading = (self.body_size * 1.4).round();
        child.add(
            egui::Label::new(
                RichText::new("Unable to play")
                    .size(heading)
                    .strong()
                    .color(Color32::WHITE),
            )
            .selectable(false),
        );
        child.add_space(8.0);
        if !title.is_empty() {
            child
                .add(
                    egui::Label::new(RichText::new(&title).color(Color32::from_gray(220)))
                        .truncate()
                        .selectable(false),
                )
                .on_hover_text(&title);
            child.add_space(4.0);
        }
        child.add(
            egui::Label::new(RichText::new(error).color(Color32::from_gray(170)))
                .wrap()
                .halign(Align::Center)
                .selectable(false),
        );
        let used = child.min_rect().height() - top;
        ui.ctx().data_mut(|d| d.insert_temp(height_id, used));
    }

    /// Ongoing work worth a word over the picture: opening a link, or an A-B loop.
    fn notice(&self) -> Option<String> {
        self.session.queue.current?;
        if let Some(activity) = self.session.streams.activity() {
            return Some(
                match activity {
                    Activity::Checking => "Opening link…",
                    Activity::Streamlink => "Connecting with Streamlink…",
                    Activity::Ytdlp => "Resolving link with yt-dlp…",
                }
                .to_owned(),
            );
        }
        match self.session.loop_points() {
            (Some(a), b) => Some(format!(
                "Loop {} – {}",
                clock(a),
                b.map_or("…".to_owned(), clock)
            )),
            _ => None,
        }
    }

    /// The welcome page, laid out like a GNOME status page: what to do, and recent media
    /// when there is room.
    fn empty_state(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let ctx = ui.ctx().clone();
        let p = theme::palette(&ctx);
        ui.painter().rect_filled(rect, 0.0, p.window);
        let queued = !self.session.queue.items.is_empty();
        let tall = rect.height() > 330.0;
        let recent: Vec<String> = if rect.height() > 560.0 && !queued {
            self.session
                .store
                .settings
                .recent
                .iter()
                .take(3)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let icon = if tall { 96.0 } else { 0.0 };
        // Centred on the height the page took last frame.
        let height_id = egui::Id::new("empty-state-height");
        let content: f32 = ctx.data(|d| d.get_temp(height_id)).unwrap_or(300.0);
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(24.0, 0.0)))
                .layout(Layout::top_down(Align::Center)),
        );
        child.spacing_mut().item_spacing.y = 0.0;
        let top = ((rect.height() - content) / 2.0).max(8.0);
        child.add_space(top);
        if tall {
            let (at, _) = child.allocate_exact_size(Vec2::splat(icon), Sense::hover());
            icons::paint(
                &child,
                Icon::VideoFile,
                at,
                icon,
                p.text.gamma_multiply(0.35),
            );
            child.add_space(28.0);
        }
        let (heading, detail) = if queued {
            ("Ready when you are", "Press play to start the queue.")
        } else {
            ("Drop media here", "Or open a file or a link.")
        };
        theme::heading(&mut child, heading, Some((self.body_size * 1.6).round()));
        child.add_space(10.0);
        child.label(theme::muted(detail));
        child.add_space(28.0);
        if queued {
            if theme::pill(&mut child, "Play", true, 200.0).clicked() {
                self.session.toggle();
            }
        } else {
            if theme::pill(&mut child, "Open…", true, 200.0).clicked() {
                self.perform(&ctx, Action::Open);
            }
            child.add_space(12.0);
            if theme::pill(&mut child, "Open link…", false, 200.0).clicked() {
                self.perform(&ctx, Action::OpenUrl);
            }
        }
        if !recent.is_empty() {
            child.add_space(36.0);
            let width = 360.0_f32.min(rect.width() - 48.0);
            child.allocate_ui_with_layout(
                Vec2::new(width, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(width);
                    theme::group_title(ui, "Recent");
                    ui.add_space(6.0);
                    theme::boxed_list(ui, |list| {
                        for source in recent {
                            let icon = if is_audio(&source) {
                                Icon::AudioFile
                            } else {
                                Icon::VideoFile
                            };
                            let name = crate::queue::title_of(&source);
                            if list.button_row(&name, Some(icon), &source).clicked() {
                                self.session.add(&[source], true);
                            }
                        }
                    });
                },
            );
        }
        let used = child.min_rect().height() - top;
        ctx.data_mut(|d| d.insert_temp(height_id, used));
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
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_sized(
                [56.0, 24.0],
                egui::Label::new(RichText::new(clock(shown)).weak()),
            );
            let width = (ui.available_width() - 60.0).max(40.0);
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
                RichText::new("Live").color(theme::palette(ui.ctx()).accent_text)
            } else {
                RichText::new(clock(duration.unwrap_or(0))).weak()
            };
            ui.add_sized([56.0, 24.0], egui::Label::new(end));
        });
    }

    /// Speed and quality on the left, the playback buttons in the middle, sound and
    /// fullscreen on the right.
    fn transport_row(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), PLAY_SIZE), Sense::hover());
        let narrow = rect.width() < NARROW - 60.0;

        // The middle group, centred on the row.
        let group = 4.0 * theme::BUTTON_HEIGHT + PLAY_SIZE + 4.0 * 2.0 + 2.0 * 6.0;
        let centre = egui::Rect::from_center_size(rect.center(), Vec2::new(group, PLAY_SIZE));
        let mut middle = ui.new_child(
            UiBuilder::new()
                .max_rect(centre)
                .layout(Layout::left_to_right(Align::Center)),
        );
        middle.spacing_mut().item_spacing.x = 2.0;
        let hint = |action| self.key_hint(action);
        let (shuffle_tip, prev_tip, next_tip, play_tip) = (
            "Shuffle".to_owned(),
            format!("Previous ({})", hint(Action::Previous)),
            format!("Next ({})", hint(Action::Next)),
            hint(Action::Play),
        );
        let queue = &mut self.session.queue;
        if theme::icon_toggle(
            &mut middle,
            Icon::Shuffle,
            "Shuffle",
            &shuffle_tip,
            queue.shuffle,
        )
        .clicked()
        {
            queue.shuffle = !queue.shuffle;
        }
        if theme::icon_button(&mut middle, Icon::Previous, "Previous", &prev_tip).clicked() {
            self.session.advance(-1);
        }
        middle.add_space(4.0);
        let playing = self.session.state() == State::Playing;
        let (icon, name) = if playing {
            (Icon::Pause, "Pause")
        } else {
            (Icon::Play, "Play")
        };
        if theme::circular(
            &mut middle,
            icon,
            name,
            &format!("{name} ({play_tip})"),
            PLAY_SIZE,
        )
        .clicked()
        {
            self.session.toggle();
        }
        middle.add_space(4.0);
        if theme::icon_button(&mut middle, Icon::Next, "Next", &next_tip).clicked() {
            self.session.advance(1);
        }
        let queue = &mut self.session.queue;
        if theme::icon_toggle(
            &mut middle,
            Icon::Repeat,
            "Repeat",
            "Repeat the queue",
            queue.repeat,
        )
        .clicked()
        {
            queue.repeat = !queue.repeat;
        }

        // Speed and quality.
        if !narrow {
            let room =
                egui::Rect::from_min_max(rect.min, egui::pos2(centre.left() - 8.0, rect.max.y));
            let mut left = ui.new_child(
                UiBuilder::new()
                    .max_rect(room)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            left.spacing_mut().item_spacing.x = 2.0;
            self.speed_menu(&mut left);
            self.quality_menu(&mut left);
        }

        // Sound and fullscreen.
        let room = egui::Rect::from_min_max(egui::pos2(centre.right() + 8.0, rect.min.y), rect.max);
        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(room)
                .layout(Layout::right_to_left(Align::Center)),
        );
        right.spacing_mut().item_spacing.x = 2.0;
        let fullscreen = is_fullscreen(&ctx);
        let (icon, name) = if fullscreen {
            (Icon::LeaveFullscreen, "Leave fullscreen")
        } else {
            (Icon::Fullscreen, "Fullscreen")
        };
        let tip = format!("{name} ({})", self.key_hint(Action::Fullscreen));
        if theme::icon_button(&mut right, icon, name, &tip).clicked() {
            self.perform(&ctx, Action::Fullscreen);
        }
        let volume = self.session.store.settings.volume;
        let muted = self.session.engine.muted();
        if !narrow && room.width() > 2.0 * theme::BUTTON_HEIGHT + 100.0 {
            let output = bar::show(
                &mut right,
                Vec2::new(100.0, 24.0),
                &Bar {
                    value: volume as f32 / 200.0,
                    enabled: true,
                    markers: &[0.5],
                    band: None,
                },
            );
            geometry::note(&ctx, "Volume bar", output.response.rect);
            output.response.on_hover_text(format!("Volume {volume}%"));
            if let (true, Some(value)) = (output.pressed || output.released, output.pointer) {
                self.session
                    .set_volume((value * 200.0 / 5.0).round() as u32 * 5);
            }
        }
        let icon = match volume {
            _ if muted => Icon::Muted,
            0 => Icon::Muted,
            1..=33 => Icon::VolumeLow,
            34..=80 => Icon::VolumeMedium,
            _ => Icon::VolumeHigh,
        };
        let (name, verb) = if muted {
            ("Unmute", "Unmute")
        } else {
            ("Mute", "Mute")
        };
        let tip = format!(
            "{verb} ({}) · volume {volume}%",
            self.key_hint(Action::Mute)
        );
        if theme::icon_button(&mut right, icon, name, &tip).clicked() {
            self.session.toggle_mute();
        }
    }

    fn speed_menu(&mut self, ui: &mut egui::Ui) {
        let rate = self.session.rate();
        let mut chosen = None;
        let tip = format!(
            "Playback speed ({} / {})",
            self.key_hint(Action::Slower),
            self.key_hint(Action::Faster)
        );
        theme::menu_text(ui, &format!("{rate}×"), "Speed", &tip, |ui| {
            menus::begin(ui, 120.0, true);
            for candidate in RATES {
                if menus::choice(ui, &format!("{candidate}×"), candidate == rate) {
                    chosen = Some(candidate);
                }
            }
        });
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
        theme::menu_text(ui, &current, "Quality", "Stream quality", |ui| {
            menus::begin(ui, 140.0, true);
            for quality in &qualities {
                if menus::choice(ui, quality, *quality == current) {
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
            // Controls float over the picture on a dark fade, drawn light whatever the
            // desktop scheme, as GNOME's video player does.
            let normal = theme::palette(ctx);
            theme::use_palette(
                ctx,
                theme::Palette::new(appearance::Scheme::Dark, normal.accent),
            );
            egui::Area::new(egui::Id::new("overlay_controls"))
                .anchor(Align2::LEFT_BOTTOM, Vec2::ZERO)
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    *ui.visuals_mut() = egui::Visuals::dark();
                    let visuals = ui.visuals_mut();
                    visuals.widgets.noninteractive.fg_stroke.color = Color32::WHITE;
                    visuals.weak_text_color = Some(Color32::from_gray(210));
                    let fade = ui.painter().add(egui::Shape::Noop);
                    let margin = if fullscreen { 24 } else { 12 };
                    let frame = egui::Frame::new()
                        .inner_margin(Margin {
                            left: margin,
                            right: margin,
                            top: 40,
                            bottom: if fullscreen { 16 } else { 8 },
                        })
                        .show(ui, |ui| {
                            ui.set_width(width - 2.0 * f32::from(margin));
                            self.seek_row(ui);
                            self.transport_row(ui);
                        });
                    let rect = frame.response.rect;
                    let mut mesh = egui::Mesh::default();
                    let clear = Color32::TRANSPARENT;
                    let dark = Color32::from_black_alpha(190);
                    mesh.colored_vertex(rect.left_top(), clear);
                    mesh.colored_vertex(rect.right_top(), clear);
                    mesh.colored_vertex(rect.right_bottom(), dark);
                    mesh.colored_vertex(rect.left_bottom(), dark);
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    ui.painter().set(fade, egui::Shape::mesh(mesh));
                });
            theme::use_palette(ctx, normal);
        }
    }
}

/// Whether `source` names a sound file, judging by its extension.
fn is_audio(source: &str) -> bool {
    const AUDIO: [&str; 12] = [
        "mp3", "flac", "wav", "ogg", "oga", "opus", "m4a", "aac", "wma", "alac", "ape", "mka",
    ];
    std::path::Path::new(source)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO.contains(&e.to_ascii_lowercase().as_str()))
}

pub fn is_fullscreen(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}
