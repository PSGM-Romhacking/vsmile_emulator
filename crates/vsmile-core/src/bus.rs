//! Interface de barramento da CPU unSP.
//!
//! A CPU unSP é *word-addressed*: cada endereço aponta para uma palavra de 16
//! bits, não para um byte. O espaço de endereçamento efetivo é de 22 bits
//! (`CS:PC` / `DS:Rs`), então usamos `u32` para os endereços.
//!
//! Leituras podem ter efeitos colaterais (registradores de I/O mapeados em
//! memória), por isso `read_word` recebe `&mut self`.

/// Barramento de memória de 16 bits visto pela CPU unSP.
pub trait Bus {
    /// Lê uma palavra de 16 bits no endereço (em palavras) dado.
    fn read_word(&mut self, addr: u32) -> u16;

    /// Escreve uma palavra de 16 bits no endereço (em palavras) dado.
    fn write_word(&mut self, addr: u32, value: u16);
}

/// Barramento plano de 4 MiW (palavras) — útil para testes e para rodar código
/// unSP cru sem os periféricos do SPG200.
pub struct FlatBus {
    pub mem: Vec<u16>,
}

impl FlatBus {
    /// Cria um barramento plano capaz de endereçar `0..2^bits` palavras.
    pub fn new(bits: u32) -> Self {
        Self {
            mem: vec![0; 1usize << bits],
        }
    }

    /// Carrega palavras em `addr` (útil para montar programas de teste).
    pub fn load(&mut self, addr: u32, words: &[u16]) {
        let base = addr as usize;
        self.mem[base..base + words.len()].copy_from_slice(words);
    }
}

impl Bus for FlatBus {
    fn read_word(&mut self, addr: u32) -> u16 {
        self.mem[(addr as usize) & (self.mem.len() - 1)]
    }

    fn write_word(&mut self, addr: u32, value: u16) {
        let mask = self.mem.len() - 1;
        self.mem[(addr as usize) & mask] = value;
    }
}
