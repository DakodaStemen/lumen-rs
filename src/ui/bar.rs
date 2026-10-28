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
    let inset = 7.0;
    let track_rect = Rect::from_center_size(
        rect.center(),
        Vec2::new((rect.width() - 2.0 * inset).max(1.0), 5.0),
    );
    let active = bar.enabled && (response.hovered() || response.dragged());
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
        let painter = ui.painter();
        let x = |v: f32| track_rect.left() + v.clamp(0.0, 1.0) * track_rect.width();
        let round = CornerRadius::same(3);
        painter.rect_filled(track_rect, round, theme::TRACK);

        if let Some((a, b)) = bar.band {
            let band = Rect::from_min_max(
                Pos2::new(x(a), rect.center().y - 5.0),
                Pos2::new(x(b).max(x(a) + 2.0), rect.center().y + 5.0),
            );
            painter.rect_filled(
                band,
                CornerRadius::same(2),
                theme::ACCENT.gamma_multiply(0.32),
            );
        }
        let shown = bar.value.clamp(0.0, 1.0);
        let fill = if bar.enabled {
            theme::ACCENT
        } else {
            theme::ACCENT.gamma_multiply(0.45)
        };
        painter.rect_filled(
            Rect::from_min_max(track_rect.min, Pos2::new(x(shown), track_rect.max.y)),
            round,
            fill,
        );

        for &marker in bar.markers {
            let at = x(marker);
            painter.line_segment(
                [
                    Pos2::new(at, rect.center().y - 5.0),
                    Pos2::new(at, rect.center().y + 5.0),
                ],
                egui::Stroke::new(1.5_f32, Color32::from_black_alpha(160)),
            );
        }
        if bar.enabled {
            let radius = if active { 8.0 } else { 6.0 };
            painter.circle_filled(Pos2::new(x(shown), rect.center().y), radius, theme::HANDLE);
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
