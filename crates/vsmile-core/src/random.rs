//! Gerador pseudoaleatório do SPG200 — dois LFSRs de 15 bits mapeados em
//! `0x3D2C` e `0x3D2D`. Implementação determinística (o hardware é um
//! registrador de deslocamento com realimentação dos bits 15 e 14).

/// LFSR de 15 bits.
pub struct Random {
    seed: u16,
}

impl Random {
    /// Cria o LFSR com uma semente inicial.
    pub fn new(seed: u16) -> Self {
        Self { seed }
    }

    /// Define a semente (escrita no registrador).
    pub fn set(&mut self, value: u16) {
        self.seed = value;
    }

    /// Lê o valor corrente (15 bits) e avança o registrador.
    pub fn get(&mut self) -> u16 {
        let value = self.seed & 0x7fff;
        self.update();
        value
    }

    fn update(&mut self) {
        let shifted = self.seed << 1;
        let feedback = (((shifted & 0x8000) != 0) ^ ((shifted & 0x4000) != 0)) as u16;
        self.seed = (shifted & 0x7fff) | feedback;
    }
}

impl Default for Random {
    fn default() -> Self {
        Self::new(0)
    }
}

crate::impl_state!(Random { seed });
