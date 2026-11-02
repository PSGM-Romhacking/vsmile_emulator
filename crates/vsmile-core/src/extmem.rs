//! Controlador de memória externa (External Memory / chip-selects).
//!
//! Decodifica os acessos acima de `0x4000` nos quatro chip-selects
//! (`ROMCSB`, `CSB1`, `CSB2`, `CSB3`) conforme o campo *address decode* do
//! registrador de controle (`0x3D23`). O layout depende de quantos bits de
//! endereço o barramento externo expõe.

use crate::io::SystemIo;

/// Registrador de controle em `0x3D23`.
///
/// Campos: `wait_state` (bits 1..3), `bus_arbiter` (bits 3..6),
/// `address_decode` (bits 6..8), `ram_decode` (bits 8..12).
pub struct Extmem {
    ctrl: u16,
}

const WRITE_MASK: u16 = 0x0ffe;

impl Extmem {
    pub fn new() -> Self {
        let mut e = Self { ctrl: 0 };
        e.reset();
        e
    }

    pub fn reset(&mut self) {
        // bus_arbiter = 5 (bits 3..6); demais zerados.
        self.ctrl = 5 << 3;
    }

    pub fn control(&self) -> u16 {
        self.ctrl
    }

    pub fn set_control(&mut self, value: u16) {
        self.ctrl = value & WRITE_MASK;
    }

    #[inline]
    fn address_decode(&self) -> u16 {
        (self.ctrl >> 6) & 0x3
    }

    /// Lê uma palavra da memória externa, roteando para o chip-select correto.
    pub fn read<Io: SystemIo>(&self, io: &mut Io, addr: u32) -> u16 {
        match self.address_decode() {
            0 => io.read_rom_csb(addr),
            1 => match addr >> 21 {
                0 => io.read_rom_csb(addr & 0x1fffff),
                _ => io.read_csb1(addr & 0x1fffff),
            },
            // 2 e 3: barramento com 4 chip-selects de 1 MiW cada.
            _ => match addr >> 20 {
                0 => io.read_rom_csb(addr & 0x0fffff),
                1 => io.read_csb1(addr & 0x0fffff),
                2 => io.read_csb2(addr & 0x0fffff),
                _ => io.read_csb3(addr & 0x0fffff),
            },
        }
    }

    /// Escreve uma palavra na memória externa.
    pub fn write<Io: SystemIo>(&self, io: &mut Io, addr: u32, value: u16) {
        match self.address_decode() {
            0 => io.write_rom_csb(addr, value),
            1 => match addr >> 21 {
                0 => io.write_rom_csb(addr & 0x1fffff, value),
                _ => io.write_csb1(addr & 0x1fffff, value),
            },
            _ => match addr >> 20 {
                0 => io.write_rom_csb(addr & 0x0fffff, value),
                1 => io.write_csb1(addr & 0x0fffff, value),
                2 => io.write_csb2(addr & 0x0fffff, value),
                _ => io.write_csb3(addr & 0x0fffff, value),
            },
        }
    }
}

impl Default for Extmem {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(Extmem { ctrl });
