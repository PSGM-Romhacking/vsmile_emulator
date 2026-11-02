//! The emulation loop: frame pacing, input, rewind, fast forward, sound and the texture
//! of the current frame.

#[allow(unused_imports)]
use super::*;

impl App {
    /// Smooth sound: 1-pole low-pass (~4 kHz) + DC blocker.
    pub(super) fn soften(&mut self, samples: &mut [f32]) {
        let a = 1.0 - (-2.0 * std::f32::consts::PI * 4000.0 / self.audio.rate as f32).exp();
        // interleaved stereo: one filter per channel
        for (i, x) in samples.iter_mut().enumerate() {
            let c = i & 1;
            self.lp[c] += a * (*x - self.lp[c]);
            let hp = self.lp[c] - self.dc_x[c] + 0.995 * self.dc_y[c];
            self.dc_x[c] = self.lp[c];
            self.dc_y[c] = hp;
            *x = hp;
        }
    }


    pub(super) fn emulate(&mut self, ctx: &egui::Context) {
        let now = self.now();
        if std::mem::take(&mut self.dirty_frame) {
            self.upload(ctx);
        }
        let focused = ctx.input(|i| i.focused);
        // with the debugger open the focus goes to its window: don't pause for that
        self.focus_paused = self.loaded && self.pause_unfocused && !focused && !self.debugger.open && !self.cheats.open;
        // FPS/speed: frames emulated per wall-clock second
        if now - self.fps_t0 >= 1.0 {
            self.fps_value = self.fps_frames as f64 / (now - self.fps_t0);
            self.fps_frames = 0;
            self.fps_t0 = now;
        }
        if self.loaded && self.paused && std::mem::take(&mut self.step_frame) {
            // Debugger › Advance 1 frame: one frame, no input, no sound
            self.cheats.apply(self.core.as_mut());
            self.core.run_frame(&vec![0; self.sys.players]);
            self.core.take_audio(&mut Vec::new());
            self.frame_no += 1;
            self.upload(ctx);
        }
        if !self.loaded || self.paused || self.focus_paused || self.menu_open || self.dialog != Dialog::None || self.nav.quick.is_some() {
            self.next_frame = now;
            self.session_tick = now;
            return;
        }
        // play time: only while the game runs (a long gap, e.g. the window hidden, counts little)
        if let Some(s) = &mut self.session {
            s.2 += (now - self.session_tick).clamp(0.0, 0.25);
        }
        self.session_tick = now;
        let devs = self.pads.snapshot();
        let hold = |f: Func| focused && (self.kb_level(f) || self.pad_level(&devs, f));
        let (ff_held, rw_held) = (hold(Func::FastForward), hold(Func::Rewind));
        // 1st controller = player 1, 2nd = player 2…; "P2" actions from any
        // controller go to player 2
        let players = self.sys.players;
        let mut input = vec![0u16; players];
        if focused {
            input = self.kb_bits();
            for (n, d) in devs.iter().enumerate() {
                for (p, b) in gamepad::bits(&self.mapping_for(d), d, players).into_iter().enumerate() {
                    let to = if p == 0 { n } else { p };
                    if let Some(v) = input.get_mut(to) {
                        *v |= b;
                    }
                }
            }
        }
        self.filter_input(&mut input);

        // rewind: one saved state per displayed frame (since they are saved every
        // 2 frames, it goes back at ~2x); the frame is re-run from the state
        if self.rewind_enabled && rw_held {
            self.speed = Speed::Rewind;
            if now >= self.next_frame {
                if let Some(st) = self.history.pop_back() {
                    let _ = self.core.load_state(&st);
                    self.cheats.apply(self.core.as_mut());
                    self.core.run_frame(&input);
                    self.fps_frames += 1;
                    self.core.take_audio(&mut Vec::new());
                    self.upload(ctx);
                }
                self.next_frame = now + self.frame_seconds();
            }
            return;
        }

        let fast = self.ff_toggle || ff_held;
        self.speed = if fast { Speed::Fast } else { Speed::Normal };
        let mult = if fast { self.ff_speed } else { 1 };
        let mut ran = 0;
        while now >= self.next_frame && ran < 3 * mult {
            if self.rewind_enabled && self.frame_no.is_multiple_of(REWIND_EVERY) {
                if self.history.len() >= REWIND_MAX {
                    self.history.pop_front();
                }
                self.history.push_back(self.core.save_state());
            }
            self.cheats.apply(self.core.as_mut());
            self.core.run_frame(&input);
            self.cheevos.frame(&*self.core);
            self.frame_no += 1;
            self.fps_frames += 1;
            let mut samples = Vec::new();
            self.core.take_audio(&mut samples);
            if self.soft_audio {
                self.soften(&mut samples);
            }
            if let Some(r) = &mut self.recorder {
                r.frame(&packed(&self.core.video()));
                r.audio(&samples); // records sound even with sound off
            }
            if !self.muted && !fast {
                self.audio.push(&samples, self.volume as f32 / 100.0); // muted while fast forwarding
            }
            self.next_frame += self.frame_seconds() / mult as f64;
            ran += 1;
        }
        if now > self.next_frame + 0.25 {
            self.next_frame = now; // too far behind: don't try to catch up
        }
        if ran > 0 {
            self.upload(ctx);
        }
    }

    pub(super) fn upload(&mut self, ctx: &egui::Context) {
        let v = self.core.video();
        let mut rgb = Vec::with_capacity(v.width * v.height * 3);
        for y in 0..v.height {
            for &p in &v.pixels[y * v.pitch..y * v.pitch + v.width] {
                rgb.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
            }
        }
        let img = egui::ColorImage::from_rgb([v.width, v.height], &rgb);
        let opts = if self.filter == Filter::Crt { egui::TextureOptions::LINEAR } else { egui::TextureOptions::NEAREST };
        match &mut self.tex {
            Some(t) => t.set(img, opts),
            None => self.tex = Some(ctx.load_texture("screen", img, opts)),
        }
    }
}
