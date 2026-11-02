//! Core de emulação do V.Smile — port Rust independente.
//!
//! Escrito do zero, usando os emuladores open source (veesem, VFrown) e o MAME
//! apenas como referência de *comportamento* do hardware. Não há código copiado.
//!
//! O alvo é o SoC SunPlus **SPG200**, que contém a CPU **unSP** de 16 bits.
//!
//! Estado atual (fase 1):
//! - [`cpu`]: CPU unSP completa e testável.
//! - [`bus`]: trait [`Bus`] + barramento plano para testes.
//! - [`memory`]: [`Spg200Mem`] — mapa de memória do SPG200 (RAM + sistema +
//!   memória externa); periféricos PPU/SPU/etc. ainda são stub.
//! - [`io`]: trait [`SystemIo`] — controlador de I/O (ROM/cartucho, portas,
//!   ADC, UART).
//! - [`extmem`]: decodificação de chip-selects da memória externa.
//! - [`random`]: LFSRs do RNG.
//!
//! Veja o ROADMAP no `README.md` do projeto.

pub mod adc;
pub mod adpcm;
pub mod bus;
pub mod clock;
pub mod core;
pub mod cpu;
pub mod debugger;
pub mod disasm;
pub mod dma;
pub mod extmem;
mod games;
pub mod gpio;
pub mod inflate;
pub mod io;
pub mod irq;
pub mod joy;
pub mod memory;
pub mod ppu;
pub mod ppu_view;
pub mod random;
pub mod save;
pub mod settings;
pub mod spg;
pub mod spu;
pub mod timer;
pub mod uart;
pub mod vsmile;
pub mod watchdog;
pub mod zip;

pub use core::{INFO, VsmileCore};
pub use bus::Bus;
pub use cpu::Cpu;
pub use io::{Region, SystemIo};
pub use irq::Irq;
pub use memory::{Spg200Mem, VideoTiming};
pub use ppu::Ppu;
pub use settings::PpuViewSettings;
pub use joy::{JoyInput, JoyLeds};
pub use spg::Spg200;
pub use vsmile::{VSmile, VSmileIo};

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// I/O de teste: expõe uma ROM única no chip-select `ROMCSB` (decode 0,
    /// o padrão após reset).
    struct RomIo {
        rom: Region,
    }

    impl SystemIo for RomIo {
        fn read_rom_csb(&mut self, addr: u32) -> u16 {
            self.rom.read(addr)
        }
    }

    /// Boot mínimo: a CPU lê o vetor de reset em `0xFFF7` (via memória
    /// externa), salta para a rotina na ROM e esta grava um valor na RAM.
    #[test]
    fn boot_rom_escreve_na_ram() {
        // Programa na ROM em 0x8000:
        //   LOAD R1, 0x1234      ; imm16
        //   [0x0100] = R1        ; store [imm16]
        const ALUOP_LOAD: u16 = 9;
        const ALUOP_STORE: u16 = 13;

        // LOAD imm16 para R1 (op1n=33). rs irrelevante no LOAD.
        let load = (ALUOP_LOAD << 12) | (1 << 9) | (33 << 3) | 1;
        // STORE [imm16] (op1n=35): grava regs[rs]=R1 no endereço seguinte.
        let store = (ALUOP_STORE << 12) | (0 << 9) | (35 << 3) | 1;

        let mut rom = vec![0xFFFFu16; 0x10000];
        rom[0xfff7] = 0x8000; // vetor de reset → 0x8000
        rom[0x8000] = load;
        rom[0x8001] = 0x1234;
        rom[0x8002] = store;
        rom[0x8003] = 0x0100; // endereço alvo na RAM

        let io = RomIo {
            rom: Region::new(rom),
        };
        let mut mem = Spg200Mem::new(io, VideoTiming::Ntsc);
        let mut cpu = Cpu::new();

        cpu.reset(&mut mem);
        assert_eq!(cpu.regs()[7], 0x8000, "PC carregado do vetor de reset");

        cpu.step(&mut mem); // LOAD R1, 0x1234
        assert_eq!(cpu.regs()[1], 0x1234);

        cpu.step(&mut mem); // [0x0100] = R1
        assert_eq!(mem.ram[0x0100], 0x1234, "valor gravado na RAM via store");
        assert_eq!(mem.unimplemented_accesses, 0, "nenhum acesso a periférico stub");
    }

    #[test]
    fn ppu_desenha_tile_e_gera_vblank() {
        use crate::ppu::{SCREEN_W, SCREEN_H};

        let io = RomIo {
            rom: Region::empty(2),
        };
        let mut mem = Spg200Mem::new(io, VideoTiming::Ntsc);

        // Paleta: índice 1 = vermelho puro (R=31) no formato RGB555.
        mem.write_word(0x2b00 + 1, (31u16) << 10);

        // Tile map do BG0 em 0x0100 (na RAM); tile (ch)=1 na posição 0.
        mem.write_word(0x0100, 0x0001);
        // Dados do tile: line_segment_addr = (segment<<6) + ch*tile_h*tile_w*bpp/16.
        // Com segment=8 → base 0x200; tile 8x8 @ 2bpp ocupa 8 palavras, então o
        // tile ch=1 começa em 0x200 + 8 = 0x208.
        // Linha 0 do tile: 8 pixels de índice 1 (2 bits cada) = 0b01 repetido.
        // O hardware faz byte-swap em bpp!=16, então escrevemos já trocado.
        let line: u16 = 0b01_01_01_01_01_01_01_01;
        let swapped = (line >> 8) | (line << 8);
        mem.write_word(0x0208, swapped);

        // Configura BG0: tile_map_ptr=0x0100, segment_ptr=8, attr: 4bpp? usamos
        // color_mode=0 → 2bpp, palette=0. ctrl: enabled.
        mem.write_word(0x2814, 0x0100); // bg0 tile_map_ptr
        mem.write_word(0x2820, 8); // bg0 segment_ptr
        mem.write_word(0x2812, 0x0000); // bg0 attr: color_mode=0 (2bpp), palette 0, depth 0
        mem.write_word(0x2813, 0x0008); // bg0 ctrl: enabled (bit 3)

        // vblank IRQ habilitada.
        mem.write_word(0x2862, 0x0001);

        // Roda ciclos suficientes para desenhar o quadro inteiro (262 scanlines
        // * período). Procuramos o fim de quadro.
        let mut frame_done = false;
        for _ in 0..100_000 {
            if mem.run_ppu(64) {
                frame_done = true;
                break;
            }
        }
        assert!(frame_done, "um quadro deve ter sido concluído");
        assert_eq!(mem.ppu.frame_counter(), 1);
        assert!(mem.ppu.irq_line(), "vblank deve ter levantado a IRQ da PPU");

        // O pixel (0,0) deve ser vermelho (índice 1 da paleta).
        let fb = mem.ppu.framebuffer();
        assert_eq!(fb.len(), SCREEN_W * SCREEN_H);
        assert_eq!(fb[0], (31u16) << 10, "pixel (0,0) vermelho");
    }

    #[test]
    fn vblank_irq_dispara_handler() {
        use crate::spg::Spg200;

        // Instruções (ver testes da CPU para o encoding):
        let load_r1 = (9u16 << 12) | (1 << 9) | (33 << 3) | 1; // LOAD R1, imm16
        let store = (13u16 << 12) | (0 << 9) | (35 << 3) | 1; // [imm16] = R1
        let enable_irq = 0xf141u16; // habilita IRQ na CPU (op1=5, imm6=1)
        let jmp_self = 0xEE41u16; // JMP backward 1 → loop no próprio endereço
        let reti = 0x9A98u16; // pop {SR,PC} (reti)

        let mut rom = vec![0xFFFFu16; 0x10000];
        rom[0xfff7] = 0x8000; // vetor de reset
        rom[0xfff8] = 0x9000; // vetor de IRQ0 (PPU)

        // Main: habilita vblank IRQ na PPU, habilita IRQ na CPU, loop.
        rom[0x8000] = load_r1;
        rom[0x8001] = 0x0001;
        rom[0x8002] = store;
        rom[0x8003] = 0x2862; // PPU irq_ctrl: bit0 = vblank
        rom[0x8004] = enable_irq;
        rom[0x8005] = jmp_self;

        // Handler: RAM[0x0050] = 0xBEEF; reti.
        rom[0x9000] = load_r1;
        rom[0x9001] = 0xBEEF;
        rom[0x9002] = store;
        rom[0x9003] = 0x0050;
        rom[0x9004] = reti;

        let io = RomIo {
            rom: Region::new(rom),
        };
        let mut spg = Spg200::new(io, VideoTiming::Ntsc);
        // SP inicial em cima da RAM para o push/pop do IRQ.
        spg.reset();
        spg.cpu.set_reg(0, 0x27ff);

        let mut serviced = false;
        for _ in 0..2_000_000 {
            spg.step();
            if spg.mem.ram[0x0050] == 0xBEEF {
                serviced = true;
                break;
            }
        }
        assert!(serviced, "o handler de vblank IRQ deveria ter rodado");
        assert!(spg.frame_counter() >= 1);
    }

    #[test]
    fn registrador_de_controle_de_sistema_mascara() {
        let mem_io = RomIo {
            rom: Region::empty(2),
        };
        let mut mem = Spg200Mem::new(mem_io, VideoTiming::Pal);
        // Máscara de escrita 0xc3f6: bits fora dela não "pegam".
        mem.write_word(0x3d20, 0xffff);
        assert_eq!(mem.read_word(0x3d20), 0xc3f6);
        // 0x3d2b reflete a temporização PAL.
        assert_eq!(mem.read_word(0x3d2b), 1);
    }
}
