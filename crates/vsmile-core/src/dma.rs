//! DMA genérico do SPG200 (`0x3E00..=0x3E03`): copia blocos de palavras de
//! um endereço de origem (até 22 bits) para um destino na RAM/registradores
//! (14 bits). A cópia em si percorre o barramento completo e, por isso, é
//! executada pelo [`crate::memory::Spg200Mem`] (que é o barramento); aqui
//! ficam apenas os registradores.

pub struct Dma {
    pub source: u32,
    pub target: u16,
    pub length: u16,
}

impl Dma {
    pub fn new() -> Self {
        Self {
            source: 0,
            target: 0,
            length: 0,
        }
    }

    pub fn reset(&mut self) {
        self.source = 0;
        self.target = 0;
        self.length = 0;
    }

    pub fn source_lo(&self) -> u16 {
        (self.source & 0xffff) as u16
    }
    pub fn set_source_lo(&mut self, value: u16) {
        self.source = (self.source & !0xffff) | value as u32;
    }
    pub fn source_hi(&self) -> u16 {
        ((self.source >> 16) & 0x3f) as u16
    }
    pub fn set_source_hi(&mut self, value: u16) {
        self.source = (((value & 0x3f) as u32) << 16) | (self.source & 0xffff);
    }
    pub fn length(&self) -> u16 {
        self.length
    }
    pub fn target(&self) -> u16 {
        self.target
    }
    pub fn set_target(&mut self, value: u16) {
        self.target = value & 0x3fff;
    }
}

impl Default for Dma {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(Dma { source, target, length });
