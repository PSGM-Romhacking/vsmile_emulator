//! Controlador de interrupções do SPG200.
//!
//! Agrega as fontes de interrupção em **8 linhas de IRQ** (0..7) da CPU unSP,
//! mais a linha de **FIQ** (selecionável). Algumas fontes são "diretas"
//! (PPU → linha 0, canal do SPU → linha 1, beat do SPU → linha 4); as demais
//! passam pelo par de registradores de controle/status de I/O (`0x3D21` e
//! `0x3D22`), mascaradas por habilitação.
//!
//! O FIQ seleciona uma única fonte via `0x3D2E` (valor 7 = nenhuma).

// Bits do registrador de interrupções de I/O.
const IO_TMB1: u16 = 1 << 0;
const IO_TMB2: u16 = 1 << 1;
const IO_TICK_4HZ: u16 = 1 << 3;
const IO_TICK_1024: u16 = 1 << 4;
const IO_TICK_2048: u16 = 1 << 5;
const IO_TICK_4096: u16 = 1 << 6;
const IO_KEY_CHANGE: u16 = 1 << 7;
const IO_UART: u16 = 1 << 8;
const IO_EXT1: u16 = 1 << 9;
const IO_TIMER_B: u16 = 1 << 10;
const IO_TIMER_A: u16 = 1 << 11;
const IO_EXT2: u16 = 1 << 12;
const IO_ADC: u16 = 1 << 13;

/// Estado do controlador de interrupções.
pub struct Irq {
    io_ctrl: u16,
    io_status: u16,
    fiq_select: u8,

    ppu_active: bool,
    spu_channel_active: bool,
    spu_beat_active: bool,
}

impl Irq {
    pub fn new() -> Self {
        let mut irq = Self {
            io_ctrl: 0,
            io_status: 0,
            fiq_select: 7,
            ppu_active: false,
            spu_channel_active: false,
            spu_beat_active: false,
        };
        irq.reset();
        irq
    }

    pub fn reset(&mut self) {
        self.io_ctrl = 0;
        self.io_status = 0;
        self.fiq_select = 7;
        self.ppu_active = false;
        self.spu_channel_active = false;
        self.spu_beat_active = false;
    }

    // ---- registradores (0x3D21 / 0x3D22 / 0x3D2E) ----------------------

    pub fn io_control(&self) -> u16 {
        self.io_ctrl
    }
    pub fn set_io_control(&mut self, value: u16) {
        self.io_ctrl = value;
    }
    pub fn io_status(&self) -> u16 {
        self.io_status
    }
    pub fn clear_io_status(&mut self, value: u16) {
        self.io_status &= !value;
    }
    pub fn fiq_select(&self) -> u16 {
        self.fiq_select as u16
    }
    pub fn set_fiq_select(&mut self, value: u16) {
        self.fiq_select = (value & 7) as u8;
    }

    // ---- fontes diretas -------------------------------------------------

    pub fn set_ppu(&mut self, value: bool) {
        self.ppu_active = value;
    }
    pub fn set_spu_channel(&mut self, value: bool) {
        self.spu_channel_active = value;
    }
    pub fn set_spu_beat(&mut self, value: bool) {
        self.spu_beat_active = value;
    }

    // ---- fontes de I/O (setam bits de status) --------------------------

    fn set_io(&mut self, bit: u16, value: bool) {
        if value {
            self.io_status |= bit;
        } else {
            self.io_status &= !bit;
        }
    }

    pub fn set_timer_a(&mut self, v: bool) {
        self.set_io(IO_TIMER_A, v);
    }
    pub fn set_timer_b(&mut self, v: bool) {
        self.set_io(IO_TIMER_B, v);
    }
    pub fn set_uart(&mut self, v: bool) {
        self.set_io(IO_UART, v);
    }
    pub fn set_adc(&mut self, v: bool) {
        self.set_io(IO_ADC, v);
    }
    pub fn set_ext1(&mut self, v: bool) {
        self.set_io(IO_EXT1, v);
    }
    pub fn set_ext2(&mut self, v: bool) {
        self.set_io(IO_EXT2, v);
    }
    pub fn set_key_change(&mut self, v: bool) {
        self.set_io(IO_KEY_CHANGE, v);
    }
    pub fn set_4hz(&mut self, v: bool) {
        self.set_io(IO_TICK_4HZ, v);
    }
    pub fn set_1024hz(&mut self, v: bool) {
        self.set_io(IO_TICK_1024, v);
    }
    pub fn set_2048hz(&mut self, v: bool) {
        self.set_io(IO_TICK_2048, v);
    }
    pub fn set_4096hz(&mut self, v: bool) {
        self.set_io(IO_TICK_4096, v);
    }
    pub fn set_tmb1(&mut self, v: bool) {
        self.set_io(IO_TMB1, v);
    }
    pub fn set_tmb2(&mut self, v: bool) {
        self.set_io(IO_TMB2, v);
    }

    // ---- agregação para a CPU ------------------------------------------

    #[inline]
    fn active_io(&self) -> u16 {
        self.io_ctrl & self.io_status
    }

    /// Estado das 8 linhas de IRQ da CPU, uma por bit (bit 0 = IRQ0).
    pub fn cpu_irq_bits(&self) -> u8 {
        let a = self.active_io();
        let mut bits = 0u8;
        if self.ppu_active {
            bits |= 1 << 0;
        }
        if self.spu_channel_active {
            bits |= 1 << 1;
        }
        if a & (IO_TIMER_A | IO_TIMER_B) != 0 {
            bits |= 1 << 2;
        }
        if a & (IO_UART | IO_ADC) != 0 {
            bits |= 1 << 3;
        }
        if self.spu_beat_active {
            bits |= 1 << 4;
        }
        if a & (IO_EXT1 | IO_EXT2) != 0 {
            bits |= 1 << 5;
        }
        if a & (IO_TICK_4096 | IO_TICK_2048 | IO_TICK_1024) != 0 {
            bits |= 1 << 6;
        }
        if a & (IO_KEY_CHANGE | IO_TICK_4HZ | IO_TMB1 | IO_TMB2) != 0 {
            bits |= 1 << 7;
        }
        bits
    }

    /// Estado da linha de FIQ, segundo a fonte selecionada.
    pub fn cpu_fiq(&self) -> bool {
        let a = self.active_io();
        match self.fiq_select {
            0 => self.ppu_active,
            1 => self.spu_channel_active,
            2 => a & IO_TIMER_A != 0,
            3 => a & IO_TIMER_B != 0,
            4 => a & IO_UART != 0,
            5 => a & (IO_EXT1 | IO_EXT2) != 0,
            6 => a & IO_ADC != 0,
            _ => false,
        }
    }
}

impl Default for Irq {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(Irq {
    io_ctrl, io_status, fiq_select,
    ppu_active, spu_channel_active, spu_beat_active
});
