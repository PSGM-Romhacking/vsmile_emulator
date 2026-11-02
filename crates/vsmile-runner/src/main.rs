//! Runner headless do emulador V.Smile.
//!
//! Carrega um cartucho (e opcionalmente a ROM do sistema), executa um número de
//! quadros e grava um screenshot em PPM (P6). Serve para validar o core sem
//! interface gráfica.
//!
//! Uso:
//! ```text
//! vsmile-run --cart JOGO.bin [--sysrom SYSROM.bin] [--region 0xE]
//!            [--frames 120] [--pal] [--out screenshot.ppm]
//! ```

use std::fs;
use std::process::ExitCode;

use vsmile_core::debugger::{Debugger, DisasmLine, Stop};
use vsmile_core::disasm::disassemble;
use vsmile_core::ppu::{SCREEN_H, SCREEN_W};
use vsmile_core::ppu_view;
use vsmile_core::Bus;
use vsmile_core::vsmile::{
    dummy_sysrom, nvram_to_save_bytes, rgb555_to_rgb888, rom_from_bytes_le, rom_from_file,
};
use vsmile_core::{VSmile, VideoTiming};

struct Args {
    cart: Option<String>,
    sysrom: Option<String>,
    nvram: Option<String>,
    /// Carrega este save state antes de rodar os quadros.
    load_state: Option<String>,
    /// Grava um save state neste caminho ao final.
    save_state: Option<String>,
    /// Modo desmontagem: endereço inicial (em palavras). Desliga a emulação.
    disasm: Option<u32>,
    /// Quantas instruções desmontar (`--disasm`).
    disasm_count: u32,
    /// Entra no REPL do depurador após carregar o cartucho.
    debugger: bool,
    /// Modo lote: roda todas as ROMs de um diretório e reporta compatibilidade.
    batch: Option<String>,
    /// Quadros por ROM no modo lote.
    batch_frames: u32,
    /// Grava o status de cada ROM (CSV, sem truncar) neste caminho.
    batch_list: Option<String>,
    /// BIOS a usar no modo lote (senão, usa a sysrom dummy).
    batch_sysrom: Option<String>,
    /// Imprime paleta/backgrounds/sprites da PPU após rodar os quadros.
    ppu_info: bool,
    /// Grava a imagem da paleta (grade 16×16) neste caminho.
    ppu_palette: Option<String>,
    /// Grava o *tile sheet* (tiles do BG escolhido) neste caminho.
    ppu_tiles: Option<String>,
    /// Qual background usa para os parâmetros do *tile sheet* (0 ou 1).
    ppu_tiles_bg: usize,
    region: u16,
    frames: u32,
    pal: bool,
    out: String,
    debug: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            cart: None,
            sysrom: None,
            nvram: None,
            load_state: None,
            save_state: None,
            disasm: None,
            disasm_count: 32,
            debugger: false,
            batch: None,
            batch_frames: 240,
            batch_list: None,
            batch_sysrom: None,
            ppu_info: false,
            ppu_palette: None,
            ppu_tiles: None,
            ppu_tiles_bg: 0,
            region: 0xe, // UK English (padrão do veesem)
            frames: 120,
            pal: false,
            out: "screenshot.bmp".to_string(),
            debug: false,
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--cart" => args.cart = Some(it.next().ok_or("--cart requer um caminho")?),
            "--sysrom" => args.sysrom = Some(it.next().ok_or("--sysrom requer um caminho")?),
            "--nvram" => args.nvram = Some(it.next().ok_or("--nvram requer um caminho")?),
            "--load-state" => {
                args.load_state = Some(it.next().ok_or("--load-state requer um caminho")?)
            }
            "--save-state" => {
                args.save_state = Some(it.next().ok_or("--save-state requer um caminho")?)
            }
            "--disasm" => {
                let v = it.next().ok_or("--disasm requer um endereço")?;
                let v = v.strip_prefix("0x").unwrap_or(&v);
                args.disasm = Some(u32::from_str_radix(v, 16).map_err(|e| e.to_string())?);
            }
            "--disasm-count" => {
                let v = it.next().ok_or("--disasm-count requer um valor")?;
                args.disasm_count = v.parse().map_err(|_| "--disasm-count inválido".to_string())?;
            }
            "--debugger" => args.debugger = true,
            "--batch" => args.batch = Some(it.next().ok_or("--batch requer um diretório")?),
            "--batch-frames" => {
                let v = it.next().ok_or("--batch-frames requer um valor")?;
                args.batch_frames = v.parse().map_err(|_| "--batch-frames inválido".to_string())?;
            }
            "--batch-list" => {
                args.batch_list = Some(it.next().ok_or("--batch-list requer um caminho")?)
            }
            "--batch-sysrom" => {
                args.batch_sysrom = Some(it.next().ok_or("--batch-sysrom requer um caminho")?)
            }
            "--ppu-info" => args.ppu_info = true,
            "--ppu-palette" => {
                args.ppu_palette = Some(it.next().ok_or("--ppu-palette requer um caminho")?)
            }
            "--ppu-tiles" => {
                args.ppu_tiles = Some(it.next().ok_or("--ppu-tiles requer um caminho")?)
            }
            "--ppu-tiles-bg" => {
                let v = it.next().ok_or("--ppu-tiles-bg requer 0 ou 1")?;
                args.ppu_tiles_bg = v.parse().map_err(|_| "--ppu-tiles-bg inválido".to_string())?;
            }
            "--region" => {
                let v = it.next().ok_or("--region requer um valor")?;
                let v = v.strip_prefix("0x").unwrap_or(&v);
                args.region = u16::from_str_radix(v, 16).map_err(|e| e.to_string())?;
            }
            "--frames" => {
                let v = it.next().ok_or("--frames requer um valor")?;
                args.frames = v.parse().map_err(|_| "--frames inválido".to_string())?;
            }
            "--pal" => args.pal = true,
            "--debug" => args.debug = true,
            "--out" => args.out = it.next().ok_or("--out requer um caminho")?,
            "-h" | "--help" => return Err("help".to_string()),
            other => return Err(format!("argumento desconhecido: {other}")),
        }
    }
    Ok(args)
}

fn write_ppm(path: &str, fb: &[u16]) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(16 + SCREEN_W * SCREEN_H * 3);
    out.extend_from_slice(format!("P6\n{SCREEN_W} {SCREEN_H}\n255\n").as_bytes());
    for &c in fb {
        let (r, g, b) = rgb555_to_rgb888(c);
        out.push(r);
        out.push(g);
        out.push(b);
    }
    fs::write(path, out)
}

/// Escreve um BMP 24 bits da tela (320×240).
fn write_bmp(path: &str, fb: &[u16]) -> std::io::Result<()> {
    write_bmp_sized(path, fb, SCREEN_W, SCREEN_H)
}

/// Escreve um BMP 24 bits de dimensão arbitrária (RGB555 → BGR, bottom-up).
/// Cada linha é preenchida (*padding*) a múltiplo de 4 bytes, como manda o BMP.
fn write_bmp_sized(path: &str, fb: &[u16], w: usize, h: usize) -> std::io::Result<()> {
    let row_bytes = w * 3;
    let padded = (row_bytes + 3) & !3; // múltiplo de 4
    let pad = padded - row_bytes;
    let pixel_data = padded * h;
    let file_size = 54 + pixel_data;

    let mut out = Vec::with_capacity(file_size);
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // reservado
    out.extend_from_slice(&54u32.to_le_bytes()); // offset dos dados
    // BITMAPINFOHEADER
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes()); // positivo = bottom-up
    out.extend_from_slice(&1u16.to_le_bytes()); // planos
    out.extend_from_slice(&24u16.to_le_bytes()); // bpp
    out.extend_from_slice(&0u32.to_le_bytes()); // sem compressão
    out.extend_from_slice(&(pixel_data as u32).to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes()); // ~72 DPI
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    // Dados: BGR, de baixo para cima, com padding por linha.
    for y in (0..h).rev() {
        for x in 0..w {
            let (r, g, b) = rgb555_to_rgb888(fb[y * w + x]);
            out.push(b);
            out.push(g);
            out.push(r);
        }
        for _ in 0..pad {
            out.push(0);
        }
    }
    fs::write(path, out)
}

fn write_image(path: &str, fb: &[u16]) -> std::io::Result<()> {
    if path.to_ascii_lowercase().ends_with(".ppm") {
        write_ppm(path, fb)
    } else {
        write_bmp(path, fb)
    }
}

/// Procura, no diretório de `ref_path`, uma BIOS de V.Smile **padrão**
/// (ignora Baby/Motion), preferindo a versão mais nova (v103 > v102 > v100).
/// Serve para o auto-boot via BIOS sem o usuário precisar passar `--sysrom`.
fn find_bios(ref_path: &str) -> Option<std::path::PathBuf> {
    let dir = std::path::Path::new(ref_path).parent().filter(|d| !d.as_os_str().is_empty());
    let dir = dir.unwrap_or_else(|| std::path::Path::new("."));
    let mut best: Option<(u32, std::path::PathBuf)> = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        let name = match p.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_ascii_lowercase(),
            None => continue,
        };
        if name.contains("[bios]")
            && name.contains("v.smile")
            && !name.contains("baby")
            && !name.contains("motion")
        {
            let ver = if name.contains("v103") {
                103
            } else if name.contains("v102") {
                102
            } else if name.contains("v100") {
                100
            } else {
                1
            };
            if best.as_ref().is_none_or(|(v, _)| ver > *v) {
                best = Some((ver, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Resolve a sysrom: `--sysrom` explícito > BIOS auto-detectada na pasta do
/// cartucho > dummy. Nunca obriga: se não houver BIOS, usa a dummy.
fn load_sysrom(explicit: Option<&str>, cart_path: &str) -> Result<Vec<u16>, String> {
    if let Some(p) = explicit {
        let b = fs::read(p).map_err(|e| format!("lendo sysrom: {e}"))?;
        println!("sysrom: {p} ({} bytes)", b.len());
        return rom_from_file(&b).map_err(|e| format!("sysrom: {e}"));
    }
    if let Some(bios) = find_bios(cart_path) {
        let b = fs::read(&bios).map_err(|e| format!("lendo BIOS: {e}"))?;
        println!("sysrom: BIOS auto-detectada — {}", bios.display());
        return rom_from_file(&b).map_err(|e| format!("BIOS: {e}"));
    }
    println!("sysrom: dummy (nenhuma BIOS na pasta; jogos dão boot pelo cartucho)");
    Ok(dummy_sysrom())
}

fn usage() {
    eprintln!(
        "Uso: vsmile-run --cart JOGO.bin|JOGO.zip [--sysrom SYSROM.bin] [--nvram SAVE.sav] \
         [--load-state S.state] [--save-state S.state] [--disasm 0x8000 [--disasm-count 32]] \
         [--debugger] [--batch DIR [--batch-frames 240]] \
         [--ppu-info] [--ppu-palette P.bmp] [--ppu-tiles T.bmp [--ppu-tiles-bg 0]] \
         [--region 0xE] [--frames 120] [--pal] [--out screenshot.ppm]"
    );
}

fn run() -> Result<(), String> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if e != "help" {
                eprintln!("erro: {e}");
            }
            usage();
            return Err(e);
        }
    };

    // Modo lote: roda todas as ROMs de um diretório e encerra.
    if let Some(dir) = &args.batch {
        return run_batch(
            dir,
            args.batch_frames,
            args.batch_list.as_deref(),
            args.batch_sysrom.as_deref(),
        );
    }

    let cart_path = args.cart.as_ref().ok_or_else(|| {
        usage();
        "--cart é obrigatório".to_string()
    })?;

    let cart_bytes = fs::read(cart_path).map_err(|e| format!("lendo cartucho: {e}"))?;
    let cart = rom_from_file(&cart_bytes).map_err(|e| format!("cartucho: {e}"))?;
    println!("cartucho: {} palavras ({} bytes no arquivo)", cart.len(), cart_bytes.len());

    // Modo desmontagem: lista instruções a partir de um endereço e encerra.
    if let Some(start) = args.disasm {
        disasm_dump(&cart, start, args.disasm_count);
        return Ok(());
    }

    let sysrom = load_sysrom(args.sysrom.as_deref(), cart_path)?;

    let timing = if args.pal {
        VideoTiming::Pal
    } else {
        VideoTiming::Ntsc
    };

    let mut vsmile = VSmile::new(sysrom, cart, args.region, true, timing);

    // NVRAM (Art Studio): carrega o save antes do reset, se pedido.
    if let Some(path) = &args.nvram {
        let words = match fs::read(path) {
            Ok(bytes) => {
                println!("nvram: carregada de {path} ({} bytes)", bytes.len());
                rom_from_bytes_le(&bytes)
            }
            Err(_) => {
                println!("nvram: {path} (novo)");
                Vec::new()
            }
        };
        vsmile.enable_nvram(words);
    }

    if args.debug {
        vsmile.spg.mem.enable_read_hist();
    }
    vsmile.reset();

    // Carrega um save state antes de rodar, se pedido.
    if let Some(path) = &args.load_state {
        let bytes = fs::read(path).map_err(|e| format!("lendo save state: {e}"))?;
        vsmile.load_state(&bytes).map_err(|e| format!("save state: {e}"))?;
        println!("save state carregado de {path} ({} bytes)", bytes.len());
    }

    // REPL do depurador: toma o controle e encerra ao sair.
    if args.debugger {
        debug_repl(&mut vsmile);
        return Ok(());
    }

    println!("rodando {} quadros ({timing:?})...", args.frames);
    let mut max_nonblack = 0usize;
    let mut audio_samples = 0u64;
    let mut audio_nonsilent = 0u64;
    let mut audio_peak = 0i32;
    for f in 0..args.frames {
        vsmile.run_frame();
        if args.debug {
            for s in vsmile.take_audio() {
                audio_samples += 1;
                let v = (*s ^ 0x8000) as i16 as i32;
                if v != 0 {
                    audio_nonsilent += 1;
                }
                audio_peak = audio_peak.max(v.abs());
            }
        }
        if args.debug {
            let nb = vsmile.framebuffer().iter().filter(|&&c| c & 0x7fff != 0).count();
            max_nonblack = max_nonblack.max(nb);
            if f % 60 == 0 || f == args.frames - 1 {
                let pc = vsmile.spg.cpu.cs_pc();
                let mem = &mut vsmile.spg.mem;
                let bg0 = mem.ppu.read_reg(0x2813).unwrap_or(0);
                let bg1 = mem.ppu.read_reg(0x2819).unwrap_or(0);
                let spr = mem.ppu.read_reg(0x2842).unwrap_or(0);
                eprintln!("f{f:>4} cs_pc={pc:06x} bg0={bg0:04x} bg1={bg1:04x} spr={spr} nonblack={nb}");
            }
        }
    }

    if args.debug {
        eprintln!("max_nonblack: {max_nonblack}");
        eprintln!(
            "áudio: {audio_samples} amostras, {audio_nonsilent} não-silenciosas, pico={audio_peak}"
        );
        let csb = vsmile.spg.mem.io.csb_reads;
        eprintln!(
            "chip-select reads: ROMCSB={} CSB1={} CSB2={} CSB3(sysrom)={}",
            csb[0], csb[1], csb[2], csb[3]
        );
        eprintln!("instruções desconhecidas (NOP): {}", vsmile.spg.cpu.unknown_ops);
        if let Some((pc, iw)) = vsmile.spg.cpu.dbg_first_unknown {
            eprintln!("primeira desconhecida: CS:PC={pc:06x} iw={iw:04x}");
        }
        if let Some(ring) = vsmile.spg.cpu.dbg_pre_unknown {
            eprintln!("-- 16 instruções antes da divergência --");
            for (pc, iw) in ring {
                let insn = disassemble(iw, 0, pc);
                eprintln!("  {pc:06X}: {iw:04X}  {}", insn.text);
            }
        }
        if let Some((pc, iw, target)) = vsmile.spg.cpu.dbg_bigjump {
            let op0 = (iw >> 12) & 0xf;
            let op1n = (iw >> 3) & 0x3f;
            eprintln!(
                "primeiro salto p/ banco alto: de CS:PC={pc:06x} iw={iw:04x} \
                 (op0={op0:x} op1n={op1n}) -> {target:06x}"
            );
        }
        if let Some(hist) = vsmile.spg.mem.io_read_hist.as_ref() {
            let mut top: Vec<(u32, u64)> = hist
                .iter()
                .enumerate()
                .filter(|(_, &n)| n > 0)
                .map(|(i, &n)| (0x2800 + i as u32, n))
                .collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            eprintln!("-- top leituras de I/O --");
            for (addr, n) in top.iter().take(15) {
                eprintln!("  {addr:04x}: {n}");
            }
        }
    }

    write_image(&args.out, vsmile.framebuffer()).map_err(|e| format!("gravando imagem: {e}"))?;
    println!(
        "quadros desenhados: {} | screenshot: {}",
        vsmile.frame_counter(),
        args.out
    );

    // Inspeção da PPU (depurador visual).
    if args.ppu_info {
        ppu_print_info(&vsmile);
    }
    if let Some(path) = &args.ppu_palette {
        let (img, w, h) = ppu_view::palette_image(&vsmile.spg.mem.ppu.palette(), 12);
        write_bmp_sized(path, &img, w, h).map_err(|e| format!("gravando paleta: {e}"))?;
        println!("paleta da PPU: {path} ({w}×{h})");
    }
    if let Some(path) = &args.ppu_tiles {
        ppu_write_tiles(&mut vsmile, args.ppu_tiles_bg, path)?;
    }

    // Grava um save state ao final, se pedido.
    if let Some(path) = &args.save_state {
        let bytes = vsmile.save_state();
        fs::write(path, &bytes).map_err(|e| format!("gravando save state: {e}"))?;
        println!("save state gravado em {path} ({} bytes)", bytes.len());
    }

    // Persiste a NVRAM se o cartucho tiver gravado algo.
    if let (Some(path), true) = (&args.nvram, vsmile.nvram_dirty()) {
        if let Some(words) = vsmile.nvram_words() {
            let bytes = nvram_to_save_bytes(words);
            fs::write(path, &bytes).map_err(|e| format!("gravando nvram: {e}"))?;
            println!("nvram: salva em {path} ({} bytes)", bytes.len());
        }
    }
    Ok(())
}

/// Desmonta `count` instruções a partir de `start` (endereço em palavras) na
/// imagem do cartucho e imprime `END: IW  mnemônico`.
fn disasm_dump(cart: &[u16], start: u32, count: u32) {
    let mut addr = start;
    for _ in 0..count {
        let iw = cart.get(addr as usize).copied().unwrap_or(0xFFFF);
        let next = cart.get(addr as usize + 1).copied().unwrap_or(0xFFFF);
        let insn = disassemble(iw, next, addr);
        if insn.words == 2 {
            println!("{addr:06X}: {iw:04X} {next:04X}  {}", insn.text);
        } else {
            println!("{addr:06X}: {iw:04X}       {}", insn.text);
        }
        addr = addr.wrapping_add(insn.words);
    }
}

// ---- Modo lote (compatibilidade) ----------------------------------------

/// Resultado de rodar uma ROM no modo lote.
enum RomResult {
    /// Bootou limpo: nenhuma instrução desconhecida e desenhou algo na tela.
    Ok { nonblack: usize },
    /// Rodou sem desenhar nada (tela preta).
    Black,
    /// Executou instruções desconhecidas (opcode não implementado).
    Unknown { count: u64, first: Option<(u32, u16)> },
    /// Falha ao carregar/descomprimir o cartucho.
    LoadError(String),
    /// A emulação deu panic (crash a investigar).
    Panic,
}

/// Roda `frames` quadros numa ROM e classifica o resultado. Isola panics para
/// que uma ROM problemática não derrube o lote inteiro.
fn run_one_rom(path: &std::path::Path, frames: u32, sysrom: &[u16]) -> RomResult {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return RomResult::LoadError(e.to_string()),
    };
    let cart = match rom_from_file(&bytes) {
        Ok(c) => c,
        Err(e) => return RomResult::LoadError(e),
    };
    let sysrom = sysrom.to_vec();

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut vsmile = VSmile::new(sysrom, cart, 0xe, true, VideoTiming::Ntsc);
        vsmile.reset();
        let mut max_nonblack = 0usize;
        for _ in 0..frames {
            vsmile.run_frame();
            let nb = vsmile.framebuffer().iter().filter(|&&c| c & 0x7fff != 0).count();
            max_nonblack = max_nonblack.max(nb);
        }
        (
            vsmile.spg.cpu.unknown_ops,
            vsmile.spg.cpu.dbg_first_unknown,
            max_nonblack,
        )
    }));

    match outcome {
        Err(_) => RomResult::Panic,
        Ok((unknown, first, nonblack)) => {
            if unknown > 0 {
                RomResult::Unknown { count: unknown, first }
            } else if nonblack > 500 {
                RomResult::Ok { nonblack }
            } else {
                RomResult::Black
            }
        }
    }
}

/// Roda todas as `.zip`/`.bin` de um diretório e imprime um relatório de
/// compatibilidade. Silencia os hooks de panic para o relatório ficar limpo.
/// Detecta ROMs marcadas como *bad dump* na convenção No-Intro/GoodTools:
/// `[b]`, `[b1]`, `[b2]`… — cópias corrompidas que não se espera que rodem.
fn is_bad_dump(name: &str) -> bool {
    let b = name.as_bytes();
    for i in 0..b.len().saturating_sub(2) {
        if b[i] == b'[' && b[i + 1] == b'b' {
            // Aceita "[b]" ou "[b" + dígitos + "]".
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j < b.len() && b[j] == b']' {
                return true;
            }
        }
    }
    false
}

/// Detecta a variante pelo nome do arquivo.
fn variant_of(name: &str) -> &'static str {
    let l = name.to_ascii_lowercase();
    if l.contains("v.smile baby") {
        "baby"
    } else if l.contains("v.smile motion") {
        "motion"
    } else {
        "padrao"
    }
}

fn run_batch(
    dir: &str,
    frames: u32,
    list_path: Option<&str>,
    sysrom_path: Option<&str>,
) -> Result<(), String> {
    // Carrega a BIOS uma vez: explícita > auto-detectada na pasta > dummy.
    // Um caminho fictício dentro de `dir` faz o find_bios olhar nessa pasta.
    let probe = format!("{dir}/x");
    let sysrom = load_sysrom(sysrom_path, &probe)?;
    let mut paths: Vec<std::path::PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("lendo diretório: {e}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()),
                Some(ref e) if e == "zip" || e == "bin"
            )
        })
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("nenhuma ROM (.zip/.bin) em {dir}"));
    }

    // Silencia a impressão padrão de panic (o catch_unwind já trata).
    std::panic::set_hook(Box::new(|_| {}));

    let total = paths.len();
    println!("modo lote: {total} ROMs, {frames} quadros cada\n");

    let mut ok = 0usize;
    let mut bad_dumps = 0usize;
    let mut black = Vec::new();
    let mut unknown = Vec::new();
    let mut load_err = Vec::new();
    let mut panic = Vec::new();
    // Agrupa opcodes desconhecidos para priorizar o que implementar.
    let mut opcode_hist: std::collections::BTreeMap<u16, u32> = std::collections::BTreeMap::new();
    // Lista completa para o CSV: (nome, variante, status, detalhe).
    let mut full: Vec<(String, &'static str, &'static str, String)> = Vec::with_capacity(total);

    for (i, path) in paths.iter().enumerate() {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?").to_string();
        let variant = variant_of(&name);

        // Ignora bad dumps e arquivos de BIOS: não são jogos / não contam.
        if is_bad_dump(&name) || name.contains("[BIOS]") {
            bad_dumps += 1;
            let tag = if name.contains("[BIOS]") { "bios" } else { "bad_dump" };
            full.push((name, variant, tag, String::new()));
            if (i + 1) % 50 == 0 || i + 1 == total {
                eprintln!("  …{}/{total}", i + 1);
            }
            continue;
        }

        let (status, detail): (&'static str, String) = match run_one_rom(path, frames, &sysrom) {
            RomResult::Ok { nonblack } => {
                ok += 1;
                ("ok", format!("nonblack={nonblack}"))
            }
            RomResult::Black => {
                black.push(name.clone());
                ("tela_preta", String::new())
            }
            RomResult::Unknown { count, first } => {
                if let Some((_, iw)) = first {
                    *opcode_hist.entry(iw).or_default() += 1;
                }
                unknown.push((name.clone(), count, first));
                let d = match first {
                    Some((pc, iw)) => format!("{count}x CS:PC={pc:06X} iw={iw:04X}"),
                    None => format!("{count}x"),
                };
                ("opcode_desconhecido", d)
            }
            RomResult::LoadError(e) => {
                load_err.push((name.clone(), e.clone()));
                ("erro_carregamento", e)
            }
            RomResult::Panic => {
                panic.push(name.clone());
                ("panic", String::new())
            }
        };
        full.push((name, variant, status, detail));
        if (i + 1) % 50 == 0 || i + 1 == total {
            eprintln!("  …{}/{total}", i + 1);
        }
    }

    // CSV completo (sem truncar), se pedido.
    if let Some(path) = list_path {
        let mut csv = String::from("arquivo;variante;status;detalhe\n");
        for (name, variant, status, detail) in &full {
            // Aspas no nome para não quebrar com vírgulas/ponto-e-vírgula.
            csv.push_str(&format!("\"{name}\";{variant};{status};{detail}\n"));
        }
        fs::write(path, csv).map_err(|e| format!("gravando lista: {e}"))?;
        println!("lista completa das {total} ROMs: {path}");
    }

    // Denominador de compatibilidade = acervo válido (sem os bad dumps).
    let valid = total - bad_dumps;
    let pct = |n: usize| if valid == 0 { 0.0 } else { n as f64 * 100.0 / valid as f64 };
    println!("\n==== relatório de compatibilidade ====");
    println!("  acervo: {total} ROMs ({bad_dumps} excluída(s): bad dump/BIOS → {valid} jogos)");
    println!("  boot limpo (desenhou algo): {ok}/{valid} ({:.1}%)", pct(ok));
    println!("  tela preta:                 {}", black.len());
    println!("  opcode desconhecido:        {}", unknown.len());
    println!("  erro de carregamento:       {}", load_err.len());
    println!("  panic (crash):              {}", panic.len());

    // Taxa de boot por variante de console (exclui bad dumps).
    println!("\n-- boot limpo por variante (sem bad dumps) --");
    for v in ["padrao", "motion", "baby"] {
        let tot = full
            .iter()
            .filter(|r| r.1 == v && r.2 != "bad_dump" && r.2 != "bios")
            .count();
        if tot == 0 {
            continue;
        }
        let okv = full.iter().filter(|r| r.1 == v && r.2 == "ok").count();
        println!("  {v:7}: {okv}/{tot} ({:.1}%)", okv as f64 * 100.0 / tot as f64);
    }

    if !opcode_hist.is_empty() {
        println!("\n-- opcodes desconhecidos (1ª ocorrência) por frequência --");
        let mut v: Vec<_> = opcode_hist.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        for (iw, n) in v.iter().take(20) {
            let op0 = (iw >> 12) & 0xf;
            let op1n = (iw >> 3) & 0x3f;
            println!("  iw=0x{iw:04X} (op0={op0:x} op1n={op1n:2}) em {n} ROM(s)");
        }
    }
    let dump = |title: &str, list: &[String]| {
        if !list.is_empty() {
            println!("\n-- {title} --");
            for n in list.iter().take(30) {
                println!("  {n}");
            }
            if list.len() > 30 {
                println!("  ... (+{})", list.len() - 30);
            }
        }
    };
    dump("tela preta", &black);
    dump("panic", &panic);
    if !unknown.is_empty() {
        println!("\n-- opcode desconhecido (ROM: contagem, 1ª instr.) --");
        for (n, c, first) in unknown.iter().take(30) {
            match first {
                Some((pc, iw)) => println!("  {n}: {c}x, 1ª CS:PC={pc:06X} iw={iw:04X}"),
                None => println!("  {n}: {c}x"),
            }
        }
        if unknown.len() > 30 {
            println!("  ... (+{})", unknown.len() - 30);
        }
    }
    if !load_err.is_empty() {
        println!("\n-- erro de carregamento --");
        for (n, e) in load_err.iter().take(20) {
            println!("  {n}: {e}");
        }
    }
    Ok(())
}

// ---- Inspeção da PPU ----------------------------------------------------

fn ppu_print_info(vsmile: &VSmile) {
    let ppu = &vsmile.spg.mem.ppu;

    println!("-- backgrounds --");
    for bg in ppu.bg_info() {
        println!(
            "BG{}: {} bitmap={} {}bpp tile={}x{} pal={} scroll=({},{}) \
             tilemap=0x{:04X} attrmap=0x{:04X} seg=0x{:04X}",
            bg.index,
            if bg.enabled { "on " } else { "off" },
            bg.bitmap as u8,
            bg.bpp,
            bg.tile_w,
            bg.tile_h,
            bg.palette,
            bg.xscroll,
            bg.yscroll,
            bg.tile_map_ptr,
            bg.attribute_map_ptr,
            bg.segment_ptr,
        );
    }

    println!(
        "-- sprites (camada {}) --",
        if ppu.sprites_enabled() { "ligada" } else { "desligada" }
    );
    let sprites = ppu.sprites_info();
    let ativos: Vec<_> = sprites.iter().filter(|s| s.ch != 0).collect();
    println!("{} sprites ativos (ch != 0) de {}", ativos.len(), sprites.len());
    for s in ativos.iter().take(24) {
        println!(
            "  #{:<3} ch=0x{:04X} pos=({:>3},{:>3}) {}x{} {}bpp pal={} depth={} h{}v{}{}",
            s.index,
            s.ch,
            s.x,
            s.y,
            s.tile_w,
            s.tile_h,
            s.bpp,
            s.palette,
            s.depth,
            s.hflip as u8,
            s.vflip as u8,
            if s.blend { " blend" } else { "" },
        );
    }
    if ativos.len() > 24 {
        println!("  ... (+{} sprites)", ativos.len() - 24);
    }

    // Resumo da paleta: quantas cores não são preto puro.
    let pal = ppu.palette();
    let usadas = pal.iter().filter(|&&c| c & 0x7fff != 0).count();
    println!("-- paleta -- {usadas}/256 entradas não-pretas");
}

fn ppu_write_tiles(vsmile: &mut VSmile, bg_index: usize, path: &str) -> Result<(), String> {
    let bgs = vsmile.spg.mem.ppu.bg_info();
    let bg = bgs[bg_index.min(1)];
    let palette = vsmile.spg.mem.ppu.palette();
    if bg.bitmap {
        return Err(format!("BG{bg_index} está em modo bitmap — sem tiles para desenhar"));
    }
    // 256 tiles em 16 colunas, fundo cinza-escuro.
    let (img, w, h) = ppu_view::tile_sheet(
        |a| vsmile.spg.mem.read_word(a),
        bg.segment_ptr,
        bg.bpp,
        bg.tile_w,
        bg.tile_h,
        bg.palette,
        &palette,
        0,
        256,
        16,
        0x2108, // cinza-escuro
    );
    write_bmp_sized(path, &img, w, h).map_err(|e| format!("gravando tiles: {e}"))?;
    println!(
        "tile sheet (BG{bg_index}, seg=0x{:04X}, {}bpp, {}x{}): {path} ({w}×{h})",
        bg.segment_ptr, bg.bpp, bg.tile_w, bg.tile_h
    );
    Ok(())
}

// ---- REPL do depurador --------------------------------------------------

fn parse_num(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse().ok()
    }
}

fn print_dbg_help() {
    println!(
        "comandos do depurador:\n  \
         s [n]        passo: executa n instruções (padrão 1)\n  \
         c [max]      continua até breakpoint/watchpoint (padrão 50M instr.)\n  \
         f [n]        avança n quadros inteiros (ignora breakpoints)\n  \
         b [addr]     lista breakpoints, ou adiciona um em addr (CS:PC)\n  \
         db addr      remove o breakpoint em addr\n  \
         w [addr]     lista watchpoints de RAM, ou adiciona um em addr\n  \
         dw addr      remove o watchpoint em addr\n  \
         r            mostra os registradores\n  \
         m addr [n]   dump de n palavras de RAM (padrão 8)\n  \
         d [addr] [n] desmonta n instruções (padrão: PC atual, 8)\n  \
         h            esta ajuda\n  \
         q            sair\n  \
         (endereços em hex com 0x ou decimal)"
    );
}

fn show_location(vsmile: &mut VSmile, dbg: &Debugger) {
    let pc = vsmile.spg.cpu.cs_pc();
    let lines = dbg.disassemble(&mut vsmile.spg, pc, 1);
    if let Some(l) = lines.first() {
        println!("=> {}", fmt_disasm_line(l));
    }
}

fn fmt_disasm_line(l: &DisasmLine) -> String {
    if l.insn.words == 2 {
        format!("{:06X}: {:04X} {:04X}  {}", l.addr, l.word, l.next, l.insn.text)
    } else {
        format!("{:06X}: {:04X}       {}", l.addr, l.word, l.insn.text)
    }
}

fn print_regs(vsmile: &VSmile) {
    let r = vsmile.spg.cpu.regs();
    let sr = r[6];
    let (n, z, s, c) = (
        (sr >> 9) & 1,
        (sr >> 8) & 1,
        (sr >> 7) & 1,
        (sr >> 6) & 1,
    );
    println!(
        "SP={:04X} R1={:04X} R2={:04X} R3={:04X} R4={:04X} BP={:04X} SR={:04X} PC={:04X}",
        r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7]
    );
    println!(
        "CS:PC={:06X}  DS={:X}  flags: N={n} Z={z} S={s} C={c}",
        vsmile.spg.cpu.cs_pc(),
        vsmile.spg.cpu.ds()
    );
}

fn report_stop(stop: &Stop) {
    match stop {
        Stop::Breakpoint(pc) => println!("* breakpoint em {pc:06X}"),
        Stop::Watchpoint { addr, old, new } => {
            println!("* watchpoint: RAM[{addr:04X}] {old:04X} -> {new:04X}")
        }
        Stop::Limit => println!("* limite de instruções atingido"),
    }
}

fn debug_repl(vsmile: &mut VSmile) {
    use std::io::{BufRead, Write};

    let mut dbg = Debugger::new();
    println!("depurador V.Smile — 'h' para ajuda, 'q' para sair");
    show_location(vsmile, &dbg);

    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        print!("(dbg) ");
        let _ = std::io::stdout().flush();
        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break, // EOF (Ctrl-D / fim do pipe)
        };
        let tok: Vec<&str> = line.split_whitespace().collect();
        match tok.as_slice() {
            [] => {}
            ["q"] | ["quit"] => break,
            ["h"] | ["help"] | ["?"] => print_dbg_help(),
            ["r"] | ["regs"] => print_regs(vsmile),
            ["b"] => {
                let bps: Vec<u32> = dbg.breakpoints().collect();
                if bps.is_empty() {
                    println!("(nenhum breakpoint)");
                } else {
                    for b in bps {
                        println!("  bp {b:06X}");
                    }
                }
            }
            ["b", a] => match parse_num(a) {
                Some(x) => {
                    dbg.add_breakpoint(x);
                    println!("breakpoint em {x:06X}");
                }
                None => println!("endereço inválido: {a}"),
            },
            ["db", a] => match parse_num(a) {
                Some(x) if dbg.remove_breakpoint(x) => println!("removido breakpoint {x:06X}"),
                Some(x) => println!("não havia breakpoint em {x:06X}"),
                None => println!("endereço inválido: {a}"),
            },
            ["w"] => {
                let wps: Vec<u32> = dbg.watchpoints().collect();
                if wps.is_empty() {
                    println!("(nenhum watchpoint)");
                } else {
                    for w in wps {
                        println!("  watch RAM[{w:04X}]");
                    }
                }
            }
            ["w", a] => match parse_num(a) {
                Some(x) => {
                    dbg.add_watchpoint(x);
                    println!("watchpoint em RAM[{x:04X}]");
                }
                None => println!("endereço inválido: {a}"),
            },
            ["dw", a] => match parse_num(a) {
                Some(x) if dbg.remove_watchpoint(x) => println!("removido watchpoint {x:04X}"),
                Some(x) => println!("não havia watchpoint em {x:04X}"),
                None => println!("endereço inválido: {a}"),
            },
            ["s"] | ["s", _] => {
                let n = tok.get(1).and_then(|s| parse_num(s)).unwrap_or(1);
                for _ in 0..n {
                    let pc = vsmile.spg.cpu.cs_pc();
                    let line = dbg.disassemble(&mut vsmile.spg, pc, 1);
                    if let Some(l) = line.first() {
                        println!("{}", fmt_disasm_line(l));
                    }
                    if let Some(stop) = dbg.step(&mut vsmile.spg) {
                        report_stop(&stop);
                        break;
                    }
                }
                show_location(vsmile, &dbg);
            }
            ["c"] | ["c", _] => {
                let max = tok.get(1).and_then(|s| parse_num(s)).unwrap_or(50_000_000) as u64;
                let stop = dbg.run(&mut vsmile.spg, max);
                report_stop(&stop);
                show_location(vsmile, &dbg);
            }
            ["f"] | ["f", _] => {
                let n = tok.get(1).and_then(|s| parse_num(s)).unwrap_or(1);
                for _ in 0..n {
                    vsmile.run_frame();
                }
                println!("quadro atual: {}", vsmile.frame_counter());
                show_location(vsmile, &dbg);
            }
            ["m", a] | ["m", a, _] => {
                let addr = match parse_num(a) {
                    Some(x) => x,
                    None => {
                        println!("endereço inválido: {a}");
                        continue;
                    }
                };
                let n = tok.get(2).and_then(|s| parse_num(s)).unwrap_or(8);
                let words = Debugger::read_ram(&vsmile.spg, addr, n);
                for (i, chunk) in words.chunks(8).enumerate() {
                    let base = addr + i as u32 * 8;
                    let hex: Vec<String> = chunk.iter().map(|w| format!("{w:04X}")).collect();
                    println!("{base:04X}: {}", hex.join(" "));
                }
            }
            ["d"] | ["d", _] | ["d", _, _] => {
                let addr = tok
                    .get(1)
                    .and_then(|s| parse_num(s))
                    .unwrap_or_else(|| vsmile.spg.cpu.cs_pc());
                let n = tok.get(2).and_then(|s| parse_num(s)).unwrap_or(8);
                for l in dbg.disassemble(&mut vsmile.spg, addr, n) {
                    println!("{}", fmt_disasm_line(&l));
                }
            }
            _ => println!("comando desconhecido; 'h' para ajuda"),
        }
    }
    println!("saindo do depurador.");
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
