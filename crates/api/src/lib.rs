//! Contract between an emulator core and the frontend (`emu-frontend`).
//!
//! Follows the libretro conventions, so the same core can become a
//! RetroArch core without format adaptation:
//! - input: **RetroPad** bitmask per player (`RETRO_DEVICE_ID_JOYPAD_*`);
//! - video: `XRGB8888` (`RETRO_PIXEL_FORMAT_XRGB8888`);
//! - save state: opaque bytes (`retro_serialize`);
//! - memory: system RAM (`RETRO_MEMORY_SYSTEM_RAM`).
//!
//! Audio is interleaved stereo `f32` (the libretro core converts it to `i16`).

/// RetroPad buttons, in libretro bit order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PadButton {
    B,
    Y,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
    A,
    X,
    L,
    R,
    L2,
    R2,
    L3,
    R3,
}

impl PadButton {
    pub const ALL: [PadButton; 16] = [
        PadButton::B,
        PadButton::Y,
        PadButton::Select,
        PadButton::Start,
        PadButton::Up,
        PadButton::Down,
        PadButton::Left,
        PadButton::Right,
        PadButton::A,
        PadButton::X,
        PadButton::L,
        PadButton::R,
        PadButton::L2,
        PadButton::R2,
        PadButton::L3,
        PadButton::R3,
    ];

    /// Bit in the player bitmask (= `RETRO_DEVICE_ID_JOYPAD_*`).
    pub fn bit(self) -> u16 {
        1 << self as u16
    }

    /// Stable name (configuration).
    pub fn key(self) -> &'static str {
        match self {
            PadButton::B => "b",
            PadButton::Y => "y",
            PadButton::Select => "select",
            PadButton::Start => "start",
            PadButton::Up => "up",
            PadButton::Down => "down",
            PadButton::Left => "left",
            PadButton::Right => "right",
            PadButton::A => "a",
            PadButton::X => "x",
            PadButton::L => "l",
            PadButton::R => "r",
            PadButton::L2 => "l2",
            PadButton::R2 => "r2",
            PadButton::L3 => "l3",
            PadButton::R3 => "r3",
        }
    }
}

/// Button the system uses, with the name shown in the frontend (source text in
/// English; the translation comes from the app's language catalogs).
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub id: PadButton,
    pub label: &'static str,
}

/// Video frame in XRGB8888 (`0x00RRGGBB`), `pitch` in pixels.
pub struct Frame<'a> {
    pub pixels: &'a [u32],
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    /// Display aspect ratio of this frame (width / height on screen); it may change
    /// with the core's options.
    pub aspect: f32,
}

/// Description of the emulated system (the "retro_get_system_info" + "av_info").
pub struct SystemInfo {
    /// Full name ("Example Console").
    pub name: &'static str,
    /// Short name ("Example").
    pub short_name: &'static str,
    /// Accepted ROM extensions, without the dot (.zip/.7z archives are handled by the frontend).
    pub extensions: &'static [&'static str],
    /// System folder in libretro-thumbnails ("Example - Console").
    pub thumbnails: &'static str,
    /// Shape of the box art in the library (width / height): about 0.66 for portrait boxes,
    /// 1.3 for landscape cases. The frontend sizes the covers from it.
    pub cover_aspect: f32,
    pub players: usize,
    /// Buttons used, in display order.
    pub buttons: &'static [Button],
    /// Visible area of the frame.
    pub width: usize,
    pub height: usize,
    /// Display aspect ratio (width / height).
    pub aspect: f32,
    /// CPU address of `Core::system_ram()[0]` (only for display in the debugger).
    pub ram_address: u32,
    /// Frames per second as an exact fraction (num / den).
    pub fps_num: u64,
    pub fps_den: u64,
    /// The game database: a game's data by ROM content.
    pub game_info: fn(&[u8]) -> Option<&'static GameInfo>,
    /// The games' texts, one JSON per language: (code, JSON), keyed by `GameInfo::id`
    /// (format in `docs/sheets.md`).
    pub game_texts: &'static [(&'static str, &'static str)],
    /// System files (BIOS, boot ROMs) the core can use; empty if none. The frontend
    /// looks for them by CRC32 and hands them to `Core::set_bios` before each `load`.
    pub bios: &'static [BiosFile],
    /// A .zip/.7z holding several ROM files is one game spread over several chips: the
    /// frontend joins the files, largest first. When false it takes a single file.
    pub multi_file_roms: bool,
    /// Creates the core for the frontend's audio rate.
    pub create: fn(sample_rate: u32) -> Box<dyn Core>,
}

/// A system file (BIOS, boot ROM). The frontend finds it by CRC32 in its BIOS folder and
/// in the game's folder, loose or inside a .zip, whatever the file is called.
pub struct BiosFile {
    /// Usual file name (shown to the user when it is missing).
    pub name: &'static str,
    pub crc32: u32,
    /// Size in bytes (shown to the user; 0 = not known).
    pub size: u32,
    /// Without it no game starts: the frontend shows what is missing instead.
    pub required: bool,
}

/// Data of a known game, the same in every language. The texts (description, notes)
/// are in `SystemInfo::game_texts`.
pub struct GameInfo {
    /// Stable identifier ("pooyan"): the game's key in the database and in the texts.
    pub id: String,
    pub title: String,
    /// Original title (e.g. in Japanese).
    pub alt_title: Option<String>,
    pub year: Option<String>,
    pub publisher: Option<String>,
    pub developer: Option<String>,
    /// Cartridge code ("GPA-101").
    pub serial: Option<String>,
    /// Most players at once, when known.
    pub players: Option<u8>,
    /// The name of this ROM's images, as in libretro-thumbnails and RetroArch: its No-Intro
    /// name ("Pooyan (Japan)").
    pub thumbnail: Option<String>,
}

/// CRC-32 (IEEE 802.3, the same as zip files and DATs).
pub fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    };
    !data.iter().fold(!0u32, |c, &b| TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8))
}

impl SystemInfo {
    pub fn fps(&self) -> f64 {
        self.fps_num as f64 / self.fps_den as f64
    }

    pub fn frame_seconds(&self) -> f64 {
        self.fps_den as f64 / self.fps_num as f64
    }
}

/// Something the debugger shows besides the RAM. Titles and row names are source
/// text (they go through `tr`); text values are shown as they are, except "muted",
/// which is translated too.
pub enum DebugView {
    /// An image (e.g. the tile set), XRGB8888, drawn enlarged. With `cell` > 0 it is a
    /// grid of `cell` × `cell` items (tiles): the debugger draws the grid and shows the
    /// number of the item under the mouse.
    Image { title: &'static str, width: usize, height: usize, pixels: Vec<u32>, cell: usize },
    /// Groups of name → value rows (registers, ports, sound channels…), one card each.
    Table { title: &'static str, groups: Vec<DebugGroup> },
}

/// A titled group of rows of a `DebugView::Table`.
pub struct DebugGroup {
    pub title: &'static str,
    pub rows: Vec<(&'static str, DebugValue)>,
}

/// A value of a debugger row.
pub enum DebugValue {
    /// Shown in monospace, as it is.
    Text(String),
    /// An on/off light (a flag, a switch).
    Flag(bool),
    /// A color swatch (0xRRGGBB) and a text beside it.
    Color(u32, String),
}

/// A core setting, like a libretro "core option": the frontend shows the values and
/// remembers the choice. The first value is the default. Labels go through `tr`.
pub struct CoreOption {
    pub key: &'static str,
    pub label: &'static str,
    /// (value, label) pairs.
    pub values: &'static [(&'static str, &'static str)],
}

/// The core of a system.
pub trait Core {
    fn info(&self) -> &'static SystemInfo;
    /// Frame rate now (numerator, denominator); a core whose timing depends on a
    /// setting overrides it. Defaults to the one in `info()`.
    fn fps(&self) -> (u64, u64) {
        (self.info().fps_num, self.info().fps_den)
    }
    fn load(&mut self, rom: &[u8]) -> Result<(), String>;
    fn reset(&mut self);
    /// Emulates one frame; `pads[n]` = RetroPad bitmask of player n.
    fn run_frame(&mut self, pads: &[u16]);
    fn video(&self) -> Frame<'_>;
    /// Appends to `out` the audio of the last frame (interleaved stereo f32).
    fn take_audio(&mut self, out: &mut Vec<f32>);
    fn save_state(&self) -> Vec<u8>;
    /// Restores and also updates the video frame.
    fn load_state(&mut self, data: &[u8]) -> Result<(), String>;
    /// System RAM, in the address space used by achievements.
    fn system_ram(&self) -> &[u8];
    /// The same RAM, writable (cheats, debugger, libretro frontends).
    fn system_ram_mut(&mut self) -> &mut [u8];
    /// The system's internals for the debugger (tiles, registers…); none by default.
    fn debug_views(&self) -> Vec<DebugView> {
        Vec::new()
    }
    /// The system files found, in `SystemInfo::bios` order (`None` = not found). Called
    /// before every `load`; cores without BIOS ignore it.
    fn set_bios(&mut self, _files: &[Option<Vec<u8>>]) {}
    /// Battery-backed save memory of the loaded game (cartridge SRAM), if any. The
    /// frontend keeps it in a .sav file and writes it when it changes.
    fn battery_ram(&self) -> Option<&[u8]> {
        None
    }
    /// The same memory, writable (libretro frontends load the save straight into it).
    fn battery_ram_mut(&mut self) -> Option<&mut [u8]> {
        None
    }
    /// Puts back the contents of a .sav file; called right after `load`.
    fn load_battery(&mut self, _data: &[u8]) {}
    /// Sound channels the debugger can mute (source text, goes through `tr`).
    fn sound_channels(&self) -> &'static [&'static str] {
        &[]
    }
    /// Mutes a sound channel (debugging only: not part of the state).
    fn set_channel_muted(&mut self, _channel: usize, _muted: bool) {}
    /// The core's settings; none by default.
    fn options(&self) -> &'static [CoreOption] {
        &[]
    }
    /// Applies a setting (unknown keys or values are ignored). Takes effect at once.
    fn set_option(&mut self, _key: &str, _value: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_standard() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926); // CRC-32 check value
    }

    #[test]
    fn retropad_bits_match_libretro() {
        // RETRO_DEVICE_ID_JOYPAD_B = 0, START = 3, UP = 4, A = 8, R3 = 15
        assert_eq!(PadButton::B.bit(), 1 << 0);
        assert_eq!(PadButton::Start.bit(), 1 << 3);
        assert_eq!(PadButton::Up.bit(), 1 << 4);
        assert_eq!(PadButton::A.bit(), 1 << 8);
        assert_eq!(PadButton::R3.bit(), 1 << 15);
    }
}
