//! Lumen's look: flat and quiet, in the desktop's light or dark scheme and accent colour,
//! with colours modelled on libadwaita so the window sits naturally on a GNOME desktop.

use eframe::egui::{
    self, Button, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Margin,
    Response, RichText, Shadow, Stroke, TextStyle, Ui, Vec2, WidgetInfo, WidgetType,
};

use super::appearance::{Scheme, SystemFonts};

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
    visuals.menu_corner_radius = CornerRadius::same(WINDOW_RADIUS - 2);
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

/// A borderless button with a label that only shows its shape under the pointer.
pub fn flat(ui: &mut Ui, text: &str) -> Response {
    let response = ui.add(
        Button::new(text)
            .frame_when_inactive(false)
            .min_size(Vec2::new(0.0, BUTTON_HEIGHT)),
    );
    super::geometry::note(ui.ctx(), text, response.rect);
    response
}

/// A flat square button with a glyph, named for screen readers and as a tooltip.
pub fn icon(ui: &mut Ui, glyph: &str, name: &str) -> Response {
    let text = RichText::new(glyph).size(16.0);
    named(
        ui.add(
            Button::new(text)
                .frame_when_inactive(false)
                .min_size(Vec2::splat(BUTTON_HEIGHT)),
        ),
        name,
    )
}

/// A flat icon button that stays lit, in the accent colour, while `on`.
pub fn toggle(ui: &mut Ui, glyph: &str, name: &str, on: bool) -> Response {
    let p = palette(ui.ctx());
    let mut text = RichText::new(glyph).size(16.0);
    if on {
        text = text.color(p.accent_text);
    }
    let mut button = Button::new(text)
        .frame_when_inactive(on)
        .min_size(Vec2::splat(BUTTON_HEIGHT));
    if on {
        button = button.fill(p.selected);
    }
    let response = named(ui.add(button), name);
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, on, name));
    response
}

/// The round play/pause button.
pub fn circular(ui: &mut Ui, glyph: &str, name: &str) -> Response {
    let size = BUTTON_HEIGHT + 6.0;
    let response = ui.add(
        Button::new(RichText::new(glyph).size(18.0))
            .corner_radius(CornerRadius::same((size / 2.0) as u8))
            .min_size(Vec2::splat(size)),
    );
    named(response, name)
}

/// Gives a widget an accessible name and the same text as a tooltip.
pub fn named(response: Response, name: &str) -> Response {
    super::geometry::note(&response.ctx, name, response.rect);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name));
    response.on_hover_text(name)
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
