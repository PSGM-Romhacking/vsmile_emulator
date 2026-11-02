//! Toy Console: a tiny made-up system that exists to be a working example of the
//! `emu-api` contract. It has no real hardware to copy: a square you move with the
//! D-pad over an animated background, a tone while A is held, and all of its state in
//! 16 bytes of "system RAM" (which makes cheats, the debugger, save states and rewind
//! work with no extra code).
//!
//! Use it as the model for a real adapter, and as a test bed for the frontend:
//! `cargo run -p toy`.

use emu_api::{Button, Core, CoreOption, DebugGroup, DebugValue, DebugView, Frame, GameInfo, PadButton, SystemInfo};

const W: usize = 160;
const H: usize = 120;
const SQUARE: usize = 16;
const MAX_X: u8 = (W - SQUARE) as u8;
const MAX_Y: u8 = (H - SQUARE) as u8;
const TONE_HZ: f32 = 440.0;

// layout of the system RAM: everything the toy remembers
const X: usize = 0; // u8
const Y: usize = 1; // u8
const FRAME: std::ops::Range<usize> = 2..6; // u32 LE, frames since reset
const PHASE: std::ops::Range<usize> = 6..10; // u32 LE, tone phase in samples
const RAM_SIZE: usize = 16;

static OPTIONS: [CoreOption; 1] = [CoreOption {
    key: "background",
    label: "Background",
    values: &[("gradient", "Gradient"), ("solid", "Solid")],
}];

pub static INFO: SystemInfo = SystemInfo {
    name: "Toy Console",
    short_name: "Toy",
    // any file works: the content is ignored
    extensions: &["toy", "bin"],
    thumbnails: "Example - Toy",
    cover_aspect: 0.66,
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
    aspect: W as f32 / H as f32,
    ram_address: 0,
    fps_num: 60,
    fps_den: 1,
    game_info,
    game_texts: &[],
    bios: &[],
    multi_file_roms: false,
    create: |rate| Box::new(ToyCore::new(rate)),
};

fn game_info(_rom: &[u8]) -> Option<&'static GameInfo> {
    None
}

pub struct ToyCore {
    sample_rate: u32,
    fb: Vec<u32>,
    ram: Vec<u8>,
    solid: bool,
    loaded: bool,
    audio: Vec<f32>,
}

impl ToyCore {
    pub fn new(sample_rate: u32) -> Self {
        let mut core = Self { sample_rate, fb: vec![0; W * H], ram: vec![0; RAM_SIZE], solid: false, loaded: false, audio: Vec::new() };
        core.reset();
        core
    }

    fn frame_count(&self) -> u32 {
        u32::from_le_bytes(self.ram[FRAME].try_into().unwrap())
    }

    fn phase(&self) -> u32 {
        u32::from_le_bytes(self.ram[PHASE].try_into().unwrap())
    }

    fn draw(&mut self, pads: u16) {
        let t = self.frame_count();
        let (sx, sy) = (self.ram[X] as usize, self.ram[Y] as usize);
        let color = if pads & PadButton::A.bit() != 0 {
            0x00FF_FFFF
        } else if pads & PadButton::B.bit() != 0 {
            0x00FF_3030
        } else {
            0x0030_D0FF
        };
        for y in 0..H {
            for x in 0..W {
                self.fb[y * W + x] = if (sx..sx + SQUARE).contains(&x) && (sy..sy + SQUARE).contains(&y) {
                    color
                } else if self.solid {
                    0x0020_2040
                } else {
                    // a gradient that scrolls one pixel per frame
                    let r = (x as u32 + t) & 0xFF;
                    let g = (y as u32 * 2 + t / 2) & 0xFF;
                    (r / 2) << 16 | (g / 2) << 8 | 0x40
                };
            }
        }
    }
}

impl Core for ToyCore {
    fn info(&self) -> &'static SystemInfo {
        &INFO
    }

    fn load(&mut self, _rom: &[u8]) -> Result<(), String> {
        self.loaded = true;
        self.reset();
        Ok(())
    }

    fn reset(&mut self) {
        self.ram.fill(0);
        self.ram[X] = MAX_X / 2;
        self.ram[Y] = MAX_Y / 2;
        self.draw(0);
    }

    fn run_frame(&mut self, pads: &[u16]) {
        let pad = pads.first().copied().unwrap_or(0);
        let held = |b: PadButton| pad & b.bit() != 0;
        // the position is read back from RAM every frame, so a cheat that writes it works
        let (mut x, mut y) = (self.ram[X], self.ram[Y]);
        if held(PadButton::Left) {
            x = x.saturating_sub(2);
        }
        if held(PadButton::Right) {
            x = (x + 2).min(MAX_X);
        }
        if held(PadButton::Up) {
            y = y.saturating_sub(2);
        }
        if held(PadButton::Down) {
            y = (y + 2).min(MAX_Y);
        }
        if held(PadButton::Start) {
            x = MAX_X / 2;
            y = MAX_Y / 2;
        }
        self.ram[X] = x.min(MAX_X);
        self.ram[Y] = y.min(MAX_Y);
        let t = self.frame_count().wrapping_add(1);
        self.ram[FRAME].copy_from_slice(&t.to_le_bytes());

        // one frame of audio: a square wave while A is held, else silence (stereo f32)
        let samples = (self.sample_rate / 60) as usize;
        let half_period = (self.sample_rate as f32 / TONE_HZ / 2.0).max(1.0) as u32;
        let mut phase = self.phase();
        for _ in 0..samples {
            let s = if held(PadButton::A) && (phase / half_period).is_multiple_of(2) { 0.2 } else if held(PadButton::A) { -0.2 } else { 0.0 };
            self.audio.extend_from_slice(&[s, s]);
            phase = phase.wrapping_add(1);
        }
        self.ram[PHASE].copy_from_slice(&phase.to_le_bytes());
        self.draw(pad);
    }

    fn video(&self) -> Frame<'_> {
        Frame { pixels: &self.fb, width: W, height: H, pitch: W, aspect: INFO.aspect }
    }

    fn take_audio(&mut self, out: &mut Vec<f32>) {
        out.append(&mut self.audio);
    }

    fn save_state(&self) -> Vec<u8> {
        self.ram.clone()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() != RAM_SIZE {
            return Err(format!("invalid state size ({} bytes)", data.len()));
        }
        self.ram.copy_from_slice(data);
        self.draw(0);
        Ok(())
    }

    fn system_ram(&self) -> &[u8] {
        &self.ram
    }

    fn system_ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    fn debug_views(&self) -> Vec<DebugView> {
        vec![DebugView::Table {
            title: "Toy",
            groups: vec![DebugGroup {
                title: "State",
                rows: vec![
                    ("X", DebugValue::Text(self.ram[X].to_string())),
                    ("Y", DebugValue::Text(self.ram[Y].to_string())),
                    ("Frame", DebugValue::Text(self.frame_count().to_string())),
                    ("Solid background", DebugValue::Flag(self.solid)),
                ],
            }],
        }]
    }

    fn options(&self) -> &'static [CoreOption] {
        &OPTIONS
    }

    fn set_option(&mut self, key: &str, value: &str) {
        if key == "background" {
            self.solid = value == "solid";
            self.draw(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(core: &mut ToyCore, pad: u16, frames: usize) {
        for _ in 0..frames {
            core.run_frame(&[pad]);
        }
    }

    #[test]
    fn deterministic_for_the_same_input() {
        let (mut a, mut b) = (ToyCore::new(48000), ToyCore::new(48000));
        for n in 0..120u16 {
            let pad = if n % 7 < 3 { PadButton::Right.bit() } else { PadButton::Down.bit() };
            a.run_frame(&[pad]);
            b.run_frame(&[pad]);
        }
        assert_eq!(a.fb, b.fb);
        assert_eq!(a.ram, b.ram);
    }

    #[test]
    fn dpad_moves_the_square_and_stops_at_the_edges() {
        let mut c = ToyCore::new(48000);
        let x0 = c.ram[X];
        run(&mut c, PadButton::Right.bit(), 5);
        assert_eq!(c.ram[X], x0 + 10);
        run(&mut c, PadButton::Right.bit(), 500);
        assert_eq!(c.ram[X], MAX_X);
        run(&mut c, PadButton::Up.bit(), 500);
        assert_eq!(c.ram[Y], 0);
        run(&mut c, PadButton::Start.bit(), 1);
        assert_eq!((c.ram[X], c.ram[Y]), (MAX_X / 2, MAX_Y / 2));
    }

    #[test]
    fn state_round_trip_restores_the_frame() {
        let mut c = ToyCore::new(48000);
        run(&mut c, PadButton::Left.bit(), 10);
        let state = c.save_state();
        let frame = c.fb.clone();
        run(&mut c, PadButton::Down.bit(), 30);
        assert_ne!(c.fb, frame);
        c.load_state(&state).unwrap();
        assert_eq!(c.save_state(), state);
        // load_state redraws, with no button held
        let mut again = ToyCore::new(48000);
        again.load_state(&state).unwrap();
        assert_eq!(c.fb, again.fb);
        assert!(c.load_state(&[0; 3]).is_err());
    }

    #[test]
    fn writing_the_ram_moves_the_square_like_a_cheat() {
        let mut c = ToyCore::new(48000);
        c.system_ram_mut()[X] = 7;
        c.system_ram_mut()[Y] = 9;
        run(&mut c, 0, 1);
        assert_eq!((c.ram[X], c.ram[Y]), (7, 9));
        assert_eq!(c.fb[9 * W + 7], 0x0030_D0FF);
    }

    #[test]
    fn audio_is_one_frame_of_stereo_and_only_sounds_with_a() {
        let mut c = ToyCore::new(48000);
        let mut out = Vec::new();
        run(&mut c, 0, 1);
        c.take_audio(&mut out);
        assert_eq!(out.len(), 800 * 2);
        assert!(out.iter().all(|&s| s == 0.0));
        out.clear();
        run(&mut c, PadButton::A.bit(), 1);
        c.take_audio(&mut out);
        assert_eq!(out.len(), 800 * 2);
        assert!(out.iter().any(|&s| s > 0.0) && out.iter().any(|&s| s < 0.0));
    }

    #[test]
    fn option_changes_the_background() {
        let mut c = ToyCore::new(48000);
        c.set_option("background", "solid");
        assert_eq!(c.fb[0], 0x0020_2040);
        c.set_option("background", "gradient");
        assert_ne!(c.fb[0], 0x0020_2040);
    }
}
