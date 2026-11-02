//! Audio output (cpal): queue of interleaved stereo samples (L, R, L, R…).
//! Outputs with another channel count get the average (mono) or repeated L/R.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct Audio {
    _stream: Option<cpal::Stream>,
    queue: Arc<Mutex<VecDeque<f32>>>,
    pub rate: u32,
}

impl Audio {
    /// Opens the default output. With no device, carries on muted (rate 48000).
    pub fn open() -> Self {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        match Self::build(queue.clone()) {
            Ok((stream, rate)) => Self { _stream: Some(stream), queue, rate },
            Err(e) => {
                eprintln!("audio unavailable: {e}");
                Self { _stream: None, queue, rate: 48000 }
            }
        }
    }

    fn build(queue: Arc<Mutex<VecDeque<f32>>>) -> Result<(cpal::Stream, u32), String> {
        let host = cpal::default_host();
        let dev = host.default_output_device().ok_or("no audio output")?;
        let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
        let rate = cfg.sample_rate().0;
        let ch = cfg.channels() as usize;
        let err = |e| eprintln!("audio: {e}");
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => dev.build_output_stream(
                &cfg.into(),
                move |data: &mut [f32], _| {
                    let mut q = queue.lock().unwrap();
                    for frame in data.chunks_mut(ch) {
                        let (l, r) = pop(&mut q);
                        put(frame, l, r);
                    }
                },
                err,
                None,
            ),
            cpal::SampleFormat::I16 => dev.build_output_stream(
                &cfg.into(),
                move |data: &mut [i16], _| {
                    let mut q = queue.lock().unwrap();
                    let mut tmp = vec![0.0f32; ch];
                    for frame in data.chunks_mut(ch) {
                        let (l, r) = pop(&mut q);
                        put(&mut tmp, l, r);
                        for (o, s) in frame.iter_mut().zip(&tmp) {
                            *o = (s * 32767.0) as i16;
                        }
                    }
                },
                err,
                None,
            ),
            f => return Err(format!("unsupported audio format: {f:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok((stream, rate))
    }

    /// Queues stereo samples with volume (0..1); drops them if ~0.2 s is already queued.
    pub fn push(&self, samples: &[f32], volume: f32) {
        let mut q = self.queue.lock().unwrap();
        if q.len() < self.rate as usize * 2 / 5 {
            q.extend(samples.iter().map(|s| s * volume));
        }
    }

    pub fn clear(&self) {
        self.queue.lock().unwrap().clear();
    }
}

fn pop(q: &mut VecDeque<f32>) -> (f32, f32) {
    let l = q.pop_front().unwrap_or(0.0);
    (l, q.pop_front().unwrap_or(l))
}

/// Spreads L/R across the output channels: mono gets the average.
fn put(frame: &mut [f32], l: f32, r: f32) {
    match frame.len() {
        1 => frame[0] = (l + r) * 0.5,
        _ => {
            for (i, o) in frame.iter_mut().enumerate() {
                *o = if i % 2 == 0 { l } else { r };
            }
        }
    }
}
