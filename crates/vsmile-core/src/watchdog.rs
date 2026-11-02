//! Watchdog do SPG200. Quando habilitado (bit do controle de sistema), reinicia
//! a CPU se o software não "chutá-lo" (escrevendo `0x55AA` em `0x3D24`) dentro
//! de ~0,75 s. Aqui o [`Watchdog::run_cycles`] apenas sinaliza o estouro; quem
//! efetivamente reinicia a CPU é o [`crate::spg::Spg200`].

use crate::clock::SimpleClock;

pub struct Watchdog {
    enabled: bool,
    clock: SimpleClock,
}

impl Watchdog {
    pub fn new() -> Self {
        Self {
            enabled: false,
            clock: SimpleClock::new(27_000_000 * 3 / 4), // 0,75 s
        }
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.clock.reset();
    }

    /// Avança o watchdog. Retorna `true` se estourou (a CPU deve ser reiniciada).
    pub fn run_cycles(&mut self, cycles: u32) -> bool {
        if !self.enabled {
            return false;
        }
        self.clock.tick(cycles)
    }

    pub fn set_enabled(&mut self, enable: bool) {
        if enable && !self.enabled {
            self.clock.reset();
        }
        self.enabled = enable;
    }

    pub fn clear_timer(&mut self, value: u16) {
        if !self.enabled || value != 0x55aa {
            return;
        }
        self.clock.reset();
    }
}

impl Default for Watchdog {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(Watchdog { enabled, clock });
