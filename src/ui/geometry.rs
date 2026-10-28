//! Where named widgets were drawn on the last frame, so tools that script the interface
//! (the demo recorder) can aim at them by name instead of by pixel position.

use eframe::egui::{Context, Id, Rect};

fn key(label: &str) -> Id {
    Id::new(("lumen-geometry", label.to_owned()))
}

pub fn note(ctx: &Context, label: &str, rect: Rect) {
    ctx.data_mut(|d| d.insert_temp(key(label), rect));
}

pub fn find(ctx: &Context, label: &str) -> Option<Rect> {
    ctx.data(|d| d.get_temp(key(label)))
}
