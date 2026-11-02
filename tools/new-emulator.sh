#!/usr/bin/env bash
# Generates the skeleton of a new emulator on top of emu-api / emu-frontend:
#   crates/<slug>-core/   adapter (`impl emu_api::Core`) with TODOs where the real core plugs in
#   crates/<slug>/        executable (brand + eframe + App), runs already with a black screen
#
# Usage: tools/new-emulator.sh [--libretro] [--ffi-c] <slug> "<Display Name>" ["<libretro-thumbnails folder>"]
#   e.g.: tools/new-emulator.sh --libretro nes "Mil Nintendos" "Nintendo - Nintendo Entertainment System"
# <slug>: lowercase letters, digits and "-" (becomes the crate and folder name).
# Options:
#   --libretro   also generates crates/<slug>-libretro (the core as a libretro .so, from
#                tools/templates/libretro.rs.in; nothing to edit, it follows the Core trait)
#   --ffi-c      the real core is C/C++: adds build.rs (cc crate) and src/ffi.rs to <slug>-core;
#                put the sources in crates/<slug>-core/c_src/ and declare the functions in ffi.rs
# Next step: fill in the TODO(core) marks in crates/<slug>-core/src/core.rs.
set -euo pipefail

libretro=0; ffi_c=0; pos=()
for a in "$@"; do
  case "$a" in
    --libretro) libretro=1 ;;
    --ffi-c) ffi_c=1 ;;
    --*) echo "unknown option: $a" >&2; exit 1 ;;
    *) pos+=("$a") ;;
  esac
done
slug="${pos[0]:-}"; name="${pos[1]:-}"; thumbs="${pos[2]:-}"
[[ "$slug" =~ ^[a-z][a-z0-9-]*$ && -n "$name" ]] || { sed -n '2,13p' "$0" | sed 's/^# \?//'; exit 1; }
root="$(cd "$(dirname "$0")/.." && pwd)"
core="$root/crates/$slug-core"; app="$root/crates/$slug"; lr="$root/crates/$slug-libretro"
[[ ! -e "$core" && ! -e "$app" && ! -e "$lr" ]] || { echo "crates/$slug, $slug-core or $slug-libretro already exists" >&2; exit 1; }
id="${slug//-/_}"           # identifier for config folder and Rust paths
pascal="$(sed -E 's/(^|-)([a-z0-9])/\U\2/g' <<<"$slug")"
mkdir -p "$core/src" "$app/src"

cat > "$core/Cargo.toml" <<EOF
[package]
name = "$slug-core"
description = "$name: core adapter behind the emu-api contract"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
emu-api.workspace = true
# TODO(core): the real core goes here (crate, path, or an FFI wrapper)
EOF

if (( ffi_c )); then
  mkdir -p "$core/c_src"
  cat >> "$core/Cargo.toml" <<EOF

[build-dependencies]
cc = "1"
EOF
  cat > "$core/build.rs" <<'EOF'
// Compiles the C/C++ sources of the real core (c_src/*.c, *.cc, *.cpp).
// TODO(core): add include dirs, defines and flags the core needs.
use std::path::Path;

fn main() {
    let dir = Path::new("c_src");
    println!("cargo:rerun-if-changed=c_src");
    let files: Vec<_> = std::fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("c" | "cc" | "cpp"))).collect())
        .unwrap_or_default();
    if files.is_empty() {
        return;
    }
    cc::Build::new().files(files).include(dir).warnings(false).compile("realcore");
}
EOF
  cat > "$core/src/ffi.rs" <<'EOF'
//! Declarations of the C functions of the real core (compiled by build.rs from c_src/).
//! Keep every `unsafe` call inside a safe wrapper in core.rs, not in the public interface.
//! TODO(core): replace these examples with the real functions (or generate them with bindgen).

#![allow(dead_code)]

unsafe extern "C" {
    // pub fn core_init();
    // pub fn core_load(rom: *const u8, len: usize) -> bool;
    // pub fn core_run_frame(input: u32);
    // pub fn core_framebuffer() -> *const u32;
}
EOF
fi

cat > "$core/src/lib.rs" <<EOF
//! $name core behind the \`emu-api\` contract. It depends on no frontend.

pub mod core;
$( (( ffi_c )) && echo 'mod ffi;' || true )

pub use core::{INFO, ${pascal}Core};
EOF

cat > "$core/src/core.rs" <<EOF
//! The system behind the \`emu-api\` contract: what the frontend (and a future libretro
//! core) see. This is the only file that knows both sides; keep the real core's
//! types out of the public interface.

use emu_api::{Button, Core, Frame, GameInfo, PadButton, SystemInfo};

// TODO(core): visible resolution of the real hardware
const W: usize = 256;
const H: usize = 224;

pub static INFO: SystemInfo = SystemInfo {
    name: "$name",
    short_name: "$name",
    // TODO(core): ROM extensions without the dot (.zip/.7z are handled by the frontend)
    extensions: &["bin"],
    thumbnails: "$thumbs",
    cover_aspect: 0.66,
    // TODO(core): number of players and ONLY the buttons the real controller has
    players: 1,
    buttons: &[
        Button { id: PadButton::Up, label: "Up" },
        Button { id: PadButton::Down, label: "Down" },
        Button { id: PadButton::Left, label: "Left" },
        Button { id: PadButton::Right, label: "Right" },
        Button { id: PadButton::B, label: "Button 1" },
        Button { id: PadButton::A, label: "Button 2" },
        Button { id: PadButton::Start, label: "Start" },
    ],
    width: W,
    height: H,
    // TODO(core): width / height as shown on a screen (not the pixel ratio)
    aspect: W as f32 / H as f32,
    // TODO(core): CPU address of system_ram()[0] (debugger display only)
    ram_address: 0,
    // TODO(core): exact frame rate as a fraction (e.g. clock / cycles per frame)
    fps_num: 60,
    fps_den: 1,
    game_info,
    game_texts: &[],
    bios: &[],
    multi_file_roms: false,
    create: |rate| Box::new(${pascal}Core::new(rate)),
};

/// Game database by ROM content (\`emu_api::crc32\`); none for now.
fn game_info(_rom: &[u8]) -> Option<&'static GameInfo> {
    None
}

pub struct ${pascal}Core {
    #[allow(dead_code)]
    sample_rate: u32,
    fb: Vec<u32>,
    ram: Vec<u8>,
    // TODO(core): the real core's instance
}

impl ${pascal}Core {
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate, fb: vec![0; W * H], ram: vec![0; 0x2000] }
    }
}

impl Core for ${pascal}Core {
    fn info(&self) -> &'static SystemInfo {
        &INFO
    }

    fn load(&mut self, _rom: &[u8]) -> Result<(), String> {
        // TODO(core): load the ROM into the real core; Err("reason") if invalid
        Ok(())
    }

    fn reset(&mut self) {
        // TODO(core)
    }

    /// One frame. \`pads[n]\` = RetroPad bitmask of player n (\`PadButton::bit\`).
    fn run_frame(&mut self, _pads: &[u16]) {
        // TODO(core): RetroPad -> native input, run one frame, convert the picture
        // to XRGB8888 (0x00RRGGBB) into self.fb
    }

    fn video(&self) -> Frame<'_> {
        Frame { pixels: &self.fb, width: W, height: H, pitch: W, aspect: INFO.aspect }
    }

    /// Appends the last frame's audio: interleaved stereo f32, at the rate given to \`new\`.
    fn take_audio(&mut self, _out: &mut Vec<f32>) {
        // TODO(core): convert/resample to stereo f32 and push into \`out\`
    }

    fn save_state(&self) -> Vec<u8> {
        // TODO(core): serialize the whole state (the frontend adds the header and ROM hash)
        Vec::new()
    }

    /// Restores the state and also refreshes the video frame.
    fn load_state(&mut self, _data: &[u8]) -> Result<(), String> {
        // TODO(core)
        Err("save states not implemented".into())
    }

    fn system_ram(&self) -> &[u8] {
        &self.ram
    }

    fn system_ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }
}
EOF

cat > "$app/Cargo.toml" <<EOF
[package]
name = "$id"
description = "$name emulator ($slug-core core + emu-frontend frontend)"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "$id"
path = "src/main.rs"

[dependencies]
emu-api.workspace = true
$slug-core = { path = "../$slug-core" }
emu-frontend.workspace = true
eframe.workspace = true
egui.workspace = true
EOF

cat > "$app/src/main.rs" <<EOF
//! $name: ${slug}-core + emu-frontend.
//!
//! Usage: $id [GAME.zip|.bin | FOLDER] [--scale N]

// on Windows, a window app (no console window behind it)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;

use emu_frontend::{app, brand, config, i18n};

/// App identity; must come before loading the configuration (the folders use the \`id\`).
fn setup() {
    brand::set(brand::Brand {
        name: "$name",
        id: "$id",
        version: env!("CARGO_PKG_VERSION"),
        icon_png: &[], // TODO: include_bytes!("../../../assets/icons/<icon>.png")
        about: "$name emulator.",
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

    let info = &${slug//-/_}_core::INFO;
    let explicit = scale.is_some();
    let scale = scale.unwrap_or(cfg.prefs.scale).clamp(1, 6);
    let size = if explicit { [(info.width as u32 * scale) as f32, (info.height as u32 * scale) as f32 + 28.0] } else { app::library_window_size(info) };
    let viewport = egui::ViewportBuilder::default()
        .with_title("$name")
        .with_app_id("$id")
        .with_inner_size(size)
        .with_fullscreen(cfg.prefs.couch);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    if let Err(e) = eframe::run_native("$name", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, info, rom, scale))))) {
        eprintln!("$id: {e}");
    }
}
EOF

if (( libretro )); then
  mkdir -p "$lr/src"
  exts="bin" # placeholder for the .info file only; the .so reads INFO.extensions
  cat > "$lr/Cargo.toml" <<EOF
[package]
name = "$slug-libretro"
description = "$name as a libretro core"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[lib]
name = "${id}_libretro"
crate-type = ["cdylib"]

[dependencies]
emu-api.workspace = true
$slug-core = { path = "../$slug-core" }
EOF
  sed -e "s/@CORE@/${id}_core/g" -e "s/@NAME@/$name/g" -e "s/@ID@/$id/g" -e "s/@IDUP@/${id^^}/g" -e "s/@EXTS@/$exts/g" \
    "$root/tools/templates/libretro.rs.in" > "$lr/src/lib.rs"
  cat > "$lr/${id}_libretro.info" <<EOF
display_name = "$name ($name)"
authors = "TODO"
supported_extensions = "$exts"
corename = "$name"
categories = "Emulator"
license = "GPL-3.0-or-later"
display_version = "0.1.0"
manufacturer = "TODO"
systemname = "$name"
systemid = "$id"
database = "$name"
supports_no_game = "false"
savestate = "true"
savestate_features = "basic"
cheats = "false"
hw_render = "false"
EOF
fi

cat <<EOF
Created:
  crates/$slug-core/   (fill in the TODO(core) marks in src/core.rs)
  crates/$slug/        (fill in the icon and links in src/main.rs)
$( (( libretro )) && echo "  crates/$slug-libretro/   (nothing to edit; only the .info file has placeholders)" || true )
$( (( ffi_c )) && echo "  C sources go in crates/$slug-core/c_src/; declare them in src/ffi.rs" || true )
Next: cargo build -p $id && cargo run -p $id -- GAME
EOF
