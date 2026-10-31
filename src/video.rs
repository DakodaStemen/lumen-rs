//! Renders mpv's picture into an offscreen texture that egui shows like any other image.

use std::ffi::{CStr, CString, c_void};
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use eframe::egui;
use eframe::glow::{self, HasContext};
use libmpv2::Mpv;
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};

use crate::engine::Engine;

/// Largest picture mpv renders; bigger windows scale this texture up.
pub const TARGET_WIDTH: i32 = 3840;
pub const TARGET_HEIGHT: i32 = 2160;

/// eframe's GL loader, only valid while the render context is being created.
struct Loader(*const (dyn Fn(&CStr) -> *const c_void + 'static));

fn get_proc_address(loader: &Loader, name: &str) -> *mut c_void {
    let name = CString::new(name).expect("GL symbol names contain no NUL");
    unsafe { (*loader.0)(&name) as *mut c_void }
}

pub struct Video {
    // Declared first so it is dropped before the player it borrows from.
    render: RenderContext<'static>,
    _mpv: Arc<Mpv>,
    /// Size of the picture drawn into the top-left of the target this frame.
    size: (i32, i32),
    target: Option<Target>,
}

/// The offscreen framebuffer mpv draws into, with the texture behind it.
struct Target {
    framebuffer: glow::Framebuffer,
    texture: egui::TextureId,
    /// The context the framebuffer belongs to, kept so `Drop` can delete it.
    gl: Arc<glow::Context>,
}

// The render context is only used from the paint callback, with the GL context current.
unsafe impl Send for Video {}

impl Video {
    pub fn new(engine: &Engine, cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let mpv = engine.handle().clone();
        let proc_address = cc.get_proc_address.context("eframe exposes no GL loader")?;
        // mpv only calls the loader while the context is created, so the borrow never outlives this call.
        let loader = Loader(unsafe {
            std::mem::transmute::<&dyn Fn(&CStr) -> *const c_void, _>(proc_address)
        });
        let mut render = mpv
            .create_render_context([
                RenderParam::ApiType(RenderParamApiType::OpenGl),
                RenderParam::InitParams(OpenGLInitParams {
                    get_proc_address,
                    ctx: loader,
                }),
            ])
            .map_err(|e| anyhow!("could not create the mpv render context: {e}"))?;
        let repaint = cc.egui_ctx.clone();
        render.set_update_callback(move || repaint.request_repaint());
        // SAFETY: `_mpv` keeps the player alive for as long as this render context exists.
        let render =
            unsafe { std::mem::transmute::<RenderContext<'_>, RenderContext<'static>>(render) };
        Ok(Self {
            render,
            _mpv: mpv,
            size: (1, 1),
            target: None,
        })
    }

    pub fn set_size(&mut self, width: i32, height: i32) {
        self.size = (width.clamp(1, TARGET_WIDTH), height.clamp(1, TARGET_HEIGHT));
    }

    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    /// Creates the target on first use and returns the texture to show.
    pub fn texture(
        &mut self,
        gl: &Arc<glow::Context>,
        frame: &mut eframe::Frame,
    ) -> egui::TextureId {
        if let Some(target) = &self.target {
            return target.texture;
        }
        unsafe {
            let texture = gl.create_texture().expect("create texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                TARGET_WIDTH,
                TARGET_HEIGHT,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            let framebuffer = gl.create_framebuffer().expect("create framebuffer");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            let id = frame.register_native_glow_texture(texture);
            self.target = Some(Target {
                framebuffer,
                texture: id,
                gl: gl.clone(),
            });
            id
        }
    }

    /// The part of the texture holding the picture, for use as image UVs.
    pub fn uv(&self) -> egui::Rect {
        egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(
                self.size.0 as f32 / TARGET_WIDTH as f32,
                self.size.1 as f32 / TARGET_HEIGHT as f32,
            ),
        )
    }

    /// Renders the current frame. Must run with eframe's GL context current.
    pub fn draw(&mut self, gl: &glow::Context) {
        let Some(framebuffer) = self.target.as_ref().map(|t| t.framebuffer) else {
            return;
        };
        unsafe {
            let previous = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) as u32;
            // Not flipped: row 0 of the texture is the top of the picture, as egui expects.
            if let Err(err) = self.render.render::<Loader>(
                framebuffer.0.get() as i32,
                self.size.0,
                self.size.1,
                false,
            ) {
                eprintln!("mpv render: {err}");
            }
            gl.bind_framebuffer(
                glow::FRAMEBUFFER,
                std::num::NonZeroU32::new(previous).map(glow::NativeFramebuffer),
            );
        }
    }
}

impl Drop for Video {
    /// Frees the framebuffer; `App::on_exit` drops the video while eframe's GL context is still
    /// current. The texture is not deleted here: it was registered with egui's painter, which
    /// owns it and deletes it when the painter is destroyed. `render` is freed after this runs,
    /// and before `_mpv`, by field order.
    fn drop(&mut self) {
        if let Some(target) = self.target.take() {
            // SAFETY: the framebuffer was created on `target.gl` and is used by nothing else.
            unsafe { target.gl.delete_framebuffer(target.framebuffer) };
        }
    }
}
