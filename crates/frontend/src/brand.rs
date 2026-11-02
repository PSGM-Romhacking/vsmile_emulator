//! Identity of the application using the frontend: name, folders, icon and "About".
//! The app calls `set` once, before loading the configuration.

use std::sync::OnceLock;

pub struct Brand {
    /// Displayed name (window title, "About", Pictures and Videos folders).
    pub name: &'static str,
    /// Identifier for the config, data and cache folders (`~/.config/<id>`).
    pub id: &'static str,
    pub version: &'static str,
    /// PNG icon (window and "About").
    pub icon_png: &'static [u8],
    /// "About" text (source text, goes through `tr`).
    pub about: &'static str,
    /// Repository; with `<` it is a placeholder and shows as text, not a link.
    pub repo_url: &'static str,
    /// Frames drawn around the game when there is room on the sides ("Video › Frame").
    pub bezels: &'static [Bezel],
    /// Where to get free, legal games for the system, shown on the welcome screen
    /// (label as source text for `tr`, URL).
    pub free_games: &'static [(&'static str, &'static str)],
    /// A second source of images in the libretro-thumbnails layout
    /// (`<base>/<system>/Named_Boxarts/<game>.png`), tried when libretro-thumbnails has none.
    pub thumbnails_url: &'static str,
}

/// A frame image (PNG with a transparent hole) and where the game goes in it.
pub struct Bezel {
    /// Saved in the preferences ("tv").
    pub key: &'static str,
    /// Menu label (source text, goes through `tr`).
    pub label: &'static str,
    pub png: &'static [u8],
    /// Game area in the image, as fractions of its size: x, y, width, height.
    pub screen: [f32; 4],
}

static BRAND: OnceLock<Brand> = OnceLock::new();

const GENERIC: Brand = Brand { name: "Emulator", id: "emu-frontend", version: env!("CARGO_PKG_VERSION"), icon_png: &[], about: "", repo_url: "", bezels: &[], free_games: &[], thumbnails_url: "" };

pub fn set(b: Brand) {
    let _ = BRAND.set(b);
}

pub fn get() -> &'static Brand {
    BRAND.get().unwrap_or(&GENERIC)
}
