//! Barramento de memória do SPG200 — o mapa visto pela CPU unSP.
//!
//! Layout (endereços em palavras):
//!
//! | Faixa               | Conteúdo                                         |
//! |---------------------|--------------------------------------------------|
//! | `0x0000..=0x27FF`   | RAM interna (10 KiW)                              |
//! | `0x2800..=0x2FFF`   | PPU: registradores, scroll, paleta, sprites      |
//! | `0x3000..=0x34FF`   | SPU: canais e controle de áudio (stub)           |
//! | `0x3D00..=0x3D36`   | GPIO, timers, controle de sistema, IRQ, ADC,     |
//! |                     | RNG, UART, memória externa                        |
//! | `0x3E00..=0x3E03`   | DMA                                               |
//! | `0x4000..=0x3FFFFF` | memória externa (ROM/cartucho via chip-select)   |
//!
//! Tudo integrado exceto o SPU (`0x3000..=0x34FF`), que ainda responde como
//! *stub* (leitura 0, escrita ignorada).

use crate::adc::Adc;
use crate::bus::Bus;
use crate::dma::Dma;
use crate::extmem::Extmem;
use crate::gpio::Gpio;
use crate::io::SystemIo;
use crate::irq::Irq;
use crate::ppu::Ppu;
use crate::random::Random;
use crate::spu::Spu;
use crate::timer::Timer;
use crate::uart::Uart;
use crate::watchdog::Watchdog;

/// Temporização de vídeo da região do console.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VideoTiming {
    Pal,
    Ntsc,
}

const RAM_WORDS: usize = 0x2800;

/// Máscara de escrita do registrador de controle de sistema (`0x3D20`).
const SYSCTRL_WRITE_MASK: u16 = 0xc3f6;

/// Lê uma palavra para a PPU buscar tile maps / gráficos: apenas RAM e memória
/// externa (os únicos destinos de leitura gráfica no hardware real).
fn read_graphics<Io: SystemIo>(ram: &[u16], extmem: &Extmem, io: &mut Io, addr: u32) -> u16 {
    let addr = addr & 0x3fffff;
    match addr {
        0x0000..=0x27ff => ram[addr as usize],
        0x4000..=0x3fffff => extmem.read(io, addr),
        _ => 0,
    }
}

/// Barramento do SPG200, parametrizado pela implementação de I/O concreta
/// (que fornece ROM/cartucho, portas, ADC e UART).
pub struct Spg200Mem<Io: SystemIo> {
    /// RAM interna.
    pub ram: Box<[u16; RAM_WORDS]>,
    /// Controlador de I/O externo.
    pub io: Io,
    /// Unidade de vídeo.
    pub ppu: Ppu,
    /// Unidade de áudio.
    pub spu: Spu,
    /// Controlador de interrupções.
    pub irq: Irq,
    /// Temporizadores.
    pub timer: Timer,
    /// UART (joystick no V.Smile).
    pub uart: Uart,
    /// Conversor analógico-digital.
    pub adc: Adc,
    /// Portas de I/O de propósito geral.
    pub gpio: Gpio,
    /// DMA genérico.
    pub dma: Dma,
    /// Watchdog.
    pub watchdog: Watchdog,
    /// Temporização de vídeo (lida em `0x3D2B`).
    pub video_timing: VideoTiming,

    extmem: Extmem,
    system_ctrl: u16,
    random1: Random,
    random2: Random,

    /// Sombra do segmento de dados (DS) para leitura/escrita em `0x3D2F`.
    ds_shadow: u16,
    /// `true` se `0x3D2F` foi escrito desde a última sincronização — só então o
    /// DS deve ser reescrito na CPU (senão clobbaria mudanças de SR feitas pela
    /// própria CPU, como entrada de IRQ ou `reti`).
    ds_written: bool,

    /// Contador de acessos a regiões ainda não implementadas (diagnóstico).
    pub unimplemented_accesses: u64,

    /// Histograma de leituras de I/O (`0x2800..0x4000`), para diagnóstico.
    /// `None` por padrão (sem custo); habilite com [`Spg200Mem::enable_read_hist`].
    pub io_read_hist: Option<Box<[u64]>>,
}

impl<Io: SystemIo> Spg200Mem<Io> {
    pub fn new(io: Io, video_timing: VideoTiming) -> Self {
        let mut mem = Self {
            ram: Box::new([0; RAM_WORDS]),
            io,
            ppu: Ppu::new(video_timing),
            spu: Spu::new(),
            irq: Irq::new(),
            timer: Timer::new(),
            uart: Uart::new(),
            adc: Adc::new(),
            gpio: Gpio::new(),
            dma: Dma::new(),
            watchdog: Watchdog::new(),
            video_timing,
            extmem: Extmem::new(),
            system_ctrl: 0,
            random1: Random::new(0x1418),
            random2: Random::new(0x1658),
            ds_shadow: 0,
            ds_written: false,
            unimplemented_accesses: 0,
            io_read_hist: None,
        };
        mem.reset();
        mem
    }

    /// Habilita o histograma de leituras de I/O (diagnóstico).
    pub fn enable_read_hist(&mut self) {
        self.io_read_hist = Some(vec![0u64; 0x1800].into_boxed_slice());
    }

    /// Reinicia o estado do barramento (sem recriar a I/O).
    pub fn reset(&mut self) {
        self.ram.fill(0);
        self.ppu.reset();
        self.spu.reset();
        self.irq.reset();
        self.timer.reset();
        self.uart.reset();
        self.adc.reset();
        self.gpio.reset();
        self.dma.reset();
        self.watchdog.reset();
        self.extmem.reset();
        self.set_system_control(0);
        self.random1.set(0x1418);
        self.random2.set(0x1658);
        self.ds_shadow = 0;
    }

    /// Avança os periféricos (exceto PPU) por `cycles` ciclos de CPU.
    pub fn run_peripherals(&mut self, cycles: u32) {
        let Self {
            io,
            adc,
            uart,
            timer,
            spu,
            irq,
            ram,
            extmem,
            ..
        } = self;

        io.run_cycles(cycles);
        adc.run_cycles(cycles, irq, io);
        uart.run_cycles(cycles, irq, io);
        timer.run_cycles(cycles, irq);
        spu.run_cycles(cycles, |addr| read_graphics(&ram[..], extmem, io, addr), irq);

        // Dreno dos sinais que o dispositivo externo (ex.: controlador) quer
        // enviar ao SoC: bytes para a UART RX e bordas de IRQ externa.
        while let Some(byte) = io.take_uart_rx() {
            uart.rx_start(byte);
        }
        let (ext1, ext2) = io.take_ext_irq();
        if ext1 {
            irq.set_ext1(true);
        }
        if ext2 {
            irq.set_ext2(true);
        }
    }

    /// Esvazia o buffer de áudio do SPU (amostras estéreo L,R intercaladas).
    pub fn take_audio(&mut self) -> &[u16] {
        self.spu.take_audio()
    }

    /// Avança a PPU por `cycles` ciclos de CPU. Retorna `true` quando um quadro
    /// completo foi desenhado.
    pub fn run_ppu(&mut self, cycles: u32) -> bool {
        let Self {
            ppu,
            ram,
            extmem,
            io,
            ..
        } = self;
        ppu.run_cycles(cycles, |addr| read_graphics(&ram[..], extmem, io, addr))
    }

    /// Avança o watchdog. Retorna `true` se estourou (a CPU deve ser reiniciada).
    pub fn run_watchdog(&mut self, cycles: u32) -> bool {
        self.watchdog.run_cycles(cycles)
    }

    /// Estado das linhas de IRQ (8 bits) e FIQ para a CPU.
    pub fn irq_lines(&mut self) -> (u8, bool) {
        self.irq.set_ppu(self.ppu.irq_line());
        // TODO: sincronizar fontes do SPU quando existir.
        (self.irq.cpu_irq_bits(), self.irq.cpu_fiq())
    }

    pub fn set_ext1_irq(&mut self, value: bool) {
        self.irq.set_ext1(value);
    }
    pub fn set_ext2_irq(&mut self, value: bool) {
        self.irq.set_ext2(value);
    }

    pub fn system_control(&self) -> u16 {
        self.system_ctrl
    }

    fn set_system_control(&mut self, value: u16) {
        self.system_ctrl = value & SYSCTRL_WRITE_MASK;
        self.watchdog.set_enabled(self.system_ctrl & 0x8000 != 0);
    }

    /// Sincroniza o espelho de DS a partir da CPU, limpando o flag de escrita.
    /// Chamado pelo [`crate::spg::Spg200`] antes de cada passo.
    pub fn sync_ds_from_cpu(&mut self, ds: u16) {
        self.ds_shadow = ds;
        self.ds_written = false;
    }
    /// Retorna o DS a ser aplicado na CPU se, e somente se, `0x3D2F` foi escrito
    /// durante o passo.
    pub fn ds_to_apply(&self) -> Option<u16> {
        if self.ds_written {
            Some(self.ds_shadow)
        } else {
            None
        }
    }

    #[cold]
    #[inline]
    fn unimplemented(&mut self) {
        self.unimplemented_accesses = self.unimplemented_accesses.wrapping_add(1);
    }
}

impl<Io: SystemIo> Bus for Spg200Mem<Io> {
    fn read_word(&mut self, addr: u32) -> u16 {
        let addr = addr & 0x3fffff;
        if let Some(hist) = self.io_read_hist.as_mut() {
            if (0x2800..0x4000).contains(&addr) {
                hist[(addr - 0x2800) as usize] += 1;
            }
        }
        match addr {
            0x0000..=0x27ff => self.ram[addr as usize],

            // ---- PPU ----
            0x2800..=0x2fff => self.ppu.read_reg(addr).unwrap_or(0),

            // ---- SPU ----
            0x3000..=0x34ff => self.spu.read_reg(addr).unwrap_or(0),

            // ---- GPIO ----
            0x3d00 => self.gpio.mode(),
            0x3d01 | 0x3d06 | 0x3d0b => {
                let port = ((addr - 0x3d01) / 5) as usize;
                self.gpio.data(port, &mut self.io)
            }
            0x3d02 | 0x3d07 | 0x3d0c => self.gpio.buffer(((addr - 0x3d01) / 5) as usize),
            0x3d03 | 0x3d08 | 0x3d0d => self.gpio.dir(((addr - 0x3d01) / 5) as usize),
            0x3d04 | 0x3d09 | 0x3d0e => self.gpio.attrib(((addr - 0x3d01) / 5) as usize),
            0x3d05 | 0x3d0a | 0x3d0f => self.gpio.mask(((addr - 0x3d01) / 5) as usize),

            // ---- timers ----
            0x3d10 => self.timer.timebase_setup(),
            0x3d12 => self.timer.a_data(),
            0x3d13 => self.timer.a_control(),
            0x3d14 => self.timer.a_enabled(),
            0x3d16 => self.timer.b_data(),
            0x3d17 => self.timer.b_control(),
            0x3d18 => self.timer.b_enabled(),
            0x3d1c => self.ppu.read_reg(addr).unwrap_or(0),

            // ---- sistema / IRQ / extmem / ADC / RNG / DS ----
            0x3d20 => self.system_ctrl,
            0x3d21 => self.irq.io_control(),
            0x3d22 => self.irq.io_status(),
            0x3d23 => self.extmem.control(),
            0x3d25 => self.adc.control(),
            0x3d27 => self.adc.data(),
            0x3d2b => (self.video_timing == VideoTiming::Pal) as u16,
            0x3d2c => self.random1.get(),
            0x3d2d => self.random2.get(),
            0x3d2e => self.irq.fiq_select(),
            0x3d2f => self.ds_shadow,

            // ---- UART ----
            0x3d30 => self.uart.control(),
            0x3d31 => self.uart.status(),
            0x3d33 => self.uart.baud_lo(),
            0x3d34 => self.uart.baud_hi(),
            0x3d35 => self.uart.tx_reg(),
            0x3d36 => self.uart.rx(),

            // ---- DMA ----
            0x3e00 => self.dma.source_lo(),
            0x3e01 => self.dma.source_hi(),
            0x3e02 => self.dma.length(),
            0x3e03 => self.dma.target(),

            // ---- memória externa (ROM / cartucho) ----
            0x4000..=0x3fffff => {
                let ext = &self.extmem;
                ext.read(&mut self.io, addr)
            }

            // ---- SPU e outros ainda não implementados (stub) ----
            0x3000..=0x3fff => {
                self.unimplemented();
                0
            }

            _ => 0,
        }
    }

    fn write_word(&mut self, addr: u32, value: u16) {
        let addr = addr & 0x3fffff;
        match addr {
            0x0000..=0x27ff => self.ram[addr as usize] = value,

            // ---- PPU: DMA de sprite precisa ler o barramento ----
            0x2872 => {
                let Self {
                    ppu,
                    ram,
                    extmem,
                    io,
                    ..
                } = self;
                ppu.start_sprite_dma(value, |addr| read_graphics(&ram[..], extmem, io, addr));
            }
            0x2800..=0x2fff => {
                self.ppu.write_reg(addr, value);
            }

            // ---- SPU ----
            0x3000..=0x34ff => {
                self.spu.write_reg(addr, value, &mut self.irq);
            }

            // ---- GPIO ----
            0x3d00 => self.gpio.set_mode(value),
            0x3d01 | 0x3d02 | 0x3d06 | 0x3d07 | 0x3d0b | 0x3d0c => {
                let port = ((addr - 0x3d01) / 5) as usize;
                self.gpio.set_buffer(port, value, &mut self.io);
            }
            0x3d03 | 0x3d08 | 0x3d0d => {
                let port = ((addr - 0x3d01) / 5) as usize;
                self.gpio.set_dir(port, value, &mut self.io);
            }
            0x3d04 | 0x3d09 | 0x3d0e => {
                let port = ((addr - 0x3d01) / 5) as usize;
                self.gpio.set_attrib(port, value, &mut self.io);
            }
            0x3d05 | 0x3d0a | 0x3d0f => {
                let port = ((addr - 0x3d01) / 5) as usize;
                self.gpio.set_mask(port, value, &mut self.io);
            }

            // ---- timers ----
            0x3d10 => self.timer.set_timebase_setup(value),
            0x3d11 => self.timer.clear_timebase_counter(),
            0x3d12 => self.timer.set_a_data(value),
            0x3d13 => self.timer.set_a_control(value),
            0x3d14 => self.timer.set_a_enabled(value),
            0x3d15 => self.timer.clear_a_irq(&mut self.irq),
            0x3d16 => self.timer.set_b_data(value),
            0x3d17 => self.timer.set_b_control(value),
            0x3d18 => self.timer.set_b_enabled(value),
            0x3d19 => self.timer.clear_b_irq(&mut self.irq),

            // ---- sistema / IRQ / extmem / watchdog / ADC / RNG / DS ----
            0x3d20 => self.set_system_control(value),
            0x3d21 => self.irq.set_io_control(value),
            0x3d22 => self.irq.clear_io_status(value),
            0x3d23 => self.extmem.set_control(value),
            0x3d24 => self.watchdog.clear_timer(value),
            0x3d25 => self.adc.set_control(value, &mut self.irq),
            0x3d2c => self.random1.set(value),
            0x3d2d => self.random2.set(value),
            0x3d2e => self.irq.set_fiq_select(value),
            0x3d2f => {
                self.ds_shadow = value;
                self.ds_written = true;
            }

            // ---- UART ----
            0x3d30 => self.uart.set_control(value, &mut self.irq),
            0x3d31 => self.uart.set_status(value, &mut self.irq),
            0x3d32 => self.uart.soft_reset(),
            0x3d33 => self.uart.set_baud_lo(value),
            0x3d34 => self.uart.set_baud_hi(value),
            0x3d35 => self.uart.tx(value),

            // ---- DMA ----
            0x3e00 => self.dma.set_source_lo(value),
            0x3e01 => self.dma.set_source_hi(value),
            0x3e02 => self.start_dma(value),
            0x3e03 => self.dma.set_target(value),

            // ---- memória externa ----
            0x4000..=0x3fffff => {
                let ext = &self.extmem;
                ext.write(&mut self.io, addr, value);
            }

            // ---- SPU e outros ainda não implementados (stub) ----
            0x3000..=0x3fff => self.unimplemented(),

            _ => {}
        }
    }
}

impl<Io: SystemIo> Spg200Mem<Io> {
    /// Executa o DMA genérico: copia `length` palavras de `source` para
    /// `target`, percorrendo o barramento completo.
    fn start_dma(&mut self, length: u16) {
        self.dma.length = length;
        while self.dma.length != 0 {
            let word = self.read_word(self.dma.source);
            self.dma.source = self.dma.source.wrapping_add(1);
            let target = self.dma.target as u32;
            self.write_word(target, word);
            self.dma.target = self.dma.target.wrapping_add(1) & 0x3fff;
            self.dma.length -= 1;
        }
    }
}

impl<Io: SystemIo + crate::save::State> crate::save::State for Spg200Mem<Io> {
    fn save(&self, w: &mut Vec<u8>) {
        self.ram.save(w);
        self.io.save(w);
        self.ppu.save(w);
        self.spu.save(w);
        self.irq.save(w);
        self.timer.save(w);
        self.uart.save(w);
        self.adc.save(w);
        self.gpio.save(w);
        self.dma.save(w);
        self.watchdog.save(w);
        self.extmem.save(w);
        self.system_ctrl.save(w);
        self.random1.save(w);
        self.random2.save(w);
        self.ds_shadow.save(w);
        self.ds_written.save(w);
    }
    fn load(&mut self, r: &mut crate::save::Reader) -> Result<(), String> {
        self.ram.load(r)?;
        self.io.load(r)?;
        self.ppu.load(r)?;
        self.spu.load(r)?;
        self.irq.load(r)?;
        self.timer.load(r)?;
        self.uart.load(r)?;
        self.adc.load(r)?;
        self.gpio.load(r)?;
        self.dma.load(r)?;
        self.watchdog.load(r)?;
        self.extmem.load(r)?;
        self.system_ctrl.load(r)?;
        self.random1.load(r)?;
        self.random2.load(r)?;
        self.ds_shadow.load(r)?;
        self.ds_written.load(r)?;
        Ok(())
    }
}
