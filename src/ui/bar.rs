//! A slim, draggable bar used for both the seek bar and the volume control.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Response, Sense, Ui, Vec2};

use super::theme;

pub struct Bar<'a> {
    /// Where the handle sits, from 0 to 1.
    pub value: f32,
    pub enabled: bool,
    /// Small ticks along the track, such as chapter starts.
    pub markers: &'a [f32],
    /// A highlighted stretch, such as an A-B loop.
    pub band: Option<(f32, f32)>,
}

pub struct BarOutput {
    pub response: Response,
    /// The value under the pointer while it hovers or drags.
    pub pointer: Option<f32>,
    pub pressed: bool,
    /// True on the frame the pointer is released or clicks.
    pub released: bool,
}

/// The value a pointer at `x` selects on a bar spanning `rect`.
pub fn value_at(x: f32, rect: Rect) -> f32 {
    ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0)
}

pub fn show(ui: &mut Ui, size: Vec2, bar: &Bar) -> BarOutput {
    let sense = if bar.enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let active = bar.enabled && (response.hovered() || response.dragged());
    let thickness = if active { 6.0 } else { 4.0 };
    let inset = 7.0;
    let track_rect = Rect::from_center_size(
        rect.center(),
        Vec2::new((rect.width() - 2.0 * inset).max(1.0), thickness),
    );
    let pointer = ui
        .input(|i| i.pointer.latest_pos())
        .filter(|_| {
            bar.enabled
                && (response.hovered()
                    || response.dragged()
                    || response.drag_stopped()
                    || response.clicked())
        })
        .map(|p| value_at(p.x, track_rect));
    let pressed = bar.enabled && response.is_pointer_button_down_on();
    let released = bar.enabled && (response.drag_stopped() || response.clicked());

    if ui.is_rect_visible(rect) {
        let p = theme::palette(ui.ctx());
        let painter = ui.painter();
        let x = |v: f32| track_rect.left() + v.clamp(0.0, 1.0) * track_rect.width();
        let round = CornerRadius::same((thickness / 2.0).ceil() as u8);
        painter.rect_filled(track_rect, round, p.track);

        if let Some((a, b)) = bar.band {
            let band = Rect::from_min_max(
                Pos2::new(x(a), track_rect.top() - 2.0),
                Pos2::new(x(b).max(x(a) + 2.0), track_rect.bottom() + 2.0),
            );
            painter.rect_filled(band, CornerRadius::same(2), p.warning.gamma_multiply(0.4));
        }
        let shown = bar.value.clamp(0.0, 1.0);
        let fill = if bar.enabled {
            p.accent
        } else {
            p.dim.gamma_multiply(0.5)
        };
        let played = Rect::from_min_max(track_rect.min, Pos2::new(x(shown), track_rect.max.y));
        if played.width() > 0.5 {
            painter.rect_filled(played, round, fill);
        }

        let tick = if p.dark() {
            Color32::from_white_alpha(90)
        } else {
            Color32::from_black_alpha(90)
        };
        for &marker in bar.markers {
            let at = x(marker);
            painter.line_segment(
                [
                    Pos2::new(at, track_rect.top() - 1.0),
                    Pos2::new(at, track_rect.bottom() + 1.0),
                ],
                egui::Stroke::new(1.0_f32, tick),
            );
        }
        if bar.enabled && active {
            let centre = Pos2::new(x(shown), rect.center().y);
            painter.circle_filled(
                centre + Vec2::new(0.0, 1.0),
                7.5,
                Color32::from_black_alpha(50),
            );
            painter.circle_filled(centre, 7.0, Color32::WHITE);
        }
        if response.has_focus() {
            painter.rect_stroke(
                rect.shrink(1.0),
                CornerRadius::same(theme::RADIUS),
                egui::Stroke::new(2.0_f32, p.accent.gamma_multiply(0.6)),
                egui::StrokeKind::Inside,
            );
        }
    }
    BarOutput {
        response,
        pointer,
        pressed,
        released,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_positions_map_onto_the_bar() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 0.0), Vec2::new(200.0, 20.0));
        assert_eq!(value_at(100.0, rect), 0.0);
        assert_eq!(value_at(200.0, rect), 0.5);
        assert_eq!(value_at(300.0, rect), 1.0);
        assert_eq!(
            value_at(-50.0, rect),
            0.0,
            "dragging past the left end clamps"
        );
        assert_eq!(
            value_at(900.0, rect),
            1.0,
            "dragging past the right end clamps"
        );
    }
}
