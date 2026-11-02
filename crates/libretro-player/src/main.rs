//! Plays any libretro core with the emu-frontend interface.
//!
//! Usage: libretro_player --core CORE.so [GAME.zip|.bin | FOLDER] [--scale N] [--thumbnails "Folder - Name"]

// on Windows, a window app (no console window behind it)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;

use emu_frontend::{app, brand, config, i18n};
use libretro_host::HostConfig;

fn main() {
    brand::set(brand::Brand {
        name: "Libretro Player",
        id: "libretro_player",
        version: env!("CARGO_PKG_VERSION"),
        icon_png: &[],
        about: "Plays libretro cores with the emu-frontend interface.",
        repo_url: "<repository>", // with "<" it shows as text, not a link
        bezels: &[],
        free_games: &[],
        thumbnails_url: "",
    });
    let cfg = config::Config::load();
    i18n::set_lang(&cfg.prefs.language);

    let mut core: Option<PathBuf> = None;
    let mut rom: Option<PathBuf> = None;
    let mut scale: Option<u32> = None;
    let mut host = HostConfig::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--core" => core = args.next().map(PathBuf::from),
            "--scale" => scale = args.next().and_then(|v| v.parse().ok()).map(|v: u32| v.clamp(1, 6)),
            "--thumbnails" => host.thumbnails = args.next().unwrap_or_default(),
            _ if !a.starts_with('-') => rom = Some(PathBuf::from(a)),
            _ => eprintln!("unknown option: {a}"),
        }
    }
    let Some(core) = core else {
        eprintln!("usage: libretro_player --core CORE.so [GAME | FOLDER] [--scale N] [--thumbnails \"Folder - Name\"]");
        std::process::exit(1);
    };
    let info = match libretro_host::open(&core, host) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("libretro_player: {e}");
            std::process::exit(1);
        }
    };

    let explicit = scale.is_some();
    let scale = scale.unwrap_or(cfg.prefs.scale).clamp(1, 6);
    let size = if explicit { [(info.width as u32 * scale) as f32, (info.height as u32 * scale) as f32 + 28.0] } else { app::library_window_size(info) };
    let viewport = egui::ViewportBuilder::default().with_title("Libretro Player").with_app_id("libretro_player").with_inner_size(size).with_fullscreen(cfg.prefs.couch);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    if let Err(e) = eframe::run_native("Libretro Player", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, info, rom, scale))))) {
        eprintln!("libretro_player: {e}");
    }
}
