//! The eframe application: the interface plus the GPU video it draws into.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32};
use eframe::{egui_glow, glow};

use crate::media_keys::{Command, MediaKeys, Playback, Status};
use crate::session::{Session, State};
use crate::ui::{Frontend, Surface};
use crate::video::Video;

pub struct App {
    pub frontend: Frontend,
    video: Arc<Mutex<Option<Video>>>,
    media_keys: Option<MediaKeys>,
}

impl App {
    pub fn new(session: Session, video: Video, ctx: &egui::Context) -> Self {
        let mut frontend = Frontend::new(session, ctx);
        frontend.follow_desktop(ctx);
        crate::platform::close_on_termination(ctx.clone());
        Self {
            frontend,
            video: Arc::new(Mutex::new(Some(video))),
            media_keys: MediaKeys::spawn(),
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

impl App {
    /// Runs what the desktop's media keys asked for and tells it what is playing.
    fn media_keys(&mut self, ctx: &egui::Context) {
        let Some(keys) = self.media_keys.as_mut() else {
            return;
        };
        let session = &mut self.frontend.session;
        for command in keys.poll() {
            match command {
                Command::Toggle => session.toggle(),
                Command::Play if session.state() == State::Paused => session.toggle(),
                Command::Pause if session.state() == State::Playing => session.toggle(),
                Command::Stop => session.stop(),
                Command::Next => session.advance(1),
                Command::Previous => session.advance(-1),
                Command::Play | Command::Pause => {}
            }
            ctx.request_repaint();
        }
        let title = session
            .queue
            .current
            .and_then(|c| session.queue.items.get(c))
            .map(|item| item.title.clone());
        let playback = match session.state() {
            State::Playing | State::Buffering | State::Opening => Playback::Playing,
            State::Paused => Playback::Paused,
            State::Idle | State::Ended => Playback::Stopped,
        };
        keys.publish(Status {
            title,
            playback,
            position_ms: session.position_ms() as u64,
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut surface = GlSurface {
            video: &self.video,
            frame,
        };
        self.frontend.show(ctx, &mut surface);
        self.media_keys(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&glow::Context>) {
        self.frontend.session.stop();
        self.frontend.session.store.save_settings();
        // The render context must be freed while eframe's GL context is still current.
        self.video.lock().unwrap().take();
    }
}
