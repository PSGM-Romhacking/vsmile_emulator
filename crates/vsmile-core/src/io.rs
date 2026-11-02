//! Controlador de I/O do SPG200 — a fronteira entre o SoC e o resto da placa.
//!
//! O SPG200 fala com o "mundo externo" por meio de:
//!
//! - **Chip-selects de memória externa** (`ROMCSB`, `CSB1`, `CSB2`, `CSB3`),
//!   decodificados pelo [`crate::extmem::Extmem`] a partir de `0x4000+`. É aqui
//!   que ficam a ROM do sistema e o cartucho.
//! - **Portas GPIO** A/B/C (leitura e escrita com máscara).
//! - **ADC** de 4 canais (bateria, etc.).
//! - **UART** (o joystick do V.Smile conversa por serial).
//!
//! A implementação concreta (ex.: a cola do V.Smile) decide o conteúdo de cada
//! região e porta. Todos os métodos têm default inócuo, então um `impl` mínimo
//! só precisa sobrescrever o que usa.

/// Interface de I/O do SPG200.
///
/// Os endereços passados aos chip-selects são **em palavras** e já vêm
/// mascarados pelo decodificador de memória externa.
#[allow(unused_variables)]
pub trait SystemIo {
    /// Avança periféricos externos no tempo (ex.: temporização da UART do
    /// joystick) pelo número de ciclos executados pela CPU.
    fn run_cycles(&mut self, cycles: u32) {}

    /// Valor do canal de ADC `channel` (0..=3), 10 bits.
    fn adc(&mut self, channel: u8) -> u16 {
        0
    }

    fn get_port_a(&mut self) -> u16 {
        0
    }
    fn set_port_a(&mut self, value: u16, mask: u16) {}
    fn get_port_b(&mut self) -> u16 {
        0
    }
    fn set_port_b(&mut self, value: u16, mask: u16) {}
    fn get_port_c(&mut self) -> u16 {
        0
    }
    fn set_port_c(&mut self, value: u16, mask: u16) {}

    // Chip-selects de memória externa (palavras).
    fn read_rom_csb(&mut self, addr: u32) -> u16 {
        0
    }
    fn write_rom_csb(&mut self, addr: u32, value: u16) {}
    fn read_csb1(&mut self, addr: u32) -> u16 {
        0
    }
    fn write_csb1(&mut self, addr: u32, value: u16) {}
    fn read_csb2(&mut self, addr: u32) -> u16 {
        0
    }
    fn write_csb2(&mut self, addr: u32, value: u16) {}
    fn read_csb3(&mut self, addr: u32) -> u16 {
        0
    }
    fn write_csb3(&mut self, addr: u32, value: u16) {}

    /// Byte transmitido pela UART do SPG200 (TX) para o dispositivo externo.
    fn tx_uart(&mut self, value: u8) {}
    /// Sinaliza que a recepção (RX) de um byte terminou.
    fn rx_uart_done(&mut self) {}

    /// Retira um byte que o dispositivo externo quer enviar à UART do SPG200
    /// (RX do console). O barramento o injeta via `uart.rx_start`. É assim que,
    /// por exemplo, o controlador do V.Smile fala com o console sem precisar de
    /// uma referência de volta ao SoC.
    fn take_uart_rx(&mut self) -> Option<u8> {
        None
    }

    /// Bordas pendentes nas linhas de interrupção externa (ext1, ext2) desde a
    /// última consulta. O barramento as traduz em IRQ.
    fn take_ext_irq(&mut self) -> (bool, bool) {
        (false, false)
    }
}

/// Região de ROM/RAM externa endereçada por palavra, com índice mascarado para
/// o tamanho (potência de dois) — espelha o acesso real dos chip-selects.
pub struct Region {
    data: Vec<u16>,
    mask: usize,
}

impl Region {
    /// Cria a região a partir de palavras. O tamanho é arredondado para cima
    /// até a próxima potência de dois (preenchendo com `0xFFFF`, como ROM vazia).
    pub fn new(words: Vec<u16>) -> Self {
        let size = words.len().next_power_of_two().max(1);
        let mut data = words;
        data.resize(size, 0xFFFF);
        Self {
            mask: size - 1,
            data,
        }
    }

    /// Cria uma região vazia (tudo `0xFFFF`) com `size` palavras.
    pub fn empty(size: usize) -> Self {
        let size = size.next_power_of_two().max(1);
        Self {
            data: vec![0xFFFF; size],
            mask: size - 1,
        }
    }

    #[inline]
    pub fn read(&self, addr: u32) -> u16 {
        self.data[(addr as usize) & self.mask]
    }

    #[inline]
    pub fn write(&mut self, addr: u32, value: u16) {
        let i = (addr as usize) & self.mask;
        self.data[i] = value;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}
