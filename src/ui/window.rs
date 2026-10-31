//! Window decorations drawn by Lumen itself, the way GNOME apps draw theirs.
//!
//! GNOME on Wayland has no server-side decorations, so without this winit stacks a plain
//! fallback title bar on top of Lumen's header bar. Instead the window opens undecorated: the
//! header bar moves the window, double-clicking it maximizes, the edges resize, and the window
//! buttons follow the desktop's button layout. Elsewhere the platform frame is kept.

use eframe::egui::{
    self, Color32, CursorIcon, Rect, ResizeDirection, Response, Sense, Stroke, Ui, Vec2,
    ViewportCommand,
};

use super::icons::{self, Icon};
use super::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// Which window buttons go on each side of the header bar, outermost last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonLayout {
    pub left: Vec<WindowButton>,
    pub right: Vec<WindowButton>,
}

impl Default for ButtonLayout {
    fn default() -> Self {
        Self {
            left: Vec::new(),
            right: vec![WindowButton::Close],
        }
    }
}

/// Whether Lumen draws its own decorations: only on GNOME under Wayland. `LUMEN_CSD=0` or `1`
/// overrides the guess.
pub fn client_side_decorations() -> bool {
    match std::env::var("LUMEN_CSD").as_deref() {
        Ok("1") => return true,
        Ok("0") => return false,
        _ => {}
    }
    if !cfg!(any(target_os = "linux", target_os = "freebsd")) {
        return false;
    }
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var("XDG_SESSION_TYPE").map_or(true, |t| t != "x11");
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    wayland && is_gnome(&desktop)
}

pub fn is_gnome(desktop: &str) -> bool {
    desktop
        .split(':')
        .any(|d| d.eq_ignore_ascii_case("GNOME") || d.eq_ignore_ascii_case("ubuntu"))
}

/// The desktop's window button layout, from GNOME's settings.
pub fn button_layout() -> ButtonLayout {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.wm.preferences", "button-layout"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|text| parse_button_layout(&text))
        .unwrap_or_default()
}

/// `'appmenu:minimize,maximize,close'` -> left [], right [minimize, maximize, close].
pub fn parse_button_layout(text: &str) -> ButtonLayout {
    let text = text.trim().trim_matches('\'');
    let (left, right) = text.split_once(':').unwrap_or(("", text));
    let parse = |side: &str| {
        side.split(',')
            .filter_map(|b| match b.trim() {
                "minimize" => Some(WindowButton::Minimize),
                "maximize" => Some(WindowButton::Maximize),
                "close" => Some(WindowButton::Close),
                _ => None,
            })
            .collect()
    };
    ButtonLayout {
        left: parse(left),
        right: parse(right),
    }
}

pub fn is_maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

/// Makes `rect` move the window when dragged and maximize it when double-clicked. Call before
/// drawing the buttons on it, so they take the pointer first.
pub fn drag_area(ui: &Ui, rect: Rect, id: &str, resizing: bool) {
    // Not a Tab stop: there is nothing to do with it from the keyboard.
    let response = ui.interact(rect, ui.id().with(id), Sense::CLICK | Sense::DRAG);
    if response.drag_started_by(egui::PointerButton::Primary) && !resizing {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if response.double_clicked() {
        let maximized = is_maximized(ui.ctx());
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }
}

/// Draws `buttons` in order. In a right-to-left layout pass them reversed, so the outermost
/// one lands at the window's edge.
pub fn buttons(ui: &mut Ui, buttons: &[WindowButton]) {
    let maximized = is_maximized(ui.ctx());
    for &button in buttons {
        let (icon, name) = match button {
            WindowButton::Minimize => (Icon::WindowMinimize, "Minimize"),
            WindowButton::Maximize if maximized => (Icon::WindowRestore, "Restore"),
            WindowButton::Maximize => (Icon::WindowMaximize, "Maximize"),
            WindowButton::Close => (Icon::Close, "Close"),
        };
        if control(ui, icon, name).clicked() {
            ui.ctx().send_viewport_cmd(match button {
                WindowButton::Minimize => ViewportCommand::Minimized(true),
                WindowButton::Maximize => ViewportCommand::Maximized(!maximized),
                WindowButton::Close => ViewportCommand::Close,
            });
        }
    }
}

/// libadwaita's window control: a 24-point circle, faintly filled, in a 34-point slot.
fn control(ui: &mut Ui, icon: Icon, name: &str) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(30.0, theme::BUTTON_HEIGHT), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = theme::palette(ui.ctx());
        let base = if p.dark() {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        let alpha = if response.is_pointer_button_down_on() {
            0.30
        } else if response.hovered() {
            0.15
        } else {
            0.10
        };
        let centre = rect.center();
        ui.painter()
            .circle_filled(centre, 12.0, base.gamma_multiply(alpha));
        icons::paint(ui, icon, rect, 16.0, p.text);
        if response.has_focus() {
            ui.painter().circle_stroke(
                centre,
                13.0,
                Stroke::new(2.0_f32, p.accent.gamma_multiply(0.6)),
            );
        }
    }
    theme::finish(response, name, "", None)
}

/// Which edge of an undecorated window the pointer is on, if any.
pub fn edge_at(pos: egui::Pos2, screen: Rect) -> Option<ResizeDirection> {
    let (edge, corner) = (5.0, 14.0);
    let left = pos.x < screen.left() + edge;
    let right = pos.x > screen.right() - edge;
    let top = pos.y < screen.top() + edge;
    let bottom = pos.y > screen.bottom() - edge;
    let near_left = pos.x < screen.left() + corner;
    let near_right = pos.x > screen.right() - corner;
    let near_top = pos.y < screen.top() + corner;
    let near_bottom = pos.y > screen.bottom() - corner;
    if (top && near_left) || (left && near_top) {
        Some(ResizeDirection::NorthWest)
    } else if (top && near_right) || (right && near_top) {
        Some(ResizeDirection::NorthEast)
    } else if (bottom && near_left) || (left && near_bottom) {
        Some(ResizeDirection::SouthWest)
    } else if (bottom && near_right) || (right && near_bottom) {
        Some(ResizeDirection::SouthEast)
    } else if top {
        Some(ResizeDirection::North)
    } else if bottom {
        Some(ResizeDirection::South)
    } else if left {
        Some(ResizeDirection::West)
    } else if right {
        Some(ResizeDirection::East)
    } else {
        None
    }
}

/// Starts a resize when the window edge is pressed. Call before the panels; returns the edge
/// under the pointer so the header bar does not also start a move.
pub fn edges(ctx: &egui::Context) -> Option<ResizeDirection> {
    let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
    if is_maximized(ctx) || fullscreen {
        return None;
    }
    let pos = ctx.input(|i| i.pointer.latest_pos())?;
    let dir = edge_at(pos, ctx.content_rect())?;
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
    }
    Some(dir)
}

/// Sets the resize cursor over the edge, after widgets have set theirs, and outlines the
/// window, which GNOME draws no border around when the app decorates itself.
pub fn frame(ctx: &egui::Context, edge: Option<ResizeDirection>) {
    if let Some(dir) = edge {
        ctx.set_cursor_icon(match dir {
            ResizeDirection::North => CursorIcon::ResizeNorth,
            ResizeDirection::South => CursorIcon::ResizeSouth,
            ResizeDirection::East => CursorIcon::ResizeEast,
            ResizeDirection::West => CursorIcon::ResizeWest,
            ResizeDirection::NorthEast => CursorIcon::ResizeNorthEast,
            ResizeDirection::NorthWest => CursorIcon::ResizeNorthWest,
            ResizeDirection::SouthEast => CursorIcon::ResizeSouthEast,
            ResizeDirection::SouthWest => CursorIcon::ResizeSouthWest,
        });
    }
    let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
    if !is_maximized(ctx) && !fullscreen {
        let p = theme::palette(ctx);
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("window_outline"),
        ))
        .rect_stroke(
            ctx.content_rect(),
            0.0,
            Stroke::new(1.0_f32, p.border),
            egui::StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Pos2;

    #[test]
    fn button_layouts_parse_like_gnome_reads_them() {
        assert_eq!(
            parse_button_layout("'appmenu:close'\n"),
            ButtonLayout {
                left: vec![],
                right: vec![WindowButton::Close]
            }
        );
        assert_eq!(
            parse_button_layout("'close,minimize:appmenu'"),
            ButtonLayout {
                left: vec![WindowButton::Close, WindowButton::Minimize],
                right: vec![]
            }
        );
        assert_eq!(
            parse_button_layout("appmenu:minimize,maximize,close").right,
            [
                WindowButton::Minimize,
                WindowButton::Maximize,
                WindowButton::Close
            ]
        );
        assert_eq!(parse_button_layout("").right, Vec::<WindowButton>::new());
    }

    #[test]
    fn only_gnome_sessions_count_as_gnome() {
        assert!(is_gnome("GNOME"));
        assert!(is_gnome("ubuntu:GNOME"));
        assert!(!is_gnome("KDE"));
        assert!(!is_gnome(""));
    }

    #[test]
    fn edges_and_corners_resize_the_right_way() {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        assert_eq!(edge_at(Pos2::new(400.0, 300.0), screen), None);
        assert_eq!(
            edge_at(Pos2::new(400.0, 2.0), screen),
            Some(ResizeDirection::North)
        );
        assert_eq!(
            edge_at(Pos2::new(798.0, 300.0), screen),
            Some(ResizeDirection::East)
        );
        assert_eq!(
            edge_at(Pos2::new(2.0, 10.0), screen),
            Some(ResizeDirection::NorthWest)
        );
        assert_eq!(
            edge_at(Pos2::new(790.0, 598.0), screen),
            Some(ResizeDirection::SouthEast)
        );
    }
}
