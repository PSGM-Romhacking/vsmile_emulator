//! ADC do SPG200 (4 canais de 10 bits). No V.Smile é usado, entre outras
//! coisas, para ler o nível da bateria. A conversão leva alguns ciclos do
//! clock dividido e gera IRQ ao terminar.

use crate::clock::DivisibleClock;
use crate::io::SystemIo;
use crate::irq::Irq;

const CTRL_WRITE_MASK: u16 = 0x177f;
const STATUS_WRITE_MASK: u16 = 0x2000;

const CTRL_REQUEST: u16 = 1 << 12;
const CTRL_INT_ENABLE: u16 = 1 << 9;
const STATUS_IRQ: u16 = 1 << 13;
const DATA_READY: u16 = 1 << 15;

pub struct Adc {
    clock: DivisibleClock,
    active_channel: i32,
    ctrl: u16,
    status: u16,
    data: u16,
}

impl Adc {
    pub fn new() -> Self {
        let mut a = Self {
            clock: DivisibleClock::new(16, 1),
            active_channel: -1,
            ctrl: 0,
            status: 0,
            data: 0,
        };
        a.reset();
        a
    }

    pub fn reset(&mut self) {
        self.clock.reset();
        self.active_channel = -1;
        self.ctrl = 0;
        self.status = 0;
        self.data = 0;
    }

    fn ctrl_channel(&self) -> i32 {
        ((self.ctrl >> 4) & 0x3) as i32
    }
    fn ctrl_clock_div(&self) -> u32 {
        ((self.ctrl >> 2) & 0x3) as u32
    }

    pub fn run_cycles<Io: SystemIo>(&mut self, cycles: u32, irq: &mut Irq, io: &mut Io) {
        if self.active_channel >= 0
            && self.clock.tick(cycles)
            && self.clock.divided_tick(self.ctrl_clock_div())
        {
            self.data = io.adc(self.active_channel as u8) & 0x3ff;
            self.data |= DATA_READY;
            self.status |= STATUS_IRQ;
            self.active_channel = -1;
            if self.ctrl & CTRL_INT_ENABLE != 0 {
                irq.set_adc(true);
            }
        }
    }

    pub fn set_control(&mut self, value: u16, irq: &mut Irq) {
        self.ctrl = value & CTRL_WRITE_MASK;
        self.status &= !(value & STATUS_WRITE_MASK);

        if self.status & STATUS_IRQ == 0 {
            irq.set_adc(false);
        }

        if self.ctrl & 0x1 != 0 {
            // enabled
            self.status |= STATUS_IRQ;
            if self.ctrl & CTRL_REQUEST != 0 {
                self.status &= !STATUS_IRQ;
                self.ctrl &= !CTRL_REQUEST;
                self.active_channel = self.ctrl_channel();
                self.data &= !DATA_READY;
            }
        } else {
            self.active_channel = -1;
        }
    }

    pub fn control(&self) -> u16 {
        self.ctrl | self.status
    }

    pub fn data(&self) -> u16 {
        self.data
    }
}

impl Default for Adc {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(Adc { clock, active_channel, ctrl, status, data });
