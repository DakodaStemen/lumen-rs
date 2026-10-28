//! Records a scripted tour of the real player as a GIF.
//!
//! ```sh
//! scripts/record-demo.sh          # makes the clips, runs this, writes docs/demo.gif
//! ```
//!
//! The tour runs in the real window. This program injects the mouse and key input itself, aims at
//! widgets by name (see `lumen_rs::ui::geometry`), draws a cursor and short captions over the
//! window, grabs a frame every tenth of a second and hands them to ffmpeg.
//!
//! It plays only video files without an audio stream and keeps the player muted, so it is silent.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Instant;

use anyhow::{Context as _, Result, anyhow, bail};
use eframe::egui::{self, Color32, Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use lumen_rs::app::App;
use lumen_rs::engine::{Engine, Output};
use lumen_rs::session::Session;
use lumen_rs::store::Store;
use lumen_rs::streams::Tools;
use lumen_rs::ui::geometry;
use lumen_rs::video::Video;

const WINDOW: [f32; 2] = [1000.0, 640.0];
/// Frames per second of the recording, and of the GIF made from it.
const FPS: u32 = 20;
const GIF_FPS: u32 = 10;
const FRAME_EVERY: f32 = 1.0 / FPS as f32;
const GIF_WIDTH: u32 = 800;

/// One step of the tour.
#[derive(Clone)]
enum Act {
    Wait(f32),
    Caption(&'static str),
    /// Files appear to hover over the window for this long.
    HoverFiles(f32),
    DropFiles,
    /// Move the cursor to a named widget, at a fraction of its width and height, over some seconds.
    MoveTo(&'static str, f32, f32, f32),
    Click,
    DoubleClick,
    Press(Key, Modifiers),
}

fn script() -> Vec<Act> {
    use Act::*;
    let none = Modifiers::NONE;
    vec![
        Wait(1.6),
        Caption("Drop anything to play it"),
        HoverFiles(1.3),
        DropFiles,
        Wait(2.2),
        Caption("Hover the seek bar for the time and chapter"),
        MoveTo("Seek bar", 0.64, 0.5, 0.9),
        Wait(1.3),
        MoveTo("Seek bar", 0.42, 0.5, 0.6),
        Click,
        Wait(1.2),
        Caption("Subtitles, audio tracks and sync"),
        MoveTo("Audio & subtitles", 0.5, 0.5, 0.8),
        Click,
        Wait(0.3),
        MoveTo("Subtitles", 0.5, 0.5, 0.5),
        Click,
        Wait(0.5),
        MoveTo("eng · English", 0.5, 0.5, 0.4),
        Click,
        Wait(1.6),
        Caption("Jump between chapters"),
        MoveTo("Video", 0.5, 0.5, 0.7),
        Click,
        Wait(0.3),
        MoveTo("Chapters", 0.5, 0.5, 0.5),
        Click,
        Wait(0.5),
        MoveTo("Zoom   0:08", 0.5, 0.5, 0.4),
        Click,
        Wait(1.4),
        Caption("Speed and A-B loop from the keyboard"),
        Press(Key::CloseBracket, none),
        Wait(0.5),
        Press(Key::CloseBracket, none),
        Wait(1.0),
        Press(Key::L, none),
        Wait(1.6),
        Press(Key::L, none),
        Wait(2.4),
        Press(Key::L, none),
        Press(Key::Equals, none),
        Wait(0.4),
        Caption("A queue with a filter"),
        MoveTo("Coastline at Dusk.mp4", 0.5, 0.5, 0.9),
        DoubleClick,
        Wait(2.0),
        Caption("Every key can be rebound"),
        MoveTo("…", 0.5, 0.5, 0.8),
        Click,
        Wait(0.3),
        MoveTo("Keyboard shortcuts", 0.5, 0.5, 0.5),
        Click,
        Wait(2.0),
        Press(Key::Escape, none),
        Wait(0.8),
    ]
}

struct Move {
    from: Pos2,
    to: Pos2,
    start: f32,
    seconds: f32,
}

struct Encoder {
    child: Child,
    stdin: Option<ChildStdin>,
}

struct Demo {
    app: App,
    media: Vec<PathBuf>,
    out: PathBuf,
    clock: Instant,
    acts: Vec<Act>,
    index: usize,
    act_started: Option<f32>,
    cursor: Pos2,
    moving: Option<Move>,
    /// Input to deliver at a time, in seconds since the start.
    scheduled: VecDeque<(f32, Event)>,
    caption: Option<String>,
    ripple: Option<f32>,
    hovering: bool,
    next_frame_at: f32,
    encoder: Option<Encoder>,
    /// An ffmpeg recording the screen itself, used when the display cannot give frames back.
    grab: Option<Child>,
    frames: u32,
    /// Frames written to the encoder, counting repeats that stand in for slow captures.
    written: u32,
    first_frame_at: Option<f32>,
    finished_at: Option<f32>,
}

impl Demo {
    fn now(&self) -> f32 {
        self.clock.elapsed().as_secs_f32()
    }

    fn push(&mut self, delay: f32, event: Event) {
        let at = self.now() + delay;
        self.scheduled.push_back((at, event));
    }

    fn button(&self, pressed: bool) -> Event {
        Event::PointerButton {
            pos: self.cursor,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    fn key(key: Key, modifiers: Modifiers, pressed: bool) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        }
    }

    /// Advances the script by one frame, adding the input it wants delivered.
    fn drive(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let t = self.now();
        if let Some(motion) = &self.moving {
            let p = ((t - motion.start) / motion.seconds).clamp(0.0, 1.0);
            let eased = p * p * (3.0 - 2.0 * p);
            self.cursor = motion.from + (motion.to - motion.from) * eased;
            raw.events.push(Event::PointerMoved(self.cursor));
            if p >= 1.0 {
                self.moving = None;
            }
        }
        if self.hovering {
            raw.hovered_files = vec![egui::HoveredFile {
                path: self.media.first().cloned(),
                mime: String::new(),
            }];
        }
        while self.scheduled.front().is_some_and(|(at, _)| *at <= t) {
            let (_, event) = self.scheduled.pop_front().expect("checked");
            if matches!(event, Event::PointerButton { pressed: true, .. }) {
                self.ripple = Some(t);
            }
            raw.events.push(event);
        }
        // A step waits for the cursor to arrive and for earlier input to be delivered.
        if self.moving.is_some() || !self.scheduled.is_empty() {
            return;
        }
        let Some(act) = self.acts.get(self.index).cloned() else {
            self.finished_at.get_or_insert(t);
            return;
        };
        let started = *self.act_started.get_or_insert(t);
        let advance = match &act {
            Act::Wait(seconds) => t - started >= *seconds,
            Act::Caption(text) => {
                self.caption = Some((*text).to_owned());
                true
            }
            Act::HoverFiles(seconds) => {
                self.hovering = t - started < *seconds;
                !self.hovering
            }
            Act::DropFiles => {
                raw.dropped_files = self
                    .media
                    .iter()
                    .map(|p| egui::DroppedFile {
                        path: Some(p.clone()),
                        ..Default::default()
                    })
                    .collect();
                true
            }
            Act::MoveTo(label, fx, fy, seconds) => match geometry::find(ctx, label) {
                Some(rect) => {
                    let to = rect.min + Vec2::new(rect.width() * fx, rect.height() * fy);
                    self.moving = Some(Move {
                        from: self.cursor,
                        to,
                        start: t,
                        seconds: *seconds,
                    });
                    true
                }
                None if t - started > 4.0 => panic!("the tour could not find {label:?} on screen"),
                None => false,
            },
            Act::Click => {
                let (down, up) = (self.button(true), self.button(false));
                self.push(0.0, down);
                self.push(0.09, up);
                true
            }
            Act::DoubleClick => {
                for (delay, pressed) in [(0.0, true), (0.07, false), (0.14, true), (0.21, false)] {
                    let event = self.button(pressed);
                    self.push(delay, event);
                }
                true
            }
            Act::Press(key, modifiers) => {
                self.push(0.0, Self::key(*key, *modifiers, true));
                self.push(0.06, Self::key(*key, *modifiers, false));
                true
            }
        };
        if advance {
            self.index += 1;
            self.act_started = None;
        }
    }

    fn draw_overlay(&self, ctx: &egui::Context) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Debug,
            egui::Id::new("demo-overlay"),
        ));
        if let Some(caption) = &self.caption {
            // The empty part of the sidebar: no menu or dialog opens over it.
            let galley = painter.layout(
                caption.clone(),
                egui::FontId::proportional(15.0),
                Color32::WHITE,
                215.0,
            );
            let size = galley.size() + Vec2::new(28.0, 18.0);
            let pill = egui::Rect::from_min_size(Pos2::new(18.0, WINDOW[1] - 215.0), size);
            painter.rect_filled(
                pill,
                egui::CornerRadius::same(14),
                Color32::from_rgba_unmultiplied(18, 20, 25, 235),
            );
            painter.rect_stroke(
                pill,
                egui::CornerRadius::same(14),
                egui::Stroke::new(1.0_f32, Color32::from_rgb(0xc7, 0x78, 0x43)),
                egui::StrokeKind::Inside,
            );
            painter.galley(pill.min + Vec2::new(14.0, 9.0), galley, Color32::WHITE);
        }
        if let Some(start) = self.ripple {
            let age = self.now() - start;
            if age < 0.4 {
                let p = age / 0.4;
                let alpha = ((1.0 - p) * 200.0) as u8;
                painter.circle_stroke(
                    self.cursor,
                    6.0 + 18.0 * p,
                    egui::Stroke::new(
                        2.5_f32,
                        Color32::from_rgba_unmultiplied(0xf8, 0x9b, 0x62, alpha),
                    ),
                );
            }
        }
        let arrow = [
            (0.0, 0.0),
            (0.0, 17.0),
            (4.2, 13.2),
            (7.2, 20.0),
            (10.2, 18.7),
            (7.3, 12.1),
            (12.6, 12.1),
        ];
        let points: Vec<Pos2> = arrow
            .iter()
            .map(|(x, y)| self.cursor + Vec2::new(*x, *y))
            .collect();
        painter.add(egui::Shape::convex_polygon(
            points,
            Color32::WHITE,
            egui::Stroke::new(1.3_f32, Color32::from_rgb(0x10, 0x12, 0x16)),
        ));
    }

    /// Takes the frames that arrived and requests the next one on schedule.
    fn capture(&mut self, ctx: &egui::Context, raw: &egui::RawInput) -> Result<()> {
        if self.grab.is_some() {
            let done = self.finished_at.is_some_and(|end| self.now() - end > 0.5);
            return if done { self.finish() } else { Ok(()) };
        }
        for event in &raw.events {
            let Event::Screenshot { image, .. } = event else {
                continue;
            };
            let [w, h] = image.size;
            if self.encoder.is_none() {
                self.encoder = Some(start_encoder(&self.out, w, h)?);
            }
            let rgb: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|p| [p.r(), p.g(), p.b()])
                .collect();
            // The recording has a fixed frame rate, but grabbing a frame sometimes takes longer than
            // one tick. Repeat the frame to fill the gap so the tour keeps its real pace.
            let now = self.clock.elapsed().as_secs_f32();
            let first = *self.first_frame_at.get_or_insert(now);
            let due = (((now - first) * FPS as f32).round() as u32 + 1).max(self.written + 1);
            let encoder = self.encoder.as_mut().expect("started above");
            let stdin = encoder.stdin.as_mut().context("encoder closed")?;
            for _ in self.written..due {
                stdin.write_all(&rgb)?;
            }
            self.written = due;
            self.frames += 1;
        }
        let t = self.now();
        let done = self.finished_at.is_some_and(|end| t - end > 0.5);
        if t >= self.next_frame_at && !done {
            self.next_frame_at = t.max(self.next_frame_at) + FRAME_EVERY;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if done {
            self.finish()?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        if let Some(mut grab) = self.grab.take() {
            // ffmpeg finishes the file when it reads `q`.
            if let Some(mut stdin) = grab.stdin.take() {
                let _ = stdin.write_all(b"q");
            }
            grab.wait()?;
            make_gif(&self.out)?;
            eprintln!("wrote {} (screen recording)", self.out.display());
            std::process::exit(0);
        }
        let Some(mut encoder) = self.encoder.take() else {
            bail!("no frames were captured")
        };
        drop(encoder.stdin.take());
        let status = encoder.child.wait()?;
        if !status.success() {
            bail!("ffmpeg failed to encode the frames");
        }
        make_gif(&self.out)?;
        eprintln!("wrote {} ({} frames)", self.out.display(), self.frames);
        std::process::exit(0);
    }
}

/// Records the X display with ffmpeg when `DEMO_GRAB=1`. The window's own screenshots read the
/// picture back from the GPU, which a software-rendered virtual display cannot do for video.
fn start_grab(out: &std::path::Path) -> Option<Child> {
    std::env::var_os("DEMO_GRAB")?;
    let display = std::env::var("DISPLAY").ok()?;
    Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "x11grab", "-framerate"])
        .arg(FPS.to_string())
        .args(["-draw_mouse", "0", "-video_size"])
        .arg(format!("{}x{}", WINDOW[0] as u32, WINDOW[1] as u32))
        .arg("-i")
        .arg(format!("{display}+0,0"))
        .args([
            "-an", "-c:v", "libx264", "-crf", "10", "-preset", "veryfast",
        ])
        .args(["-pix_fmt", "yuv420p"])
        .arg(intermediate(out))
        .stdin(Stdio::piped())
        .spawn()
        .ok()
}

fn intermediate(out: &std::path::Path) -> PathBuf {
    out.with_extension("tmp.mp4")
}

fn start_encoder(out: &std::path::Path, width: usize, height: usize) -> Result<Encoder> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
        ])
        .args([
            "-s",
            &format!("{width}x{height}"),
            "-r",
            &FPS.to_string(),
            "-i",
            "-",
            "-an",
        ])
        .args([
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2",
            "-c:v",
            "libx264",
            "-crf",
            "10",
        ])
        .args(["-preset", "veryfast", "-pix_fmt", "yuv420p"])
        .arg(intermediate(out))
        .stdin(Stdio::piped())
        .spawn()
        .context("ffmpeg is needed to record the demo")?;
    let stdin = child.stdin.take();
    Ok(Encoder { child, stdin })
}

/// Turns the intermediate video into a GIF. gifski picks a palette per frame, which keeps text and
/// gradients clean at a size GitHub will show; plain ffmpeg is the fallback when it is not installed.
fn make_gif(out: &std::path::Path) -> Result<()> {
    let result = if which("gifski") {
        gifski_gif(out)
    } else {
        ffmpeg_gif(out)
    };
    if std::env::var_os("DEMO_KEEP").is_none() {
        let _ = std::fs::remove_file(intermediate(out));
    }
    result
}

fn which(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn gifski_gif(out: &std::path::Path) -> Result<()> {
    let frames = tempfile::tempdir()?;
    let status = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-ss", "1.2", "-i"])
        .arg(intermediate(out))
        .args([
            "-vf",
            &format!("fps={GIF_FPS},scale={GIF_WIDTH}:-1:flags=lanczos"),
        ])
        .arg(frames.path().join("frame%04d.png"))
        .status()?;
    if !status.success() {
        bail!("ffmpeg failed to cut the frames");
    }
    let mut command = Command::new("gifski");
    command
        .args(["--fps", &GIF_FPS.to_string(), "--quality", "60"])
        .args(["--lossy-quality", "70"])
        .args(["--width", &GIF_WIDTH.to_string()])
        .arg("-o")
        .arg(out);
    let mut names: Vec<PathBuf> = std::fs::read_dir(frames.path())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    names.sort();
    if !command.args(&names).status()?.success() {
        bail!("gifski failed to make the GIF");
    }
    Ok(())
}

fn ffmpeg_gif(out: &std::path::Path) -> Result<()> {
    let filter = format!(
        "fps={GIF_FPS},scale={GIF_WIDTH}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192:stats_mode=diff[p];\
         [b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle"
    );
    let status = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(intermediate(out))
        .args(["-vf", &filter, "-loop", "0"])
        .arg(out)
        .status()?;
    if !status.success() {
        bail!("ffmpeg failed to make the GIF");
    }
    Ok(())
}

impl eframe::App for Demo {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Err(error) = self.capture(ctx, raw) {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        self.drive(ctx, raw);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.update(ctx, frame);
        self.draw_overlay(ctx);
        ctx.request_repaint();
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.on_exit(gl);
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow!("usage: demo OUTPUT.gif MEDIA_DIR"))?,
    );
    let media_dir = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow!("usage: demo OUTPUT.gif MEDIA_DIR"))?,
    );
    let mut media: Vec<PathBuf> = std::fs::read_dir(&media_dir)
        .with_context(|| format!("cannot read {}", media_dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "mp4" || e == "mkv"))
        .collect();
    media.sort();
    // The chaptered clip with subtitles plays first.
    media.sort_by_key(|p| {
        !p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("Into the Fractal"))
    });
    if media.len() < 2 {
        bail!("{} needs at least two clips", media_dir.display());
    }

    let profile = tempfile::tempdir()?.keep();
    let mut store = Store::open(profile);
    store.settings.recent = media
        .iter()
        .skip(1)
        .take(3)
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    store.save_settings();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(WINDOW)
            .with_resizable(false)
            .with_title("Lumen")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!(
                "../assets/icon.png"
            ))?),
        ..Default::default()
    };
    let _ = std::fs::remove_file(&out);
    eframe::run_native(
        "Lumen",
        options,
        Box::new(move |cc| {
            let engine = Engine::new(Output::Window, store.settings.volume)?;
            engine.set_muted(true);
            // Software decoding: the clips are small, and a hardware path that cannot hand frames to
            // the recording's GL context (a virtual display, say) would leave the picture black.
            engine
                .handle()
                .set_property("hwdec", "no")
                .map_err(|e| anyhow!("could not set hwdec: {e}"))?;
            let video = Video::new(&engine, cc)?;
            let mut session = Session::new(engine, store);
            session.streams.tools = Tools::default();
            let app = App::new(session, video, &cc.egui_ctx);
            Ok(Box::new(Demo {
                app,
                media,
                grab: start_grab(&out),
                out,
                clock: Instant::now(),
                acts: script(),
                index: 0,
                act_started: None,
                cursor: Pos2::new(WINDOW[0] * 0.62, WINDOW[1] * 0.55),
                moving: None,
                scheduled: VecDeque::new(),
                caption: None,
                ripple: None,
                hovering: false,
                next_frame_at: 0.0,
                encoder: None,
                frames: 0,
                written: 0,
                first_frame_at: None,
                finished_at: None,
            }))
        }),
    )
    .map_err(|e| anyhow!("{e}"))
}
