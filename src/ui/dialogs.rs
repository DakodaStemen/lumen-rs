//! Windows over the player: open a link, preferences with key rebinding, media information.

use eframe::egui::{self, Align2, Color32, RichText, Vec2};

use super::{Frontend, PrefsTab, UrlDialog, theme};
use crate::keys::{self, Action, binding_text};
use crate::streams::is_network_url;

fn window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
}

fn tool_status(ui: &mut egui::Ui, name: &str, found: bool, purpose: &str) {
    ui.horizontal(|ui| {
        if found {
            ui.label(RichText::new("✔").color(Color32::from_rgb(0x7f, 0xd1, 0x8b)));
            ui.label(format!("{name} is installed"));
        } else {
            ui.label(RichText::new("✖").color(Color32::from_rgb(0xf0, 0x8a, 0x80)));
            ui.label(format!("{name} was not found. Install it to {purpose}."));
        }
    });
}

impl Frontend {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.url_dialog(ctx);
        self.preferences(ctx);
        self.info_window(ctx);
    }

    fn url_dialog(&mut self, ctx: &egui::Context) {
        let Some(UrlDialog {
            mut text,
            mut rejected,
        }) = self.url.take()
        else {
            return;
        };
        let mut open = true;
        let mut submit = false;
        window("Open network stream")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(440.0);
                ui.label("Stream or video page URL (HTTP, HTTPS, RTSP, RTMP, UDP).");
                ui.label(theme::muted(
                    "Twitch, Kick and YouTube links play through Streamlink and yt-dlp.",
                ));
                let field = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(f32::INFINITY)
                        .hint_text("https://"),
                );
                field.request_focus();
                if rejected {
                    ui.colored_label(
                        Color32::from_rgb(0xf0, 0x8a, 0x80),
                        "Enter a supported network URL.",
                    );
                }
                ui.horizontal(|ui| {
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    submit = theme::primary(ui, "Open", Some(100.0)).clicked() || enter;
                });
            });
        if submit {
            if is_network_url(&text) {
                self.session.add(&[text.trim().to_owned()], true);
                return;
            }
            rejected = true;
        } else if !open {
            return;
        }
        self.url = Some(UrlDialog { text, rejected });
    }

    fn preferences(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.prefs.take() else {
            return;
        };
        let mut open = true;
        let mut keys_changed = false;
        let mut settings_changed = false;
        window("Preferences").open(&mut open).show(ctx, |ui| {
            ui.set_min_width(500.0);
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (PrefsTab::Playback, "Playback"),
                    (PrefsTab::Streams, "Streams"),
                    (PrefsTab::Keys, "Keyboard"),
                ] {
                    ui.selectable_value(&mut state.tab, tab, label);
                }
            });
            ui.separator();
            let s = &mut self.session.store.settings;
            match state.tab {
                PrefsTab::Playback => {
                    ui.horizontal(|ui| {
                        ui.label("Seek keys move");
                        settings_changed |= ui
                            .add(
                                egui::DragValue::new(&mut s.seek_step)
                                    .range(1..=120)
                                    .suffix(" seconds"),
                            )
                            .changed();
                    });
                    settings_changed |= ui
                        .checkbox(&mut s.resume, "Resume where you left off")
                        .changed();
                    settings_changed |= ui
                        .checkbox(
                            &mut s.remember_tracks,
                            "Remember audio and subtitle choices and sync for each file",
                        )
                        .changed();
                    settings_changed |= ui
                        .checkbox(
                            &mut s.auto_next_folder,
                            "Continue with the next file in the folder",
                        )
                        .changed();
                }
                PrefsTab::Streams => {
                    ui.horizontal(|ui| {
                        ui.label("Default quality");
                        settings_changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut s.stream_quality)
                                    .desired_width(110.0),
                            )
                            .changed();
                        ui.label(theme::muted("best, or a name such as 720p60"));
                    });
                    settings_changed |= ui
                        .checkbox(&mut s.low_latency, "Low-latency mode for Twitch and Kick")
                        .changed();
                    settings_changed |= ui
                        .checkbox(
                            &mut s.sponsorblock,
                            "Skip sponsor reads in YouTube videos (SponsorBlock)",
                        )
                        .changed();
                    ui.add_enabled_ui(s.sponsorblock, |ui| {
                        settings_changed |= ui
                            .checkbox(&mut s.sponsorblock_intros, "Also skip intros and outros")
                            .changed();
                    });
                    if s.stream_quality.trim().is_empty() {
                        s.stream_quality = "best".to_owned();
                    }
                    ui.separator();
                    let tools = &self.session.streams.tools;
                    tool_status(
                        ui,
                        "Streamlink",
                        tools.streamlink.is_some(),
                        "watch Twitch, Kick and similar live streams",
                    );
                    tool_status(
                        ui,
                        "yt-dlp",
                        tools.ytdlp.is_some(),
                        "play YouTube and other video sites",
                    );
                }
                PrefsTab::Keys => {
                    ui.label(theme::muted(
                        "Click a shortcut, then press the new keys. Escape cancels.",
                    ));
                    egui::ScrollArea::vertical()
                        .max_height(330.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            egui::Grid::new("keys")
                                .num_columns(3)
                                .spacing([14.0, 6.0])
                                .show(ui, |ui| {
                                    for &action in Action::ALL {
                                        ui.label(action.label());
                                        let capturing = state.capture == Some(action);
                                        let text = if capturing {
                                            "Press a key…".to_owned()
                                        } else {
                                            Some(binding_text(&s.keys, action))
                                                .filter(|t| !t.is_empty())
                                                .unwrap_or_else(|| "—".to_owned())
                                        };
                                        let button =
                                            egui::Button::new(RichText::new(text).monospace())
                                                .selected(capturing)
                                                .min_size(Vec2::new(150.0, 28.0));
                                        let key_button = ui.add(button);
                                        key_button.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::WidgetType::Button,
                                                true,
                                                format!("Shortcut for {}", action.label()),
                                            )
                                        });
                                        if key_button.clicked() {
                                            state.capture = Some(action);
                                            state.message = None;
                                        }
                                        ui.horizontal(|ui| {
                                            let overridden = s.keys.contains_key(action.id());
                                            if ui
                                                .add_enabled(overridden, egui::Button::new("↻"))
                                                .on_hover_text("Back to the default")
                                                .clicked()
                                            {
                                                match keys::reset(&mut s.keys, action) {
                                                    Ok(()) => keys_changed = true,
                                                    Err(other) => {
                                                        state.message = Some(format!(
                                                            "{} is used by “{}”.",
                                                            action.default_key(),
                                                            other.label()
                                                        ))
                                                    }
                                                }
                                            }
                                            let bound = !binding_text(&s.keys, action).is_empty();
                                            if ui
                                                .add_enabled(bound, egui::Button::new("✖"))
                                                .on_hover_text("Remove this shortcut")
                                                .clicked()
                                            {
                                                keys::unbind(&mut s.keys, action);
                                                keys_changed = true;
                                            }
                                        });
                                        ui.end_row();
                                    }
                                });
                        });
                    if let Some(message) = &state.message {
                        ui.colored_label(Color32::from_rgb(0xf0, 0x8a, 0x80), message);
                    }
                }
            }
            ui.separator();
            let version = self.session.engine.version();
            ui.label(theme::muted(format!(
                "Lumen Rust {} · {}",
                env!("CARGO_PKG_VERSION"),
                version
            )));
        });
        if keys_changed {
            self.rebuild_keymap();
        }
        if keys_changed || settings_changed {
            self.session.store.save_settings();
        }
        if open {
            self.prefs = Some(state);
        }
    }

    fn info_window(&mut self, ctx: &egui::Context) {
        if !self.info {
            return;
        }
        let rows = self.session.engine.media_info();
        let source = self.session.source();
        let mut open = true;
        window("Media information").open(&mut open).show(ctx, |ui| {
            ui.set_min_width(380.0);
            match source {
                None => {
                    ui.label(theme::muted("Nothing is playing."));
                }
                Some(source) => {
                    ui.add(
                        egui::Label::new(RichText::new(crate::queue::title_of(&source)).strong())
                            .truncate(),
                    );
                    ui.add(egui::Label::new(theme::muted(&source)).truncate());
                    ui.add_space(6.0);
                    egui::Grid::new("info")
                        .num_columns(2)
                        .spacing([18.0, 6.0])
                        .show(ui, |ui| {
                            for (label, value) in &rows {
                                ui.label(theme::muted(label));
                                ui.label(value);
                                ui.end_row();
                            }
                        });
                    if rows.is_empty() {
                        ui.label(theme::muted("Still opening…"));
                    }
                }
            }
        });
        self.info = open;
    }
}
