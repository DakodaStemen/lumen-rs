//! Windows over the player: open a link, preferences with key rebinding, media information.

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, RichText, Stroke, Vec2};

use super::icons::{self, Icon};
use super::menus::choice;
use super::{Frontend, PrefsTab, UrlDialog, geometry, theme};
use crate::keys::{self, Action, binding_text};
use crate::streams::{looks_like_local_path, normalize_url_input, strip_doubled_scheme};
use crate::tuning;

/// The one spacing unit between groups.
const GAP: f32 = 12.0;

/// A dialog in the libadwaita style: dims the window behind it, a header with a centred bold
/// title and a round close button, then `content` with generous padding. Returns false once the
/// close button was pressed.
fn dialog(
    ctx: &egui::Context,
    title: &str,
    width: f32,
    content: impl FnOnce(&mut egui::Ui),
) -> bool {
    let p = theme::palette(ctx);
    let mut open = true;
    let width = width.min(ctx.content_rect().width() - 24.0).max(200.0);
    let body_height = (ctx.content_rect().height() - 2.0 * GAP - 46.0).max(60.0);
    egui::Modal::new(egui::Id::new(("lumen-dialog", title)))
        .backdrop_color(Color32::from_black_alpha(if p.dark() { 110 } else { 70 }))
        .frame(
            egui::Frame::new()
                .fill(p.dialog)
                .corner_radius(CornerRadius::same(14))
                .stroke(Stroke::new(1.0_f32, p.border))
                .shadow(ctx.style().visuals.window_shadow),
        )
        .show(ctx, |ui| {
            ui.set_width(width);
            let (header, _) = ui.allocate_exact_size(Vec2::new(width, 46.0), egui::Sense::hover());
            let size = ui.style().text_styles[&egui::TextStyle::Body].size;
            let title_width = ui
                .painter()
                .layout_no_wrap(title.to_owned(), egui::FontId::proportional(size), p.text)
                .size()
                .x;
            let mut title_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_center_size(
                        header.center(),
                        Vec2::new(title_width + 2.0, header.height()),
                    ))
                    .layout(Layout::left_to_right(Align::Center)),
            );
            theme::heading(&mut title_ui, title, Some(size));
            let close_rect = egui::Rect::from_center_size(
                egui::pos2(header.right() - 6.0 - 17.0, header.center().y),
                Vec2::splat(theme::BUTTON_HEIGHT),
            );
            let mut close_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(close_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            if close_button(&mut close_ui).clicked() {
                open = false;
            }
            // In a short window the body scrolls, so the header and its close button stay
            // in view with a little of the window showing above and below.
            egui::ScrollArea::vertical()
                .id_salt("dialog-body")
                .max_height(body_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin {
                            left: 24,
                            right: 24,
                            top: 6,
                            bottom: 24,
                        })
                        .show(ui, |ui| {
                            ui.set_width(width - 48.0);
                            content(ui)
                        });
                });
        });
    open
}

/// The round close button of a GNOME dialog's header.
fn close_button(ui: &mut egui::Ui) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(theme::BUTTON_HEIGHT), egui::Sense::click());
    let fill = if response.is_pointer_button_down_on() {
        p.button_active
    } else if response.hovered() {
        p.button_hover
    } else {
        p.button
    };
    let circle = egui::Rect::from_center_size(rect.center(), Vec2::splat(24.0));
    ui.painter()
        .rect_filled(circle, CornerRadius::same(12), fill);
    icons::paint(ui, Icon::Close, circle, theme::ICON, p.text);
    geometry::note(ui.ctx(), "Close", rect);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close"));
    response.on_hover_text("Close (Esc)")
}

/// Whether a helper program is installed, as a row suffix.
fn tool_status(ui: &mut egui::Ui, found: bool) {
    let p = theme::palette(ui.ctx());
    let (icon, color, text) = if found {
        (Icon::Check, p.accent_text, "Installed")
    } else {
        (Icon::Error, p.error, "Not found")
    };
    ui.label(theme::muted(text));
    ui.add(icons::image(icon, theme::ICON, color));
}

/// A drop-down in a preferences row, like libadwaita's combo rows.
fn dropdown(
    ui: &mut egui::Ui,
    name: &str,
    shown: &str,
    width: f32,
    options: impl FnOnce(&mut egui::Ui),
) {
    theme::menu_text(ui, shown, name, "", |ui| {
        super::menus::begin(ui, width, true);
        options(ui);
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
        let mut submit = false;
        let mut cancel = false;
        let open = dialog(ctx, "Open link", 460.0, |ui| {
            ui.spacing_mut().item_spacing.y = GAP;
            ui.label(theme::muted(
                "Paste or type a stream or video page. Twitch, Kick and YouTube links play through Streamlink and yt-dlp.",
            ));
            let field = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::symmetric(12, 9))
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
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.add(icons::image(Icon::Error, theme::ICON, p.error));
                    ui.colored_label(p.error, message);
                });
            }
            ui.add_space(GAP / 2.0);
            let row = Vec2::new(ui.available_width(), theme::BUTTON_HEIGHT);
            ui.allocate_ui_with_layout(row, Layout::right_to_left(Align::Center), |ui| {
                // The field keeps focus while the dialog is open, so Enter is read directly.
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                submit = theme::suggested(ui, "Open", Some(96.0)).clicked() || enter;
                if theme::button(ui, "Cancel").clicked() {
                    cancel = true;
                }
            });
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
        let mut keys_changed = false;
        let mut settings_changed = false;
        let mut tuning_changed = false;
        let p = theme::palette(ctx);
        let devices = if state.tab == PrefsTab::Picture {
            self.session.engine.audio_devices()
        } else {
            Vec::new()
        };
        let tools = (
            self.session.streams.tools.streamlink.is_some(),
            self.session.streams.tools.ytdlp.is_some(),
        );
        let version = self.session.engine.version();
        let max_height = (ctx.content_rect().height() - 190.0).clamp(80.0, 460.0);
        let open = dialog(ctx, "Preferences", 600.0, |ui| {
            ui.spacing_mut().item_spacing.y = GAP;
            view_switcher(ui, &mut state.tab);
            let s = &mut self.session.store.settings;
            egui::ScrollArea::vertical()
                .id_salt(("prefs", state.tab))
                .max_height(max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    // Leaves room for the overlay scroll bar beside the cards.
                    ui.set_width(ui.available_width() - 4.0);
                    match state.tab {
                        PrefsTab::Playback => {
                            theme::group_title(ui, "Seeking");
                            theme::boxed_list(ui, |list| {
                                list.row("Seek step", Some("How far the arrow keys move"), |ui| {
                                    settings_changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut s.seek_step)
                                                .range(1..=120)
                                                .suffix(" s"),
                                        )
                                        .changed();
                                });
                            });
                            theme::group_title(ui, "Resuming");
                            theme::boxed_list(ui, |list| {
                                settings_changed |= list.switch_row(
                                    "Resume where you left off",
                                    None,
                                    &mut s.resume,
                                );
                                settings_changed |= list.switch_row(
                                    "Remember tracks and sync",
                                    Some("Audio and subtitle choices and delays, for each file"),
                                    &mut s.remember_tracks,
                                );
                                settings_changed |= list.switch_row(
                                    "Continue with the next file in the folder",
                                    None,
                                    &mut s.auto_next_folder,
                                );
                            });
                        }
                        PrefsTab::Picture => {
                            theme::group_title(ui, "Decoding");
                            theme::boxed_list(ui, |list| {
                                list.row("Hardware decoding", None, |ui| {
                                    let shown = tuning::HWDEC
                                        .iter()
                                        .find(|(v, _)| *v == s.hwdec)
                                        .map_or("Automatic (safe)", |(_, l)| *l);
                                    dropdown(ui, "Hardware decoding", shown, 200.0, |ui| {
                                        for (value, label) in tuning::HWDEC {
                                            if choice(ui, label, s.hwdec == value) {
                                                s.hwdec = value.to_owned();
                                                tuning_changed = true;
                                            }
                                        }
                                    });
                                });
                                list.row(
                                    "HDR tone mapping",
                                    Some("HDR video is shown tone-mapped to SDR"),
                                    |ui| {
                                        let shown = s.tone_mapping.clone();
                                        dropdown(ui, "HDR tone mapping", &shown, 160.0, |ui| {
                                            for value in tuning::TONE_MAPPING {
                                                if choice(ui, value, s.tone_mapping == value) {
                                                    s.tone_mapping = value.to_owned();
                                                    tuning_changed = true;
                                                }
                                            }
                                        });
                                    },
                                );
                            });
                            theme::group_title(ui, "Subtitles");
                            theme::boxed_list(ui, |list| {
                                list.row("Font", None, |ui| {
                                    tuning_changed |= ui
                                        .add(
                                            egui::TextEdit::singleline(&mut s.subtitles.font)
                                                .desired_width(180.0)
                                                .margin(egui::Margin::symmetric(8, 6))
                                                .hint_text("mpv default"),
                                        )
                                        .changed();
                                });
                                list.row("Size", None, |ui| {
                                    tuning_changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut s.subtitles.size)
                                                .range(8..=200),
                                        )
                                        .changed();
                                });
                                for (label, value) in [
                                    ("Text colour", &mut s.subtitles.color),
                                    ("Outline colour", &mut s.subtitles.border_color),
                                ] {
                                    list.row(label, None, |ui| {
                                        let mut rgb =
                                            tuning::color_rgb(value).unwrap_or([255, 255, 255]);
                                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                                            *value = format!(
                                                "#{:02X}{:02X}{:02X}",
                                                rgb[0], rgb[1], rgb[2]
                                            );
                                            tuning_changed = true;
                                        }
                                    });
                                }
                                list.row("Outline width", None, |ui| {
                                    tuning_changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut s.subtitles.border)
                                                .range(0.0..=10.0)
                                                .speed(0.1),
                                        )
                                        .changed();
                                });
                                list.row("Position", Some("0 is the top, 100 the bottom"), |ui| {
                                    let mut pos = i64::from(s.subtitles.pos);
                                    if theme::slider(ui, &mut pos, 0..=100, "Position").changed() {
                                        s.subtitles.pos = pos as u32;
                                        tuning_changed = true;
                                    }
                                });
                            });
                            theme::group_title(ui, "Picture");
                            theme::boxed_list(ui, |list| {
                                for (label, value) in [
                                    ("Brightness", &mut s.picture.brightness),
                                    ("Contrast", &mut s.picture.contrast),
                                    ("Saturation", &mut s.picture.saturation),
                                    ("Gamma", &mut s.picture.gamma),
                                    ("Hue", &mut s.picture.hue),
                                ] {
                                    list.row(label, None, |ui| {
                                        let mut v = i64::from(*value);
                                        if theme::slider(ui, &mut v, -100..=100, label).changed() {
                                            *value = v as i32;
                                            tuning_changed = true;
                                        }
                                    });
                                }
                                if list
                                    .button_row("Reset picture", Some(Icon::Undo), "")
                                    .clicked()
                                {
                                    s.picture = Default::default();
                                    tuning_changed = true;
                                }
                            });
                            theme::group_title(ui, "Sound");
                            theme::boxed_list(ui, |list| {
                                tuning_changed |= list.switch_row(
                                    "Night mode",
                                    Some("Evens out loud and quiet passages"),
                                    &mut s.audio.night_mode,
                                );
                                list.row("ReplayGain", None, |ui| {
                                    let shown = tuning::REPLAYGAIN
                                        .iter()
                                        .find(|(v, _)| *v == s.audio.replaygain)
                                        .map_or("Off", |(_, l)| *l);
                                    dropdown(ui, "ReplayGain", shown, 120.0, |ui| {
                                        for (value, label) in tuning::REPLAYGAIN {
                                            if choice(ui, label, s.audio.replaygain == value) {
                                                s.audio.replaygain = value.to_owned();
                                                tuning_changed = true;
                                            }
                                        }
                                    });
                                });
                                list.row("Output device", None, |ui| {
                                    let shown = devices
                                        .iter()
                                        .find(|(n, _)| *n == s.audio.device)
                                        .map_or(s.audio.device.clone(), |(_, d)| d.clone());
                                    let shown = shorten(&shown, 28);
                                    dropdown(ui, "Output device", &shown, 280.0, |ui| {
                                        for (name, description) in &devices {
                                            if choice(ui, description, s.audio.device == *name) {
                                                s.audio.device = name.clone();
                                                tuning_changed = true;
                                            }
                                        }
                                    });
                                });
                            });
                        }
                        PrefsTab::Streams => {
                            theme::group_title(ui, "Quality");
                            theme::boxed_list(ui, |list| {
                                list.row(
                                    "Default quality",
                                    Some("best, or a name such as 720p60"),
                                    |ui| {
                                        let field = ui.add(
                                            egui::TextEdit::singleline(&mut s.stream_quality)
                                                .desired_width(110.0)
                                                .margin(egui::Margin::symmetric(8, 6))
                                                .hint_text("best"),
                                        );
                                        settings_changed |= field.changed();
                                        // Fill in the default only once editing is over, so the
                                        // field can be cleared while typing.
                                        if field.lost_focus() && s.stream_quality.trim().is_empty()
                                        {
                                            s.stream_quality = "best".to_owned();
                                            settings_changed = true;
                                        }
                                    },
                                );
                            });
                            theme::group_title(ui, "Live and video sites");
                            theme::boxed_list(ui, |list| {
                                settings_changed |= list.switch_row(
                                    "Low-latency mode for Twitch and Kick",
                                    None,
                                    &mut s.low_latency,
                                );
                                settings_changed |= list.switch_row(
                                    "Skip sponsor reads in YouTube videos",
                                    Some("Uses SponsorBlock"),
                                    &mut s.sponsorblock,
                                );
                                let enabled = s.sponsorblock;
                                settings_changed |= list.switch_row_enabled(
                                    "Also skip intros and outros",
                                    None,
                                    &mut s.sponsorblock_intros,
                                    enabled,
                                );
                            });
                            theme::group_title(ui, "Helper tools");
                            theme::boxed_list(ui, |list| {
                                list.row(
                                    "Streamlink",
                                    Some("Watches Twitch, Kick and similar live streams"),
                                    |ui| tool_status(ui, tools.0),
                                );
                                list.row(
                                    "yt-dlp",
                                    Some("Plays YouTube and other video sites"),
                                    |ui| tool_status(ui, tools.1),
                                );
                            });
                        }
                        PrefsTab::Keys => {
                            ui.label(theme::muted(
                                "Click a shortcut, then press the new keys. Escape cancels.",
                            ));
                            if let Some(message) = &state.message {
                                ui.colored_label(p.error, message);
                            }
                            theme::boxed_list(ui, |list| {
                                list.row_height = 46.0;
                                for &action in Action::ALL {
                                    list.row(action.label(), None, |ui| {
                                        let bound = !binding_text(&s.keys, action).is_empty();
                                        ui.add_enabled_ui(bound, |ui| {
                                            if theme::icon_button(
                                                ui,
                                                Icon::Delete,
                                                &format!(
                                                    "Remove the shortcut for {}",
                                                    action.label()
                                                ),
                                                "Remove this shortcut",
                                            )
                                            .clicked()
                                            {
                                                keys::unbind(&mut s.keys, action);
                                                keys_changed = true;
                                            }
                                        });
                                        let overridden = s.keys.contains_key(action.id());
                                        ui.add_enabled_ui(overridden, |ui| {
                                            if theme::icon_button(
                                                ui,
                                                Icon::Undo,
                                                &format!(
                                                    "Reset the shortcut for {}",
                                                    action.label()
                                                ),
                                                "Back to the default",
                                            )
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
                                        });
                                        let capturing = state.capture == Some(action);
                                        let text = if capturing {
                                            "Press a key…".to_owned()
                                        } else {
                                            Some(binding_text(&s.keys, action))
                                                .filter(|t| !t.is_empty())
                                                .unwrap_or_else(|| "Disabled".to_owned())
                                        };
                                        let mut label = RichText::new(text);
                                        if capturing {
                                            label = label.color(p.accent_text);
                                        }
                                        let key_button = ui.add(
                                            egui::Button::new(label)
                                                .frame_when_inactive(capturing)
                                                .min_size(Vec2::new(120.0, theme::BUTTON_HEIGHT)),
                                        );
                                        let name = format!("Shortcut for {}", action.label());
                                        geometry::note(ui.ctx(), &name, key_button.rect);
                                        key_button.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::WidgetType::Button,
                                                true,
                                                name.as_str(),
                                            )
                                        });
                                        if key_button.clicked() {
                                            state.capture = Some(action);
                                            state.message = None;
                                        }
                                    });
                                }
                            });
                        }
                    }
                });
            ui.vertical_centered(|ui| {
                ui.label(
                    theme::muted(format!("Lumen {} · {}", env!("CARGO_PKG_VERSION"), version))
                        .small(),
                );
            });
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
        self.info = dialog(ctx, "Media information", 440.0, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            match source {
                None => {
                    ui.add_space(GAP);
                    ui.vertical_centered(|ui| ui.label(theme::muted("Nothing is playing.")));
                    ui.add_space(GAP);
                }
                Some(source) => {
                    ui.add(
                        egui::Label::new(RichText::new(crate::queue::title_of(&source)).strong())
                            .truncate(),
                    );
                    ui.add(egui::Label::new(theme::muted(&source).small()).truncate());
                    ui.add_space(GAP / 2.0);
                    if rows.is_empty() {
                        ui.label(theme::muted("Still opening…"));
                    } else {
                        theme::boxed_list(ui, |list| {
                            list.row_height = 40.0;
                            for (label, value) in &rows {
                                list.row(label, None, |ui| {
                                    ui.add(
                                        egui::Label::new(theme::muted(value.as_str())).truncate(),
                                    )
                                    .on_hover_text(value);
                                });
                            }
                        });
                    }
                }
            }
        });
    }
}

/// `text` cut to `max` characters with an ellipsis.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max - 1).collect();
    format!("{}…", head.trim_end())
}

/// The tabs at the top of the preferences, drawn like libadwaita's inline view switcher.
fn view_switcher(ui: &mut egui::Ui, tab: &mut PrefsTab) {
    let p = theme::palette(ui.ctx());
    let tabs = [
        (PrefsTab::Playback, "Playback"),
        (PrefsTab::Picture, "Picture & sound"),
        (PrefsTab::Streams, "Streams"),
        (PrefsTab::Keys, "Keyboard"),
    ];
    let font = egui::FontId::proportional(ui.style().text_styles[&egui::TextStyle::Button].size);
    let widths: Vec<f32> = tabs
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), p.text)
                .size()
                .x
                + 28.0
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let (outer, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), egui::Sense::hover());
    let frame = egui::Rect::from_center_size(outer.center(), Vec2::new(total, 38.0));
    ui.painter()
        .rect_filled(frame, CornerRadius::same(theme::RADIUS + 3), p.button);
    let mut x = frame.left() + 3.0;
    for ((value, label), width) in tabs.iter().zip(widths) {
        let rect =
            egui::Rect::from_min_size(egui::pos2(x, frame.top() + 3.0), Vec2::new(width, 32.0));
        x += width;
        let response = ui.interact(rect, ui.id().with(("tab", *label)), egui::Sense::click());
        let selected = *tab == *value;
        if response.clicked() {
            *tab = *value;
        }
        let fill = if selected {
            if p.dark() {
                Color32::from_white_alpha(38)
            } else {
                Color32::WHITE
            }
        } else if response.hovered() {
            p.hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS), fill);
        let galley = ui
            .painter()
            .layout_no_wrap((*label).to_owned(), font.clone(), p.text);
        let at = rect.center() - galley.size() / 2.0;
        ui.painter().galley(at, galley.clone(), p.text);
        if selected {
            ui.painter()
                .galley(at + Vec2::new(0.5, 0.0), galley, p.text);
        }
        geometry::note(ui.ctx(), label, rect);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, *label)
        });
    }
}
