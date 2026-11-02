//! Toy Console: toy-core + emu-frontend.
//!
//! Usage: toy [GAME.zip|.bin | FOLDER] [--scale N]

// on Windows, a window app (no console window behind it)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;

use emu_frontend::{app, brand, config, i18n};

/// App identity; must come before loading the configuration (the folders use the `id`).
fn setup() {
    brand::set(brand::Brand {
        name: "Toy Console",
        id: "toy",
        version: env!("CARGO_PKG_VERSION"),
        icon_png: &[], // TODO: include_bytes!("../../../assets/icons/<icon>.png")
        about: "Toy Console emulator.",
        repo_url: "<repository>", // with "<" it shows as text, not a link
        bezels: &[],
        free_games: &[], // TODO: (label, URL) of legal free games for the system
        thumbnails_url: "",
    });
}

fn main() {
    setup();
    emu_frontend::crashlog::install();
    let cfg = config::Config::load();
    i18n::set_lang(&cfg.prefs.language);
    let mut rom: Option<PathBuf> = None;
    let mut scale: Option<u32> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--scale" => scale = args.next().and_then(|v| v.parse().ok()).map(|v: u32| v.clamp(1, 6)),
            _ if !a.starts_with('-') => rom = Some(PathBuf::from(a)),
            _ => eprintln!("unknown option: {a}"),
        }
    }

    let info = &toy_core::INFO;
    let explicit = scale.is_some();
    let scale = scale.unwrap_or(cfg.prefs.scale).clamp(1, 6);
    let size = if explicit { [(info.width as u32 * scale) as f32, (info.height as u32 * scale) as f32 + 28.0] } else { app::library_window_size(info) };
    let viewport = egui::ViewportBuilder::default()
        .with_title("Toy Console")
        .with_app_id("toy")
        .with_inner_size(size)
        .with_fullscreen(cfg.prefs.couch);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    if let Err(e) = eframe::run_native("Toy Console", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, info, rom, scale))))) {
        eprintln!("toy: {e}");
    }
}
