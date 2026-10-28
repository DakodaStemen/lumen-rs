//! The eframe application: the interface plus the GPU video it draws into.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32};
use eframe::{egui_glow, glow};

use crate::session::Session;
use crate::ui::{Frontend, Surface};
use crate::video::Video;

pub struct App {
    pub frontend: Frontend,
    video: Arc<Mutex<Option<Video>>>,
}

impl App {
    pub fn new(session: Session, video: Video, ctx: &egui::Context) -> Self {
        Self {
            frontend: Frontend::new(session, ctx),
            video: Arc::new(Mutex::new(Some(video))),
        }
    }
}

/// Draws mpv's picture through a paint callback and an offscreen GL texture.
struct GlSurface<'a> {
    video: &'a Arc<Mutex<Option<Video>>>,
    frame: &'a mut eframe::Frame,
}

impl Surface for GlSurface<'_> {
    fn paint(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let scale = ui.ctx().pixels_per_point();
        let (width, height) = (
            (rect.width() * scale).round() as i32,
            (rect.height() * scale).round() as i32,
        );
        let Some(gl) = self.frame.gl().cloned() else {
            return;
        };
        let (texture, uv) = {
            let mut guard = self.video.lock().unwrap();
            let Some(video) = guard.as_mut() else { return };
            video.set_size(width, height);
            (video.texture(&gl, self.frame), video.uv())
        };
        let video = self.video.clone();
        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
                if let Some(video) = video.lock().unwrap().as_mut() {
                    video.draw(painter.gl());
                }
            })),
        });
        ui.painter().image(texture, rect, uv, Color32::WHITE);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut surface = GlSurface {
            video: &self.video,
            frame,
        };
        self.frontend.show(ctx, &mut surface);
    }

    fn on_exit(&mut self, _gl: Option<&glow::Context>) {
        self.frontend.session.stop();
        self.frontend.session.store.save_settings();
        // The render context must be freed while eframe's GL context is still current.
        self.video.lock().unwrap().take();
    }
}
