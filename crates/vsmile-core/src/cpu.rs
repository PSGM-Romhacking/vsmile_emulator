//! CPU **unSP** (SunPlus) de 16 bits — núcleo de execução.
//!
//! A unSP tem 8 registradores de 16 bits. Alguns têm papel fixo:
//!
//! | Índice | Nome | Papel                                    |
//! |--------|------|------------------------------------------|
//! | 0      | SP   | stack pointer                            |
//! | 1..4   | R1-R4| uso geral                                |
//! | 5      | BP   | base pointer                             |
//! | 6      | SR   | status: flags N/Z/S/C + segmentos DS/CS  |
//! | 7      | PC   | program counter (16 bits baixos de CS:PC)|
//!
//! O endereçamento efetivo é de 22 bits, formado por `CS:PC` (execução) e
//! `DS:Rs` (dados). Esta implementação segue o comportamento observável do
//! hardware; o ciclo retornado por [`Cpu::step`] é a estimativa de ciclos da
//! instrução, usada para sincronizar os periféricos.

use crate::bus::Bus;

// Índices dos registradores especiais.
const REG_SP: usize = 0;
const REG_R3: usize = 3;
const REG_R4: usize = 4;
const REG_BP: usize = 5;
const REG_SR: usize = 6;
const REG_PC: usize = 7;

// Bits das flags dentro de SR.
const FLAG_N: u16 = 1 << 9;
const FLAG_Z: u16 = 1 << 8;
const FLAG_S: u16 = 1 << 7;
const FLAG_C: u16 = 1 << 6;

// Operações da ALU (campo op0 da instrução).
const ALUOP_ADD: u32 = 0;
const ALUOP_ADC: u32 = 1;
const ALUOP_SUB: u32 = 2;
const ALUOP_SBC: u32 = 3;
const ALUOP_CMP: u32 = 4;
const ALUOP_NEG: u32 = 6;
const ALUOP_XOR: u32 = 8;
const ALUOP_LOAD: u32 = 9;
const ALUOP_OR: u32 = 10;
const ALUOP_AND: u32 = 11;
const ALUOP_TEST: u32 = 12;
const ALUOP_STORE: u32 = 13;

/// Núcleo da CPU unSP.
pub struct Cpu {
    /// Registradores r0..r7 (SP, R1..R4, BP, SR, PC).
    regs: [u16; 8],
    /// Shift bits (nibble de extensão dos shifts), um por modo:
    /// `[normal, irq, fiq]`.
    sb: [u8; 3],

    /// Linhas de IRQ pendentes (8 fontes), uma por bit.
    irq_signal: u8,
    /// Linha de FIQ pendente.
    fiq_signal: bool,

    /// Dentro de um handler de IRQ / FIQ.
    irq_active: bool,
    fiq_active: bool,
    /// Habilitação global de IRQ / FIQ.
    irq_enable: bool,
    fiq_enable: bool,
    /// Modo "FIR_MOV" dos multiplica-acumula (muls): quando ligado, a operação
    /// reescreve o operando `rd` com o valor lido no passo anterior.
    fir_mov: bool,

    /// Contador de instruções não reconhecidas (tratadas como NOP). Útil para
    /// diagnóstico; idealmente deve permanecer 0 ao rodar software real.
    pub unknown_ops: u64,

    /// Diagnóstico: palavra da instrução corrente e a primeira instrução
    /// desconhecida encontrada (CS:PC pós-fetch, palavra da instrução).
    cur_iw: u16,
    cur_pc: u32,
    pub dbg_first_unknown: Option<(u32, u16)>,
    /// Primeiro salto de controle para um banco "alto" (cs >= 8): (pc da
    /// instrução, palavra, alvo). Ajuda a achar saltos para lixo.
    pub dbg_bigjump: Option<(u32, u16, u32)>,
    /// Anel das últimas 16 instruções (CS:PC, iw) executadas — para rastrear o
    /// fluxo que leva a uma divergência (execução de lixo).
    dbg_ring: [(u32, u16); 16],
    dbg_ring_pos: usize,
    /// Instantâneo do anel no momento da 1ª instrução desconhecida.
    pub dbg_pre_unknown: Option<[(u32, u16); 16]>,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    /// Cria uma CPU zerada (ainda não resetada contra um barramento).
    pub fn new() -> Self {
        Self {
            regs: [0; 8],
            sb: [0; 3],
            irq_signal: 0,
            fiq_signal: false,
            irq_active: false,
            fiq_active: false,
            irq_enable: false,
            fiq_enable: false,
            fir_mov: true,
            unknown_ops: 0,
            cur_iw: 0,
            cur_pc: 0,
            dbg_first_unknown: None,
            dbg_bigjump: None,
            dbg_ring: [(0, 0); 16],
            dbg_ring_pos: 0,
            dbg_pre_unknown: None,
        }
    }

    /// Reset do processador: zera o estado e carrega o PC a partir do vetor de
    /// reset em `0xFFF7`.
    pub fn reset<B: Bus>(&mut self, bus: &mut B) {
        self.regs = [0; 8];
        self.sb = [0; 3];
        self.irq_signal = 0;
        self.fiq_signal = false;
        self.irq_active = false;
        self.fiq_active = false;
        self.irq_enable = false;
        self.fiq_enable = false;
        self.fir_mov = true;

        self.regs[REG_PC] = bus.read_word(0xfff7);
    }

    /// Sinaliza (ou limpa) a linha de IRQ número `irq` (0..=7).
    pub fn set_irq(&mut self, irq: u32, value: bool) {
        let bit = 1u8 << irq;
        if value {
            self.irq_signal |= bit;
        } else {
            self.irq_signal &= !bit;
        }
    }

    /// Sinaliza (ou limpa) a linha de FIQ.
    pub fn set_fiq(&mut self, value: bool) {
        self.fiq_signal = value;
    }

    /// Endereço completo de execução `CS:PC` (22 bits).
    pub fn cs_pc(&self) -> u32 {
        (self.cs() << 16) | self.regs[REG_PC] as u32
    }

    /// Valor do segmento de dados `DS`.
    pub fn ds(&self) -> u16 {
        (self.regs[REG_SR] >> 10) & 0x3f
    }

    /// Define o segmento de dados `DS`.
    pub fn set_ds(&mut self, val: u16) {
        let sr = self.regs[REG_SR] & !(0x3f << 10);
        self.regs[REG_SR] = sr | ((val & 0x3f) << 10);
    }

    // ---- flags / segmentos (acesso a SR) --------------------------------

    #[inline]
    fn cs(&self) -> u32 {
        (self.regs[REG_SR] & 0x3f) as u32
    }

    #[inline]
    fn set_cs(&mut self, val: u32) {
        let sr = self.regs[REG_SR] & !0x3f;
        self.regs[REG_SR] = sr | (val as u16 & 0x3f);
    }

    #[inline]
    fn flag(&self, bit: u16) -> bool {
        self.regs[REG_SR] & bit != 0
    }

    #[inline]
    fn set_flag(&mut self, bit: u16, v: bool) {
        if v {
            self.regs[REG_SR] |= bit;
        } else {
            self.regs[REG_SR] &= !bit;
        }
    }

    #[inline]
    fn set_cs_pc(&mut self, val: u32) {
        if self.dbg_bigjump.is_none() && (val >> 16) >= 8 {
            self.dbg_bigjump = Some((self.cur_pc, self.cur_iw, val));
        }
        self.regs[REG_PC] = (val & 0xffff) as u16;
        self.set_cs(val >> 16);
    }

    /// Nibble de shift ativo para o modo de execução corrente.
    #[inline]
    fn sb_index(&self) -> usize {
        if self.fiq_active {
            2
        } else if self.irq_active {
            1
        } else {
            0
        }
    }

    // ---- acesso à memória relativo ao PC e à pilha ----------------------

    #[inline]
    fn read_word_from_pc<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let cs_pc = self.cs_pc();
        let val = bus.read_word(cs_pc);
        self.set_cs_pc(cs_pc.wrapping_add(1));
        val
    }

    /// Empilha `val` usando o registrador `sp_idx` como stack pointer
    /// (pós-decremento).
    #[inline]
    fn push_word<B: Bus>(&mut self, bus: &mut B, sp_idx: usize, val: u16) {
        bus.write_word(self.regs[sp_idx] as u32, val);
        self.regs[sp_idx] = self.regs[sp_idx].wrapping_sub(1);
    }

    /// Desempilha usando `sp_idx` como stack pointer (pré-incremento).
    #[inline]
    fn pop_word<B: Bus>(&mut self, bus: &mut B, sp_idx: usize) -> u16 {
        self.regs[sp_idx] = self.regs[sp_idx].wrapping_add(1);
        bus.read_word(self.regs[sp_idx] as u32)
    }

    // ---- interrupções ---------------------------------------------------

    fn check_interrupts<B: Bus>(&mut self, bus: &mut B) -> bool {
        if self.fiq_signal && !self.fiq_active && self.fiq_enable {
            self.fiq_active = true;
            let pc = self.regs[REG_PC];
            let sr = self.regs[REG_SR];
            self.push_word(bus, REG_SP, pc);
            self.push_word(bus, REG_SP, sr);
            self.regs[REG_PC] = bus.read_word(0xfff6);
            self.regs[REG_SR] = 0;
            true
        } else if self.irq_signal != 0 && !self.irq_active && self.irq_enable {
            let irq_to_run = self.irq_signal.trailing_zeros();
            self.irq_active = true;
            let pc = self.regs[REG_PC];
            let sr = self.regs[REG_SR];
            self.push_word(bus, REG_SP, pc);
            self.push_word(bus, REG_SP, sr);
            self.regs[REG_PC] = bus.read_word(0xfff8 + irq_to_run);
            self.regs[REG_SR] = 0;
            true
        } else {
            false
        }
    }

    /// Executa uma instrução. Retorna o número estimado de ciclos consumidos.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        if self.check_interrupts(bus) {
            return 10;
        }

        self.cur_pc = self.cs_pc();
        let iw = self.read_word_from_pc(bus);
        self.cur_iw = iw;
        self.dbg_ring[self.dbg_ring_pos] = (self.cur_pc, iw);
        self.dbg_ring_pos = (self.dbg_ring_pos + 1) % 16;
        let op0 = ((iw >> 12) & 0xf) as u32;
        let rd = ((iw >> 9) & 0x7) as usize;
        let op1 = ((iw >> 6) & 0x7) as u32;
        let op1n = ((iw >> 3) & 0x3f) as u32;
        let muls_n = ((iw >> 3) & 0xf) as u32;
        let opn = ((iw >> 3) & 0x7) as u32;
        let rs = (iw & 0x7) as usize;
        let imm6 = (iw & 0x3f) as u32;

        if op0 == 0xf {
            self.exec_group_f(bus, op1, rd, rs, opn, muls_n, imm6)
        } else {
            self.exec_alu_branch(bus, op0, rd, rs, op1n, opn, imm6)
        }
    }

    /// Grupo de opcodes com `op0 == 0xf`: multiplicações, call, goto, muls,
    /// controle de interrupções e nop/break.
    fn exec_group_f<B: Bus>(
        &mut self,
        bus: &mut B,
        op1: u32,
        rd: usize,
        rs: usize,
        opn: u32,
        muls_n: u32,
        imm6: u32,
    ) -> u32 {
        match op1 {
            0 => {
                // mul (unsigned * signed)
                if rd != REG_PC && rs != REG_PC && opn == 1 {
                    let result =
                        (self.regs[rd] as u32).wrapping_mul(self.regs[rs] as i16 as u32);
                    self.regs[REG_R3] = (result & 0xffff) as u16;
                    self.regs[REG_R4] = ((result >> 16) & 0xffff) as u16;
                    12
                } else {
                    self.unknown("mul")
                }
            }
            1 => {
                // call
                let hi = imm6 << 16;
                let lo = self.read_word_from_pc(bus) as u32;
                let new_pc = hi | lo;
                let pc = self.regs[REG_PC];
                let sr = self.regs[REG_SR];
                self.push_word(bus, REG_SP, pc);
                self.push_word(bus, REG_SP, sr);
                self.set_cs_pc(new_pc);
                9
            }
            2 if rd == REG_PC => {
                // goto
                let hi = imm6 << 16;
                let lo = self.read_word_from_pc(bus) as u32;
                self.set_cs_pc(hi | lo);
                5
            }
            // muls (unsigned * signed) — FIR filter multiply-accumulate.
            2 | 3 => {
                if rd != REG_PC && rs != REG_PC {
                    self.muls(bus, rd, rs, muls_n, false)
                } else {
                    self.unknown("muls us")
                }
            }
            4 => {
                // mul (signed * signed)
                if rd != REG_PC && rs != REG_PC && opn == 1 {
                    let result =
                        (self.regs[rd] as i16 as u32).wrapping_mul(self.regs[rs] as i16 as u32);
                    self.regs[REG_R3] = (result & 0xffff) as u16;
                    self.regs[REG_R4] = ((result >> 16) & 0xffff) as u16;
                    12
                } else {
                    self.unknown("mul ss")
                }
            }
            5 => self.exec_misc(bus, imm6),
            6 | 7 => {
                // muls (signed * signed)
                if rd != REG_PC && rs != REG_PC {
                    self.muls(bus, rd, rs, muls_n, true)
                } else {
                    self.unknown("muls ss")
                }
            }
            _ => self.unknown("group f"),
        }
    }

    /// Multiply-accumulate dos filtros FIR (`muls`). `signed_rd` indica se o
    /// primeiro operando também é tratado como sinalizado.
    fn muls<B: Bus>(&mut self, bus: &mut B, rd: usize, rs: usize, muls_n: u32, signed_rd: bool) -> u32 {
        let n = if muls_n != 0 { muls_n } else { 16 };
        let mut sum: i64 = 0;
        let mut old_val1: u16 = 0;

        for i in 0..n {
            let val1 = bus.read_word(self.regs[rd] as u32);
            let val2 = bus.read_word(self.regs[rs] as u32);

            let a = if signed_rd {
                val1 as i16 as i64
            } else {
                val1 as i64
            };
            sum += a * (val2 as i16 as i64);

            if self.fir_mov {
                if i > 0 {
                    bus.write_word(self.regs[rd] as u32, old_val1);
                }
                old_val1 = val1;
            }

            self.regs[rd] = self.regs[rd].wrapping_add(1);
            self.regs[rs] = self.regs[rs].wrapping_add(1);
        }

        self.regs[REG_R3] = (sum & 0xffff) as u16;
        self.regs[REG_R4] = ((sum >> 16) & 0xffff) as u16;
        10 * n + 6
    }

    /// Subgrupo `op1 == 5`: controle de IRQ/FIQ, FIR_MOV, break e nop.
    fn exec_misc<B: Bus>(&mut self, bus: &mut B, imm6: u32) -> u32 {
        match imm6 {
            0..=3 => {
                self.irq_enable = imm6 & 1 != 0;
                self.fiq_enable = imm6 & 2 != 0;
                2
            }
            4..=5 => {
                self.fir_mov = imm6 & 1 == 0;
                2
            }
            8..=9 => {
                self.irq_enable = imm6 & 1 != 0;
                2
            }
            12 | 14 => {
                self.fiq_enable = imm6 & 2 != 0;
                2
            }
            32 | 40 | 48 | 56 => {
                // break
                let pc = self.regs[REG_PC];
                let sr = self.regs[REG_SR];
                self.push_word(bus, REG_SP, pc);
                self.push_word(bus, REG_SP, sr);
                self.regs[REG_PC] = bus.read_word(0xfff5);
                self.regs[REG_SR] = 0;
                10
            }
            37 => 2, // nop
            _ => self.unknown("misc"),
        }
    }

    /// Operações de ALU e desvios condicionais (`op0 != 0xf`).
    fn exec_alu_branch<B: Bus>(
        &mut self,
        bus: &mut B,
        op0: u32,
        rd: usize,
        rs: usize,
        op1n: u32,
        opn: u32,
        imm6: u32,
    ) -> u32 {
        match op1n {
            // [bp+imm6], ou branch para frente se rd == PC.
            0..=7 => {
                if rd == REG_PC {
                    let do_branch = self.check_branch(op0);
                    if do_branch {
                        let cs_pc = self.cs_pc();
                        self.set_cs_pc(cs_pc.wrapping_add(imm6));
                    }
                    if do_branch {
                        4
                    } else {
                        2
                    }
                } else {
                    let addr = self.regs[REG_BP] as u32 + imm6;
                    if op0 != ALUOP_STORE {
                        let value = bus.read_word(addr);
                        self.alu_to(op0, rd, self.regs[rd], value);
                    } else {
                        bus.write_word(addr, self.regs[rd]);
                    }
                    6
                }
            }
            // imm6, ou branch para trás se rd == PC.
            8..=15 => {
                if rd == REG_PC {
                    let do_branch = self.check_branch(op0);
                    if do_branch {
                        let cs_pc = self.cs_pc();
                        self.set_cs_pc(cs_pc.wrapping_sub(imm6));
                    }
                    if do_branch {
                        4
                    } else {
                        2
                    }
                } else {
                    debug_assert!(op0 != ALUOP_STORE, "store com endereçamento imediato");
                    self.alu_to(op0, rd, self.regs[rd], imm6 as u16);
                    2
                }
            }
            // push / pop (e reti via pseudo-op).
            16..=23 => self.exec_push_pop(bus, op0, rd, rs, opn),
            // [Rs] com pré/pós inc/dec, opcionalmente com DS.
            24..=31 => {
                let addr = self.resolve_indirect(op1n, rs);
                if op0 != ALUOP_STORE {
                    let value = bus.read_word(addr);
                    self.alu_to(op0, rd, self.regs[rd], value);
                } else {
                    bus.write_word(addr, self.regs[rd]);
                }
                if rd == REG_PC {
                    7
                } else {
                    6
                }
            }
            // registrador direto.
            32 => {
                debug_assert!(op0 != ALUOP_STORE, "store em modo registrador");
                self.alu_to(op0, rd, self.regs[rd], self.regs[rs]);
                if rd == REG_PC {
                    5
                } else {
                    3
                }
            }
            // imm16.
            33 => {
                let rs_val = self.regs[rs];
                let imm = self.read_word_from_pc(bus);
                debug_assert!(op0 != ALUOP_STORE, "store em modo imediato");
                self.alu_to(op0, rd, rs_val, imm);
                if rd == REG_PC {
                    5
                } else {
                    4
                }
            }
            // [imm16] leitura.
            34 => {
                let rs_val = self.regs[rs];
                let addr = self.read_word_from_pc(bus) as u32;
                debug_assert!(op0 != ALUOP_STORE, "store em modo [imm16] de leitura");
                let value = bus.read_word(addr);
                self.alu_to(op0, rd, rs_val, value);
                if rd == REG_PC {
                    8
                } else {
                    7
                }
            }
            // [imm16] escrita.
            35 => {
                let rs_val = self.regs[rs];
                let rd_val = self.regs[rd];
                let addr = self.read_word_from_pc(bus) as u32;
                if op0 != ALUOP_STORE {
                    let result = self.alu(op0, rs_val, rd_val, rd != REG_PC);
                    bus.write_word(addr, result);
                } else {
                    bus.write_word(addr, rs_val);
                }
                if rd == REG_PC {
                    8
                } else {
                    7
                }
            }
            // registrador com shift/rotate (asr/lsl/lsr/rol/ror).
            36..=55 => {
                let value = self.shift_rotate(op1n, opn, rs);
                debug_assert!(op0 != ALUOP_STORE, "store em modo shift");
                self.alu_to(op0, rd, self.regs[rd], value);
                if rd == REG_PC {
                    5
                } else {
                    3
                }
            }
            // [A6] — endereçamento direto de 6 bits (página zero).
            56..=63 => {
                if op0 != ALUOP_STORE {
                    let value = bus.read_word(imm6);
                    self.alu_to(op0, rd, self.regs[rd], value);
                } else {
                    bus.write_word(imm6, self.regs[rd]);
                }
                if rd == REG_PC {
                    6
                } else {
                    5
                }
            }
            _ => self.unknown("alu/branch"),
        }
    }

    /// Resolve o endereço efetivo dos modos indiretos `[Rs]` (op1n 24..=31).
    fn resolve_indirect(&mut self, op1n: u32, rs: usize) -> u32 {
        match op1n {
            24 => self.regs[rs] as u32,
            25 => {
                let a = self.regs[rs] as u32;
                self.regs[rs] = self.regs[rs].wrapping_sub(1);
                a
            }
            26 => {
                let a = self.regs[rs] as u32;
                self.regs[rs] = self.regs[rs].wrapping_add(1);
                a
            }
            27 => {
                self.regs[rs] = self.regs[rs].wrapping_add(1);
                self.regs[rs] as u32
            }
            28 => (self.ds() as u32) << 16 | self.regs[rs] as u32,
            29 => {
                let a = (self.ds() as u32) << 16 | self.regs[rs] as u32;
                self.regs[rs] = self.regs[rs].wrapping_sub(1);
                if self.regs[rs] == 0xFFFF {
                    self.set_ds(self.ds().wrapping_sub(1));
                }
                a
            }
            30 => {
                let a = (self.ds() as u32) << 16 | self.regs[rs] as u32;
                self.regs[rs] = self.regs[rs].wrapping_add(1);
                if self.regs[rs] == 0x0000 {
                    self.set_ds(self.ds().wrapping_add(1));
                }
                a
            }
            31 => {
                self.regs[rs] = self.regs[rs].wrapping_add(1);
                if self.regs[rs] == 0x0000 {
                    self.set_ds(self.ds().wrapping_add(1));
                }
                (self.ds() as u32) << 16 | self.regs[rs] as u32
            }
            _ => unreachable!(),
        }
    }

    /// Calcula o valor dos modos de shift/rotate e atualiza o nibble `sb`.
    fn shift_rotate(&mut self, op1n: u32, opn: u32, rs: usize) -> u16 {
        let idx = self.sb_index();
        let cur_sb = self.sb[idx] as u32;
        let n = (opn & 0x03) + 1;
        let rs_val = self.regs[rs] as u32;

        let (value, new_sb) = match op1n {
            36..=39 => {
                // asr — arithmetic shift right de um valor de 20 bits.
                let signed = sext20((rs_val << 4) | cur_sb) >> n;
                (((signed >> 4) & 0xffff) as u16, (signed & 0xf) as u32)
            }
            40..=43 => {
                // lsl
                let shift = ((cur_sb << 16) | rs_val) << n;
                ((shift & 0xffff) as u16, (shift >> 16) & 0xf)
            }
            44..=47 => {
                // lsr
                let shift = ((rs_val << 4) | cur_sb) >> n;
                (((shift >> 4) & 0xffff) as u16, shift & 0xf)
            }
            48..=51 => {
                // rol
                let shift = rotl20((cur_sb << 16) | rs_val, n);
                ((shift & 0xffff) as u16, (shift >> 16) & 0xf)
            }
            52..=55 => {
                // ror
                let shift = rotr20((rs_val << 4) | cur_sb, n);
                (((shift >> 4) & 0xffff) as u16, shift & 0xf)
            }
            _ => unreachable!(),
        };

        self.sb[idx] = new_sb as u8;
        value
    }

    /// push (STORE) / pop (LOAD) de múltiplos registradores, mais o pseudo-op
    /// `reti`.
    fn exec_push_pop<B: Bus>(&mut self, bus: &mut B, op0: u32, rd: usize, rs: usize, opn: u32) -> u32 {
        let mut n = opn as i32;
        let mut reg = rd as i32;

        match op0 {
            ALUOP_LOAD => {
                // pop / reti (pseudo-op)
                if rd == REG_BP && opn == 3 && rs == REG_SP {
                    if self.fiq_active {
                        self.fiq_active = false;
                    } else if self.irq_active {
                        self.irq_active = false;
                    }
                    n = 2; // demais registradores são restaurados normalmente
                }

                let mut left = n;
                while left > 0 && (reg + 1) <= 7 {
                    reg += 1;
                    let v = self.pop_word(bus, rs);
                    self.regs[reg as usize] = v;
                    left -= 1;
                }
            }
            ALUOP_STORE => {
                let mut left = n;
                while left > 0 && reg >= 0 {
                    let v = self.regs[reg as usize];
                    self.push_word(bus, rs, v);
                    reg -= 1;
                    left -= 1;
                }
            }
            _ => return self.unknown("push/pop alu op"),
        }

        // `n` pode ter sido reduzido para 2 no caso reti.
        2 * n as u32 + 4
    }

    // ---- ALU ------------------------------------------------------------

    /// Executa uma operação de ALU e grava o resultado em `regs[rd]` quando a
    /// operação produz escrita (tudo exceto CMP e TEST).
    #[inline]
    fn alu_to(&mut self, op: u32, rd: usize, val1: u16, val2: u16) {
        let update_flags = rd != REG_PC;
        let result = self.alu(op, val1, val2, update_flags);
        if op != ALUOP_CMP && op != ALUOP_TEST {
            self.regs[rd] = result;
        }
    }

    /// Núcleo da ALU. Retorna o resultado de 16 bits (para CMP/TEST o chamador
    /// descarta). Atualiza as flags quando `update_flags`.
    fn alu(&mut self, op: u32, val1: u16, val2: u16, update_flags: bool) -> u16 {
        match op {
            ALUOP_ADD | ALUOP_ADC => {
                let carry = if op == ALUOP_ADC { self.flag(FLAG_C) as u32 } else { 0 };
                let result = val1 as u32 + val2 as u32 + carry;
                let result_signed = val1 as i16 as i32 + val2 as i16 as i32 + carry as i32;
                if update_flags {
                    self.update_nzsc(result, result_signed);
                }
                (result & 0xffff) as u16
            }
            ALUOP_SUB | ALUOP_SBC | ALUOP_CMP => {
                let carry = if op == ALUOP_SBC { self.flag(FLAG_C) as u32 } else { 1 };
                let nv2 = !val2;
                let result = val1 as u32 + nv2 as u32 + carry;
                let result_signed = val1 as i16 as i32 + nv2 as i16 as i32 + carry as i32;
                if update_flags {
                    self.update_nzsc(result, result_signed);
                }
                (result & 0xffff) as u16
            }
            ALUOP_NEG => {
                let result = (!val2 as u32).wrapping_add(1);
                if update_flags {
                    self.update_nz(result);
                }
                (result & 0xffff) as u16
            }
            ALUOP_XOR => {
                let result = val1 ^ val2;
                if update_flags {
                    self.update_nz(result as u32);
                }
                result
            }
            ALUOP_LOAD => {
                if update_flags {
                    self.update_nz(val2 as u32);
                }
                val2
            }
            ALUOP_OR => {
                let result = val1 | val2;
                if update_flags {
                    self.update_nz(result as u32);
                }
                result
            }
            ALUOP_AND | ALUOP_TEST => {
                let result = val1 & val2;
                if update_flags {
                    self.update_nz(result as u32);
                }
                result
            }
            _ => self.unknown_val("alu mode"),
        }
    }

    #[inline]
    fn update_nz(&mut self, result: u32) {
        self.set_flag(FLAG_N, result & 0x8000 != 0);
        self.set_flag(FLAG_Z, result & 0xffff == 0);
    }

    #[inline]
    fn update_nzsc(&mut self, result: u32, result_signed: i32) {
        self.set_flag(FLAG_N, result & 0x8000 != 0);
        self.set_flag(FLAG_Z, result & 0xffff == 0);
        self.set_flag(FLAG_S, result_signed < 0);
        self.set_flag(FLAG_C, result & 0x10000 != 0);
    }

    /// Avalia uma condição de desvio (campo op0 no modo branch).
    fn check_branch(&mut self, branchop: u32) -> bool {
        let (n, z, s, c) = (
            self.flag(FLAG_N),
            self.flag(FLAG_Z),
            self.flag(FLAG_S),
            self.flag(FLAG_C),
        );
        match branchop {
            0 => !c,              // JB  — below (unsigned)
            1 => c,               // JAE — above or equal (unsigned)
            2 => !s,              // JGE — greater or equal (signed)
            3 => s,               // JL  — less (signed)
            4 => !z,              // JNE
            5 => z,               // JE
            6 => !n,              // JPL
            7 => n,               // JMI
            8 => !(!z && c),      // JBE — below or equal (unsigned)
            9 => !z && c,         // JA  — above (unsigned)
            10 => !(!z && !s),    // JLE — less or equal (signed)
            11 => !z && !s,       // JG  — greater (signed)
            12 => n == s,         // JVC — overflow clear
            13 => n != s,         // JVS — overflow set
            14 => true,           // JMP
            _ => self.unknown_bool("branch mode"),
        }
    }

    // ---- tratamento de opcode desconhecido ------------------------------

    // Instruções não reconhecidas são contadas e tratadas como NOP (em vez de
    // abortar), para robustez. `unknown_ops` deve idealmente ficar em 0.
    #[cold]
    fn note_unknown(&mut self) {
        self.unknown_ops = self.unknown_ops.wrapping_add(1);
        if self.dbg_first_unknown.is_none() {
            self.dbg_first_unknown = Some((self.cs_pc(), self.cur_iw));
            // Captura o anel reordenado (mais antigo → mais recente).
            let mut out = [(0u32, 0u16); 16];
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = self.dbg_ring[(self.dbg_ring_pos + i) % 16];
            }
            self.dbg_pre_unknown = Some(out);
        }
    }

    #[cold]
    fn unknown(&mut self, _what: &str) -> u32 {
        self.note_unknown();
        2
    }

    #[cold]
    fn unknown_val(&mut self, _what: &str) -> u16 {
        self.note_unknown();
        0
    }

    #[cold]
    fn unknown_bool(&mut self, _what: &str) -> bool {
        self.note_unknown();
        false
    }

    // ---- debug ----------------------------------------------------------

    /// Snapshot dos registradores (para debug/testes).
    pub fn regs(&self) -> [u16; 8] {
        self.regs
    }

    /// Define diretamente um registrador (para testes/debug).
    pub fn set_reg(&mut self, idx: usize, val: u16) {
        self.regs[idx] = val;
    }
}

// ---- helpers de bit --------------------------------------------------------

/// Sign-extend de um valor de 20 bits para `i32`.
#[inline]
fn sext20(value: u32) -> i32 {
    let sign = 1i32 << 19;
    ((value as i32) ^ sign) - sign
}

/// Rotate-left em um campo de 20 bits.
#[inline]
fn rotl20(val: u32, step: u32) -> u32 {
    let mask = (1u32 << 20) - 1;
    ((val << step) | (val >> (20 - step))) & mask
}

/// Rotate-right em um campo de 20 bits.
#[inline]
fn rotr20(val: u32, step: u32) -> u32 {
    let mask = (1u32 << 20) - 1;
    ((val >> step) | (val << (20 - step))) & mask
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::FlatBus;

    /// Monta uma CPU com um barramento plano de 64 KiW e PC em `0x0000`.
    fn setup(program: &[u16]) -> (Cpu, FlatBus) {
        let mut bus = FlatBus::new(16);
        bus.load(0x0000, program);
        let mut cpu = Cpu::new();
        // PC = 0, SR limpo → CS=0, DS=0.
        cpu.set_reg(REG_PC, 0);
        (cpu, bus)
    }

    #[test]
    fn reset_le_vetor() {
        let mut bus = FlatBus::new(16);
        bus.mem[0xfff7] = 0x1234;
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        assert_eq!(cpu.regs()[REG_PC], 0x1234);
    }

    #[test]
    fn load_imm16_para_r1() {
        // R1 = 0x00AB  →  op0=LOAD(9), rd=R1(1), op1n=33 (imm16), rs=0
        // iw = (9<<12)|(1<<9)|(33<<3)|0
        let iw = (ALUOP_LOAD as u16) << 12 | (1 << 9) | (33 << 3);
        let (mut cpu, mut bus) = setup(&[iw, 0x00AB]);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs()[1], 0x00AB);
        assert!(!cpu.flag(FLAG_Z));
        assert!(!cpu.flag(FLAG_N));
    }

    #[test]
    fn add_registrador_seta_carry_e_zero() {
        // R1 = 0xFFFF (via load imm16), depois R1 += R1 (add reg, rs=R1)
        let load = (ALUOP_LOAD as u16) << 12 | (1 << 9) | (33 << 3);
        // add rd=R1, modo registrador (op1n=32), rs=R1(1)
        let add = (ALUOP_ADD as u16) << 12 | (1 << 9) | (32 << 3) | 1;
        let (mut cpu, mut bus) = setup(&[load, 0xFFFF, add]);
        cpu.step(&mut bus); // R1 = 0xFFFF
        cpu.step(&mut bus); // R1 = 0xFFFE, carry set
        assert_eq!(cpu.regs()[1], 0xFFFE);
        assert!(cpu.flag(FLAG_C));
        assert!(cpu.flag(FLAG_N));
        assert!(!cpu.flag(FLAG_Z));
    }

    #[test]
    fn sub_igual_da_zero_e_carry() {
        // R1 = 5; CMP R1, 5 (imm16) → Z set, C set (no borrow).
        // No modo imm16 a ALU opera `regs[rs] OP imm`, então rs deve ser R1.
        let load = (ALUOP_LOAD as u16) << 12 | (1 << 9) | (33 << 3) | 1;
        let cmp = (ALUOP_CMP as u16) << 12 | (1 << 9) | (33 << 3) | 1;
        let (mut cpu, mut bus) = setup(&[load, 5, cmp, 5]);
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs()[1], 5); // CMP não altera R1
        assert!(cpu.flag(FLAG_Z));
        assert!(cpu.flag(FLAG_C));
    }

    #[test]
    fn push_pop_round_trip() {
        // Encodings (op1n ocupa bits 3..9; para push/pop op1n = 16 | opn):
        //   push {R1,R2} em [SP]: op0=STORE, rd=R2(2), opn=2 → op1n=18, rs=SP(0)
        //     reg começa em rd e decrementa → empilha R2 e depois R1.
        //   pop  {R1,R2} de [SP]: op0=LOAD,  rd=R0(0), opn=2 → op1n=18, rs=SP(0)
        //     reg pré-incrementa → restaura R1 e depois R2.
        let load_r1 = (ALUOP_LOAD as u16) << 12 | (1 << 9) | (33 << 3);
        let load_r2 = (ALUOP_LOAD as u16) << 12 | (2 << 9) | (33 << 3);
        let push = (ALUOP_STORE as u16) << 12 | (2 << 9) | (18 << 3) | REG_SP as u16;
        let clobber1 = (ALUOP_LOAD as u16) << 12 | (1 << 9) | (33 << 3);
        let clobber2 = (ALUOP_LOAD as u16) << 12 | (2 << 9) | (33 << 3);
        let pop = (ALUOP_LOAD as u16) << 12 | (0 << 9) | (18 << 3) | REG_SP as u16;

        let program = &[
            load_r1, 0x1111, // R1 = 0x1111
            load_r2, 0x2222, // R2 = 0x2222
            push, // push {R2,R1}
            clobber1, 0xDEAD, // R1 = 0xDEAD
            clobber2, 0xBEEF, // R2 = 0xBEEF
            pop,  // pop {R1,R2} → restaura
        ];
        let (mut cpu, mut bus) = setup(program);
        cpu.set_reg(REG_SP, 0x8000);
        for _ in 0..6 {
            cpu.step(&mut bus);
        }
        assert_eq!(cpu.regs()[1], 0x1111, "R1 restaurado");
        assert_eq!(cpu.regs()[2], 0x2222, "R2 restaurado");
        assert_eq!(cpu.regs()[REG_SP], 0x8000, "SP voltou ao valor inicial");
    }

    #[test]
    fn branch_jmp_incondicional() {
        // JMP para frente: rd=PC(7), op0=14 (JMP), op1n em 0..7 (forward), imm6=3.
        let jmp = (14u16) << 12 | (REG_PC as u16) << 9 | (0 << 3) | 3;
        let (mut cpu, mut bus) = setup(&[jmp]);
        // após buscar a instrução, PC=1; branch forward soma imm6=3 ao CS:PC → 4.
        cpu.step(&mut bus);
        assert_eq!(cpu.regs()[REG_PC], 4);
    }

    /// Regressão de timing: os ciclos de cada modo de instrução devem bater com
    /// os valores de referência do **veesem** (auditados instrução a instrução).
    /// Trava o modelo de timing contra refatorações futuras.
    #[test]
    fn ciclos_por_instrucao_batem_com_veesem() {
        // (programa, ciclos esperados, descrição).
        let cases: &[(&[u16], u32, &str)] = &[
            (&[(0 << 12) | (1 << 9) | (32 << 3) | 2], 3, "ALU registrador"),
            (&[(9 << 12) | (1 << 9) | (33 << 3), 0x00AB], 4, "LOAD imm16"),
            (&[(0 << 12) | (1 << 9) | (34 << 3), 0x0100], 7, "ALU [imm16]"),
            (&[(13 << 12) | (35 << 3) | 1, 0x0100], 7, "STORE [imm16]"),
            (&[(0 << 12) | (1 << 9) | (8 << 3) | 1], 2, "ALU imm6"),
            (&[(0 << 12) | (1 << 9) | (0 << 3) | 1], 6, "ALU [BP+imm6]"),
            (&[(0 << 12) | (1 << 9) | (24 << 3) | 2], 6, "ALU [Rs]"),
            (&[(0 << 12) | (1 << 9) | (56 << 3) | 1], 5, "ALU [A6]"),
            (&[(0 << 12) | (1 << 9) | (36 << 3) | 2], 3, "ALU shift (asr)"),
            (&[(0xf << 12) | (7 << 9) | (2 << 6), 0x1234], 5, "goto"),
            (&[(0xf << 12) | (1 << 6), 0x1234], 9, "call"),
            (&[(0xf << 12) | (5 << 6) | 37], 2, "nop"),
            (&[(0xf << 12) | (5 << 6) | 32], 10, "break"),
            (&[(14 << 12) | (7 << 9) | 0], 4, "branch tomado (JMP)"),
            (&[(5 << 12) | (7 << 9) | 0], 2, "branch não-tomado (JE, Z=0)"),
        ];
        for (prog, expected, desc) in cases {
            let (mut cpu, mut bus) = setup(prog);
            cpu.set_reg(REG_SP, 0x2fff); // pilha válida (break empilha)
            let got = cpu.step(&mut bus);
            assert_eq!(got, *expected, "ciclos de '{desc}'");
        }
    }

    /// A entrada de IRQ custa 10 ciclos (igual ao veesem).
    #[test]
    fn ciclos_de_entrada_de_irq() {
        // Programa: habilita IRQ (0xf141), depois um nop.
        let enable_irq = (0xfu16 << 12) | (5 << 6) | 1; // exec_misc(1): irq on
        let (mut cpu, mut bus) = setup(&[enable_irq, (0xf << 12) | (5 << 6) | 37]);
        bus.mem[0xfff8] = 0x9000; // vetor de IRQ0
        cpu.set_reg(REG_SP, 0x2fff);
        assert_eq!(cpu.step(&mut bus), 2, "habilitar IRQ custa 2");
        cpu.set_irq(0, true); // sinaliza IRQ0
        assert_eq!(cpu.step(&mut bus), 10, "entrada de IRQ custa 10");
    }
}

crate::impl_state!(Cpu {
    regs, sb, irq_signal, fiq_signal,
    irq_active, fiq_active, irq_enable, fiq_enable, fir_mov,
    cur_iw, cur_pc
});
