//! Lumen's look: flat and quiet, in the desktop's light or dark scheme and accent colour,
//! with colours modelled on libadwaita so the window sits naturally on a GNOME desktop.

use eframe::egui::{
    self, Button, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Margin,
    Response, RichText, Sense, Shadow, Stroke, StrokeKind, TextStyle, Ui, Vec2, WidgetInfo,
    WidgetType,
};

use super::appearance::{Scheme, SystemFonts};
use super::icons::{self, Icon};

/// Corner radius of buttons, entries and rows, as in Adwaita.
pub const RADIUS: u8 = 6;
/// Corner radius of dialogs and popovers.
pub const WINDOW_RADIUS: u8 = 12;
pub const BUTTON_HEIGHT: f32 = 34.0;

/// Every colour the interface draws with, for one scheme and accent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub scheme: Scheme,
    pub window: Color32,
    pub view: Color32,
    pub sidebar: Color32,
    pub popover: Color32,
    pub dialog: Color32,
    /// Boxed lists and other cards, and the lines between their rows.
    pub card: Color32,
    pub card_border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub border: Color32,
    /// Flat widgets under the pointer.
    pub hover: Color32,
    pub button: Color32,
    pub button_hover: Color32,
    pub button_active: Color32,
    pub entry: Color32,
    /// A selected row or a toggle that is on.
    pub selected: Color32,
    pub accent: Color32,
    /// The accent as a text or line colour, adjusted to read well on the background.
    pub accent_text: Color32,
    pub on_accent: Color32,
    pub track: Color32,
    pub error: Color32,
    pub warning: Color32,
}

/// `color` at `alpha` (0 to 1), as an unmultiplied colour.
fn alpha(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (alpha * 255.0).round() as u8,
    )
}

/// `top` laid over `base` at `amount`, as an opaque colour.
fn mix(base: Color32, top: Color32, amount: f32) -> Color32 {
    base.lerp_to_gamma(top, amount)
}

impl Palette {
    pub fn new(scheme: Scheme, accent: Color32) -> Self {
        match scheme {
            Scheme::Dark => {
                let fg = Color32::WHITE;
                let window = Color32::from_rgb(0x22, 0x22, 0x26);
                Self {
                    scheme,
                    window,
                    view: Color32::from_rgb(0x1d, 0x1d, 0x20),
                    sidebar: Color32::from_rgb(0x2e, 0x2e, 0x32),
                    popover: Color32::from_rgb(0x36, 0x36, 0x3a),
                    dialog: Color32::from_rgb(0x36, 0x36, 0x3a),
                    card: alpha(fg, 0.08),
                    card_border: Color32::from_black_alpha(92),
                    text: fg,
                    dim: mix(window, fg, 0.55),
                    border: alpha(fg, 0.12),
                    hover: alpha(fg, 0.07),
                    button: alpha(fg, 0.10),
                    button_hover: alpha(fg, 0.15),
                    button_active: alpha(fg, 0.30),
                    entry: alpha(fg, 0.08),
                    selected: alpha(fg, 0.10),
                    accent,
                    accent_text: mix(accent, fg, 0.35),
                    on_accent: Color32::WHITE,
                    track: alpha(fg, 0.15),
                    error: Color32::from_rgb(0xff, 0x93, 0x8c),
                    warning: Color32::from_rgb(0xff, 0xc2, 0x52),
                }
            }
            Scheme::Light => {
                let fg = Color32::from_rgb(0x00, 0x00, 0x06);
                let window = Color32::from_rgb(0xfa, 0xfa, 0xfb);
                Self {
                    scheme,
                    window,
                    view: Color32::WHITE,
                    sidebar: Color32::from_rgb(0xeb, 0xeb, 0xed),
                    popover: Color32::WHITE,
                    dialog: window,
                    card: Color32::WHITE,
                    card_border: alpha(fg, 0.09),
                    text: mix(window, fg, 0.8),
                    dim: mix(window, fg, 0.45),
                    border: alpha(fg, 0.12),
                    hover: alpha(fg, 0.05),
                    button: alpha(fg, 0.08),
                    button_hover: alpha(fg, 0.12),
                    button_active: alpha(fg, 0.24),
                    entry: alpha(fg, 0.07),
                    selected: alpha(fg, 0.08),
                    accent,
                    accent_text: mix(accent, fg, 0.15),
                    on_accent: Color32::WHITE,
                    track: alpha(fg, 0.15),
                    error: Color32::from_rgb(0xc3, 0x00, 0x00),
                    warning: Color32::from_rgb(0x9c, 0x6e, 0x03),
                }
            }
        }
    }

    pub fn dark(&self) -> bool {
        self.scheme == Scheme::Dark
    }
}

fn palette_id() -> Id {
    Id::new("lumen-palette")
}

/// The palette in use.
pub fn palette(ctx: &egui::Context) -> Palette {
    ctx.data(|d| d.get_temp(palette_id()))
        .unwrap_or_else(|| Palette::new(Scheme::Dark, super::appearance::DEFAULT_ACCENT))
}

/// Draws with `palette` until changed again, without restyling egui's own widgets.
pub fn use_palette(ctx: &egui::Context, palette: Palette) {
    ctx.data_mut(|d| d.insert_temp(palette_id(), palette));
}

/// Applies `palette` to the whole interface, with body text `body` points tall.
pub fn apply(ctx: &egui::Context, palette: Palette, body: f32) {
    let p = palette;
    ctx.data_mut(|d| d.insert_temp(palette_id(), p));
    let mut visuals = if p.dark() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.override_text_color = None;
    visuals.weak_text_color = Some(p.dim);
    visuals.panel_fill = p.window;
    visuals.window_fill = p.dialog;
    visuals.window_stroke = Stroke::new(1.0_f32, p.border);
    visuals.window_corner_radius = CornerRadius::same(WINDOW_RADIUS);
    visuals.menu_corner_radius = CornerRadius::same(WINDOW_RADIUS);
    let shadow = Color32::from_black_alpha(if p.dark() { 90 } else { 40 });
    visuals.window_shadow = Shadow {
        offset: [0, 4],
        blur: 24,
        spread: 0,
        color: shadow,
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 2],
        blur: 12,
        spread: 0,
        color: shadow,
    };
    // Translucent, so entries read the same on the window, the sidebar and in dialogs.
    visuals.extreme_bg_color = p.entry;
    visuals.text_edit_bg_color = Some(p.entry);
    visuals.faint_bg_color = mix(p.window, p.text, 0.03);
    visuals.code_bg_color = p.entry;
    visuals.hyperlink_color = p.accent_text;
    // Selected rows, tabs and menu entries get a neutral fill with accent text; a focused
    // entry gets an accent outline.
    visuals.selection.bg_fill = mix(p.window, p.text, if p.dark() { 0.14 } else { 0.10 });
    visuals.selection.stroke = Stroke::new(1.0_f32, p.accent_text);
    visuals.text_cursor.stroke = Stroke::new(1.0_f32, p.text);
    visuals.warn_fg_color = p.warning;
    visuals.error_fg_color = p.error;
    visuals.slider_trailing_fill = true;
    visuals.striped = false;
    visuals.indent_has_left_vline = false;

    let radius = CornerRadius::same(RADIUS);
    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = p.window;
    w.noninteractive.weak_bg_fill = p.window;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.noninteractive.corner_radius = radius;
    for (state, fill) in [
        (&mut w.inactive, p.button),
        (&mut w.hovered, p.button_hover),
        (&mut w.active, p.button_active),
        (&mut w.open, p.button_hover),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::NONE;
        state.corner_radius = radius;
        state.fg_stroke = Stroke::new(1.0_f32, p.text);
        state.expansion = 0.0;
    }
    // A focused entry gets the accent outline, as in Adwaita.
    w.active.bg_stroke = Stroke::NONE;
    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(6.0, 6.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = BUTTON_HEIGHT - 4.0;
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.window_margin = Margin::same(18);
        style.spacing.combo_width = 70.0;
        style.spacing.icon_width = 18.0;
        style.spacing.icon_width_inner = 10.0;
        style.spacing.scroll = egui::style::ScrollStyle::floating();
        for (text, size) in [
            (TextStyle::Body, body),
            (TextStyle::Button, body),
            (TextStyle::Heading, body + 3.0),
            (TextStyle::Small, (body - 2.0).max(10.0)),
        ] {
            style
                .text_styles
                .insert(text, FontId::proportional(size.round()));
        }
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::monospace((body - 1.0).round()),
        );
    });
}

/// The body text size for a desktop font of `size_pt` points (GNOME's 11 pt is about 15 px).
pub fn body_size(size_pt: Option<f32>) -> f32 {
    size_pt.map_or(14.0, |pt| (pt * 4.0 / 3.0).clamp(12.0, 20.0))
}

/// Puts the desktop's fonts in front of egui's, which stay as fallbacks for symbols.
/// Returns the body text size to use.
pub fn install_fonts(ctx: &egui::Context, fonts: &SystemFonts) -> f32 {
    let mut defs = FontDefinitions::default();
    for (name, font, family) in [
        ("system-ui", &fonts.ui, FontFamily::Proportional),
        ("system-mono", &fonts.mono, FontFamily::Monospace),
    ] {
        let Some(font) = font else { continue };
        let Ok(bytes) = std::fs::read(&font.path) else {
            continue;
        };
        let mut data = FontData::from_owned(bytes);
        data.index = font.index;
        defs.font_data
            .insert(name.to_owned(), std::sync::Arc::new(data));
        defs.families
            .entry(family)
            .or_default()
            .insert(0, name.to_owned());
    }
    ctx.set_fonts(defs);
    body_size(fonts.ui.as_ref().and_then(|f| f.size_pt))
}

/// Secondary text, in the dimmed foreground colour.
pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).weak()
}

/// A heading in bold. egui draws one weight per font, so bold is drawn as two passes a
/// fraction of a point apart, which reads as the semibold GNOME uses for headings.
pub fn heading(ui: &mut Ui, text: &str, size: Option<f32>) -> Response {
    let font = FontId::proportional(size.unwrap_or_else(|| {
        ui.style()
            .text_styles
            .get(&TextStyle::Body)
            .map_or(14.0, |f| f.size)
    }));
    let color = ui.visuals().strong_text_color();
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + Vec2::new(1.0, 0.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.galley(rect.min, galley.clone(), color);
        painter.galley(rect.min + Vec2::new(0.6, 0.0), galley, color);
    }
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, text));
    response
}

/// The accent-coloured button for the one main action in view.
pub fn suggested(ui: &mut Ui, text: &str, width: Option<f32>) -> Response {
    let p = palette(ui.ctx());
    let label = RichText::new(text).color(p.on_accent);
    let button = Button::new(label)
        .fill(p.accent)
        .corner_radius(RADIUS)
        .min_size(Vec2::new(width.unwrap_or(0.0), BUTTON_HEIGHT));
    let response = ui.add(button);
    super::geometry::note(ui.ctx(), text, response.rect);
    if response.hovered() || response.is_pointer_button_down_on() {
        let tint = if response.is_pointer_button_down_on() {
            Color32::from_black_alpha(40)
        } else {
            Color32::from_white_alpha(18)
        };
        ui.painter()
            .rect_filled(response.rect, CornerRadius::same(RADIUS), tint);
    }
    response
}

/// A plain button with a label, as in a toolbar.
pub fn button(ui: &mut Ui, text: &str) -> Response {
    let response = ui.add(Button::new(text).min_size(Vec2::new(0.0, BUTTON_HEIGHT)));
    super::geometry::note(ui.ctx(), text, response.rect);
    response
}

/// A rounded "pill" button, as on GNOME's status pages; `primary` gives it the accent colour.
pub fn pill(ui: &mut Ui, text: &str, primary: bool, width: f32) -> Response {
    let p = palette(ui.ctx());
    let size = Vec2::new(width, BUTTON_HEIGHT + 6.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let base = if primary { p.accent } else { p.button };
        let painter = ui.painter();
        let radius = CornerRadius::same((size.y / 2.0) as u8);
        painter.rect_filled(rect, radius, base);
        if response.is_pointer_button_down_on() {
            painter.rect_filled(rect, radius, Color32::from_black_alpha(40));
        } else if response.hovered() {
            painter.rect_filled(rect, radius, Color32::from_white_alpha(14));
        }
        let color = if primary { p.on_accent } else { p.text };
        let font = FontId::proportional(ui.style().text_styles[&TextStyle::Button].size);
        let galley = painter.layout_no_wrap(text.to_owned(), font, color);
        let at = rect.center() - galley.size() / 2.0;
        painter.galley(at, galley.clone(), color);
        painter.galley(at + Vec2::new(0.5, 0.0), galley, color);
        focus_ring(ui, &response, radius);
    }
    super::geometry::note(ui.ctx(), text, rect);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), text));
    response
}

/// Size of symbolic icons, as GTK draws them.
pub const ICON: f32 = 16.0;

fn focus_ring(ui: &Ui, response: &Response, radius: CornerRadius) {
    if response.has_focus() {
        let p = palette(ui.ctx());
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            radius,
            Stroke::new(2.0_f32, p.accent.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
    }
}

/// The background of a flat button: nothing until hovered, lit while `lit` (a toggle that is
/// on, or a menu that is open), as Adwaita draws them.
fn flat_fill(ui: &Ui, response: &Response, lit: bool) -> Color32 {
    let p = palette(ui.ctx());
    if !ui.is_enabled() {
        if lit { p.hover } else { Color32::TRANSPARENT }
    } else if response.is_pointer_button_down_on() {
        p.button_active
    } else if lit {
        if response.hovered() {
            p.button_hover
        } else {
            p.button
        }
    } else if response.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    }
}

/// The colour icons and labels are drawn in, dimmed while disabled.
pub fn ink(ui: &Ui) -> Color32 {
    let p = palette(ui.ctx());
    if ui.is_enabled() {
        p.text
    } else {
        p.text.gamma_multiply(0.45)
    }
}

fn paint_icon_button(ui: &Ui, response: &Response, icon: Icon, lit: bool, round: bool) {
    let rect = response.rect;
    if !ui.is_rect_visible(rect) {
        return;
    }
    let radius = if round {
        CornerRadius::same((rect.height() / 2.0) as u8)
    } else {
        CornerRadius::same(RADIUS)
    };
    ui.painter()
        .rect_filled(rect, radius, flat_fill(ui, response, lit));
    icons::paint(ui, icon, rect, ICON, ink(ui));
    focus_ring(ui, response, radius);
}

/// Names a widget for screen readers and the demo recorder, with `tip` (or the name) as its tooltip.
fn finish(response: Response, name: &str, tip: &str, selected: Option<bool>) -> Response {
    super::geometry::note(&response.ctx, name, response.rect);
    let enabled = response.enabled();
    response.widget_info(|| match selected {
        Some(on) => WidgetInfo::selected(WidgetType::Button, enabled, on, name),
        None => WidgetInfo::labeled(WidgetType::Button, enabled, name),
    });
    let tip = if tip.is_empty() { name } else { tip };
    response.on_hover_text(tip)
}

/// A flat square button with a symbolic icon, as in a header bar.
pub fn icon_button(ui: &mut Ui, icon: Icon, name: &str, tip: &str) -> Response {
    let (_, response) = ui.allocate_exact_size(Vec2::splat(BUTTON_HEIGHT), Sense::click());
    paint_icon_button(ui, &response, icon, false, false);
    finish(response, name, tip, None)
}

/// A flat icon button that stays lit while `on`.
pub fn icon_toggle(ui: &mut Ui, icon: Icon, name: &str, tip: &str, on: bool) -> Response {
    let (_, response) = ui.allocate_exact_size(Vec2::splat(BUTTON_HEIGHT), Sense::click());
    paint_icon_button(ui, &response, icon, on, false);
    finish(response, name, tip, Some(on))
}

/// The round play/pause button, `size` points across.
pub fn circular(ui: &mut Ui, icon: Icon, name: &str, tip: &str, size: f32) -> Response {
    let (_, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    // Always framed, so the main control stands out from the flat ones around it.
    paint_icon_button(ui, &response, icon, true, true);
    finish(response, name, tip, None)
}

/// Sets up a popover's rows the way GTK lays out a menu: full-width rows 32 points tall,
/// rounded under the pointer, with no gaps between them.
pub fn menu_rows(ui: &mut Ui, min_width: f32) {
    let p = palette(ui.ctx());
    ui.set_min_width(min_width);
    let spacing = ui.spacing_mut();
    spacing.button_padding = Vec2::new(10.0, 6.0);
    spacing.item_spacing = Vec2::new(10.0, 0.0);
    spacing.interact_size.y = 32.0;
    let widgets = &mut ui.visuals_mut().widgets;
    widgets.hovered.weak_bg_fill = p.button;
    widgets.open.weak_bg_fill = p.button;
    widgets.active.weak_bg_fill = p.button_active;
    for state in [&mut widgets.hovered, &mut widgets.open, &mut widgets.active] {
        state.corner_radius = CornerRadius::same(RADIUS);
    }
}

/// A separator between groups of menu rows.
pub fn menu_separator(ui: &mut Ui) {
    ui.add(egui::Separator::default().spacing(12.0));
}

/// Opens `content` as a popover menu under `response` when it is clicked.
fn popover<R>(response: &Response, width: f32, content: impl FnOnce(&mut Ui) -> R) {
    egui::Popup::menu(response).gap(4.0).show(|ui| {
        menu_rows(ui, width);
        content(ui)
    });
}

/// Whether the popover belonging to `response` is open.
fn popover_open(response: &Response) -> bool {
    egui::Popup::is_id_open(&response.ctx, egui::Popup::default_response_id(response))
}

/// A flat icon button that opens a menu, lit while the menu is open.
pub fn menu_icon<R>(
    ui: &mut Ui,
    icon: Icon,
    name: &str,
    tip: &str,
    width: f32,
    content: impl FnOnce(&mut Ui) -> R,
) -> Response {
    let (_, response) = ui.allocate_exact_size(Vec2::splat(BUTTON_HEIGHT), Sense::click());
    let open = popover_open(&response);
    paint_icon_button(ui, &response, icon, open, false);
    let response = if open {
        super::geometry::note(ui.ctx(), name, response.rect);
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name));
        response
    } else {
        finish(response, name, tip, None)
    };
    popover(&response, width, content);
    response
}

/// A flat button showing the current choice and a drop-down arrow that opens a menu.
pub fn menu_text<R>(
    ui: &mut Ui,
    text: &str,
    name: &str,
    tip: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> Response {
    let font = FontId::proportional(ui.style().text_styles[&TextStyle::Button].size);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, ink(ui));
    let size = Vec2::new(galley.size().x + 10.0 + 6.0 + ICON + 8.0, BUTTON_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let open = popover_open(&response);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius::same(RADIUS),
            flat_fill(ui, &response, open),
        );
        let at = egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0);
        painter.galley(at, galley, ink(ui));
        let arrow = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 8.0 - ICON, rect.center().y - ICON / 2.0),
            Vec2::splat(ICON),
        );
        icons::paint(ui, Icon::PanDown, arrow, ICON, ink(ui));
        focus_ring(ui, &response, CornerRadius::same(RADIUS));
    }
    let response = if open {
        super::geometry::note(ui.ctx(), name, rect);
        response
    } else {
        finish(response, name, tip, None)
    };
    popover(&response, 120.0, content);
    response
}

/// A GTK-style scale: a thin track filled in the accent colour with a round white knob, and
/// its value in dim text beside it.
pub fn slider(
    ui: &mut Ui,
    value: &mut i64,
    range: std::ops::RangeInclusive<i64>,
    name: &str,
) -> Response {
    let p = palette(ui.ctx());
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(160.0, 26.0), Sense::click_and_drag());
    let knob = 9.0;
    let track = egui::Rect::from_min_max(
        egui::pos2(rect.left() + knob, rect.center().y - 2.0),
        egui::pos2(rect.right() - knob, rect.center().y + 2.0),
    );
    let (lo, hi) = (*range.start(), *range.end());
    if let Some(pos) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let new = lo + ((hi - lo) as f32 * t).round() as i64;
        if new != *value {
            *value = new;
            response.mark_changed();
        }
    }
    if response.has_focus() {
        let step = ui.input(|i| {
            i64::from(i.key_pressed(egui::Key::ArrowRight))
                - i64::from(i.key_pressed(egui::Key::ArrowLeft))
        });
        if step != 0 {
            *value = (*value + step).clamp(lo, hi);
            response.mark_changed();
        }
    }
    if ui.is_rect_visible(rect) {
        let t = (*value - lo) as f32 / (hi - lo).max(1) as f32;
        let x = track.left() + track.width() * t;
        let painter = ui.painter();
        painter.rect_filled(track, CornerRadius::same(2), p.track);
        let filled = egui::Rect::from_min_max(track.min, egui::pos2(x, track.max.y));
        let fill = if ui.is_enabled() { p.accent } else { p.dim };
        painter.rect_filled(filled, CornerRadius::same(2), fill);
        let centre = egui::pos2(x, rect.center().y);
        painter.circle_filled(
            centre + Vec2::new(0.0, 1.0),
            knob,
            Color32::from_black_alpha(50),
        );
        painter.circle_filled(centre, knob, Color32::WHITE);
        if response.has_focus() {
            painter.circle_stroke(
                centre,
                knob + 2.0,
                Stroke::new(2.0_f32, p.accent.gamma_multiply(0.6)),
            );
        }
    }
    let shown = *value;
    ui.add_sized(
        Vec2::new(34.0, 26.0),
        egui::Label::new(muted(shown.to_string())),
    );
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::slider(enabled, shown as f64, name));
    response.on_hover_text(format!("{name}: {shown}"))
}

/// Size of a GTK switch.
const SWITCH: Vec2 = Vec2::new(46.0, 26.0);

fn paint_switch(ui: &Ui, rect: egui::Rect, on: bool, response: &Response) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let p = palette(ui.ctx());
    let how_on = ui
        .ctx()
        .animate_bool_responsive(response.id.with("switch"), on);
    let off_fill = if response.hovered() {
        p.button_hover
    } else {
        p.track
    };
    let mut fill = off_fill.lerp_to_gamma(p.accent, how_on);
    if !ui.is_enabled() {
        fill = fill.gamma_multiply(0.5);
    }
    let painter = ui.painter();
    let radius = CornerRadius::same((SWITCH.y / 2.0) as u8);
    painter.rect_filled(rect, radius, fill);
    let r = SWITCH.y / 2.0 - 3.0;
    let x = egui::lerp((rect.left() + 3.0 + r)..=(rect.right() - 3.0 - r), how_on);
    let centre = egui::pos2(x, rect.center().y);
    painter.circle_filled(
        centre + Vec2::new(0.0, 1.0),
        r,
        Color32::from_black_alpha(40),
    );
    let knob = if ui.is_enabled() {
        Color32::WHITE
    } else {
        Color32::from_gray(190)
    };
    painter.circle_filled(centre, r, knob);
}

/// An on/off switch, drawn like GTK's.
pub fn switch(ui: &mut Ui, on: &mut bool, name: &str) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(SWITCH, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    paint_switch(ui, rect, *on, &response);
    focus_ring(ui, &response, CornerRadius::same(13));
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, *on, name));
    response
}

/// A rounded card holding rows separated by thin lines, like libadwaita's boxed lists.
pub fn boxed_list<R>(ui: &mut Ui, content: impl FnOnce(&mut BoxedList<'_>) -> R) -> R {
    let p = palette(ui.ctx());
    egui::Frame::new()
        .fill(p.card)
        .corner_radius(CornerRadius::same(WINDOW_RADIUS))
        .stroke(Stroke::new(1.0_f32, p.card_border))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.set_width(ui.available_width());
            let mut list = BoxedList {
                ui,
                rows: 0,
                hover: None,
                row_height: 50.0,
            };
            let out = content(&mut list);
            // The row under the pointer is lit edge to edge, rounded where it meets the card's corners.
            if let Some((shape, rect, index)) = list.hover {
                let r = WINDOW_RADIUS;
                let (first, last) = (index == 1, index == list.rows);
                let radius = CornerRadius {
                    nw: if first { r } else { 0 },
                    ne: if first { r } else { 0 },
                    sw: if last { r } else { 0 },
                    se: if last { r } else { 0 },
                };
                list.ui.painter().set(
                    shape,
                    egui::epaint::RectShape::filled(rect, radius, p.hover),
                );
            }
            out
        })
        .inner
}

pub struct BoxedList<'a> {
    pub ui: &'a mut Ui,
    rows: usize,
    /// Where to light the hovered row once the last row is known.
    hover: Option<(egui::layers::ShapeIdx, egui::Rect, usize)>,
    /// Height of a one-line row.
    pub row_height: f32,
}

impl BoxedList<'_> {
    /// A row with `title` (and an optional dimmed `subtitle`) on the left and `suffix` on the right.
    pub fn row<R>(
        &mut self,
        title: &str,
        subtitle: Option<&str>,
        suffix: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        self.separator();
        let ui = &mut *self.ui;
        let height = if subtitle.is_some() {
            58.0
        } else {
            self.row_height
        };
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let inner = rect.shrink2(Vec2::new(12.0, 0.0));
        // The title keeps its room; the control on the right gets the rest.
        let body = ui.style().text_styles[&TextStyle::Body].size;
        let title_width = ui
            .painter()
            .layout_no_wrap(title.to_owned(), FontId::proportional(body), Color32::WHITE)
            .size()
            .x;
        let suffix_rect = egui::Rect::from_min_max(
            egui::pos2(
                inner.left() + (title_width + 24.0).min(inner.width() * 0.6),
                inner.top(),
            ),
            inner.max,
        );
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(suffix_rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        row.spacing_mut().item_spacing.x = 6.0;
        let out = suffix(&mut row);
        let left = egui::Rect::from_min_max(
            inner.min,
            egui::pos2(row.min_rect().left() - 12.0, inner.max.y),
        );
        let mut text = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(left)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        text.spacing_mut().item_spacing.y = 2.0;
        let lines = if subtitle.is_some() { 2.0 } else { 1.0 };
        let body = text.style().text_styles[&TextStyle::Body].size;
        text.add_space(((height - lines * (body + 4.0)) / 2.0).max(0.0));
        text.add(egui::Label::new(title).truncate());
        if let Some(subtitle) = subtitle {
            text.add(egui::Label::new(muted(subtitle).small()).truncate());
        }
        out
    }

    /// A row whose whole width switches `on`.
    pub fn switch_row(&mut self, title: &str, subtitle: Option<&str>, on: &mut bool) -> bool {
        self.switch_row_enabled(title, subtitle, on, true)
    }

    /// A switch row that can be greyed out while it has nothing to do.
    pub fn switch_row_enabled(
        &mut self,
        title: &str,
        subtitle: Option<&str>,
        on: &mut bool,
        enabled: bool,
    ) -> bool {
        self.separator();
        let height = if subtitle.is_some() { 58.0 } else { 50.0 };
        let width = self.ui.available_width();
        let (rect, _) = self
            .ui
            .allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let mut builder = egui::UiBuilder::new().max_rect(rect);
        if !enabled {
            builder = builder.disabled();
        }
        let ui = &mut self.ui.new_child(builder);
        let response = ui.interact(rect, ui.id().with(title), Sense::click());
        let p = palette(ui.ctx());
        let lit = self.ui.painter().add(egui::Shape::Noop);
        if response.hovered() && ui.is_enabled() {
            self.hover = Some((lit, rect, self.rows));
        }
        let inner = rect.shrink2(Vec2::new(12.0, 0.0));
        let knob = egui::Rect::from_center_size(
            egui::pos2(inner.right() - SWITCH.x / 2.0, inner.center().y),
            SWITCH,
        );
        let changed = response.clicked();
        if changed {
            *on = !*on;
        }
        paint_switch(ui, knob, *on, &response);
        let enabled = ui.is_enabled();
        response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, *on, title));
        let painter = ui.painter();
        let body = ui.style().text_styles[&TextStyle::Body].size;
        let color = ink(ui);
        let width = (knob.left() - inner.left() - 12.0).max(40.0);
        let title_galley =
            painter.layout(title.to_owned(), FontId::proportional(body), color, width);
        let mut y = inner.center().y - title_galley.size().y / 2.0;
        if let Some(subtitle) = subtitle {
            let small = ui.style().text_styles[&TextStyle::Small].size;
            let dim = if enabled {
                p.dim
            } else {
                p.dim.gamma_multiply(0.5)
            };
            let sub = painter.layout(subtitle.to_owned(), FontId::proportional(small), dim, width);
            y = inner.center().y - (title_galley.size().y + 2.0 + sub.size().y) / 2.0;
            painter.galley(
                egui::pos2(inner.left(), y + title_galley.size().y + 2.0),
                sub,
                dim,
            );
        }
        painter.galley(egui::pos2(inner.left(), y), title_galley, color);
        changed
    }

    /// A row that is a button across the whole width.
    pub fn button_row(&mut self, title: &str, icon: Option<Icon>, hint: &str) -> Response {
        self.separator();
        let ui = &mut *self.ui;
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
        let p = palette(ui.ctx());
        let lit = ui.painter().add(egui::Shape::Noop);
        if response.hovered() {
            self.hover = Some((lit, rect, self.rows));
        }
        let mut x = rect.left() + 12.0;
        if let Some(icon) = icon {
            let at = egui::Rect::from_min_size(
                egui::pos2(x, rect.center().y - ICON / 2.0),
                Vec2::splat(ICON),
            );
            icons::paint(ui, icon, at, ICON, p.dim);
            x += ICON + 12.0;
        }
        let body = ui.style().text_styles[&TextStyle::Body].size;
        let width = (rect.right() - 12.0 - x).max(20.0);
        let mut job = egui::text::LayoutJob::simple_singleline(
            title.to_owned(),
            FontId::proportional(body),
            p.text,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(width);
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(
            egui::pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            p.text,
        );
        super::geometry::note(ui.ctx(), title, rect);
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, title));
        if hint.is_empty() {
            response
        } else {
            response.on_hover_text(hint)
        }
    }

    fn separator(&mut self) {
        if self.rows > 0 {
            let p = palette(self.ui.ctx());
            let (rect, _) = self
                .ui
                .allocate_exact_size(Vec2::new(self.ui.available_width(), 1.0), Sense::hover());
            self.ui.painter().rect_filled(rect, 0.0, p.card_border);
        }
        self.rows += 1;
    }
}

/// The bold heading over a group of rows.
pub fn group_title(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    let size = ui.style().text_styles[&TextStyle::Body].size;
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        heading(ui, title, Some(size));
    });
    ui.add_space(2.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_text_follows_the_desktop_font_size() {
        assert_eq!(body_size(None), 14.0);
        assert!((body_size(Some(11.0)) - 14.666).abs() < 0.01);
        assert_eq!(body_size(Some(4.0)), 12.0, "tiny sizes stay legible");
        assert_eq!(body_size(Some(40.0)), 20.0);
    }

    #[test]
    fn both_schemes_keep_text_readable() {
        let accent = Color32::from_rgb(0xe6, 0x2d, 0x42);
        for scheme in [Scheme::Dark, Scheme::Light] {
            let p = Palette::new(scheme, accent);
            let luma =
                |c: Color32| 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
            assert!(
                (luma(p.text) - luma(p.window)).abs() > 150.0,
                "{scheme:?} text must stand out from the window"
            );
            assert_eq!(p.accent, accent);
        }
    }
}
