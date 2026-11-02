//! Desmontador (*disassembler*) da CPU **unSP** — base para o depurador.
//!
//! Decodifica uma palavra de instrução (e, quando o modo pede, a palavra
//! seguinte) no mnemônico correspondente, **espelhando exatamente** o decodor de
//! [`crate::cpu`]. É puro e sem efeitos colaterais: serve tanto para um comando
//! de linha de comando quanto para uma futura janela de depuração.
//!
//! A sintaxe segue o estilo "expressão" do assembler unSP da SunPlus
//! (`Rd = Rd + [BP+3]`, `[0x1234] = R2`, `JMP $-1`), que é inequívoca quanto a
//! origem e destino dos operandos.

/// Nomes dos 8 registradores por índice.
const REG: [&str; 8] = ["SP", "R1", "R2", "R3", "R4", "BP", "SR", "PC"];

/// Resultado da desmontagem de uma instrução.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insn {
    /// Texto legível da instrução.
    pub text: String,
    /// Quantas palavras a instrução ocupa (1 ou 2).
    pub words: u32,
}

impl Insn {
    fn one(text: String) -> Self {
        Self { text, words: 1 }
    }
    fn two(text: String) -> Self {
        Self { text, words: 2 }
    }
}

/// Símbolo do operador de ALU no estilo expressão (ou `None` para LOAD/STORE,
/// tratados à parte).
fn alu_sym(op0: u32) -> &'static str {
    match op0 {
        0 => "+",
        1 => "+c",   // ADC: soma com carry
        2 => "-",
        3 => "-c",   // SBC: subtrai com borrow
        4 => "cmp",  // CMP (não escreve)
        6 => "neg",  // NEG (unário)
        8 => "^",
        10 => "|",
        11 => "&",
        12 => "test", // TEST (não escreve)
        _ => "?",
    }
}

/// Condição de desvio (quando `rd == PC`), indexada por `op0`.
fn branch_mnemonic(op0: u32) -> &'static str {
    match op0 {
        0 => "JB",
        1 => "JAE",
        2 => "JGE",
        3 => "JL",
        4 => "JNE",
        5 => "JE",
        6 => "JPL",
        7 => "JMI",
        8 => "JBE",
        9 => "JA",
        10 => "JLE",
        11 => "JG",
        12 => "JVC",
        13 => "JVS",
        14 => "JMP",
        _ => "J??",
    }
}

/// Monta o lado direito de uma operação de ALU: `Rd = <val1> <op> <operand>`,
/// com os casos especiais de LOAD (só copia) e as operações unárias/sem escrita.
fn alu_expr(op0: u32, rd: usize, operand: &str) -> String {
    match op0 {
        9 => format!("{} = {operand}", REG[rd]),          // LOAD
        4 => format!("CMP {}, {operand}", REG[rd]),       // CMP
        12 => format!("TEST {}, {operand}", REG[rd]),     // TEST
        6 => format!("{} = -{operand}", REG[rd]),         // NEG
        _ => format!("{0} = {0} {1} {operand}", REG[rd], alu_sym(op0)),
    }
}

/// Desmonta a instrução cuja palavra é `iw`; `next` é a palavra seguinte (usada
/// nos modos de 2 palavras) e `pc` é o endereço (em palavras) da instrução,
/// usado para resolver alvos de desvio absolutos.
pub fn disassemble(iw: u16, next: u16, pc: u32) -> Insn {
    let op0 = ((iw >> 12) & 0xf) as u32;
    let rd = ((iw >> 9) & 0x7) as usize;
    let op1 = ((iw >> 6) & 0x7) as u32;
    let op1n = ((iw >> 3) & 0x3f) as u32;
    let opn = ((iw >> 3) & 0x7) as u32;
    let rs = (iw & 0x7) as usize;
    let imm6 = (iw & 0x3f) as u32;

    if op0 == 0xf {
        return disasm_group_f(op1, rd, rs, opn, imm6, next, pc);
    }

    match op1n {
        // [BP+imm6], ou desvio para frente se rd == PC.
        0..=7 => {
            if rd == 7 {
                let target = pc.wrapping_add(1).wrapping_add(imm6);
                Insn::one(format!("{} 0x{target:06X}", branch_mnemonic(op0)))
            } else {
                Insn::one(alu_expr(op0, rd, &format!("[BP+0x{imm6:02X}]")))
            }
        }
        // imm6, ou desvio para trás se rd == PC.
        8..=15 => {
            if rd == 7 {
                let target = pc.wrapping_add(1).wrapping_sub(imm6);
                Insn::one(format!("{} 0x{target:06X}", branch_mnemonic(op0)))
            } else {
                Insn::one(alu_expr(op0, rd, &format!("#0x{imm6:02X}")))
            }
        }
        // push / pop (e reti).
        16..=23 => disasm_push_pop(op0, rd, rs, opn),
        // [Rs] com pré/pós inc/dec, opcionalmente com DS.
        24..=31 => {
            let operand = indirect_operand(op1n, rs);
            if op0 == 13 {
                Insn::one(format!("{operand} = {}", REG[rd]))
            } else {
                Insn::one(alu_expr(op0, rd, &operand))
            }
        }
        // registrador direto.
        32 => Insn::one(alu_expr(op0, rd, REG[rs])),
        // imm16.
        33 => {
            if op0 == 9 {
                Insn::two(format!("{} = #0x{next:04X}", REG[rd]))
            } else if op0 == 4 || op0 == 12 {
                Insn::two(alu_expr(op0, rd, &format!("#0x{next:04X}")))
            } else {
                // D = Rs <op> #imm16 (três operandos).
                Insn::two(format!(
                    "{} = {} {} #0x{next:04X}",
                    REG[rd],
                    REG[rs],
                    alu_sym(op0)
                ))
            }
        }
        // [imm16] leitura.
        34 => {
            if op0 == 9 {
                Insn::two(format!("{} = [0x{next:04X}]", REG[rd]))
            } else {
                Insn::two(format!(
                    "{} = {} {} [0x{next:04X}]",
                    REG[rd],
                    REG[rs],
                    alu_sym(op0)
                ))
            }
        }
        // [imm16] escrita.
        35 => {
            if op0 == 13 {
                Insn::two(format!("[0x{next:04X}] = {}", REG[rs]))
            } else {
                Insn::two(format!(
                    "[0x{next:04X}] = {} {} {}",
                    REG[rs],
                    alu_sym(op0),
                    REG[rd]
                ))
            }
        }
        // registrador com shift/rotate.
        36..=55 => {
            let n = (op1n & 3) + 1;
            let shift = match op1n {
                36..=39 => "ASR",
                40..=43 => "LSL",
                44..=47 => "LSR",
                48..=51 => "ROL",
                _ => "ROR",
            };
            Insn::one(alu_expr(op0, rd, &format!("({} {shift} {n})", REG[rs])))
        }
        // [A6] — direto de 6 bits (página zero).
        56..=63 => {
            if op0 == 13 {
                Insn::one(format!("[0x{imm6:02X}] = {}", REG[rd]))
            } else {
                Insn::one(alu_expr(op0, rd, &format!("[0x{imm6:02X}]")))
            }
        }
        _ => Insn::one(format!(".word 0x{iw:04X}")),
    }
}

/// Modos indiretos `[Rs]` (op1n 24..=31).
fn indirect_operand(op1n: u32, rs: usize) -> String {
    let r = REG[rs];
    match op1n {
        24 => format!("[{r}]"),
        25 => format!("[{r}--]"),
        26 => format!("[{r}++]"),
        27 => format!("[++{r}]"),
        28 => format!("DS:[{r}]"),
        29 => format!("DS:[{r}--]"),
        30 => format!("DS:[{r}++]"),
        _ => format!("DS:[++{r}]"),
    }
}

/// push/pop de múltiplos registradores e o pseudo-op `reti`.
fn disasm_push_pop(op0: u32, rd: usize, rs: usize, opn: u32) -> Insn {
    match op0 {
        // LOAD → pop (restaura rd+1..rd+opn).
        9 => {
            if rd == 5 && opn == 3 && rs == 0 {
                return Insn::one("RETI".to_string());
            }
            let mut regs = Vec::new();
            let mut reg = rd as i32;
            let mut left = opn as i32;
            while left > 0 && reg + 1 <= 7 {
                reg += 1;
                regs.push(REG[reg as usize]);
                left -= 1;
            }
            Insn::one(format!("POP {{{}}} from {}", regs.join(", "), REG[rs]))
        }
        // STORE → push (empilha rd..rd-opn+1).
        13 => {
            let mut regs = Vec::new();
            let mut reg = rd as i32;
            let mut left = opn as i32;
            while left > 0 && reg >= 0 {
                regs.push(REG[reg as usize]);
                reg -= 1;
                left -= 1;
            }
            Insn::one(format!("PUSH {{{}}} to {}", regs.join(", "), REG[rs]))
        }
        _ => Insn::one(format!("push/pop? op0={op0}")),
    }
}

/// Grupo `op0 == 0xf`: mul/muls, call, goto, controle de IRQ, nop/break.
/// `_pc` ainda não altera os alvos (call/goto já são absolutos), mas mantém a
/// assinatura pronta para anotações futuras.
fn disasm_group_f(op1: u32, rd: usize, rs: usize, opn: u32, imm6: u32, next: u16, _pc: u32) -> Insn {
    match op1 {
        0 if opn == 1 => Insn::one(format!("MUL {}, {} (u*s)", REG[rd], REG[rs])),
        1 => {
            let target = (imm6 << 16) | next as u32;
            Insn::two(format!("CALL 0x{target:06X}"))
        }
        2 if rd == 7 => {
            let target = (imm6 << 16) | next as u32;
            Insn::two(format!("GOTO 0x{target:06X}"))
        }
        2 | 3 => {
            let n = if opn != 0 { opn } else { 16 };
            Insn::one(format!("MULS {}, {}, {n} (u*s)", REG[rd], REG[rs]))
        }
        4 if opn == 1 => Insn::one(format!("MUL {}, {} (s*s)", REG[rd], REG[rs])),
        5 => disasm_misc(imm6),
        6 | 7 => {
            let n = if opn != 0 { opn } else { 16 };
            Insn::one(format!("MULS {}, {}, {n} (s*s)", REG[rd], REG[rs]))
        }
        _ => Insn::one(format!("<grupo F op1={op1}>")),
    }
}

/// Instruções `misc` (controle de IRQ/FIQ, nop, break), por `imm6`.
fn disasm_misc(imm6: u32) -> Insn {
    let text = match imm6 {
        0 => "INT OFF".to_string(),
        1 => "INT IRQ".to_string(),
        2 => "INT FIQ".to_string(),
        3 => "INT FIQ,IRQ".to_string(),
        4 => "FIR_MOV ON".to_string(),
        5 => "FIR_MOV OFF".to_string(),
        8 => "IRQ OFF".to_string(),
        9 => "IRQ ON".to_string(),
        12 => "FIQ OFF".to_string(),
        14 => "FIQ ON".to_string(),
        32 | 40 | 48 | 56 => "BREAK".to_string(),
        37 => "NOP".to_string(),
        other => format!("<misc 0x{other:02X}>"),
    };
    Insn::one(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(iw: u16) -> String {
        disassemble(iw, 0, 0x8000).text
    }
    fn d2(iw: u16, next: u16) -> Insn {
        disassemble(iw, next, 0x8000)
    }

    #[test]
    fn instrucoes_conhecidas() {
        // Vetores retirados dos testes da CPU.
        assert_eq!(d2(0x9309, 0x0001).text, "R1 = #0x0001"); // LOAD R1, imm16
        assert_eq!(d2(0x9309, 0x0001).words, 2);
        assert_eq!(d2(0xD119, 0x2862).text, "[0x2862] = R1"); // STORE R1 -> [imm16]
        assert_eq!(d2(0xD11A, 0x2B01).text, "[0x2B01] = R2");
        assert_eq!(d(0x0441), "R2 = R2 + #0x01"); // ADD R2, #1
        assert_eq!(d(0x9A98), "RETI");
        assert_eq!(d(0xF141), "INT IRQ"); // habilita IRQ
    }

    #[test]
    fn desvios_relativos() {
        // 0xEE41: JMP para trás 1 → alvo = pc+1-1 = pc.
        assert_eq!(d(0xEE41), "JMP 0x008000");
        // Mesma forma para frente: op1n 0..7 com rd=PC.
        // JMP $+2: op0=14, rd=7, op1n=0, imm6=2 → iw = 0xE<rd=7><...>.
        let fwd = (14u16 << 12) | (7 << 9) | (0 << 3) | 2;
        assert_eq!(disassemble(fwd, 0, 0x8000).text, "JMP 0x008003");
    }

    #[test]
    fn call_e_goto() {
        // CALL: op0=F, op1=1, imm6=hi, next=lo.
        let call = (0xfu16 << 12) | (1 << 6) | 0x02; // imm6=2
        assert_eq!(d2(call, 0x1234), Insn::two("CALL 0x021234".to_string()));
        // GOTO: op0=F, op1=2, rd=PC.
        let goto = (0xfu16 << 12) | (7 << 9) | (2 << 6);
        assert_eq!(disassemble(goto, 0x4567, 0).text, "GOTO 0x004567");
    }

    #[test]
    fn modos_de_enderecamento() {
        // Registrador direto: ADD R1, R2 (op0=0, rd=1, op1n=32, rs=2).
        let add_rr = (0u16 << 12) | (1 << 9) | (32 << 3) | 2;
        assert_eq!(disassemble(add_rr, 0, 0).text, "R1 = R1 + R2");
        // [Rs++]: LOAD R1, [R2++] (op0=9, rd=1, op1n=26, rs=2).
        let ld_inc = (9u16 << 12) | (1 << 9) | (26 << 3) | 2;
        assert_eq!(disassemble(ld_inc, 0, 0).text, "R1 = [R2++]");
    }
}
