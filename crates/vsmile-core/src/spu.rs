//! SPU (Sound Processing Unit) do SPG200 — 16 canais de áudio.
//!
//! Cada canal reproduz uma forma de onda (PCM 8/16 bits ou ADPCM) com fase
//! (pitch), envelope de volume programável (com reload automático via ponteiro
//! na memória), pitch-bend e rampdown. A mixagem gera amostras estéreo de 16
//! bits a ~281,25 kHz (27 MHz / 96), coletadas em um buffer que o frontend
//! reamostra para a taxa da placa de som.
//!
//! Interrupções: fim de amostra por canal → IRQ de canal (linha 1 da CPU);
//! beat/envelope → IRQ de beat (linha 4). Lê formas de onda e tabelas de
//! envelope do barramento via closure (como a PPU), evitando referência cíclica.

use crate::adpcm::Adpcm;
use crate::clock::{DivisibleClock, SimpleClock};
use crate::irq::Irq;

const ENVELOPE_FRAME_DIVIDES: [u32; 16] =
    [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 13, 13, 13, 13];
const RAMPDOWN_FRAME_DIVIDES: [u32; 8] = [2, 4, 6, 8, 10, 12, 13, 13];
const PITCHBEND_FRAME_DIVIDES: [u32; 8] = [3, 4, 5, 6, 7, 8, 9, 10];

const AUDIO_BUFFER_LEN: usize = 6144 * 2;

// Helpers de bitfield.
#[inline]
fn bf_get(raw: u16, pos: u32, len: u32) -> u16 {
    (raw >> pos) & ((1 << len) - 1)
}
#[inline]
fn bf_set(raw: &mut u16, pos: u32, len: u32, val: u16) {
    let mask = ((1u16 << len) - 1) << pos;
    *raw = (*raw & !mask) | ((val << pos) & mask);
}
#[inline]
fn bits_to_u16(bits: &[bool; 16]) -> u16 {
    let mut v = 0u16;
    for (i, &b) in bits.iter().enumerate() {
        if b {
            v |= 1 << i;
        }
    }
    v
}
#[inline]
fn u16_to_bits(v: u16) -> [bool; 16] {
    let mut b = [false; 16];
    for (i, slot) in b.iter_mut().enumerate() {
        *slot = v & (1 << i) != 0;
    }
    b
}

#[derive(Clone, Copy)]
struct ChannelData {
    wave_address: u32,
    loop_address: u32,
    wave_shift: u8,
    envelope_address: u32,
    mode: u16,
    pan: u16,
    envelope0: u16,
    envelope1: u16,
    envelope_irq: u16,
    envelope_data: u16,
    envelope_loop_control: u16,
    wave_data_0: u16,
    wave_data: u16,
    phase: u32,
    phase_acc: u32,
    target_phase: u32,
    env_clk: u8,
    rampdown_clk: u8,
    pitch_bend_control: u16,
    adpcm: Adpcm,
}

impl Default for ChannelData {
    fn default() -> Self {
        Self {
            wave_address: 0,
            loop_address: 0,
            wave_shift: 0,
            envelope_address: 0,
            mode: 0,
            pan: 0,
            envelope0: 0,
            envelope1: 0,
            envelope_irq: 0,
            envelope_data: 0,
            envelope_loop_control: 0,
            wave_data_0: 0x8000,
            wave_data: 0x8000,
            phase: 0,
            phase_acc: 0,
            target_phase: 0,
            env_clk: 0,
            rampdown_clk: 0,
            pitch_bend_control: 0,
            adpcm: Adpcm::default(),
        }
    }
}

impl ChannelData {
    // mode: adpcm(15) tone_color(14) tone_mode(12..14)
    fn mode_adpcm(&self) -> bool {
        self.mode & 0x8000 != 0
    }
    fn set_mode_adpcm(&mut self, v: bool) {
        if v {
            self.mode |= 0x8000;
        } else {
            self.mode &= !0x8000;
        }
    }
    fn mode_tone_color(&self) -> bool {
        self.mode & 0x4000 != 0
    }
    fn mode_tone_mode(&self) -> u16 {
        bf_get(self.mode, 12, 2)
    }
    // pan: pan(8..15) volume(0..7)
    fn pan_pan(&self) -> i32 {
        bf_get(self.pan, 8, 7) as i32
    }
    fn pan_volume(&self) -> i32 {
        bf_get(self.pan, 0, 7) as i32
    }
    // envelope0: target(8..15) sign(7) inc(0..7)
    fn env0_target(&self) -> i32 {
        bf_get(self.envelope0, 8, 7) as i32
    }
    fn env0_sign(&self) -> bool {
        self.envelope0 & 0x80 != 0
    }
    fn env0_inc(&self) -> i32 {
        bf_get(self.envelope0, 0, 7) as i32
    }
    // envelope1: repeat_count(9..16) repeat(8) load(0..8)
    fn env1_repeat_count(&self) -> u16 {
        bf_get(self.envelope1, 9, 7)
    }
    fn set_env1_repeat_count(&mut self, v: u16) {
        bf_set(&mut self.envelope1, 9, 7, v);
    }
    fn env1_repeat(&self) -> bool {
        self.envelope1 & 0x100 != 0
    }
    fn env1_load(&self) -> u16 {
        bf_get(self.envelope1, 0, 8)
    }
    // envelope_irq: irq_fire_address(7..16) irq_enable(6)
    fn envirq_fire_address(&self) -> u16 {
        bf_get(self.envelope_irq, 7, 9)
    }
    fn envirq_enable(&self) -> bool {
        self.envelope_irq & 0x40 != 0
    }
    // envelope_data: count(8..16) edd(0..7)
    fn envdata_count(&self) -> u16 {
        bf_get(self.envelope_data, 8, 8)
    }
    fn set_envdata_count(&mut self, v: u16) {
        bf_set(&mut self.envelope_data, 8, 8, v);
    }
    fn envdata_edd(&self) -> i32 {
        bf_get(self.envelope_data, 0, 7) as i32
    }
    fn set_envdata_edd(&mut self, v: i32) {
        bf_set(&mut self.envelope_data, 0, 7, v as u16);
    }
    // envelope_loop_control: rampdown_offset(9..16) ea_offset(0..9)
    fn elc_rampdown_offset(&self) -> i32 {
        bf_get(self.envelope_loop_control, 9, 7) as i32
    }
    fn set_elc_rampdown_offset(&mut self, v: u16) {
        bf_set(&mut self.envelope_loop_control, 9, 7, v);
    }
    fn elc_ea_offset(&self) -> u16 {
        bf_get(self.envelope_loop_control, 0, 9)
    }
    fn set_elc_ea_offset(&mut self, v: u16) {
        bf_set(&mut self.envelope_loop_control, 0, 9, v);
    }
    // pitch_bend_control: time_step(13..16) sign(12) offset(0..12)
    fn pbc_time_step(&self) -> usize {
        bf_get(self.pitch_bend_control, 13, 3) as usize
    }
    fn pbc_sign(&self) -> bool {
        self.pitch_bend_control & 0x1000 != 0
    }
    fn pbc_offset(&self) -> i32 {
        bf_get(self.pitch_bend_control, 0, 12) as i32
    }
}

pub struct Spu {
    audio_buffer: Box<[u16]>,
    audio_pos: usize,
    sample_clock: SimpleClock,
    envelope_clock: DivisibleClock,
    rampdown_clock: DivisibleClock,

    channel: [ChannelData; 16],

    channel_enable: [bool; 16],
    fiq_enable: [bool; 16],
    fiq_status: [bool; 16],
    env_rampdown: [bool; 16],
    stop: [bool; 16],
    zero_cross: [bool; 16],
    repeat: [bool; 16],
    env_mode: [bool; 16],
    tone_release: [bool; 16],
    env_irq: [bool; 16],
    pitch_bend: [bool; 16],

    wave_out_l: u16,
    wave_out_r: u16,
    wave_in_l: u16,
    wave_in_r: u16,

    main_volume: u8,
    beat_base_count: u16,
    current_beat_base_count: u16,
    beat_count: u16,
    control: u16,

    /// Estado do filtro passa-baixa de saída (um polo), por canal estéreo.
    /// Só usado quando `control.low_pass_enable` (bit 8) está ligado.
    lp_l: i32,
    lp_r: i32,
}

const CONTROL_WRITE_MASK: u16 = 0x388;

impl Spu {
    pub fn new() -> Self {
        let mut s = Self {
            audio_buffer: vec![0u16; AUDIO_BUFFER_LEN].into_boxed_slice(),
            audio_pos: 0,
            sample_clock: SimpleClock::new(96),
            envelope_clock: DivisibleClock::new(384, 1),
            rampdown_clock: DivisibleClock::new(13, 1),
            channel: [ChannelData::default(); 16],
            channel_enable: [false; 16],
            fiq_enable: [false; 16],
            fiq_status: [false; 16],
            env_rampdown: [false; 16],
            stop: [false; 16],
            zero_cross: [false; 16],
            repeat: [false; 16],
            env_mode: [false; 16],
            tone_release: [false; 16],
            env_irq: [false; 16],
            pitch_bend: [false; 16],
            wave_out_l: 0x8000,
            wave_out_r: 0x8000,
            wave_in_l: 0x8000,
            wave_in_r: 0x8000,
            main_volume: 0,
            beat_base_count: 0,
            current_beat_base_count: 0,
            beat_count: 0,
            control: 0,
            lp_l: 0,
            lp_r: 0,
        };
        s.reset();
        s
    }

    pub fn reset(&mut self) {
        self.audio_pos = 0;
        self.sample_clock.reset();
        self.envelope_clock.reset();
        self.rampdown_clock.reset();
        self.channel = [ChannelData::default(); 16];
        self.channel_enable = [false; 16];
        self.fiq_enable = [false; 16];
        self.fiq_status = [false; 16];
        self.env_rampdown = [false; 16];
        self.stop = [false; 16];
        self.zero_cross = [false; 16];
        self.repeat = [false; 16];
        self.env_mode = [false; 16];
        self.tone_release = [false; 16];
        self.env_irq = [false; 16];
        self.pitch_bend = [false; 16];
        self.main_volume = 0;
        self.wave_out_l = 0x8000;
        self.wave_out_r = 0x8000;
        self.wave_in_l = 0x8000;
        self.wave_in_r = 0x8000;
        self.beat_base_count = 0;
        self.current_beat_base_count = 0;
        self.beat_count = 0;
        self.control = 0;
        self.lp_l = 0;
        self.lp_r = 0;
    }

    /// Esvazia o buffer de áudio acumulado (amostras estéreo intercaladas L,R).
    pub fn take_audio(&mut self) -> &[u16] {
        let n = self.audio_pos;
        self.audio_pos = 0;
        &self.audio_buffer[..n]
    }

    // ---- avanço no tempo ------------------------------------------------

    pub fn run_cycles<F: FnMut(u32) -> u16>(&mut self, cycles: u32, mut read: F, irq: &mut Irq) {
        if self.sample_clock.tick(cycles) {
            self.generate_sample(&mut read, irq);
        }

        if self.envelope_clock.tick(cycles) {
            self.update_envelopes(&mut read, irq);

            if self.rampdown_clock.tick(1) {
                self.update_rampdowns();
            }

            if self.current_beat_base_count != 0 {
                self.current_beat_base_count -= 1;
                if self.current_beat_base_count == 0 {
                    self.current_beat_base_count = self.beat_base_count;
                    let bc = bf_get(self.beat_count, 0, 14);
                    if bc != 0 {
                        bf_set(&mut self.beat_count, 0, 14, bc - 1);
                    }
                    if bf_get(self.beat_count, 0, 14) == 0 && self.beat_count & 0x8000 != 0 {
                        self.beat_count |= 0x4000; // irq_status
                        self.update_beat_irq(irq);
                    }
                }
            }
        }
    }

    fn generate_sample<F: FnMut(u32) -> u16>(&mut self, read: &mut F, irq: &mut Irq) {
        let mut left_out: i32 = 0;
        let mut right_out: i32 = 0;

        for ch in 0..16 {
            if !self.channel_enable[ch] || self.stop[ch] {
                continue;
            }
            self.tick_channel(ch, read, irq);

            let c = &self.channel[ch];
            // Interpolação linear entre a amostra anterior e a atual conforme a
            // fração de fase — a menos que o bit `no_interpolation` (9) peça o
            // segurador de ordem zero (amostra atual direta).
            let mut sample = if self.control & 0x200 != 0 {
                (c.wave_data as i32) - 0x8000
            } else {
                let prev =
                    ((c.wave_data_0 as u64 * ((1u64 << 19) - c.phase_acc as u64)) >> 19) as u16;
                let cur = ((c.wave_data as u64 * c.phase_acc as u64) >> 19) as u16;
                (prev as i32 + cur as i32 - 0x8000) as i16 as i32
            };

            let left_pan = ((0x80 - c.pan_pan()) * 2).clamp(0, 0x7f);
            let right_pan = (c.pan_pan() * 2).clamp(0, 0x7f);

            sample = ((sample * c.envdata_edd()) >> 7) as i16 as i32;

            left_out += (sample * left_pan * c.pan_volume()) >> 14;
            right_out += (sample * right_pan * c.pan_volume()) >> 14;
        }

        let high_volume = bf_get(self.control, 6, 2) as i32;
        left_out >>= 4 - high_volume;
        right_out >>= 4 - high_volume;

        let mut left_final =
            (((left_out + (self.wave_in_l as i32 - 0x8000)) * self.main_volume as i32) >> 7) as i16
                as i32;
        let mut right_final =
            (((right_out + (self.wave_in_r as i32 - 0x8000)) * self.main_volume as i32) >> 7) as i16
                as i32;

        // Filtro passa-baixa de saída (um polo, α≈1/4). Acompanha o sinal mesmo
        // desligado, para não "estourar" ao ser ligado; só substitui a saída
        // quando `low_pass_enable` (bit 8) está ativo.
        self.lp_l += (left_final - self.lp_l) >> 2;
        self.lp_r += (right_final - self.lp_r) >> 2;
        if self.control & 0x100 != 0 {
            left_final = self.lp_l.clamp(i16::MIN as i32, i16::MAX as i32);
            right_final = self.lp_r.clamp(i16::MIN as i32, i16::MAX as i32);
        }

        self.wave_out_l = (left_final as i16 as u16) ^ 0x8000;
        self.wave_out_r = (right_final as i16 as u16) ^ 0x8000;

        if self.audio_pos + 2 > self.audio_buffer.len() {
            self.audio_pos = 0;
        }
        self.audio_buffer[self.audio_pos] = self.wave_out_l;
        self.audio_buffer[self.audio_pos + 1] = self.wave_out_r;
        self.audio_pos += 2;
    }

    fn tick_channel<F: FnMut(u32) -> u16>(&mut self, ch: usize, read: &mut F, irq: &mut Irq) {
        let phase_acc = self.channel[ch].phase_acc + self.channel[ch].phase;
        self.channel[ch].phase_acc = phase_acc & 0x7ffff;
        if phase_acc < 0x80000 {
            return;
        }

        if self.fiq_enable[ch] {
            self.fiq_status[ch] = true;
            self.update_channel_irq(irq);
        }

        self.channel[ch].wave_data_0 = self.channel[ch].wave_data;

        if self.channel[ch].mode_tone_mode() == 0 {
            return;
        }

        let word = read(self.channel[ch].wave_address);

        if self.channel[ch].mode_adpcm() {
            if word == 0xffff {
                self.handle_end_marker(ch);
            } else {
                let shift = self.channel[ch].wave_shift;
                let v = ((word >> shift) & 0xf) as u8;
                let decoded = self.channel[ch].adpcm.decode(v);
                self.channel[ch].wave_data = (decoded as u16) ^ 0x8000;
            }
            self.channel[ch].wave_shift += 4;
            if self.channel[ch].wave_shift >= 16 {
                self.channel[ch].wave_shift = 0;
                self.channel[ch].wave_address = self.channel[ch].wave_address.wrapping_add(1);
            }
        } else if !self.channel[ch].mode_tone_color() {
            let shift = self.channel[ch].wave_shift;
            let pcm = ((word >> shift) & 0xff) as u8;
            if pcm == 0xff {
                self.handle_end_marker(ch);
            } else {
                self.channel[ch].wave_data = ((pcm as u16) << 8) | pcm as u16;
            }
            self.channel[ch].wave_shift += 8;
            if self.channel[ch].wave_shift >= 16 {
                self.channel[ch].wave_shift = 0;
                self.channel[ch].wave_address = self.channel[ch].wave_address.wrapping_add(1);
            }
        } else {
            if word == 0xffff {
                self.handle_end_marker(ch);
            } else {
                self.channel[ch].wave_data = word;
            }
            self.channel[ch].wave_address = self.channel[ch].wave_address.wrapping_add(1);
        }
    }

    fn handle_end_marker(&mut self, ch: usize) {
        match self.channel[ch].mode_tone_mode() {
            1 => self.stop_channel(ch),
            2 => {
                self.channel[ch].wave_address = self.channel[ch].loop_address;
                self.channel[ch].wave_shift = 0;
                self.channel[ch].set_mode_adpcm(false);
            }
            _ => {}
        }
    }

    fn update_envelopes<F: FnMut(u32) -> u16>(&mut self, read: &mut F, irq: &mut Irq) {
        for ch in 0..16 {
            if !self.channel_enable[ch] || self.stop[ch] {
                continue;
            }
            self.tick_channel_envelope(ch, read, irq);
            self.tick_channel_pitchbend(ch);
        }
    }

    fn update_rampdowns(&mut self) {
        for ch in 0..16 {
            if !self.channel_enable[ch] || self.stop[ch] {
                continue;
            }
            self.tick_channel_rampdown(ch);
        }
    }

    fn tick_channel_envelope<F: FnMut(u32) -> u16>(&mut self, ch: usize, read: &mut F, irq: &mut Irq) {
        if self.env_mode[ch] || self.env_rampdown[ch] {
            return;
        }
        let div = ENVELOPE_FRAME_DIVIDES[self.channel[ch].env_clk as usize];
        if !self.envelope_clock.divided_tick(div) {
            return;
        }

        let count = self.channel[ch].envdata_count();
        if count != 0 {
            self.channel[ch].set_envdata_count(count - 1);
        }
        if self.channel[ch].envdata_count() != 0 {
            return;
        }

        // count chegou a zero: avança o envelope.
        if self.channel[ch].envdata_edd() != self.channel[ch].env0_target() {
            if self.channel[ch].env0_sign() {
                let edd = (self.channel[ch].envdata_edd() - self.channel[ch].env0_inc())
                    .clamp(self.channel[ch].env0_target(), 0x7f);
                self.channel[ch].set_envdata_edd(edd);
                if edd == 0 {
                    self.stop_channel(ch);
                    return;
                }
            } else {
                let edd = (self.channel[ch].envdata_edd() + self.channel[ch].env0_inc())
                    .clamp(0, self.channel[ch].env0_target());
                self.channel[ch].set_envdata_edd(edd);
            }
        }

        if self.channel[ch].envdata_edd() == self.channel[ch].env0_target() {
            let addr = self.channel[ch]
                .envelope_address
                .wrapping_add(self.channel[ch].elc_ea_offset() as u32);
            if self.channel[ch].env1_repeat() {
                let rc = self.channel[ch].env1_repeat_count();
                if rc != 0 {
                    self.channel[ch].set_env1_repeat_count(rc - 1);
                }
                if self.channel[ch].env1_repeat_count() == 0 {
                    self.channel[ch].envelope0 = read(addr);
                    self.channel[ch].envelope1 = read(addr.wrapping_add(1));
                    let old_rd = self.channel[ch].elc_rampdown_offset();
                    self.channel[ch].envelope_loop_control = read(addr.wrapping_add(2));
                    self.channel[ch].set_elc_rampdown_offset(old_rd as u16);
                    if self.channel[ch].envirq_enable()
                        && self.channel[ch].elc_ea_offset() == self.channel[ch].envirq_fire_address()
                    {
                        self.env_irq[ch] = true;
                        self.update_beat_irq(irq);
                    }
                }
            } else {
                self.channel[ch].envelope0 = read(addr);
                self.channel[ch].envelope1 = read(addr.wrapping_add(1));
                let ea = self.channel[ch].elc_ea_offset().wrapping_add(2) & 0x1ff;
                self.channel[ch].set_elc_ea_offset(ea);
                if self.channel[ch].envirq_enable()
                    && self.channel[ch].elc_ea_offset() == self.channel[ch].envirq_fire_address()
                {
                    self.env_irq[ch] = true;
                    self.update_beat_irq(irq);
                }
            }
        }

        let load = self.channel[ch].env1_load();
        self.channel[ch].set_envdata_count(load);
    }

    fn tick_channel_pitchbend(&mut self, ch: usize) {
        if !self.pitch_bend[ch] || self.channel[ch].phase == self.channel[ch].target_phase {
            return;
        }
        let div = PITCHBEND_FRAME_DIVIDES[self.channel[ch].pbc_time_step()];
        if !self.envelope_clock.divided_tick(div) {
            return;
        }
        let phase = self.channel[ch].phase as i32;
        let target = self.channel[ch].target_phase as i32;
        let offset = self.channel[ch].pbc_offset();
        let new = if self.channel[ch].pbc_sign() {
            (phase - offset).clamp(target, 0x7ffff)
        } else {
            (phase + offset).clamp(0, target)
        };
        self.channel[ch].phase = new as u32;
    }

    fn tick_channel_rampdown(&mut self, ch: usize) {
        if !self.env_rampdown[ch] {
            return;
        }
        let div = RAMPDOWN_FRAME_DIVIDES[self.channel[ch].rampdown_clk as usize];
        if !self.rampdown_clock.divided_tick(div) {
            return;
        }
        let edd = (self.channel[ch].envdata_edd() - self.channel[ch].elc_rampdown_offset())
            .clamp(0, 0x7f);
        self.channel[ch].set_envdata_edd(edd);
        if edd == 0 {
            self.stop_channel(ch);
        }
    }

    fn start_channel(&mut self, ch: usize) {
        self.channel[ch].wave_shift = 0;
        self.channel[ch].adpcm.reset();
        if !self.env_mode[ch] {
            let load = self.channel[ch].env1_load();
            self.channel[ch].set_envdata_count(load);
        }
    }

    fn stop_channel(&mut self, ch: usize) {
        self.stop[ch] = true;
        self.tone_release[ch] = false;
        self.env_rampdown[ch] = false;
        self.channel[ch].set_mode_adpcm(false);
    }

    fn update_channel_irq(&mut self, irq: &mut Irq) {
        irq.set_spu_channel(self.fiq_status.iter().any(|&b| b));
    }

    fn update_beat_irq(&mut self, irq: &mut Irq) {
        let beat = self.beat_count & 0x8000 != 0 && self.beat_count & 0x4000 != 0;
        let env = self.env_irq.iter().any(|&b| b);
        irq.set_spu_beat(beat || env);
    }

    // ---- dispatch de registradores (0x3000..=0x34FF) --------------------

    pub fn read_reg(&mut self, addr: u32) -> Option<u16> {
        let v = match addr {
            0x3000..=0x30ff => {
                let ch = ((addr >> 4) & 0xf) as usize;
                let c = &self.channel[ch];
                match addr & 0xf {
                    0 => (c.wave_address & 0xffff) as u16,
                    1 => {
                        let wa_hi = (c.wave_address >> 16) as u16;
                        let la_hi = (c.loop_address >> 16) as u16;
                        c.mode | (la_hi << 6) | wa_hi
                    }
                    2 => (c.loop_address & 0xffff) as u16,
                    3 => c.pan,
                    4 => c.envelope0,
                    5 => c.envelope_data,
                    6 => c.envelope1,
                    7 => c.envelope_irq | (c.envelope_address >> 16) as u16,
                    8 => (c.envelope_address & 0xffff) as u16,
                    9 => c.wave_data_0,
                    10 => c.envelope_loop_control,
                    11 => c.wave_data,
                    _ => 0,
                }
            }
            0x3200..=0x32ff => {
                let ch = ((addr >> 4) & 0xf) as usize;
                let c = &self.channel[ch];
                match addr & 0xf {
                    0 => (c.phase >> 16) as u16,
                    1 => (c.phase_acc >> 16) as u16,
                    2 => (c.target_phase >> 16) as u16,
                    3 => c.rampdown_clk as u16,
                    4 => (c.phase & 0xffff) as u16,
                    5 => (c.phase_acc & 0xffff) as u16,
                    6 => (c.target_phase & 0xffff) as u16,
                    7 => c.pitch_bend_control,
                    _ => 0,
                }
            }
            0x3400 => bits_to_u16(&self.channel_enable),
            0x3401 => self.main_volume as u16,
            0x3402 => bits_to_u16(&self.fiq_enable),
            0x3403 => bits_to_u16(&self.fiq_status),
            0x3404 => self.beat_base_count,
            0x3405 => self.beat_count,
            0x3406 => self.env_clk_group(0),
            0x3407 => self.env_clk_group(4),
            0x3408 => self.env_clk_group(8),
            0x3409 => self.env_clk_group(12),
            0x340a => bits_to_u16(&self.env_rampdown),
            0x340b => bits_to_u16(&self.stop),
            0x340c => bits_to_u16(&self.zero_cross),
            0x340d => self.control,
            0x340f => bits_to_u16(&self.channel_enable) & !bits_to_u16(&self.stop),
            0x3412 => self.wave_out_l,
            0x3413 => self.wave_out_r,
            0x3414 => bits_to_u16(&self.repeat),
            0x3415 => bits_to_u16(&self.env_mode),
            0x3416 => bits_to_u16(&self.tone_release),
            0x3417 => bits_to_u16(&self.env_irq),
            0x3418 => bits_to_u16(&self.pitch_bend),
            _ => return None,
        };
        Some(v)
    }

    pub fn write_reg(&mut self, addr: u32, value: u16, irq: &mut Irq) -> bool {
        match addr {
            0x3000..=0x30ff => {
                let ch = ((addr >> 4) & 0xf) as usize;
                let c = &mut self.channel[ch];
                match addr & 0xf {
                    0 => {
                        c.wave_address = (c.wave_address & !0xffff) | value as u32;
                        c.wave_shift = 0;
                    }
                    1 => {
                        c.mode = value & 0xf000;
                        c.wave_address = ((value as u32 & 0x3f) << 16) | (c.wave_address & 0xffff);
                        c.loop_address =
                            (((value as u32 >> 6) & 0x3f) << 16) | (c.loop_address & 0xffff);
                    }
                    2 => c.loop_address = (c.loop_address & !0xffff) | value as u32,
                    3 => c.pan = value & 0x7f7f,
                    4 => c.envelope0 = value & 0x7fff,
                    5 => c.envelope_data = value & 0xff7f,
                    6 => c.envelope1 = value,
                    7 => {
                        c.envelope_irq = value & 0xffc0;
                        c.envelope_address =
                            ((value as u32 & 0x3f) << 16) | (c.envelope_address & 0xffff);
                    }
                    8 => c.envelope_address = (c.envelope_address & !0xffff) | value as u32,
                    9 => c.wave_data_0 = value,
                    10 => c.envelope_loop_control = value,
                    11 => c.wave_data = value,
                    _ => {}
                }
            }
            0x3200..=0x32ff => {
                let ch = ((addr >> 4) & 0xf) as usize;
                let c = &mut self.channel[ch];
                match addr & 0xf {
                    0 => c.phase = ((value as u32 & 0x07) << 16) | (c.phase & 0xffff),
                    1 => c.phase_acc = ((value as u32 & 0x07) << 16) | (c.phase_acc & 0xffff),
                    2 => c.target_phase = ((value as u32 & 0x07) << 16) | (c.target_phase & 0xffff),
                    3 => c.rampdown_clk = (value & 0x07) as u8,
                    4 => c.phase = (c.phase & !0xffff) | value as u32,
                    5 => c.phase_acc = (c.phase_acc & !0xffff) | value as u32,
                    6 => c.target_phase = (c.target_phase & !0xffff) | value as u32,
                    7 => c.pitch_bend_control = value,
                    _ => {}
                }
            }
            0x3400 => self.set_channel_enable(value),
            0x3401 => self.main_volume = (value & 0x7f) as u8,
            0x3402 => self.fiq_enable = u16_to_bits(value),
            0x3403 => {
                let clear = u16_to_bits(value);
                for i in 0..16 {
                    if clear[i] {
                        self.fiq_status[i] = false;
                    }
                }
                self.update_channel_irq(irq);
            }
            0x3404 => {
                self.beat_base_count = value & 0xfff;
                self.current_beat_base_count = self.beat_base_count;
            }
            0x3405 => {
                let old_status = self.beat_count & 0x4000 != 0;
                self.beat_count = value;
                // irq_status = old && !new_status
                let new_status = self.beat_count & 0x4000 != 0;
                if old_status && !new_status {
                    self.beat_count |= 0x4000;
                } else {
                    self.beat_count &= !0x4000;
                }
                self.update_beat_irq(irq);
            }
            0x3406 => self.set_env_clk_group(0, value),
            0x3407 => self.set_env_clk_group(4, value),
            0x3408 => self.set_env_clk_group(8, value),
            0x3409 => self.set_env_clk_group(12, value),
            0x340a => {
                let status = bits_to_u16(&self.channel_enable) & !bits_to_u16(&self.stop);
                self.env_rampdown = u16_to_bits(value & status);
            }
            0x340b => self.clear_channel_stop(value),
            0x340c => self.zero_cross = u16_to_bits(value),
            0x340d => {
                let old_overflow = self.control & 0x20;
                self.control = (value & CONTROL_WRITE_MASK & !0x20) | old_overflow;
            }
            0x3410 => self.wave_in_l = value,
            0x3411 => self.wave_in_r = value,
            0x3414 => self.repeat = u16_to_bits(value),
            0x3415 => self.env_mode = u16_to_bits(value),
            0x3416 => self.tone_release = u16_to_bits(value),
            0x3417 => {
                let clear = u16_to_bits(value);
                for i in 0..16 {
                    if clear[i] {
                        self.env_irq[i] = false;
                    }
                }
            }
            0x3418 => self.pitch_bend = u16_to_bits(value),
            _ => return false,
        }
        true
    }

    fn env_clk_group(&self, base: usize) -> u16 {
        (self.channel[base].env_clk as u16)
            | ((self.channel[base + 1].env_clk as u16) << 4)
            | ((self.channel[base + 2].env_clk as u16) << 8)
            | ((self.channel[base + 3].env_clk as u16) << 12)
    }
    fn set_env_clk_group(&mut self, base: usize, value: u16) {
        self.channel[base].env_clk = (value & 0x0f) as u8;
        self.channel[base + 1].env_clk = ((value >> 4) & 0x0f) as u8;
        self.channel[base + 2].env_clk = ((value >> 8) & 0x0f) as u8;
        self.channel[base + 3].env_clk = ((value >> 12) & 0x0f) as u8;
    }

    fn set_channel_enable(&mut self, value: u16) {
        let old = self.channel_enable;
        self.channel_enable = u16_to_bits(value);
        for ch in 0..16 {
            if old[ch] == self.channel_enable[ch] || self.stop[ch] {
                continue;
            }
            if self.channel_enable[ch] {
                self.start_channel(ch);
            } else {
                self.stop_channel(ch);
            }
        }
    }

    fn clear_channel_stop(&mut self, value: u16) {
        let clear = u16_to_bits(value);
        for ch in 0..16 {
            if clear[ch] && self.stop[ch] {
                self.stop[ch] = false;
                if self.channel_enable[ch] {
                    self.start_channel(ch);
                }
            }
        }
    }
}

impl Default for Spu {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(ChannelData {
    wave_address, loop_address, wave_shift, envelope_address, mode, pan,
    envelope0, envelope1, envelope_irq, envelope_data, envelope_loop_control,
    wave_data_0, wave_data, phase, phase_acc, target_phase,
    env_clk, rampdown_clk, pitch_bend_control, adpcm
});
crate::impl_state!(Spu {
    audio_buffer, audio_pos, sample_clock, envelope_clock, rampdown_clock,
    channel,
    channel_enable, fiq_enable, fiq_status, env_rampdown, stop, zero_cross,
    repeat, env_mode, tone_release, env_irq, pitch_bend,
    wave_out_l, wave_out_r, wave_in_l, wave_in_r,
    main_volume, beat_base_count, current_beat_base_count, beat_count, control,
    lp_l, lp_r
});
