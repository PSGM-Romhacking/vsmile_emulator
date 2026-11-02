//! Topo do SoC SPG200: junta a CPU unSP ao barramento ([`Spg200Mem`]) e aos
//! periféricos, executando o sistema instrução a instrução.
//!
//! A cada passo:
//! 1. sincroniza o segmento de dados (DS) entre a CPU e o registrador `0x3D2F`;
//! 2. executa uma instrução da CPU (que lê/escreve no barramento);
//! 3. avança os periféricos (I/O externo, PPU; demais a caminho) pelos ciclos
//!    consumidos;
//! 4. empurra as 8 linhas de IRQ e a linha de FIQ de volta para a CPU.
//!
//! Essa organização mantém CPU e barramento como campos **separados**, então
//! `cpu.step(&mut mem)` usa empréstimos disjuntos — sem referência cíclica.

use crate::cpu::Cpu;
use crate::io::SystemIo;
use crate::memory::{Spg200Mem, VideoTiming};

/// O SoC SPG200 completo (CPU + barramento + periféricos).
pub struct Spg200<Io: SystemIo> {
    pub cpu: Cpu,
    pub mem: Spg200Mem<Io>,
    cycles_in_frame: u32,
}

impl<Io: SystemIo> Spg200<Io> {
    pub fn new(io: Io, video_timing: VideoTiming) -> Self {
        Self {
            cpu: Cpu::new(),
            mem: Spg200Mem::new(io, video_timing),
            cycles_in_frame: 0,
        }
    }

    /// Reinicia o SoC: barramento primeiro (para o vetor de reset estar
    /// disponível), depois a CPU.
    pub fn reset(&mut self) {
        self.mem.reset();
        self.cpu.reset(&mut self.mem);
    }

    /// Executa uma instrução e avança os periféricos. Retorna `true` quando um
    /// quadro de vídeo completo acabou de ser desenhado.
    pub fn step(&mut self) -> bool {
        // DS: o registrador 0x3D2F espelha o DS da CPU. Só reescrevemos o DS na
        // CPU se o jogo realmente escreveu em 0x3D2F — caso contrário
        // clobbaríamos mudanças de SR feitas pela própria CPU (IRQ/reti).
        self.mem.sync_ds_from_cpu(self.cpu.ds());
        let cycles = self.cpu.step(&mut self.mem);
        if let Some(ds) = self.mem.ds_to_apply() {
            self.cpu.set_ds(ds);
        }

        // Periféricos (I/O externo, ADC, UART, timers). SPU a caminho.
        self.mem.run_peripherals(cycles);

        let frame_done = self.mem.run_ppu(cycles);

        // Entrega o estado das interrupções para a CPU.
        let (irq_bits, fiq) = self.mem.irq_lines();
        for i in 0..8u32 {
            self.cpu.set_irq(i, irq_bits & (1 << i) != 0);
        }
        self.cpu.set_fiq(fiq);

        // Watchdog é verificado ao fim de cada quadro.
        self.cycles_in_frame = self.cycles_in_frame.wrapping_add(cycles);
        if frame_done {
            if self.mem.run_watchdog(self.cycles_in_frame) {
                self.cpu.reset(&mut self.mem);
            }
            self.cycles_in_frame = 0;
        }

        frame_done
    }

    /// Executa até o fim do quadro corrente.
    pub fn run_frame(&mut self) {
        while !self.step() {}
    }

    /// Framebuffer corrente (RGB555, 320×240).
    pub fn framebuffer(&self) -> &[u16] {
        self.mem.ppu.framebuffer()
    }

    /// Esvazia o buffer de áudio (amostras estéreo L,R intercaladas, ~281 kHz).
    pub fn take_audio(&mut self) -> &[u16] {
        self.mem.take_audio()
    }

    /// Número de quadros desenhados desde o reset.
    pub fn frame_counter(&self) -> i64 {
        self.mem.ppu.frame_counter()
    }
}

impl<Io: SystemIo + crate::save::State> crate::save::State for Spg200<Io> {
    fn save(&self, w: &mut Vec<u8>) {
        self.cpu.save(w);
        self.mem.save(w);
        self.cycles_in_frame.save(w);
    }
    fn load(&mut self, r: &mut crate::save::Reader) -> Result<(), String> {
        self.cpu.load(r)?;
        self.mem.load(r)?;
        self.cycles_in_frame.load(r)?;
        Ok(())
    }
}
