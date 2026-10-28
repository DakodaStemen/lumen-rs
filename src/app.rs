//! The egui window: video, transport controls, play queue and key handling.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};
use eframe::{egui_glow, glow};

use crate::clock::clock;
use crate::engine::TrackKind;
use crate::keys::{Action, Keymap};
use crate::session::{RATES, Session, State};
use crate::video::Video;

const ACCENT: Color32 = Color32::from_rgb(0xf8, 0x9b, 0x62);
const TICK: Duration = Duration::from_millis(200);
/// Controls hide in fullscreen after the pointer has been still this long.
const CHROME_HIDE: Duration = Duration::from_secs(2);

pub struct App {
    session: Session,
    keymap: Keymap,
    video: Arc<Mutex<Option<Video>>>,
    show_queue: bool,
    selected: Option<usize>,
    /// Seek-bar position (0..=1) while it is being dragged.
    scrubbing: Option<f64>,
    last_tick: Instant,
    last_pointer_move: Instant,
    title: String,
}

impl App {
    pub fn new(session: Session, video: Video, ctx: &egui::Context) -> Self {
        apply_style(ctx);
        let keymap = Keymap::from_settings(&session.store.settings.keys);
        Self {
            session,
            keymap,
            video: Arc::new(Mutex::new(Some(video))),
            show_queue: true,
            selected: None,
            scrubbing: None,
            last_tick: Instant::now(),
            last_pointer_move: Instant::now(),
            title: String::new(),
        }
    }

    fn pressed_actions(&self, ctx: &egui::Context) -> Vec<Action> {
        if ctx.wants_keyboard_input() {
            return Vec::new();
        }
        let mut pressed = Vec::new();
        ctx.input_mut(|input| {
            for (action, binding) in self.keymap.bindings() {
                if input.consume_key(binding.modifiers, binding.key) {
                    pressed.push(*action);
                }
            }
        });
        pressed
    }

    fn perform(&mut self, ctx: &egui::Context, action: Action) {
        let s = &mut self.session;
        let step = s.seek_step_ms();
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
            Action::AudioDelayUp => s.shift_delay(TrackKind::Audio, 50),
            Action::AudioDelayDown => s.shift_delay(TrackKind::Audio, -50),
            Action::SubDelayUp => s.shift_delay(TrackKind::Subtitle, 50),
            Action::SubDelayDown => s.shift_delay(TrackKind::Subtitle, -50),
            Action::CycleAudio => s.cycle_track(TrackKind::Audio),
            Action::CycleSub => s.cycle_track(TrackKind::Subtitle),
            Action::Screenshot => s.screenshot(),
            Action::Loop => s.ab_loop(),
            Action::Bookmark => s.add_bookmark(),
            Action::Fullscreen => {
                let fullscreen = is_fullscreen(ctx);
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
            }
            Action::ExitFullscreen => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false))
            }
            Action::ToggleQueue => self.show_queue = !self.show_queue,
            Action::Remove => {
                if let Some(index) = self.selected.take() {
                    self.session.remove(index);
                }
            }
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        let position = self.session.position_ms();
        let duration = self.session.engine.duration_ms();
        let seekable = self.session.engine.seekable();
        let live = duration.is_none() && self.session.state() == State::Playing;

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let shown = self
                .scrubbing
                .zip(duration)
                .map_or(position, |(f, d)| (f * d as f64) as i64);
            ui.monospace(clock(shown));
            let mut fraction = self.scrubbing.unwrap_or_else(|| {
                duration
                    .filter(|d| *d > 0)
                    .map_or(0.0, |d| (position as f64 / d as f64).clamp(0.0, 1.0))
            });
            ui.spacing_mut().slider_width = (ui.available_width() - 80.0).max(50.0);
            let bar = ui.add_enabled(
                seekable && duration.is_some(),
                egui::Slider::new(&mut fraction, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            );
            if bar.changed() {
                self.scrubbing = Some(fraction);
            }
            if (bar.drag_stopped() || (bar.changed() && !bar.dragged()))
                && let (Some(f), Some(d)) = (self.scrubbing.take(), duration)
            {
                self.session.seek_ms((f * d as f64) as i64);
            }
            ui.monospace(if live {
                "● LIVE".to_owned()
            } else {
                clock(duration.unwrap_or(0))
            });
        });

        ui.horizontal(|ui| {
            let s = &mut self.session;
            if ui.button("⏮").on_hover_text("Previous").clicked() {
                s.advance(-1);
            }
            let playing = s.state() == State::Playing;
            if ui
                .button(if playing { "⏸" } else { "▶" })
                .on_hover_text("Play / pause")
                .clicked()
            {
                s.toggle();
            }
            if ui.button("⏹").on_hover_text("Stop").clicked() {
                s.stop();
            }
            if ui.button("⏭").on_hover_text("Next").clicked() {
                s.advance(1);
            }
            ui.separator();
            if ui
                .button(if s.engine.muted() { "🔇" } else { "🔊" })
                .on_hover_text("Mute")
                .clicked()
            {
                s.toggle_mute();
            }
            let mut volume = s.store.settings.volume;
            ui.spacing_mut().slider_width = 110.0;
            if ui
                .add(egui::Slider::new(&mut volume, 0..=200).suffix("%"))
                .changed()
            {
                s.set_volume(volume);
            }
            ui.separator();
            egui::ComboBox::from_id_salt("speed")
                .width(64.0)
                .selected_text(format!("{}×", s.rate()))
                .show_ui(ui, |ui| {
                    for rate in RATES {
                        if ui
                            .selectable_label(rate == s.rate(), format!("{rate}×"))
                            .clicked()
                        {
                            s.set_rate(rate);
                        }
                    }
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .selectable_label(self.show_queue, "☰")
                    .on_hover_text("Play queue")
                    .clicked()
                {
                    self.show_queue = !self.show_queue;
                }
                ui.label(RichText::new(s.status()).small().color(Color32::GRAY));
            });
        });
        ui.add_space(2.0);
    }

    fn queue_panel(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.session;
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("PLAY QUEUE  ·  {}", s.queue.items.len()))
                .small()
                .color(Color32::GRAY),
        );
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.queue.shuffle, "Shuffle");
            ui.checkbox(&mut s.queue.repeat, "Repeat");
        });
        ui.separator();
        let mut play = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, item) in s.queue.items.iter().enumerate() {
                    let current = s.queue.current == Some(index);
                    let label = if current {
                        RichText::new(&item.title).color(ACCENT)
                    } else {
                        RichText::new(&item.title)
                    };
                    let row = ui
                        .selectable_label(self.selected == Some(index), label)
                        .on_hover_text(&item.source);
                    if row.clicked() {
                        self.selected = Some(index);
                    }
                    if row.double_clicked() {
                        play = Some(index);
                    }
                }
                if s.queue.items.is_empty() {
                    ui.label(RichText::new("Drop files here to play them.").color(Color32::GRAY));
                }
            });
        if let Some(index) = play {
            s.play_index(index, None, false);
        }
    }

    fn video_view(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let rect = ui.max_rect();
        let scale = ui.ctx().pixels_per_point();
        let (width, height) = (
            (rect.width() * scale).round() as i32,
            (rect.height() * scale).round() as i32,
        );
        let Some(gl) = frame.gl().cloned() else {
            return;
        };

        let (texture, uv) = {
            let mut guard = self.video.lock().unwrap();
            let Some(video) = guard.as_mut() else { return };
            video.set_size(width, height);
            (video.texture(&gl, frame), video.uv())
        };
        let video = self.video.clone();
        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
                if let Some(video) = video.lock().unwrap().as_mut() {
                    video.draw(painter.gl());
                }
            })),
        });
        ui.painter().image(texture, rect, uv, Color32::WHITE);

        if self.session.queue.current.is_none() && self.session.queue.items.is_empty() {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop a file to play",
                egui::FontId::proportional(20.0),
                Color32::from_gray(120),
            );
        }
        if ui
            .interact(rect, ui.id().with("video"), egui::Sense::click())
            .double_clicked()
        {
            self.perform(ui.ctx(), Action::Fullscreen);
        }
    }
}

fn is_fullscreen(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}

fn apply_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    visuals.hyperlink_color = ACCENT;
    visuals.panel_fill = Color32::from_rgb(0x14, 0x15, 0x17);
    ctx.set_visuals(visuals);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let dropped: Vec<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.as_ref().map(|p| p.to_string_lossy().into_owned()))
                .collect()
        });
        if !dropped.is_empty() {
            self.session.add(&dropped, true);
        }
        for action in self.pressed_actions(ctx) {
            self.perform(ctx, action);
        }
        if self.last_tick.elapsed() >= TICK {
            self.last_tick = Instant::now();
            self.session.tick();
        }
        if ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_down()) {
            self.last_pointer_move = Instant::now();
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

        let chrome = !is_fullscreen(ctx) || self.last_pointer_move.elapsed() < CHROME_HIDE;
        if chrome {
            egui::TopBottomPanel::bottom("controls").show(ctx, |ui| self.controls(ui));
            if self.show_queue {
                egui::SidePanel::right("queue")
                    .default_width(280.0)
                    .show(ctx, |ui| self.queue_panel(ui));
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ctx, |ui| self.video_view(ui, frame));

        ctx.request_repaint_after(TICK);
    }

    fn on_exit(&mut self, _gl: Option<&glow::Context>) {
        self.session.stop();
        self.session.store.save_settings();
        // The render context must be freed while eframe's GL context is still current.
        self.video.lock().unwrap().take();
    }
}
