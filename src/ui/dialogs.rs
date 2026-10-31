//! Windows over the player: open a link, preferences with key rebinding, media information.

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{self, Align, Align2, Layout, RichText, Vec2};

use super::{Frontend, PrefsTab, UrlDialog, geometry, theme};
use crate::keys::{self, Action, binding_text};
use crate::streams::{looks_like_local_path, normalize_url_input, strip_doubled_scheme};
use crate::tuning;

fn window<'a>(ctx: &egui::Context, title: &'a str) -> egui::Window<'a> {
    egui::Window::new(RichText::new(title).strong())
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(18)))
}

/// The one spacing unit between groups.
const GAP: f32 = 12.0;

/// The heading that starts a group of settings.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(GAP / 2.0);
    theme::heading(ui, title, None);
}

fn tool_status(ui: &mut egui::Ui, name: &str, found: bool, purpose: &str) {
    let p = theme::palette(ui.ctx());
    ui.horizontal(|ui| {
        if found {
            ui.label(RichText::new("✔").color(p.accent_text));
            ui.label(format!("{name} is installed"));
        } else {
            ui.label(RichText::new("✖").color(p.error));
            ui.label(format!("{name} was not found. Install it to {purpose}."));
        }
    });
}

/// Puts the text cursor at the end of the field `id`.
fn cursor_to_end(ui: &egui::Ui, id: egui::Id, text: &str) {
    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
        let end = CCursor::new(text.chars().count());
        state.cursor.set_char_range(Some(CCursorRange::one(end)));
        state.store(ui.ctx(), id);
    }
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
            mut fresh,
        }) = self.url.take()
        else {
            return;
        };
        let p = theme::palette(ctx);
        let mut open = true;
        let mut submit = false;
        let mut cancel = false;
        window(ctx, "Open network stream")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.spacing_mut().item_spacing.y = GAP;
                ui.label(theme::muted(
                    "A stream or video page. Twitch, Kick and YouTube links play through Streamlink and yt-dlp.",
                ));
                let field = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(10, 8))
                        .hint_text("twitch.tv/channel"),
                );
                if field.changed() {
                    rejected = false;
                    // A whole link pasted after the prefilled https:// replaces it.
                    if let Some(fixed) = strip_doubled_scheme(&text) {
                        text = fixed;
                        cursor_to_end(ui, field.id, &text);
                    }
                }
                if fresh {
                    // Start typing after the https:// rather than in front of it.
                    cursor_to_end(ui, field.id, &text);
                    fresh = false;
                }
                field.request_focus();
                if rejected {
                    let message = if looks_like_local_path(&text) {
                        "That is a file on this computer. Use Open… for files."
                    } else {
                        "Enter a supported network URL."
                    };
                    ui.colored_label(p.error, message);
                }
                ui.add_space(GAP / 2.0);
                let row = Vec2::new(ui.available_width(), theme::BUTTON_HEIGHT);
                ui.allocate_ui_with_layout(row, Layout::right_to_left(Align::Center), |ui| {
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    submit = theme::suggested(ui, "Open", Some(88.0)).clicked() || enter;
                    if theme::button(ui, "Cancel").clicked() {
                        cancel = true;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        if submit {
            if let Some(link) = normalize_url_input(&text) {
                self.session.add(&[link], true);
                return;
            }
            rejected = true;
        } else if !open || cancel {
            return;
        }
        self.url = Some(UrlDialog {
            text,
            rejected,
            fresh,
        });
    }

    fn preferences(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.prefs.take() else {
            return;
        };
        let mut open = true;
        let mut keys_changed = false;
        let mut settings_changed = false;
        let mut tuning_changed = false;
        let p = theme::palette(ctx);
        window(ctx, "Preferences").open(&mut open).show(ctx, |ui| {
            ui.set_min_width(500.0);
            ui.spacing_mut().item_spacing.y = GAP;
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (PrefsTab::Playback, "Playback"),
                    (PrefsTab::Picture, "Picture & sound"),
                    (PrefsTab::Streams, "Streams"),
                    (PrefsTab::Keys, "Keyboard"),
                ] {
                    let response = ui.selectable_value(&mut state.tab, tab, label);
                    geometry::note(ctx, label, response.rect);
                }
            });
            let s = &mut self.session.store.settings;
            match state.tab {
                PrefsTab::Playback => {
                    section(ui, "Seeking");
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
                    section(ui, "Resuming");
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
                PrefsTab::Picture => {
                    section(ui, "Decoding and HDR");
                    ui.horizontal(|ui| {
                        ui.label("Hardware decoding");
                        let shown = tuning::HWDEC
                            .iter()
                            .find(|(v, _)| *v == s.hwdec)
                            .map_or("Automatic (safe)", |(_, l)| *l);
                        egui::ComboBox::from_id_salt("hwdec")
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                for (value, label) in tuning::HWDEC {
                                    tuning_changed |= ui
                                        .selectable_value(&mut s.hwdec, value.to_owned(), label)
                                        .changed();
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("HDR tone mapping");
                        egui::ComboBox::from_id_salt("tone")
                            .selected_text(s.tone_mapping.clone())
                            .show_ui(ui, |ui| {
                                for value in tuning::TONE_MAPPING {
                                    tuning_changed |= ui
                                        .selectable_value(&mut s.tone_mapping, value.to_owned(), value)
                                        .changed();
                                }
                            });
                    });
                    ui.label(theme::muted(
                        "Video is drawn into an 8-bit RGBA texture, so HDR is shown tone-mapped to SDR.",
                    ));
                    section(ui, "Subtitles");
                    ui.horizontal(|ui| {
                        ui.label("Font");
                        tuning_changed |= ui
                            .add(egui::TextEdit::singleline(&mut s.subtitles.font).desired_width(160.0).hint_text("mpv default"))
                            .changed();
                        ui.label("Size");
                        tuning_changed |= ui
                            .add(egui::DragValue::new(&mut s.subtitles.size).range(8..=200))
                            .changed();
                    });
                    ui.horizontal(|ui| {
                        for (label, value) in [("Text", &mut s.subtitles.color), ("Outline", &mut s.subtitles.border_color)] {
                            ui.label(label);
                            let mut rgb = tuning::color_rgb(value).unwrap_or([255, 255, 255]);
                            if ui.color_edit_button_srgb(&mut rgb).changed() {
                                *value = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                                tuning_changed = true;
                            }
                        }
                        ui.label("Outline width");
                        tuning_changed |= ui
                            .add(egui::DragValue::new(&mut s.subtitles.border).range(0.0..=10.0).speed(0.1))
                            .changed();
                    });
                    ui.horizontal(|ui| {
                        ui.label("Position (0 top, 100 bottom)");
                        tuning_changed |= ui
                            .add(egui::Slider::new(&mut s.subtitles.pos, 0..=100))
                            .changed();
                    });
                    section(ui, "Picture");
                    for (label, value) in [
                        ("Brightness", &mut s.picture.brightness),
                        ("Contrast", &mut s.picture.contrast),
                        ("Saturation", &mut s.picture.saturation),
                        ("Gamma", &mut s.picture.gamma),
                        ("Hue", &mut s.picture.hue),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(format!("{label:<10}"));
                            tuning_changed |= ui.add(egui::Slider::new(value, -100..=100)).changed();
                        });
                    }
                    if ui.button("Reset picture").clicked() {
                        s.picture = Default::default();
                        tuning_changed = true;
                    }
                    section(ui, "Audio");
                    tuning_changed |= ui
                        .checkbox(&mut s.audio.night_mode, "Night mode (loudness normalisation, narrow range)")
                        .changed();
                    ui.horizontal(|ui| {
                        ui.label("ReplayGain");
                        for (value, label) in tuning::REPLAYGAIN {
                            tuning_changed |= ui
                                .selectable_value(&mut s.audio.replaygain, value.to_owned(), label)
                                .changed();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Output device");
                        let devices = self.session.engine.audio_devices();
                        let shown = devices
                            .iter()
                            .find(|(n, _)| *n == s.audio.device)
                            .map_or(s.audio.device.clone(), |(_, d)| d.clone());
                        egui::ComboBox::from_id_salt("audio-device")
                            .selected_text(shown)
                            .width(260.0)
                            .show_ui(ui, |ui| {
                                for (name, description) in &devices {
                                    tuning_changed |= ui
                                        .selectable_value(&mut s.audio.device, name.clone(), description)
                                        .changed();
                                }
                            });
                    });
                }
                PrefsTab::Streams => {
                    section(ui, "Quality");
                    ui.horizontal(|ui| {
                        ui.label("Default quality");
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut s.stream_quality)
                                .desired_width(110.0)
                                .hint_text("best"),
                        );
                        settings_changed |= field.changed();
                        // Fill in the default only once editing is over, so the field can be cleared while typing.
                        if field.lost_focus() && s.stream_quality.trim().is_empty() {
                            s.stream_quality = "best".to_owned();
                            settings_changed = true;
                        }
                        ui.label(theme::muted("best, or a name such as 720p60"));
                    });
                    section(ui, "Live and video sites");
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
                    section(ui, "Helper tools");
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
                        .max_height((ctx.content_rect().height() - 230.0).clamp(120.0, 330.0))
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
                                        geometry::note(
                                            ctx,
                                            &format!("Shortcut for {}", action.label()),
                                            key_button.rect,
                                        );
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
                        ui.colored_label(p.error, message);
                    }
                }
            }
            ui.add_space(GAP / 2.0);
            let version = self.session.engine.version();
            ui.label(theme::muted(format!(
                "Lumen {} · {}",
                env!("CARGO_PKG_VERSION"),
                version
            )));
        });
        if keys_changed {
            self.rebuild_keymap();
        }
        if tuning_changed {
            self.session.apply_settings();
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
        window(ctx, "Media information")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.spacing_mut().item_spacing.y = GAP / 2.0;
                match source {
                    None => {
                        ui.label(theme::muted("Nothing is playing."));
                    }
                    Some(source) => {
                        ui.add(
                            egui::Label::new(
                                RichText::new(crate::queue::title_of(&source)).strong(),
                            )
                            .truncate(),
                        );
                        ui.add(egui::Label::new(theme::muted(&source)).truncate());
                        section(ui, "Details");
                        egui::Grid::new("info")
                            .num_columns(2)
                            .spacing([18.0, 6.0])
                            .show(ui, |ui| {
                                for (label, value) in &rows {
                                    ui.label(theme::muted(label));
                                    ui.add(egui::Label::new(value).truncate())
                                        .on_hover_text(value);
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
