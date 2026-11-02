//! Controlador (joystick) do V.Smile.
//!
//! O controle conversa com o console por UART serial, com handshake RTS/CTS.
//! Quando ocioso, envia `0x55` periodicamente; responde a sondagens (probe) do
//! console e transmite mudanças de botões/cores/direcional. Porte fiel da
//! máquina de estados do veesem (`vsmile_joy.cc`).
//!
//! Para evitar referência cíclica com o SoC, as saídas do controlador (bytes
//! para a UART do console e a borda de RTS que gera IRQ externa) ficam em filas
//! que o [`crate::vsmile::VSmileIo`] drena pelos ganchos do [`crate::io::SystemIo`].

use std::collections::VecDeque;

use crate::clock::SimpleClock;

/// Estado dos botões e direcional do controle.
#[derive(Clone, Copy, Default)]
pub struct JoyInput {
    /// Direcional horizontal (-5..=5; negativo = esquerda).
    pub x: i32,
    /// Direcional vertical (-5..=5; negativo = baixo).
    pub y: i32,
    pub red: bool,
    pub yellow: bool,
    pub blue: bool,
    pub green: bool,
    pub enter: bool,
    pub back: bool,
    pub help: bool,
    pub abc: bool,
}

/// LEDs reportados pelo console ao controle.
#[derive(Clone, Copy, Default)]
pub struct JoyLeds {
    pub red: bool,
    pub yellow: bool,
    pub blue: bool,
    pub green: bool,
}

const TX_BUFFER_SIZE: usize = 16;

pub struct VSmileJoy {
    idle_timer: SimpleClock,         // 1 s
    rts_timeout_timer: SimpleClock,  // 0,5 s
    tx_start_timer: SimpleClock,     // 3,6 ms
    current: JoyInput,
    last_sent: JoyInput,
    tx_buffer: [u8; TX_BUFFER_SIZE],
    tx_write: usize,
    tx_read: usize,
    probe_history: [i32; 2],
    cts: bool,
    rts: bool,
    tx_busy: bool,
    joy_active: bool,
    tx_starting: bool,
    current_updated: bool,
    leds: JoyLeds,

    // Saídas drenadas pelo barramento:
    uart_out: VecDeque<u8>, // bytes para a UART RX do console
    ext1_pulse: bool,       // borda de descida de RTS → IRQ externa 1
}

impl VSmileJoy {
    pub fn new() -> Self {
        Self {
            idle_timer: SimpleClock::new(27_000_000),
            rts_timeout_timer: SimpleClock::new(13_500_000),
            tx_start_timer: SimpleClock::new(97_200),
            current: JoyInput::default(),
            last_sent: JoyInput::default(),
            tx_buffer: [0; TX_BUFFER_SIZE],
            tx_write: 0,
            tx_read: 0,
            probe_history: [0; 2],
            cts: false,
            rts: true,
            tx_busy: false,
            joy_active: false,
            tx_starting: false,
            current_updated: false,
            leds: JoyLeds::default(),
            uart_out: VecDeque::new(),
            ext1_pulse: false,
        }
    }

    pub fn reset(&mut self) {
        self.idle_timer.reset();
        self.rts_timeout_timer.reset();
        self.tx_start_timer.reset();
        self.current = JoyInput::default();
        self.last_sent = JoyInput::default();
        self.cts = false;
        self.rts = true;
        self.tx_busy = false;
        self.joy_active = false;
        self.tx_starting = false;
        self.current_updated = false;
        self.probe_history = [0; 2];
        self.tx_read = 0;
        self.tx_write = 0;
        self.leds = JoyLeds::default();
        self.uart_out.clear();
        self.ext1_pulse = false;
    }

    // ---- saídas drenadas pelo barramento --------------------------------

    pub fn rts(&self) -> bool {
        self.rts
    }
    pub fn leds(&self) -> JoyLeds {
        self.leds
    }
    pub fn take_uart_out(&mut self) -> Option<u8> {
        self.uart_out.pop_front()
    }
    pub fn take_ext1_pulse(&mut self) -> bool {
        let p = self.ext1_pulse;
        self.ext1_pulse = false;
        p
    }

    // ---- entrada do usuário --------------------------------------------

    pub fn update_joystick(&mut self, input: JoyInput) {
        self.current = input;
        self.current_updated = true;
    }

    // ---- interface com a UART do console --------------------------------

    /// Byte recebido do console (console TX → controle).
    pub fn rx(&mut self, value: u8) {
        match value & 0xf0 {
            0x60 => {
                self.leds.green = value & 0x01 != 0;
                self.leds.blue = value & 0x02 != 0;
                self.leds.yellow = value & 0x04 != 0;
                self.leds.red = value & 0x08 != 0;
            }
            0x70 | 0xb0 => {
                self.probe_history[0] = if value & 0xf0 == 0x70 {
                    0
                } else {
                    self.probe_history[1]
                };
                self.probe_history[1] = (value & 0x0f) as i32;
                let resp =
                    0xb0u8 | (((-self.probe_history[0] - self.probe_history[1]) ^ 0xa) & 0xf) as u8;
                self.queue_tx(resp);
            }
            _ => {}
        }
    }

    pub fn set_cts(&mut self, value: bool) {
        self.cts = value;
        if self.cts && self.tx_read != self.tx_write && !self.tx_busy && !self.tx_starting {
            self.tx_starting = true;
            self.tx_start_timer.reset();
        }
    }

    /// Console terminou de receber nosso byte (UART RX done).
    pub fn tx_done(&mut self) {
        if !self.tx_busy {
            return;
        }
        self.joy_active = true;
        self.tx_busy = false;
        if self.cts && self.tx_read != self.tx_write {
            self.start_tx();
        }
    }

    // ---- avanço no tempo ------------------------------------------------

    pub fn run_cycles(&mut self, cycles: u32) {
        if !self.tx_busy && self.idle_timer.tick(cycles) {
            self.queue_tx(0x55);
        }

        if self.tx_starting && self.tx_start_timer.tick(cycles) {
            self.tx_starting = false;
            self.start_tx();
        }

        if !self.rts && !self.cts && !self.tx_starting && !self.tx_busy && self.rts_timeout_timer.tick(cycles) {
            self.set_rts(true);

            if self.joy_active {
                self.current = JoyInput::default();
                self.current_updated = false;
                self.probe_history = [0; 2];
                self.idle_timer.reset();
            }
            self.joy_active = false;

            self.tx_read = 0;
            self.tx_write = 0;
            self.tx_starting = false;
            self.queue_tx(0x55);
        }

        if self.joy_active && self.current_updated {
            self.queue_joy_updates();
        }
    }

    // ---- internos -------------------------------------------------------

    fn set_rts(&mut self, value: bool) {
        let old = self.rts;
        self.rts = value;
        if old && !value {
            self.ext1_pulse = true;
        }
    }

    fn start_tx(&mut self) {
        if self.tx_read == self.tx_write {
            return; // fila vazia
        }
        let byte = self.pop_tx();
        self.uart_out.push_back(byte);
        self.tx_busy = true;
    }

    fn queue_tx(&mut self, byte: u8) {
        let new_write = (self.tx_write + 1) % TX_BUFFER_SIZE;
        let was_empty = self.tx_read == self.tx_write;
        if new_write == self.tx_read {
            return; // fila cheia
        }
        self.tx_buffer[self.tx_write] = byte;
        self.tx_write = new_write;

        if was_empty {
            self.set_rts(false);
            if self.cts {
                if !self.tx_busy && !self.tx_starting {
                    self.tx_starting = true;
                    self.tx_start_timer.reset();
                }
            } else {
                self.rts_timeout_timer.reset();
            }
        }
        self.idle_timer.reset();
    }

    fn pop_tx(&mut self) -> u8 {
        let value = self.tx_buffer[self.tx_read];
        self.tx_read = (self.tx_read + 1) % TX_BUFFER_SIZE;
        if self.tx_write == self.tx_read {
            self.set_rts(true);
        }
        value
    }

    fn queue_joy_updates(&mut self) {
        let c = self.current;
        let l = self.last_sent;
        let update_buttons =
            c.enter != l.enter || c.back != l.back || c.help != l.help || c.abc != l.abc;
        let update_colors =
            c.green != l.green || c.blue != l.blue || c.yellow != l.yellow || c.red != l.red;
        let update_joy = c.x != l.x || c.y != l.y;

        if !update_buttons && !update_colors && !update_joy {
            return;
        }

        if update_buttons {
            let v = if c.enter {
                0xa1
            } else if c.back {
                0xa2
            } else if c.help {
                0xa3
            } else if c.abc {
                0xa4
            } else {
                0xa0
            };
            self.queue_tx(v);
        }
        if update_colors {
            let v = 0x90
                | c.green as u8
                | ((c.blue as u8) << 1)
                | ((c.yellow as u8) << 2)
                | ((c.red as u8) << 3);
            self.queue_tx(v);
        }
        if update_joy {
            let mut x_value = 0xc0u8;
            let mut y_value = 0x80u8;
            if c.x != 0 {
                let base = if c.x < 0 { 0xcb } else { 0xc3 };
                x_value = base + (c.x.abs().clamp(1, 5) - 1) as u8;
            }
            if c.y != 0 {
                let base = if c.y < 0 { 0x8b } else { 0x83 };
                y_value = base + (c.y.abs().clamp(1, 5) - 1) as u8;
            }
            self.queue_tx(x_value);
            self.queue_tx(y_value);
        }

        self.idle_timer.reset();
        self.last_sent = self.current;
        self.current_updated = false;
    }
}

impl Default for VSmileJoy {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(JoyInput { x, y, red, yellow, blue, green, enter, back, help, abc });
crate::impl_state!(JoyLeds { red, yellow, blue, green });
crate::impl_state!(VSmileJoy {
    idle_timer, rts_timeout_timer, tx_start_timer,
    current, last_sent, tx_buffer, tx_write, tx_read, probe_history,
    cts, rts, tx_busy, joy_active, tx_starting, current_updated, leds,
    uart_out, ext1_pulse
});
