//! Gameplay recording on a dedicated thread (the game doesn't wait for encoding):
//! - GIF: palette with the frame's real colors (exact up to 256 colors), 2x, ~half
//!   the FPS (delays of 3/3/4 hundredths at 60 Hz — browsers treat < 2 cs as 10 cs);
//! - MP4: `ffmpeg` (H.264 + AAC), 4x with sharp pixels, the system's exact FPS and sound. The video
//!   goes to a temporary MPEG-TS file (a streaming format: stays
//!   readable even when cut off — with MKV there was a header failure we couldn't reproduce)
//!   and the sound (f32 mono) to another; on stop, `ffmpeg` merges the two. On
//!   error, the temporary files stay next to the output file, for diagnosis.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::platform::command;
#[cfg(test)]
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

/// The system's video format: resolution and FPS as an exact fraction.
#[derive(Clone, Copy)]
pub struct VideoFormat {
    pub width: usize,
    pub height: usize,
    pub fps_num: u64,
    pub fps_den: u64,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Gif,
    Mp4,
}

enum Msg {
    Frame(Vec<u32>),
    Audio(Vec<f32>),
}

pub struct Recorder {
    pub kind: Kind,
    fmt: VideoFormat,
    tx: Option<Sender<Msg>>,
    handle: Option<JoinHandle<Result<PathBuf, String>>>,
    pub frames: u64,
}

/// Is `ffmpeg` available on PATH?
pub fn has_ffmpeg() -> bool {
    command("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

impl Recorder {
    pub fn start(kind: Kind, path: PathBuf, sample_rate: u32, fmt: VideoFormat) -> Result<Self, String> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let (tx, rx) = channel();
        let handle = match kind {
            Kind::Gif => std::thread::Builder::new().name("gif".into()).spawn(move || gif_worker(rx, path, fmt)),
            Kind::Mp4 => std::thread::Builder::new().name("mp4".into()).spawn(move || mp4_worker(rx, path, sample_rate, fmt)),
        }
        .map_err(|e| e.to_string())?;
        Ok(Self { kind, fmt, tx: Some(tx), handle: Some(handle), frames: 0 })
    }

    pub fn frame(&mut self, fb: &[u32]) {
        self.frames += 1;
        if let Some(tx) = &self.tx {
            let _ = tx.send(Msg::Frame(fb.to_vec()));
        }
    }

    pub fn audio(&mut self, samples: &[f32]) {
        if self.kind == Kind::Mp4
            && let Some(tx) = &self.tx {
                let _ = tx.send(Msg::Audio(samples.to_vec()));
            }
    }

    /// Recorded duration (game time).
    pub fn seconds(&self) -> f64 {
        self.frames as f64 * self.fmt.fps_den as f64 / self.fmt.fps_num as f64
    }

    /// Stops receiving; finalization continues on the thread (see [`Self::try_finish`]).
    pub fn stop(&mut self) {
        self.tx = None;
    }

    /// Result, if the thread has already finished.
    pub fn try_finish(&mut self) -> Option<Result<PathBuf, String>> {
        if self.handle.as_ref().is_some_and(|h| h.is_finished()) {
            return Some(self.handle.take().unwrap().join().unwrap_or_else(|_| Err("recording thread failed".into())));
        }
        None
    }
}

impl Drop for Recorder {
    /// Closing the program mid-recording: finishes the file before exiting.
    fn drop(&mut self) {
        self.tx = None;
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Frame palette: the real colors (up to 256, exact) and each pixel's index.
/// More than 256 colors: `None` (the GIF quantizes).
fn palette(fb: &[u32]) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut colors: Vec<u32> = Vec::new();
    let mut idx = Vec::with_capacity(fb.len());
    for &p in fb {
        let c = p & 0xFF_FFFF;
        let i = match colors.iter().position(|&x| x == c) {
            Some(i) => i,
            None if colors.len() < 256 => {
                colors.push(c);
                colors.len() - 1
            }
            None => return None,
        };
        idx.push(i as u8);
    }
    let pal = colors.iter().flat_map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8]).collect();
    Some((pal, idx))
}

fn gif_worker(rx: Receiver<Msg>, path: PathBuf, fmt: VideoFormat) -> Result<PathBuf, String> {
    const K: usize = 2;
    let (sw, sh) = (fmt.width, fmt.height);
    let (w, h) = (sw * K, sh * K);
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut enc = gif::Encoder::new(std::io::BufWriter::new(file), w as u16, h as u16, &[]).map_err(|e| e.to_string())?;
    enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
    let (mut n, mut written) = (0u64, 0u64);
    for msg in rx {
        let Msg::Frame(fb) = msg else { continue };
        n += 1;
        if n % 2 == 0 {
            continue; // half the frames
        }
        // upscale 2x first (nearest neighbor: sharp pixels)
        let big: Vec<u32> = (0..w * h).map(|i| fb[(i / w / K) * sw + (i % w) / K]).collect();
        let mut f = match palette(&big) {
            Some((pal, idx)) => gif::Frame { width: w as u16, height: h as u16, buffer: idx.into(), palette: Some(pal), ..Default::default() },
            None => {
                let rgb: Vec<u8> = big.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
                gif::Frame::from_rgb_speed(w as u16, h as u16, &rgb, 10)
            }
        };
        f.delay = [3, 3, 4][(written % 3) as usize];
        enc.write_frame(&f).map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(path)
}

fn mp4_worker(rx: Receiver<Msg>, path: PathBuf, rate: u32, fmt: VideoFormat) -> Result<PathBuf, String> {
    let (w, h) = (fmt.width, fmt.height);
    let tmp_video = path.with_extension("video.ts");
    let tmp_audio = path.with_extension("audio.f32");
    // the system's exact rate as a fraction (ffmpeg would round 59.538 to 59.94)
    let fps = format!("{}/{}", fmt.fps_num, fmt.fps_den);
    let mut ff = command("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{w}x{h}"), "-framerate", &fps, "-i", "-"])
        .args(["-vf", &format!("scale={}:{}:flags=neighbor", w * 4, h * 4), "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", "-f", "mpegts"])
        .arg(&tmp_video)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    let mut stdin = ff.stdin.take().unwrap();
    let mut audio = std::io::BufWriter::new(std::fs::File::create(&tmp_audio).map_err(|e| e.to_string())?);
    let mut rgb = vec![0u8; w * h * 3];
    for msg in rx {
        match msg {
            Msg::Frame(fb) => {
                for (i, p) in fb.iter().enumerate() {
                    rgb[i * 3..i * 3 + 3].copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
                }
                if stdin.write_all(&rgb).is_err() {
                    break; // ffmpeg exited; the error shows up in wait()
                }
            }
            Msg::Audio(s) => {
                for x in s {
                    let _ = audio.write_all(&x.to_le_bytes());
                }
            }
        }
    }
    drop(stdin);
    audio.flush().map_err(|e| e.to_string())?;
    drop(audio);
    let out = ff.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(ffmpeg_error(&out.stderr));
    }
    // merge video + sound
    let out = command("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&tmp_video)
        .args(["-f", "f32le", "-ar", &rate.to_string(), "-ac", "2", "-i"])
        .arg(&tmp_audio)
        .args(["-c:v", "copy", "-c:a", "aac", "-b:a", "128k", "-shortest"])
        .arg(&path)
        .output()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    if !out.status.success() {
        return Err(ffmpeg_error(&out.stderr));
    }
    cleanup(&[&tmp_video, &tmp_audio]);
    Ok(path)
}

/// Last line of ffmpeg's error (the earlier ones are usually format details).
fn ffmpeg_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    format!("ffmpeg: {}", text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unknown error").trim())
}

fn cleanup(files: &[&Path]) {
    for f in files {
        let _ = std::fs::remove_file(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 224×224 at 71590909/1202436 fps (a typical system), used in the tests.
    const FMT: VideoFormat = VideoFormat { width: 224, height: 224, fps_num: 71_590_909, fps_den: 1_202_436 };

    /// Records 2 s of MP4 with synthetic frames and sound (needs ffmpeg; otherwise, skips).
    #[test]
    fn mp4_with_sound() {
        if !has_ffmpeg() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("emu_frontend_test_{}", std::process::id()));
        let path = dir.join("test 08-26-48.mp4");
        let mut r = Recorder::start(Kind::Mp4, path.clone(), 48000, FMT).unwrap();
        let fb: Vec<u32> = (0..FMT.width * FMT.height).map(|i| if i % 7 == 0 { 0xFF0000 } else { 0x0000FF }).collect();
        for _ in 0..119 {
            r.frame(&fb);
            r.audio(&vec![0.1f32; 806 * 2]);
        }
        r.stop();
        let res = loop {
            if let Some(res) = r.try_finish() {
                break res;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        let out = res.expect("MP4 recording");
        assert!(std::fs::metadata(&out).unwrap().len() > 1000);
        // 119 frames at 59.538 fps = 2.0 s; ffprobe must agree with the rate
        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "v", "-show_entries", "stream=avg_frame_rate", "-of", "csv=p=0"])
            .arg(&out)
            .output()
            .unwrap();
        let rate = String::from_utf8_lossy(&probe.stdout).trim().to_string();
        let (n, d) = rate.split_once('/').unwrap();
        let fps = n.parse::<f64>().unwrap() / d.parse::<f64>().unwrap();
        assert!((fps - FMT.fps_num as f64 / FMT.fps_den as f64).abs() < 0.01, "rate {rate}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn palette_with_frame_colors() {
        let (pal, idx) = palette(&[0xFF0000, 0x000000, 0xFF0000, 0xFFFFFF]).unwrap();
        assert_eq!(pal, vec![255, 0, 0, 0, 0, 0, 255, 255, 255]);
        assert_eq!(idx, vec![0, 1, 0, 2]);
        let many: Vec<u32> = (0..300).collect();
        assert!(palette(&many).is_none()); // > 256 colors: quantizes
    }
}
