//! Lumen's look: a quiet, flat dark interface with a mint accent.

use eframe::egui::{
    self, Button, Color32, CornerRadius, FontId, Margin, Response, RichText, Stroke, TextStyle, Ui,
    Vec2, WidgetInfo, WidgetType,
};

pub const BG: Color32 = Color32::from_rgb(0x0a, 0x0c, 0x10);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x11, 0x14, 0x1a);
pub const RAISED: Color32 = Color32::from_rgb(0x18, 0x1c, 0x24);
pub const BORDER: Color32 = Color32::from_rgb(0x23, 0x28, 0x33);
pub const TEXT: Color32 = Color32::from_rgb(0xe9, 0xec, 0xf2);
pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x93, 0xa6);
pub const WIDGET: Color32 = RAISED;
pub const WIDGET_BORDER: Color32 = BORDER;
pub const HOVER: Color32 = Color32::from_rgb(0x20, 0x25, 0x30);
pub const INPUT: Color32 = BG;
pub const MINT: Color32 = Color32::from_rgb(0x7c, 0xf0, 0xc8);
pub const BLUE: Color32 = Color32::from_rgb(0x5b, 0x8c, 0xff);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0xb4, 0x54);
pub const ACCENT: Color32 = MINT;
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xa0, 0xf5, 0xd8);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x0a, 0x0c, 0x10);
pub const SELECTED_BG: Color32 = Color32::from_rgb(0x14, 0x24, 0x24);
pub const SELECTED_TEXT: Color32 = MINT;
pub const SELECTED_BORDER: Color32 = Color32::from_rgb(0x4a, 0xa8, 0x8e);
pub const TRACK: Color32 = Color32::from_rgb(0x2a, 0x30, 0x3c);
pub const HANDLE: Color32 = Color32::from_rgb(0xf4, 0xf7, 0xfb);
pub const POPUP: Color32 = RAISED;
pub const POPUP_BORDER: Color32 = Color32::from_rgb(0x2e, 0x35, 0x42);

pub const RADIUS: u8 = 8;
pub const BUTTON_HEIGHT: f32 = 36.0;

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = POPUP;
    visuals.window_stroke = Stroke::new(1.0_f32, POPUP_BORDER);
    visuals.window_corner_radius = CornerRadius::same(RADIUS);
    visuals.menu_corner_radius = CornerRadius::same(RADIUS);
    visuals.extreme_bg_color = INPUT;
    visuals.faint_bg_color = SIDEBAR;
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = SELECTED_BG;
    visuals.popup_shadow.color = Color32::from_black_alpha(90);
    visuals.selection.stroke = Stroke::new(1.0_f32, MINT);
    visuals.text_cursor.stroke = Stroke::new(1.5_f32, MINT);
    visuals.warn_fg_color = WARN;

    let radius = CornerRadius::same(RADIUS);
    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = SIDEBAR;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    for (state, fill, stroke) in [
        (&mut w.inactive, WIDGET, WIDGET_BORDER),
        (&mut w.hovered, HOVER, WIDGET_BORDER),
        (&mut w.active, HOVER, MINT),
        (&mut w.open, HOVER, WIDGET_BORDER),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0_f32, stroke);
        state.corner_radius = radius;
        state.fg_stroke = Stroke::new(1.0_f32, TEXT);
        state.expansion = 0.0;
    }
    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(14.0, 8.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.combo_width = 70.0;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(15.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(12.0));
    });
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(MUTED).size(11.0)
}

/// The mint call-to-action button.
pub fn primary(ui: &mut Ui, text: &str, width: Option<f32>) -> Response {
    let label = RichText::new(text).strong().color(ACCENT_TEXT);
    let mut button = Button::new(label)
        .fill(ACCENT)
        .stroke(Stroke::NONE)
        .corner_radius(RADIUS);
    button = button.min_size(Vec2::new(width.unwrap_or(0.0), BUTTON_HEIGHT));
    let response = ui.add(button);
    super::geometry::note(ui.ctx(), text, response.rect);
    if response.hovered() {
        ui.painter()
            .rect_filled(response.rect, CornerRadius::same(RADIUS), ACCENT_HOVER);
        ui.painter().text(
            response.rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::proportional(13.0),
            ACCENT_TEXT,
        );
    }
    response
}

/// A full-width secondary button.
pub fn wide(ui: &mut Ui, text: &str) -> Response {
    let response =
        ui.add(Button::new(text).min_size(Vec2::new(ui.available_width(), BUTTON_HEIGHT)));
    super::geometry::note(ui.ctx(), text, response.rect);
    response
}

/// A square button with a glyph, named for screen readers and as a tooltip.
pub fn icon(ui: &mut Ui, glyph: &str, name: &str) -> Response {
    named(
        ui.add(Button::new(RichText::new(glyph).size(16.0)).min_size(Vec2::splat(BUTTON_HEIGHT))),
        name,
    )
}

/// An icon button that stays lit while `on`.
pub fn toggle(ui: &mut Ui, glyph: &str, name: &str, on: bool) -> Response {
    let text = RichText::new(glyph)
        .size(16.0)
        .color(if on { SELECTED_TEXT } else { TEXT });
    let mut button = Button::new(text).min_size(Vec2::splat(BUTTON_HEIGHT));
    if on {
        button = button
            .fill(SELECTED_BG)
            .stroke(Stroke::new(1.0_f32, SELECTED_BORDER));
    }
    named(ui.add(button), name)
}

/// Gives a widget an accessible name and the same text as a tooltip.
pub fn named(response: Response, name: &str) -> Response {
    super::geometry::note(&response.ctx, name, response.rect);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name));
    response.on_hover_text(name)
}
