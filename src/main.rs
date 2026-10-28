use std::path::Path;

use anyhow::{Result, anyhow};
use eframe::egui;
use lumen_rs::app::App;
use lumen_rs::engine::{Engine, Output};
use lumen_rs::queue::is_url;
use lumen_rs::session::Session;
use lumen_rs::store::Store;
use lumen_rs::video::Video;

/// Local files are made absolute so a file keeps its resume point wherever Lumen is launched from.
fn absolute(source: String) -> String {
    if is_url(&source) {
        return source;
    }
    std::fs::canonicalize(Path::new(&source)).map_or(source, |p| p.to_string_lossy().into_owned())
}

fn main() -> Result<()> {
    let mut sources = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("lumen-rs {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("Usage: lumen-rs [FILE|URL]...");
                return Ok(());
            }
            _ => sources.push(absolute(arg)),
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 680.0])
            .with_min_inner_size([480.0, 320.0])
            .with_title("Lumen")
            .with_app_id("lumen-rs"),
        ..Default::default()
    };
    eframe::run_native(
        "Lumen",
        options,
        Box::new(move |cc| {
            let store = Store::open(Store::default_dir());
            let engine = Engine::new(Output::Window, store.settings.volume)?;
            let video = Video::new(&engine, cc)?;
            let mut session = Session::new(engine, store);
            session.add(&sources, true);
            Ok(Box::new(App::new(session, video, &cc.egui_ctx)))
        }),
    )
    .map_err(|e| anyhow!("{e}"))
}
