//! Pixel and audio conversions between libretro and `emu-api`.

/// `retro_pixel_format`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// 0RGB1555 (libretro's default until the core says otherwise).
    Rgb1555,
    Xrgb8888,
    Rgb565,
}

impl PixelFormat {
    pub fn from_libretro(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::Rgb1555),
            1 => Some(Self::Xrgb8888),
            2 => Some(Self::Rgb565),
            _ => None,
        }
    }
}

fn expand5(v: u32) -> u32 {
    (v << 3) | (v >> 2)
}

fn expand6(v: u32) -> u32 {
    (v << 2) | (v >> 4)
}

/// Converts a frame (`pitch` in bytes) to XRGB8888 with `w * h` pixels.
pub fn to_xrgb8888(format: PixelFormat, bytes: &[u8], w: usize, h: usize, pitch: usize, out: &mut Vec<u32>) {
    out.clear();
    out.reserve(w * h);
    for y in 0..h {
        let row = &bytes[y * pitch..];
        match format {
            PixelFormat::Xrgb8888 => {
                out.extend(row[..w * 4].as_chunks::<4>().0.iter().map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]) & 0x00FF_FFFF));
            }
            PixelFormat::Rgb565 => out.extend(row[..w * 2].as_chunks::<2>().0.iter().map(|p| {
                let v = u16::from_le_bytes([p[0], p[1]]) as u32;
                expand5(v >> 11) << 16 | expand6((v >> 5) & 0x3F) << 8 | expand5(v & 0x1F)
            })),
            PixelFormat::Rgb1555 => out.extend(row[..w * 2].as_chunks::<2>().0.iter().map(|p| {
                let v = u16::from_le_bytes([p[0], p[1]]) as u32;
                expand5((v >> 10) & 0x1F) << 16 | expand5((v >> 5) & 0x1F) << 8 | expand5(v & 0x1F)
            })),
        }
    }
}

/// Linear resampler for interleaved stereo `f32`, from the core's rate to the frontend's.
#[derive(Default)]
pub struct Resampler {
    /// Input frames consumed per output frame; 0 = passthrough.
    ratio: f64,
    /// Position of the next output frame, in the coordinates of the next chunk.
    pos: f64,
    last: [f32; 2],
}

impl Resampler {
    pub fn new(from: f64, to: f64) -> Self {
        let ratio = if from > 0.0 && to > 0.0 && (from - to).abs() > 0.5 { from / to } else { 0.0 };
        Self { ratio, pos: 0.0, last: [0.0; 2] }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.ratio == 0.0 {
            out.extend_from_slice(input);
            return;
        }
        let frames = input.len() / 2;
        if frames == 0 {
            return;
        }
        let at = |i: usize| [input[i * 2], input[i * 2 + 1]];
        let mut p = self.pos;
        while p < frames as f64 - 1.0 {
            let i = p.floor();
            let f = (p - i) as f32;
            let (a, b) = if i < 0.0 { (self.last, at(0)) } else { (at(i as usize), at(i as usize + 1)) };
            out.push(a[0] + (b[0] - a[0]) * f);
            out.push(a[1] + (b[1] - a[1]) * f);
            p += self.ratio;
        }
        self.pos = p - frames as f64;
        self.last = at(frames - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_white_and_red() {
        let mut out = Vec::new();
        let px: Vec<u8> = [0xFFFFu16, 0xF800].iter().flat_map(|v| v.to_le_bytes()).collect();
        to_xrgb8888(PixelFormat::Rgb565, &px, 2, 1, 4, &mut out);
        assert_eq!(out, vec![0x00FF_FFFF, 0x00FF_0000]);
    }

    #[test]
    fn rgb1555_blue() {
        let mut out = Vec::new();
        to_xrgb8888(PixelFormat::Rgb1555, &0x001Fu16.to_le_bytes(), 1, 1, 2, &mut out);
        assert_eq!(out, vec![0x0000_00FF]);
    }

    #[test]
    fn xrgb8888_drops_alpha_and_honours_pitch() {
        let mut out = Vec::new();
        // 1 pixel wide, 2 rows, pitch 8 bytes (4 of padding per row)
        let mut b = vec![0u8; 16];
        b[..4].copy_from_slice(&0xFF11_2233u32.to_le_bytes());
        b[8..12].copy_from_slice(&0x0044_5566u32.to_le_bytes());
        to_xrgb8888(PixelFormat::Xrgb8888, &b, 1, 2, 8, &mut out);
        assert_eq!(out, vec![0x0011_2233, 0x0044_5566]);
    }

    #[test]
    fn passthrough_when_rates_match() {
        let mut r = Resampler::new(48000.0, 48000.0);
        let mut out = Vec::new();
        r.process(&[0.1, 0.2, 0.3, 0.4], &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn resampling_keeps_the_duration_across_chunks() {
        // 32 kHz -> 48 kHz: 1 s of input gives ~1 s of output, split into frames of 533/534
        let mut r = Resampler::new(32000.0, 48000.0);
        let mut out = Vec::new();
        for _ in 0..60 {
            r.process(&vec![0.5; 533 * 2], &mut out);
        }
        let frames = out.len() / 2;
        let expected = 60 * 533 * 3 / 2;
        assert!((frames as i64 - expected as i64).abs() <= 2, "{frames} vs {expected}");
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-5 || s == 0.0 || s.abs() < 0.5));
    }
}
