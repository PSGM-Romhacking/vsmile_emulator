//! The system behind the `emu-api` contract: what the frontend (and the libretro core)
//! see. This is the only file that knows both sides; the machine's own types stay
//! behind it.

use emu_api::{BiosFile, Button, Core, Frame, PadButton, SystemInfo};

use crate::joy::JoyInput;
use crate::memory::VideoTiming;
use crate::ppu::{SCREEN_H, SCREEN_W};
use crate::vsmile::{
    bytes_from_rom_le, dummy_sysrom, rgb555_to_rgb888, rom_from_bytes_le, rom_from_file, VSmile,
};

/// SPG200 at 27 MHz; NTSC: 1716 CPU cycles per line, 262 lines (~60.05 Hz).
const FPS_NUM: u64 = 27_000_000;
const FPS_DEN: u64 = 1716 * 262;

/// The SPU writes stereo samples at 27 MHz / 96.
const SPU_RATE: f32 = 281_250.0;

/// Region code on port C (low 4 bits). 0xE is the value the original frontend used.
const REGION: u16 = 0xe;

pub static INFO: SystemInfo = SystemInfo {
    name: "VTech V.Smile",
    short_name: "V.Smile",
    extensions: &["bin"],
    thumbnails: "VTech - V.Smile",
    cover_aspect: 0.66,
    players: 2,
    // The V.Smile joystick: stick, four colour buttons, Enter, Exit (back), Help, ABC.
    buttons: &[
        Button { id: PadButton::Up, label: "Up" },
        Button { id: PadButton::Down, label: "Down" },
        Button { id: PadButton::Left, label: "Left" },
        Button { id: PadButton::Right, label: "Right" },
        Button { id: PadButton::Start, label: "Enter" },
        Button { id: PadButton::Y, label: "Red" },
        Button { id: PadButton::X, label: "Yellow" },
        Button { id: PadButton::B, label: "Blue" },
        Button { id: PadButton::A, label: "Green" },
        Button { id: PadButton::L, label: "Exit" },
        Button { id: PadButton::Select, label: "Help" },
        Button { id: PadButton::R, label: "ABC" },
    ],
    width: SCREEN_W,
    height: SCREEN_H,
    aspect: 4.0 / 3.0,
    // system_ram() is the internal RAM, word 0 = CPU address $0000 (little-endian bytes)
    ram_address: 0,
    fps_num: FPS_NUM,
    fps_den: FPS_DEN,
    game_info: crate::games::game_info,
    game_texts: &[],
    // Optional: games boot from the cartridge without it, a few call into it.
    // The newest one found is used.
    bios: &[
        BiosFile { name: "vsmile_v100.bin", crc32: 0x205c5296, size: 2 * 1024 * 1024, required: false },
        BiosFile { name: "vsmile_v102.bin", crc32: 0x0cd0bdf5, size: 2 * 1024 * 1024, required: false },
        BiosFile { name: "vsmile_v103.bin", crc32: 0x387fbc24, size: 2 * 1024 * 1024, required: false },
    ],
    multi_file_roms: false,
    create: |rate| Box::new(VsmileCore::new(rate)),
};

/// RetroPad bitmask → V.Smile joystick.
fn joy_input(bits: u16) -> JoyInput {
    let on = |b: PadButton| bits & b.bit() != 0;
    JoyInput {
        x: if on(PadButton::Left) { -5 } else if on(PadButton::Right) { 5 } else { 0 },
        y: if on(PadButton::Up) { 5 } else if on(PadButton::Down) { -5 } else { 0 },
        red: on(PadButton::Y),
        yellow: on(PadButton::X),
        blue: on(PadButton::B),
        green: on(PadButton::A),
        enter: on(PadButton::Start),
        back: on(PadButton::L),
        help: on(PadButton::Select),
        abc: on(PadButton::R),
    }
}

/// 2nd-order Butterworth low-pass (anti-aliasing before decimation), one per channel.
#[derive(Clone, Copy, Default)]
struct Biquad {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

#[derive(Clone, Copy)]
struct Coeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coeffs {
    fn low_pass(fc: f32, fs: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * fc / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}

impl Biquad {
    fn process(&mut self, x: f32, c: &Coeffs) -> f32 {
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// SPU rate → output rate: low-pass, then linear interpolation.
struct Resampler {
    coeffs: Coeffs,
    lp: [Biquad; 2],
    ratio: f32,
    acc: f32,
    prev: [f32; 2],
}

impl Resampler {
    fn new(rate: u32) -> Self {
        let rate = rate.max(1) as f32;
        Self {
            coeffs: Coeffs::low_pass((rate * 0.45).min(20_000.0), SPU_RATE),
            lp: [Biquad::default(); 2],
            ratio: rate / SPU_RATE,
            acc: 0.0,
            prev: [0.0; 2],
        }
    }

    /// `samples`: interleaved stereo u16, 0x8000 = silence.
    fn push(&mut self, samples: &[u16], out: &mut Vec<f32>) {
        for &[l, r] in samples.as_chunks::<2>().0 {
            let l = ((l ^ 0x8000) as i16) as f32 / 32768.0;
            let r = ((r ^ 0x8000) as i16) as f32 / 32768.0;
            let fl = self.lp[0].process(l, &self.coeffs);
            let fr = self.lp[1].process(r, &self.coeffs);
            self.acc += self.ratio;
            if self.acc >= 1.0 {
                self.acc -= 1.0;
                let frac = ((1.0 - self.acc) / self.ratio).clamp(0.0, 1.0);
                out.push(self.prev[0] + (fl - self.prev[0]) * frac);
                out.push(self.prev[1] + (fr - self.prev[1]) * frac);
            }
            self.prev = [fl, fr];
        }
    }
}

pub struct VsmileCore {
    resampler: Resampler,
    sample_rate: u32,
    /// The newest BIOS given by the frontend; a dummy one is used without it.
    bios: Option<Vec<u16>>,
    vsmile: Option<VSmile>,
    fb: Vec<u32>,
    /// The internal RAM as bytes (the machine has 16-bit words); written back to the
    /// machine before the next frame when it was lent out mutably.
    ram: Vec<u8>,
    ram_lent: bool,
    /// The Art Studio NVRAM as bytes, same scheme as `ram`.
    nvram: Option<Vec<u8>>,
    nvram_lent: bool,
}

impl VsmileCore {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            resampler: Resampler::new(sample_rate),
            sample_rate,
            bios: None,
            vsmile: None,
            fb: vec![0; SCREEN_W * SCREEN_H],
            ram: Vec::new(),
            ram_lent: false,
            nvram: None,
            nvram_lent: false,
        }
    }

    /// The machine, for command-line tools.
    pub fn machine(&mut self) -> Option<&mut VSmile> {
        self.vsmile.as_mut()
    }

    fn write_back(&mut self) {
        let Some(v) = &mut self.vsmile else { return };
        if self.ram_lent {
            for (w, &b) in v.spg.mem.ram.iter_mut().zip(self.ram.as_chunks::<2>().0) {
                *w = u16::from_le_bytes(b);
            }
            self.ram_lent = false;
        }
        if self.nvram_lent {
            if let Some(n) = &self.nvram {
                v.enable_nvram(rom_from_bytes_le(n));
            }
            self.nvram_lent = false;
        }
    }

    /// Copies the machine's picture, RAM and (if changed) NVRAM into the byte views.
    fn refresh(&mut self) {
        let Some(v) = &mut self.vsmile else { return };
        for (o, &c) in self.fb.iter_mut().zip(v.framebuffer()) {
            let (r, g, b) = rgb555_to_rgb888(c);
            *o = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        }
        self.ram = bytes_from_rom_le(&v.spg.mem.ram[..]);
        if v.nvram_dirty() || self.nvram.is_none() {
            self.nvram = v.nvram_words().map(bytes_from_rom_le);
            v.clear_nvram_dirty();
        }
    }
}

impl Core for VsmileCore {
    fn info(&self) -> &'static SystemInfo {
        &INFO
    }

    fn set_bios(&mut self, files: &[Option<Vec<u8>>]) {
        self.bios = files.iter().rev().flatten().next().map(|b| rom_from_bytes_le(b));
    }

    fn load(&mut self, rom: &[u8]) -> Result<(), String> {
        let cart = rom_from_file(rom)?;
        if cart.is_empty() {
            return Err("empty ROM".into());
        }
        let sys = self.bios.clone().unwrap_or_else(dummy_sysrom);
        let mut v = VSmile::new(sys, cart, REGION, true, VideoTiming::Ntsc);
        if crate::games::has_nvram(rom) {
            v.enable_nvram(Vec::new());
        }
        v.reset();
        self.vsmile = Some(v);
        self.nvram = None;
        self.resampler = Resampler::new(self.sample_rate);
        self.refresh();
        Ok(())
    }

    fn reset(&mut self) {
        self.write_back();
        if let Some(v) = &mut self.vsmile {
            v.reset();
        }
    }

    fn run_frame(&mut self, pads: &[u16]) {
        self.write_back();
        let Some(v) = &mut self.vsmile else { return };
        v.set_input(joy_input(pads.first().copied().unwrap_or(0)));
        let p2 = pads.get(1).copied();
        v.set_controller2_enabled(p2.is_some());
        if let Some(bits) = p2 {
            v.set_input2(joy_input(bits));
        }
        v.run_frame();
        self.refresh();
    }

    fn video(&self) -> Frame<'_> {
        Frame { pixels: &self.fb, width: SCREEN_W, height: SCREEN_H, pitch: SCREEN_W, aspect: INFO.aspect }
    }

    fn take_audio(&mut self, out: &mut Vec<f32>) {
        if let Some(v) = &mut self.vsmile {
            let samples = v.take_audio();
            self.resampler.push(samples, out);
        }
    }

    fn save_state(&self) -> Vec<u8> {
        self.vsmile.as_ref().map(VSmile::save_state).unwrap_or_default()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        self.write_back();
        self.vsmile.as_mut().ok_or("no game loaded")?.load_state(data)?;
        self.refresh();
        Ok(())
    }

    fn system_ram(&self) -> &[u8] {
        &self.ram
    }

    fn system_ram_mut(&mut self) -> &mut [u8] {
        self.ram_lent = true;
        &mut self.ram
    }

    fn battery_ram(&self) -> Option<&[u8]> {
        self.nvram.as_deref()
    }

    fn battery_ram_mut(&mut self) -> Option<&mut [u8]> {
        self.nvram_lent = self.nvram.is_some();
        self.nvram.as_deref_mut()
    }

    fn load_battery(&mut self, data: &[u8]) {
        let Some(v) = &mut self.vsmile else { return };
        if v.nvram_words().is_some() {
            v.enable_nvram(rom_from_bytes_le(data));
            self.nvram = v.nvram_words().map(bytes_from_rom_le);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retropad_to_joystick() {
        let j = joy_input(PadButton::Left.bit() | PadButton::Up.bit() | PadButton::Start.bit() | PadButton::Y.bit());
        assert_eq!((j.x, j.y), (-5, 5));
        assert!(j.enter && j.red && !j.green && !j.back);
    }

    /// Through the contract: a cartridge that only loops still gives frames, audio
    /// and a save state that loads back.
    #[test]
    fn contract_smoke() {
        let mut rom = vec![0xffu8; 0x20000];
        let w = |rom: &mut Vec<u8>, i: usize, v: u16| rom[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        w(&mut rom, 0xfff7, 0x8000); // reset vector
        w(&mut rom, 0x8000, 0xee41); // jmp self
        let mut c = VsmileCore::new(48_000);
        c.load(&rom).unwrap();
        let mut audio = Vec::new();
        for _ in 0..60 {
            c.run_frame(&[0]);
            c.take_audio(&mut audio);
        }
        let frames = audio.len() / 2;
        assert!((47_000..=49_000).contains(&frames), "{frames} audio frames in 60 video frames");
        assert_eq!(c.system_ram().len(), 0x2800 * 2);
        assert!(c.battery_ram().is_none());
        let s = c.save_state();
        assert!(!s.is_empty());
        c.run_frame(&[0, 0]);
        c.load_state(&s).unwrap();
        assert_eq!(c.save_state(), s);
    }

    #[test]
    fn resampler_rate() {
        let mut r = Resampler::new(48_000);
        let mut out = Vec::new();
        r.push(&vec![0x8000u16; 281_250 * 2], &mut out);
        let frames = out.len() / 2;
        assert!((47_990..=48_010).contains(&frames), "{frames}");
    }
}
