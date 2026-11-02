//! Cola específica do console **V.Smile** sobre o SoC [`Spg200`].
//!
//! Mapeia a ROM do sistema e o cartucho nos chip-selects, implementa as portas
//! (botões ON/OFF/restart na porta B; código de região e handshake dos
//! controladores na porta C), o ADC (bateria) e — futuramente — o controlador
//! (joystick) via UART.
//!
//! Layout dos chip-selects (igual ao hardware):
//! - `ROMCSB` → cartucho, offset 0
//! - `CSB1`   → cartucho, offset `0x100000`
//! - `CSB2`   → cartucho, offset `0x200000`
//! - `CSB3`   → ROM do sistema

use crate::io::SystemIo;
use crate::joy::{JoyInput, JoyLeds, VSmileJoy};
use crate::memory::VideoTiming;
use crate::ppu::{SCREEN_H, SCREEN_W};
use crate::spg::Spg200;

const CART_WORDS: usize = 4 * 1024 * 1024; // 4 MiW
const SYS_WORDS: usize = 1024 * 1024; // 1 MiW
const CART_MASK: u32 = (CART_WORDS - 1) as u32;
const SYS_MASK: u32 = (SYS_WORDS - 1) as u32;

/// Tamanho da NVRAM (SRAM com bateria) dos cartuchos Art Studio, mapeada em
/// `CSB2`. É a janela inteira do chip-select (1 MiW); o jogo usa só uma fração.
pub const NVRAM_WORDS: usize = 1024 * 1024; // 1 MiW
const NVRAM_MASK: u32 = (NVRAM_WORDS - 1) as u32;

/// Assinatura e versão do formato de *save state* (ver [`VSmile::save_state`]).
const SAVE_MAGIC: [u8; 8] = *b"VSMILES\x00";
const SAVE_VERSION: u32 = 1;

/// Converte uma cor RGB555 (bit 15 = transparência, já resolvida) em RGB888.
#[inline]
pub fn rgb555_to_rgb888(c: u16) -> (u8, u8, u8) {
    let r5 = ((c >> 10) & 0x1f) as u8;
    let g5 = ((c >> 5) & 0x1f) as u8;
    let b5 = (c & 0x1f) as u8;
    let expand = |v: u8| (v << 3) | (v >> 2);
    (expand(r5), expand(g5), expand(b5))
}

/// Converte bytes de um arquivo de ROM (little-endian) em palavras de 16 bits.
pub fn rom_from_bytes_le(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks(2)
        .map(|c| c[0] as u16 | ((*c.get(1).unwrap_or(&0) as u16) << 8))
        .collect()
}

/// Converte palavras de 16 bits em bytes little-endian (inverso de
/// [`rom_from_bytes_le`]). Usado para serializar a NVRAM em disco.
pub fn bytes_from_rom_le(words: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(words.len() * 2);
    for &w in words {
        out.push((w & 0xff) as u8);
        out.push((w >> 8) as u8);
    }
    out
}

/// Serializa a NVRAM para o arquivo de save, cortando as palavras `0xFFFF`
/// finais (estado "apagado") para não gravar 2 MiB de SRAM vazia. Ao recarregar,
/// [`rom_from_bytes_le`] + [`VSmile::enable_nvram`] repreenchem o resto com
/// `0xFFFF`, então o corte é sem perdas.
pub fn nvram_to_save_bytes(words: &[u16]) -> Vec<u8> {
    let end = words
        .iter()
        .rposition(|&w| w != 0xFFFF)
        .map_or(0, |i| i + 1);
    bytes_from_rom_le(&words[..end])
}

/// Converte o conteúdo bruto de um arquivo de cartucho em palavras de ROM,
/// aceitando tanto `.bin` cru quanto `.zip`.
///
/// Detecta o ZIP pela assinatura `PK\x03\x04`; nesse caso extrai a ROM de
/// dentro (ver [`crate::zip::extract_rom`]). Caso contrário trata os bytes como
/// uma imagem `.bin` little-endian. Mantém o core sem acesso a disco: o
/// frontend lê o arquivo e passa os bytes.
pub fn rom_from_file(bytes: &[u8]) -> Result<Vec<u16>, String> {
    let is_zip = bytes.len() >= 4 && &bytes[0..4] == b"PK\x03\x04";
    let raw = if is_zip {
        crate::zip::extract_rom(bytes)?
    } else {
        bytes.to_vec()
    };
    Ok(rom_from_bytes_le(&raw))
}

/// ROM do sistema "dummy" compatível com jogos, usada quando o usuário não
/// fornece a BIOS. Os jogos dão boot pelo cartucho; a sysrom é secundária.
pub fn dummy_sysrom() -> Vec<u16> {
    let mut rom = vec![0u16; SYS_WORDS];
    let mut i = 0xfffc0usize;
    while i < 0xfffdc {
        rom[i + 1] = 0x31;
        i += 2;
    }
    rom
}

/// Implementação de [`SystemIo`] para o V.Smile.
pub struct VSmileIo {
    sys_rom: Box<[u16]>,
    cart_rom: Box<[u16]>,
    region_code: u16,
    vtech_logo: bool,

    rts: [bool; 2],
    cts: [bool; 2],

    on_pressed: bool,
    off_pressed: bool,
    restart_pressed: bool,
    /// Controlador 0 (joystick).
    joy: VSmileJoy,
    /// Controlador 1 (segundo joystick), ativo só quando `joy2_enabled`.
    joy2: VSmileJoy,
    /// Se o segundo controlador está plugado. Desligado = comportamento
    /// idêntico ao de um V.Smile com um controle só (sem regressão).
    joy2_enabled: bool,
    /// Contadores de leitura por chip-select [ROMCSB, CSB1, CSB2, CSB3] (debug).
    pub csb_reads: [u64; 4],
    /// NVRAM (SRAM com bateria) dos cartuchos Art Studio, mapeada em `CSB2`.
    /// `None` = cartucho comum (CSB2 lê a ROM, escritas são descartadas).
    nvram: Option<Box<[u16]>>,
    /// Marca se a NVRAM foi escrita desde o último salvamento.
    nvram_dirty: bool,
}

impl VSmileIo {
    /// Cria a I/O do V.Smile. `sys_rom` e `cart_rom` são preenchidos/truncados
    /// aos tamanhos fixos do hardware (1 MiW e 4 MiW), com `0xFFFF` no restante.
    pub fn new(sys_rom: Vec<u16>, cart_rom: Vec<u16>, region_code: u16, vtech_logo: bool) -> Self {
        Self {
            sys_rom: fixed_rom(sys_rom, SYS_WORDS),
            cart_rom: fixed_rom(cart_rom, CART_WORDS),
            region_code: region_code & 0xf,
            vtech_logo,
            rts: [true, true],
            cts: [false, false],
            on_pressed: false,
            off_pressed: false,
            restart_pressed: false,
            joy: VSmileJoy::new(),
            joy2: VSmileJoy::new(),
            joy2_enabled: false,
            csb_reads: [0; 4],
            nvram: None,
            nvram_dirty: false,
        }
    }

    /// Pluga/despluga o segundo controlador.
    pub fn set_controller2_enabled(&mut self, enabled: bool) {
        self.joy2_enabled = enabled;
    }

    /// Habilita a NVRAM do cartucho (Art Studio): roteia `CSB2` para uma SRAM
    /// gravável. `words` é o conteúdo inicial (salvo em disco, ou vazio); é
    /// ajustado para exatamente [`NVRAM_WORDS`] palavras, preenchendo com
    /// `0xFFFF` (estado "apagado") o que faltar.
    pub fn enable_nvram(&mut self, words: Vec<u16>) {
        self.nvram = Some(fixed_rom(words, NVRAM_WORDS));
        self.nvram_dirty = false;
    }

    /// Conteúdo atual da NVRAM, se habilitada (para persistir em disco).
    pub fn nvram_words(&self) -> Option<&[u16]> {
        self.nvram.as_deref()
    }

    /// Indica se a NVRAM mudou desde o último [`VSmileIo::clear_nvram_dirty`].
    pub fn nvram_dirty(&self) -> bool {
        self.nvram_dirty
    }

    /// Zera o marcador de "modificada" (chamar após salvar em disco).
    pub fn clear_nvram_dirty(&mut self) {
        self.nvram_dirty = false;
    }

    /// Restaura o estado das linhas/portas (chamado no reset do console).
    pub fn reset_io_state(&mut self) {
        self.rts = [true, true];
        self.cts = [false, false];
        self.on_pressed = false;
        self.off_pressed = false;
        self.restart_pressed = false;
        self.joy.reset();
        self.joy2.reset();
    }
}

fn fixed_rom(mut data: Vec<u16>, size: usize) -> Box<[u16]> {
    data.truncate(size);
    data.resize(size, 0xFFFF);
    data.into_boxed_slice()
}

impl SystemIo for VSmileIo {
    fn run_cycles(&mut self, cycles: u32) {
        self.joy.run_cycles(cycles);
        if self.joy2_enabled {
            self.joy2.run_cycles(cycles);
        }
    }

    fn adc(&mut self, channel: u8) -> u16 {
        match channel {
            1 => 0x3ff, // bateria cheia
            _ => 0,
        }
    }

    fn get_port_a(&mut self) -> u16 {
        0
    }

    fn get_port_b(&mut self) -> u16 {
        // Ativo em baixo: bit 7 OFF, bit 6 ON, bit 3 restart.
        ((!self.off_pressed as u16) << 7)
            | ((!self.on_pressed as u16) << 6)
            | ((!self.restart_pressed as u16) << 3)
    }

    fn get_port_c(&mut self) -> u16 {
        // rts[0] vem do controlador 0; rts[1] vem do controlador 1 quando
        // plugado, senão fica na linha estática (controlador ausente).
        let rts1 = if self.joy2_enabled {
            self.joy2.rts()
        } else {
            self.rts[1]
        };
        self.region_code
            | ((self.vtech_logo as u16) << 4)
            | 0x0020
            | ((self.cts[0] as u16) << 8)
            | ((self.cts[1] as u16) << 9)
            | ((self.joy.rts() as u16) << 10)
            | ((rts1 as u16) << 12)
            | 0x6000
    }

    fn set_port_c(&mut self, value: u16, mask: u16) {
        if mask & 0x0100 != 0 {
            self.cts[0] = value & 0x0100 != 0;
            self.joy.set_cts(self.cts[0]);
        }
        if mask & 0x0200 != 0 {
            self.cts[1] = value & 0x0200 != 0;
            if self.joy2_enabled {
                self.joy2.set_cts(self.cts[1]);
            }
        }
    }

    fn read_rom_csb(&mut self, addr: u32) -> u16 {
        self.csb_reads[0] += 1;
        self.cart_rom[(addr & CART_MASK) as usize]
    }
    fn read_csb1(&mut self, addr: u32) -> u16 {
        self.csb_reads[1] += 1;
        self.cart_rom[((addr + 0x100000) & CART_MASK) as usize]
    }
    fn read_csb2(&mut self, addr: u32) -> u16 {
        self.csb_reads[2] += 1;
        match &self.nvram {
            Some(nv) => nv[(addr & NVRAM_MASK) as usize],
            None => self.cart_rom[((addr + 0x200000) & CART_MASK) as usize],
        }
    }
    fn write_csb2(&mut self, addr: u32, value: u16) {
        if let Some(nv) = &mut self.nvram {
            let i = (addr & NVRAM_MASK) as usize;
            if nv[i] != value {
                nv[i] = value;
                self.nvram_dirty = true;
            }
        }
        // Cartucho comum: CSB2 é ROM, escritas são ignoradas (como no hardware).
    }
    fn read_csb3(&mut self, addr: u32) -> u16 {
        self.csb_reads[3] += 1;
        self.sys_rom[(addr & SYS_MASK) as usize]
    }

    fn tx_uart(&mut self, value: u8) {
        if self.joy2_enabled {
            // Com dois controladores, o byte vai só para o que está endereçado
            // (CTS alto) — como no hardware, que seleciona por CTS.
            if self.cts[0] {
                self.joy.rx(value);
            }
            if self.cts[1] {
                self.joy2.rx(value);
            }
        } else {
            // Um controlador só: caminho original, sem gating (comportamento
            // preservado bit a bit).
            self.joy.rx(value);
        }
    }
    fn rx_uart_done(&mut self) {
        self.joy.tx_done();
        if self.joy2_enabled {
            self.joy2.tx_done();
        }
    }

    fn take_uart_rx(&mut self) -> Option<u8> {
        self.joy.take_uart_out().or_else(|| {
            if self.joy2_enabled {
                self.joy2.take_uart_out()
            } else {
                None
            }
        })
    }

    fn take_ext_irq(&mut self) -> (bool, bool) {
        let ext2 = self.joy2_enabled && self.joy2.take_ext1_pulse();
        (self.joy.take_ext1_pulse(), ext2)
    }
}

/// O console V.Smile completo.
pub struct VSmile {
    pub spg: Spg200<VSmileIo>,
}

impl VSmile {
    pub fn new(
        sys_rom: Vec<u16>,
        cart_rom: Vec<u16>,
        region_code: u16,
        vtech_logo: bool,
        video_timing: VideoTiming,
    ) -> Self {
        let io = VSmileIo::new(sys_rom, cart_rom, region_code, vtech_logo);
        Self {
            spg: Spg200::new(io, video_timing),
        }
    }

    /// Reset do console: reinicia o SoC e configura as linhas iniciais dos
    /// controladores (RTS alto, CTS baixo → ext1/ext2 IRQ ativos).
    pub fn reset(&mut self) {
        self.spg.reset();
        self.spg.mem.io.reset_io_state();
        self.spg.mem.set_ext1_irq(true);
        self.spg.mem.set_ext2_irq(true);
    }

    /// Habilita a NVRAM do cartucho (Art Studio) com o conteúdo inicial `words`
    /// (carregado do arquivo de save, ou vazio). Ver [`VSmileIo::enable_nvram`].
    pub fn enable_nvram(&mut self, words: Vec<u16>) {
        self.spg.mem.io.enable_nvram(words);
    }

    /// Conteúdo atual da NVRAM, se habilitada (para gravar no arquivo de save).
    pub fn nvram_words(&self) -> Option<&[u16]> {
        self.spg.mem.io.nvram_words()
    }

    /// Se a NVRAM mudou desde o último [`VSmile::clear_nvram_dirty`].
    pub fn nvram_dirty(&self) -> bool {
        self.spg.mem.io.nvram_dirty()
    }

    /// Zera o marcador de NVRAM modificada (após gravar o save).
    pub fn clear_nvram_dirty(&mut self) {
        self.spg.mem.io.clear_nvram_dirty();
    }

    /// Serializa o estado completo do console em bytes (save state).
    ///
    /// Inclui CPU, RAM e todos os periféricos; **não** inclui as ROMs (que são
    /// recarregadas do arquivo) nem a temporização de vídeo. Recarregue com
    /// [`VSmile::load_state`] num console criado com o **mesmo cartucho**.
    pub fn save_state(&self) -> Vec<u8> {
        use crate::save::State;
        let mut w = Vec::new();
        w.extend_from_slice(&SAVE_MAGIC);
        SAVE_VERSION.save(&mut w);
        self.spg.save(&mut w);
        w
    }

    /// Restaura um save state produzido por [`VSmile::save_state`]. Falha (sem
    /// alterar o console) se o cabeçalho/versão não baterem; pode deixar o
    /// console num estado parcial se os bytes estiverem truncados no meio.
    pub fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        use crate::save::State;
        let mut r = crate::save::Reader::new(data);
        let mut magic = [0u8; 8];
        magic.load(&mut r)?;
        if magic != SAVE_MAGIC {
            return Err("save state: assinatura inválida".into());
        }
        let mut ver = 0u32;
        ver.load(&mut r)?;
        if ver != SAVE_VERSION {
            return Err(format!(
                "save state: versão {ver} incompatível (esperado {SAVE_VERSION})"
            ));
        }
        self.spg.load(&mut r)?;
        r.expect_end()
    }

    /// Executa um quadro de vídeo.
    pub fn run_frame(&mut self) {
        self.spg.run_frame();
    }

    /// Framebuffer cru (RGB555, 320×240).
    pub fn framebuffer(&self) -> &[u16] {
        self.spg.framebuffer()
    }

    /// Esvazia o buffer de áudio (amostras estéreo L,R intercaladas, ~281 kHz).
    pub fn take_audio(&mut self) -> &[u16] {
        self.spg.take_audio()
    }

    /// Preenche `out` (320×240×4 bytes) com o quadro em RGBA8888.
    pub fn framebuffer_rgba(&self, out: &mut [u8]) {
        assert!(out.len() >= SCREEN_W * SCREEN_H * 4);
        for (i, &c) in self.framebuffer().iter().enumerate() {
            let (r, g, b) = rgb555_to_rgb888(c);
            out[i * 4] = r;
            out[i * 4 + 1] = g;
            out[i * 4 + 2] = b;
            out[i * 4 + 3] = 0xff;
        }
    }

    pub fn frame_counter(&self) -> i64 {
        self.spg.frame_counter()
    }

    /// Atualiza o estado do controlador 0 (direcional, botões, cores).
    pub fn set_input(&mut self, input: JoyInput) {
        self.spg.mem.io.joy.update_joystick(input);
    }

    /// Pluga/despluga o segundo controlador. Desligado (padrão), o console se
    /// comporta como um V.Smile de um controle só.
    pub fn set_controller2_enabled(&mut self, enabled: bool) {
        self.spg.mem.io.set_controller2_enabled(enabled);
    }

    /// Atualiza o estado do controlador 1 (segundo joystick).
    pub fn set_input2(&mut self, input: JoyInput) {
        self.spg.mem.io.joy2.update_joystick(input);
    }

    /// LEDs atualmente acesos no controlador 0.
    pub fn controller_leds(&self) -> JoyLeds {
        self.spg.mem.io.joy.leds()
    }

    /// LEDs atualmente acesos no controlador 1.
    pub fn controller2_leds(&self) -> JoyLeds {
        self.spg.mem.io.joy2.leds()
    }

    pub fn set_on_button(&mut self, pressed: bool) {
        self.spg.mem.io.on_pressed = pressed;
    }
    pub fn set_off_button(&mut self, pressed: bool) {
        self.spg.mem.io.off_pressed = pressed;
    }
    pub fn set_restart_button(&mut self, pressed: bool) {
        self.spg.mem.io.restart_pressed = pressed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::SystemIo;

    /// Cartucho sintético: habilita a IRQ de vblank e, a cada quadro, o handler
    /// incrementa R2 e o grava na paleta (índice 1). Faz o estado evoluir de
    /// forma determinística a cada quadro, exercitando CPU, IRQ e PPU.
    fn cart_contador() -> Vec<u16> {
        let mut rom = vec![0xFFFFu16; 0x10000];
        rom[0xfff7] = 0x8000; // vetor de reset
        rom[0xfff8] = 0x9000; // vetor de IRQ0 (PPU)
        // Main: liga vblank IRQ (0x2862 = 1), liga IRQ na CPU, loop.
        rom[0x8000] = 0x9309; // LOAD R1, imm16
        rom[0x8001] = 0x0001;
        rom[0x8002] = 0xD119; // STORE R1 -> [0x2862]
        rom[0x8003] = 0x2862;
        rom[0x8004] = 0xf141; // habilita IRQ na CPU
        rom[0x8005] = 0xEE41; // jmp self (loop)
        // Handler: R2 += 1; [0x2b01] = R2; reti.
        rom[0x9000] = 0x0441; // ADD R2, #1
        rom[0x9001] = 0xD11A; // STORE R2 -> [0x2b01] (paleta índice 1)
        rom[0x9002] = 0x2b01;
        rom[0x9003] = 0x9A98; // reti
        rom
    }

    #[test]
    fn save_state_determinismo_e_idempotencia() {
        let cart = cart_contador();

        // Console A: roda 30 quadros, tira o snapshot, roda +40 e serializa.
        let mut a = VSmile::new(dummy_sysrom(), cart.clone(), 0xe, true, VideoTiming::Ntsc);
        a.reset();
        for _ in 0..30 {
            a.run_frame();
        }
        let snap = a.save_state();
        assert!(snap.len() > 20_000, "save state deve ter conteúdo substancial");
        for _ in 0..40 {
            a.run_frame();
        }
        let final_a = a.save_state();

        // Console B: carrega o snapshot num console novo e roda os mesmos +40.
        let mut b = VSmile::new(dummy_sysrom(), cart, 0xe, true, VideoTiming::Ntsc);
        b.reset();
        // Idempotência: logo após carregar, re-serializar deve dar o snapshot.
        b.load_state(&snap).expect("load_state falhou");
        assert_eq!(b.save_state(), snap, "re-serialização após load != snapshot");

        for _ in 0..40 {
            b.run_frame();
        }
        let final_b = b.save_state();

        // Determinismo: se algum campo faltasse no save, os 40 quadros após a
        // restauração divergiriam do console que nunca foi serializado.
        assert_eq!(final_a, final_b, "estado divergiu após restaurar o save");
        // Sanidade: o contador R2 avançou (o programa realmente rodou).
        assert!(b.spg.cpu.regs()[2] >= 60, "o contador deveria ter avançado");
    }

    #[test]
    fn segundo_controlador_opt_in() {
        let cart = cart_contador();

        // Desligado (padrão).
        let mut a = VSmile::new(dummy_sysrom(), cart.clone(), 0xe, true, VideoTiming::Ntsc);
        a.reset();
        for _ in 0..90 {
            a.run_frame();
        }
        let estado_desligado = a.save_state();

        // Ligado + entrada no P2: o estado diverge do caso desligado — prova de
        // que o joy2 está vivo (handshake serial/timers correndo).
        let mut b = VSmile::new(dummy_sysrom(), cart.clone(), 0xe, true, VideoTiming::Ntsc);
        b.set_controller2_enabled(true);
        b.reset();
        b.set_input2(JoyInput {
            x: 5,
            ..Default::default()
        });
        for _ in 0..90 {
            b.run_frame();
        }
        let estado_ligado = b.save_state();
        assert_ne!(
            estado_desligado, estado_ligado,
            "com o controlador 2 ligado o estado deve diferir (joy2 ativo)"
        );

        // Round-trip: o estado do joy2 é serializado por completo.
        let mut c = VSmile::new(dummy_sysrom(), cart, 0xe, true, VideoTiming::Ntsc);
        c.set_controller2_enabled(true);
        c.reset();
        c.load_state(&estado_ligado).expect("load_state falhou");
        assert_eq!(c.save_state(), estado_ligado, "joy2 não round-tripou no save");
    }

    #[test]
    fn load_state_rejeita_lixo() {
        let mut v = VSmile::new(dummy_sysrom(), cart_contador(), 0xe, true, VideoTiming::Ntsc);
        v.reset();
        assert!(v.load_state(b"not a save").is_err(), "assinatura inválida deve falhar");
        assert!(v.load_state(&[]).is_err(), "vazio deve falhar");
    }

    #[test]
    fn nvram_round_trip_e_conversao_bytes() {
        let mut io = VSmileIo::new(vec![0xFFFF; 4], vec![0xFFFF; 4], 0xe, true);

        // Sem NVRAM: CSB2 lê a ROM e escritas são descartadas.
        io.write_csb2(0x10, 0x1234);
        assert_eq!(io.read_csb2(0x10), 0xFFFF, "cartucho comum: CSB2 é ROM");
        assert!(io.nvram_words().is_none());

        // Habilita NVRAM vazia (1 MiW de 0xFFFF).
        io.enable_nvram(Vec::new());
        assert_eq!(io.nvram_words().unwrap().len(), NVRAM_WORDS);
        assert!(!io.nvram_dirty());

        // Escrita agora persiste e marca dirty.
        io.write_csb2(0x10, 0xBEEF);
        io.write_csb2(0x11, 0xCAFE);
        assert_eq!(io.read_csb2(0x10), 0xBEEF);
        assert_eq!(io.read_csb2(0x11), 0xCAFE);
        assert!(io.nvram_dirty());

        // Escrever o mesmo valor não re-suja depois de limpar.
        io.clear_nvram_dirty();
        io.write_csb2(0x10, 0xBEEF);
        assert!(!io.nvram_dirty(), "escrita idêntica não deve sujar");

        // Serializa -> bytes -> desserializa e recarrega noutra instância.
        let bytes = bytes_from_rom_le(io.nvram_words().unwrap());
        assert_eq!(bytes.len(), NVRAM_WORDS * 2);
        let reloaded = rom_from_bytes_le(&bytes);
        let mut io2 = VSmileIo::new(vec![0xFFFF; 4], vec![0xFFFF; 4], 0xe, true);
        io2.enable_nvram(reloaded);
        assert_eq!(io2.read_csb2(0x10), 0xBEEF, "save sobreviveu ao round-trip");
        assert_eq!(io2.read_csb2(0x11), 0xCAFE);
    }
}

impl crate::save::State for VSmileIo {
    fn save(&self, w: &mut Vec<u8>) {
        self.rts.save(w);
        self.cts.save(w);
        self.on_pressed.save(w);
        self.off_pressed.save(w);
        self.restart_pressed.save(w);
        self.joy.save(w);
        self.joy2.save(w);
        self.joy2_enabled.save(w);
        // NVRAM: tag de presença + conteúdo (tamanho fixo quando presente).
        match &self.nvram {
            Some(nv) => {
                true.save(w);
                nv.save(w);
            }
            None => false.save(w),
        }
        self.nvram_dirty.save(w);
    }
    fn load(&mut self, r: &mut crate::save::Reader) -> Result<(), String> {
        self.rts.load(r)?;
        self.cts.load(r)?;
        self.on_pressed.load(r)?;
        self.off_pressed.load(r)?;
        self.restart_pressed.load(r)?;
        self.joy.load(r)?;
        self.joy2.load(r)?;
        self.joy2_enabled.load(r)?;
        let mut has_nvram = false;
        has_nvram.load(r)?;
        if has_nvram != self.nvram.is_some() {
            return Err(
                "save state incompatível: presença de NVRAM difere do console atual".into(),
            );
        }
        if let Some(nv) = &mut self.nvram {
            nv.load(r)?;
        }
        self.nvram_dirty.load(r)?;
        Ok(())
    }
}
