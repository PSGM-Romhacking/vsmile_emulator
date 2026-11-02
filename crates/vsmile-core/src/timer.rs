//! Timers do SPG200: base de tempo de 32768 Hz derivada do clock mestre de
//! 27 MHz, da qual saem as interrupções periódicas (4096/2048/1024/4 Hz, TMB1,
//! TMB2) e dois timers programáveis (A e B) com preload e IRQ no estouro.

use crate::clock::DivisibleClock;
use crate::irq::Irq;

const TIMEBASE_WRITE_MASK: u16 = 0x000f;

pub struct Timer {
    clock: DivisibleClock,
    timebase_setup: u16,

    a_divisor: i32,
    a_data: u16,
    a_preload: u16,
    a_enabled: bool,
    a_control: u16,

    b_divisor: i32,
    b_data: u16,
    b_preload: u16,
    b_enabled: bool,
    b_control: u16,
}

impl Timer {
    pub fn new() -> Self {
        let mut t = Self {
            clock: DivisibleClock::new(27_000_000, 32768),
            timebase_setup: 0,
            a_divisor: -1,
            a_data: 0,
            a_preload: 0,
            a_enabled: false,
            a_control: 0,
            b_divisor: -1,
            b_data: 0,
            b_preload: 0,
            b_enabled: false,
            b_control: 0,
        };
        t.reset();
        t
    }

    pub fn reset(&mut self) {
        self.clock.reset();
        self.timebase_setup = 0;

        self.a_divisor = -1;
        self.a_data = 0;
        self.a_preload = 0;
        self.a_enabled = false;
        // source_a = source_b = 6 (sem fonte).
        self.a_control = (6 << 3) | 6;
        self.update_a_divisors();

        self.b_divisor = -1;
        self.b_data = 0;
        self.b_preload = 0;
        self.b_enabled = false;
        self.b_control = 6; // source_c = 6
        self.update_b_divisors();
    }

    // campos do controle
    fn a_source_a(&self) -> u16 {
        self.a_control & 0x7
    }
    fn a_source_b(&self) -> u16 {
        (self.a_control >> 3) & 0x7
    }
    fn b_source_c(&self) -> u16 {
        self.b_control & 0x7
    }
    fn tmb1(&self) -> u32 {
        (self.timebase_setup & 0x3) as u32
    }
    fn tmb2(&self) -> u32 {
        ((self.timebase_setup >> 2) & 0x3) as u32
    }

    pub fn run_cycles(&mut self, cycles: u32, irq: &mut Irq) {
        while self.clock.tick(cycles) {
            if self.a_enabled && self.a_divisor >= 0 && self.clock.divided_tick(self.a_divisor as u32)
            {
                self.tick_a(irq);
            }
            if self.b_enabled && self.b_divisor >= 0 && self.clock.divided_tick(self.b_divisor as u32)
            {
                self.tick_b(irq);
            }

            if !self.clock.divided_tick(3) {
                continue;
            }
            irq.set_4096hz(true);
            if !self.clock.divided_tick(4) {
                continue;
            }
            irq.set_2048hz(true);
            if !self.clock.divided_tick(5) {
                continue;
            }
            irq.set_1024hz(true);

            if self.clock.divided_tick(8 - self.tmb2()) {
                irq.set_tmb2(true);
            }
            if self.clock.divided_tick(12 - self.tmb1()) {
                irq.set_tmb1(true);
            }

            if !self.clock.divided_tick(13) {
                continue;
            }
            irq.set_4hz(true);
        }
    }

    fn tick_a(&mut self, irq: &mut Irq) {
        self.a_data = self.a_data.wrapping_add(1);
        if self.a_data == 0 {
            self.a_data = self.a_preload;
            irq.set_timer_a(true);
        }
    }

    fn tick_b(&mut self, irq: &mut Irq) {
        self.b_data = self.b_data.wrapping_add(1);
        if self.b_data == 0 {
            self.b_data = self.b_preload;
            irq.set_timer_b(true);
        }
    }

    // ---- registradores --------------------------------------------------

    pub fn timebase_setup(&self) -> u16 {
        self.timebase_setup
    }
    pub fn set_timebase_setup(&mut self, value: u16) {
        self.timebase_setup = value & TIMEBASE_WRITE_MASK;
        self.update_a_divisors();
    }
    pub fn clear_timebase_counter(&mut self) {
        self.clock.clear_div_counter();
    }

    pub fn a_data(&self) -> u16 {
        self.a_data
    }
    pub fn set_a_data(&mut self, value: u16) {
        self.a_preload = value;
        self.a_data = value;
    }
    pub fn a_control(&self) -> u16 {
        self.a_control
    }
    pub fn set_a_control(&mut self, value: u16) {
        self.a_control = value;
        self.update_a_divisors();
    }
    pub fn a_enabled(&self) -> u16 {
        self.a_enabled as u16
    }
    pub fn set_a_enabled(&mut self, value: u16) {
        self.a_enabled = value & 1 != 0;
    }
    pub fn clear_a_irq(&mut self, irq: &mut Irq) {
        irq.set_timer_a(false);
    }

    pub fn b_data(&self) -> u16 {
        self.b_data
    }
    pub fn set_b_data(&mut self, value: u16) {
        self.b_preload = value;
        self.b_data = value;
    }
    pub fn b_control(&self) -> u16 {
        self.b_control
    }
    pub fn set_b_control(&mut self, value: u16) {
        self.b_control = value;
        self.update_b_divisors();
    }
    pub fn b_enabled(&self) -> u16 {
        self.b_enabled as u16
    }
    pub fn set_b_enabled(&mut self, value: u16) {
        self.b_enabled = value & 1 != 0;
    }
    pub fn clear_b_irq(&mut self, irq: &mut Irq) {
        irq.set_timer_b(false);
    }

    fn update_a_divisors(&mut self) {
        self.a_divisor = -1;
        let sa = self.a_source_a();
        let sb = self.a_source_b();

        if sa == 0 || sa == 1 || sa == 6 || sa == 7 {
            return;
        }

        if sa == 5 {
            self.a_divisor = match sb {
                0 => 4,                        // 2048 Hz
                1 => 5,                        // 1024 Hz
                2 => 7,                        // 256 Hz
                3 => (12 - self.tmb1()) as i32, // TMB1
                4 => 13,                       // 4 Hz
                5 => 14,                       // 2 Hz
                _ => -1,
            };
        } else if sb == 6 {
            self.a_divisor = match sa {
                2 => 0, // 32768 Hz
                3 => 2, // 8192 Hz
                4 => 3, // 4096 Hz
                _ => -1,
            };
        }
    }

    fn update_b_divisors(&mut self) {
        self.b_divisor = match self.b_source_c() {
            2 => 0, // 32768 Hz
            3 => 2, // 8192 Hz
            4 => 3, // 4096 Hz
            _ => -1,
        };
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(Timer {
    clock, timebase_setup,
    a_divisor, a_data, a_preload, a_enabled, a_control,
    b_divisor, b_data, b_preload, b_enabled, b_control
});
