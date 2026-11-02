//! Depurador do núcleo: breakpoints de execução, watchpoints de RAM, passo a
//! passo e inspeção — tudo sobre o [`Spg200`], instrução a instrução.
//!
//! É só a *mecânica* do depurador (sem I/O): guarda os pontos de parada e dirige
//! o `Spg200::step`. O frontend (REPL do `vsmile-run --debug`) é uma casca fina
//! por cima daqui, então toda a lógica fica testável *headless*.

use std::collections::BTreeSet;

use crate::bus::Bus;
use crate::disasm::{disassemble, Insn};
use crate::io::SystemIo;
use crate::spg::Spg200;

/// Motivo pelo qual a execução parou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// Prestes a executar a instrução em `CS:PC` marcada como breakpoint.
    Breakpoint(u32),
    /// Uma palavra de RAM vigiada mudou de valor.
    Watchpoint { addr: u32, old: u16, new: u16 },
    /// Atingiu o limite de instruções sem parar antes (`continue` esgotado).
    Limit,
}

/// Uma linha de desmontagem resolvida para exibição.
pub struct DisasmLine {
    pub addr: u32,
    pub word: u16,
    pub next: u16,
    pub insn: Insn,
}

/// Estado do depurador: conjuntos de breakpoints (endereços `CS:PC`) e
/// watchpoints (endereços de RAM, em palavras).
#[derive(Default)]
pub struct Debugger {
    breakpoints: BTreeSet<u32>,
    watchpoints: BTreeSet<u32>,
    /// Total de instruções executadas desde a criação (diagnóstico).
    pub total_steps: u64,
}

impl Debugger {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- breakpoints ----------------------------------------------------

    /// Adiciona um breakpoint; devolve `true` se era novo.
    pub fn add_breakpoint(&mut self, addr: u32) -> bool {
        self.breakpoints.insert(addr)
    }
    /// Remove um breakpoint; devolve `true` se existia.
    pub fn remove_breakpoint(&mut self, addr: u32) -> bool {
        self.breakpoints.remove(&addr)
    }
    pub fn breakpoints(&self) -> impl Iterator<Item = u32> + '_ {
        self.breakpoints.iter().copied()
    }

    // ---- watchpoints (RAM) ----------------------------------------------

    /// Adiciona um watchpoint de RAM; devolve `true` se era novo.
    pub fn add_watchpoint(&mut self, addr: u32) -> bool {
        self.watchpoints.insert(addr)
    }
    pub fn remove_watchpoint(&mut self, addr: u32) -> bool {
        self.watchpoints.remove(&addr)
    }
    pub fn watchpoints(&self) -> impl Iterator<Item = u32> + '_ {
        self.watchpoints.iter().copied()
    }

    // ---- execução -------------------------------------------------------

    /// Executa **uma** instrução. Devolve `Some(Stop::Watchpoint)` se alguma
    /// palavra vigiada mudou nesse passo, senão `None`.
    pub fn step<Io: SystemIo>(&mut self, spg: &mut Spg200<Io>) -> Option<Stop> {
        // Fotografa as palavras vigiadas antes de executar.
        let before: Vec<(u32, u16)> = self
            .watchpoints
            .iter()
            .map(|&a| (a, ram_peek(spg, a)))
            .collect();

        spg.step();
        self.total_steps += 1;

        for (a, old) in before {
            let new = ram_peek(spg, a);
            if new != old {
                return Some(Stop::Watchpoint { addr: a, old, new });
            }
        }
        None
    }

    /// Continua executando até bater num breakpoint/watchpoint ou esgotar
    /// `max_insns`. A instrução corrente é sempre executada (para que um
    /// `continue` parado sobre um breakpoint não trave nele).
    pub fn run<Io: SystemIo>(&mut self, spg: &mut Spg200<Io>, max_insns: u64) -> Stop {
        for i in 0..max_insns {
            if i > 0 {
                let pc = spg.cpu.cs_pc();
                if self.breakpoints.contains(&pc) {
                    return Stop::Breakpoint(pc);
                }
            }
            if let Some(stop) = self.step(spg) {
                return stop;
            }
        }
        Stop::Limit
    }

    // ---- inspeção -------------------------------------------------------

    /// Desmonta `count` instruções a partir de `addr` (endereço `CS:PC`),
    /// lendo as palavras pelo barramento (válido para regiões de código).
    pub fn disassemble<Io: SystemIo>(
        &self,
        spg: &mut Spg200<Io>,
        addr: u32,
        count: u32,
    ) -> Vec<DisasmLine> {
        let mut out = Vec::with_capacity(count as usize);
        let mut a = addr;
        for _ in 0..count {
            let word = spg.mem.read_word(a);
            let next = spg.mem.read_word(a.wrapping_add(1));
            let insn = disassemble(word, next, a);
            let words = insn.words;
            out.push(DisasmLine {
                addr: a,
                word,
                next,
                insn,
            });
            a = a.wrapping_add(words);
        }
        out
    }

    /// Lê `count` palavras de RAM a partir de `addr` (em palavras).
    pub fn read_ram<Io: SystemIo>(spg: &Spg200<Io>, addr: u32, count: u32) -> Vec<u16> {
        (0..count)
            .map(|i| ram_peek(spg, addr.wrapping_add(i)))
            .collect()
    }
}

/// Leitura **sem efeitos colaterais** de uma palavra de RAM (fora da RAM,
/// devolve 0). Essencial para watchpoints: ler pelo barramento poderia limpar
/// registradores de status.
fn ram_peek<Io: SystemIo>(spg: &Spg200<Io>, addr: u32) -> u16 {
    let i = addr as usize;
    if i < spg.mem.ram.len() {
        spg.mem.ram[i]
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::VideoTiming;
    use crate::vsmile::{dummy_sysrom, VSmile};

    /// Programa: grava 0x1234 em [0x0040] e entra num laço fechado em 0x8004.
    fn cart() -> Vec<u16> {
        let mut rom = vec![0xFFFFu16; 0x10000];
        rom[0xfff7] = 0x8000; // vetor de reset
        rom[0x8000] = 0x9309; // LOAD R1, imm16
        rom[0x8001] = 0x1234;
        rom[0x8002] = 0xD119; // STORE R1 -> [imm16]
        rom[0x8003] = 0x0040;
        rom[0x8004] = 0xEE41; // jmp self (laço)
        rom
    }

    fn console() -> VSmile {
        let mut v = VSmile::new(dummy_sysrom(), cart(), 0xe, true, VideoTiming::Ntsc);
        v.reset();
        v
    }

    #[test]
    fn breakpoint_para_antes_de_executar() {
        let mut v = console();
        let mut dbg = Debugger::new();
        assert!(dbg.add_breakpoint(0x8002));
        let stop = dbg.run(&mut v.spg, 100);
        assert_eq!(stop, Stop::Breakpoint(0x8002));
        // Parou ANTES do STORE: a RAM ainda não foi escrita.
        assert_eq!(v.spg.mem.ram[0x0040], 0x0000);
        assert_eq!(v.spg.cpu.cs_pc(), 0x8002);
    }

    #[test]
    fn watchpoint_dispara_na_escrita() {
        let mut v = console();
        let mut dbg = Debugger::new();
        dbg.add_watchpoint(0x0040);
        let stop = dbg.run(&mut v.spg, 100);
        assert_eq!(
            stop,
            Stop::Watchpoint {
                addr: 0x0040,
                old: 0x0000,
                new: 0x1234
            }
        );
        assert_eq!(v.spg.mem.ram[0x0040], 0x1234);
    }

    #[test]
    fn continue_sem_pontos_de_parada_esgota_o_limite() {
        let mut v = console();
        let mut dbg = Debugger::new();
        assert_eq!(dbg.run(&mut v.spg, 50), Stop::Limit);
        assert_eq!(dbg.total_steps, 50);
    }

    #[test]
    fn step_executa_uma_instrucao() {
        let mut v = console();
        let mut dbg = Debugger::new();
        assert_eq!(dbg.step(&mut v.spg), None); // LOAD (2 palavras)
        assert_eq!(v.spg.cpu.cs_pc(), 0x8002);
        assert_eq!(v.spg.cpu.regs()[1], 0x1234);
    }

    #[test]
    fn disassemble_resolve_o_codigo() {
        let mut v = console();
        let dbg = Debugger::new();
        let lines = dbg.disassemble(&mut v.spg, 0x8000, 3);
        assert_eq!(lines[0].insn.text, "R1 = #0x1234");
        assert_eq!(lines[1].insn.text, "[0x0040] = R1");
        assert_eq!(lines[2].insn.text, "JMP 0x008004");
    }
}
