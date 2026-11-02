//! Binding **WebAssembly** do emulador V.Smile.
//!
//! Expõe uma ABI em C mínima (sem `wasm-bindgen`) sobre [`vsmile_core::VSmile`],
//! para um frontend de navegador em `<canvas>` + Web Audio. Como o núcleo é
//! `std`-only e **sem `unsafe`**, ele compila para `wasm32-unknown-unknown` sem
//! mudança nenhuma; o `unsafe` vive só aqui, na ponte de ponteiros do ABI (não
//! no emulador).
//!
//! Fluxo no JS:
//! 1. `vsmile_alloc(len)` → ponteiro na memória linear; copie a ROM (`.bin` ou
//!    `.zip`) para lá.
//! 2. `vsmile_new(ptr, len, region, pal)` → handle (ou 0 em erro); libere o
//!    buffer com `vsmile_free_buf`.
//! 3. A cada quadro: `vsmile_set_input(...)`, `vsmile_run_frame()`, então leia
//!    `vsmile_rgba_ptr()` (320×240 RGBA) e `vsmile_audio_ptr()/len()`.
//! 4. `vsmile_free(handle)` ao descartar.

use std::alloc::{alloc, dealloc, Layout};

use vsmile_core::joy::JoyInput;
use vsmile_core::ppu::{SCREEN_H, SCREEN_W};
use vsmile_core::vsmile::{dummy_sysrom, rom_from_file};
use vsmile_core::{VSmile, VideoTiming};

const RGBA_LEN: usize = SCREEN_W * SCREEN_H * 4;

/// Estado por instância: o console mais os buffers de saída que o JS lê
/// diretamente da memória linear do wasm.
pub struct Handle {
    vsmile: VSmile,
    rgba: Vec<u8>,
    audio: Vec<u16>,
}

/// Aloca `len` bytes na memória linear (para o JS escrever a ROM). Devolve
/// ponteiro nulo se `len == 0`.
#[no_mangle]
pub extern "C" fn vsmile_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    let layout = Layout::from_size_align(len, 1).expect("layout");
    // SAFETY: layout não-zero; o JS é dono do bloco até chamar vsmile_free_buf.
    unsafe { alloc(layout) }
}

/// Libera um bloco alocado por [`vsmile_alloc`] (mesmo `len`).
///
/// # Safety
/// `ptr` deve vir de `vsmile_alloc(len)` e não ter sido liberado antes.
#[no_mangle]
pub unsafe extern "C" fn vsmile_free_buf(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let layout = Layout::from_size_align(len, 1).expect("layout");
    dealloc(ptr, layout);
}

/// Cria um console a partir dos bytes da ROM (`.bin` ou `.zip`). `pal != 0`
/// usa temporização PAL. Devolve ponteiro nulo se a ROM for inválida.
///
/// # Safety
/// `cart_ptr`/`cart_len` devem descrever uma região válida e viva.
#[no_mangle]
pub unsafe extern "C" fn vsmile_new(
    cart_ptr: *const u8,
    cart_len: usize,
    region: u16,
    pal: u32,
) -> *mut Handle {
    if cart_ptr.is_null() || cart_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(cart_ptr, cart_len);
    let cart = match rom_from_file(bytes) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let timing = if pal != 0 {
        VideoTiming::Pal
    } else {
        VideoTiming::Ntsc
    };
    let mut vsmile = VSmile::new(dummy_sysrom(), cart, region, true, timing);
    vsmile.reset();
    let handle = Box::new(Handle {
        vsmile,
        rgba: vec![0u8; RGBA_LEN],
        audio: Vec::new(),
    });
    Box::into_raw(handle)
}

/// Descarta o console.
///
/// # Safety
/// `p` deve vir de [`vsmile_new`] e não ter sido liberado antes.
#[no_mangle]
pub unsafe extern "C" fn vsmile_free(p: *mut Handle) {
    if !p.is_null() {
        drop(Box::from_raw(p));
    }
}

/// Converte um ponteiro de handle em referência mutável (uso interno).
///
/// # Safety
/// `p` deve ser um handle válido de [`vsmile_new`].
#[inline]
unsafe fn handle<'a>(p: *mut Handle) -> &'a mut Handle {
    &mut *p
}

/// Reinicia o console.
///
/// # Safety
/// `p` deve ser um handle válido.
#[no_mangle]
pub unsafe extern "C" fn vsmile_reset(p: *mut Handle) {
    handle(p).vsmile.reset();
}

/// Executa um quadro e atualiza os buffers RGBA e de áudio.
///
/// # Safety
/// `p` deve ser um handle válido.
#[no_mangle]
pub unsafe extern "C" fn vsmile_run_frame(p: *mut Handle) {
    let h = handle(p);
    h.vsmile.run_frame();
    h.vsmile.framebuffer_rgba(&mut h.rgba);
    let a = h.vsmile.take_audio();
    h.audio.clear();
    h.audio.extend_from_slice(a);
}

/// Ponteiro para o framebuffer RGBA (320×240×4 bytes) do último quadro.
///
/// # Safety
/// `p` deve ser um handle válido.
#[no_mangle]
pub unsafe extern "C" fn vsmile_rgba_ptr(p: *mut Handle) -> *const u8 {
    handle(p).rgba.as_ptr()
}

/// Tamanho do framebuffer RGBA em bytes (constante: 320×240×4).
#[no_mangle]
pub extern "C" fn vsmile_rgba_len() -> usize {
    RGBA_LEN
}

/// Largura/altura da tela (constantes).
#[no_mangle]
pub extern "C" fn vsmile_width() -> u32 {
    SCREEN_W as u32
}
#[no_mangle]
pub extern "C" fn vsmile_height() -> u32 {
    SCREEN_H as u32
}

/// Ponteiro para as amostras de áudio do último quadro (estéreo intercalado,
/// `u16` com 0x8000 = silêncio, ~281,25 kHz).
///
/// # Safety
/// `p` deve ser um handle válido.
#[no_mangle]
pub unsafe extern "C" fn vsmile_audio_ptr(p: *mut Handle) -> *const u16 {
    handle(p).audio.as_ptr()
}

/// Quantidade de amostras de áudio disponíveis no último quadro.
///
/// # Safety
/// `p` deve ser um handle válido.
#[no_mangle]
pub unsafe extern "C" fn vsmile_audio_len(p: *mut Handle) -> usize {
    handle(p).audio.len()
}

/// Taxa de amostragem do áudio bruto (Hz × 1000 não — é a taxa real do SPU).
#[no_mangle]
pub extern "C" fn vsmile_audio_rate() -> u32 {
    281_250
}

// Bits do parâmetro `buttons` de [`vsmile_set_input`].
const B_RED: u32 = 1 << 0;
const B_GREEN: u32 = 1 << 1;
const B_BLUE: u32 = 1 << 2;
const B_YELLOW: u32 = 1 << 3;
const B_ENTER: u32 = 1 << 4;
const B_BACK: u32 = 1 << 5;
const B_HELP: u32 = 1 << 6;
const B_ABC: u32 = 1 << 7;
const B_ON: u32 = 1 << 8;
const B_OFF: u32 = 1 << 9;
const B_RESTART: u32 = 1 << 10;

fn joy_from_bits(x: i32, y: i32, buttons: u32) -> JoyInput {
    JoyInput {
        x,
        y,
        red: buttons & B_RED != 0,
        green: buttons & B_GREEN != 0,
        blue: buttons & B_BLUE != 0,
        yellow: buttons & B_YELLOW != 0,
        enter: buttons & B_ENTER != 0,
        back: buttons & B_BACK != 0,
        help: buttons & B_HELP != 0,
        abc: buttons & B_ABC != 0,
    }
}

/// Define a entrada do controlador 0 e os botões ON/OFF/restart do console.
/// `x`/`y` em -5..=5 (direcional); `buttons` é a máscara `B_*`.
///
/// # Safety
/// `p` deve ser um handle válido.
#[no_mangle]
pub unsafe extern "C" fn vsmile_set_input(p: *mut Handle, x: i32, y: i32, buttons: u32) {
    let h = handle(p);
    h.vsmile.set_input(joy_from_bits(x, y, buttons));
    h.vsmile.set_on_button(buttons & B_ON != 0);
    h.vsmile.set_off_button(buttons & B_OFF != 0);
    h.vsmile.set_restart_button(buttons & B_RESTART != 0);
}

/// Pluga/despluga o segundo controlador.
///
/// # Safety
/// `p` deve ser um handle válido.
#[no_mangle]
pub unsafe extern "C" fn vsmile_set_controller2(p: *mut Handle, enabled: u32) {
    handle(p).vsmile.set_controller2_enabled(enabled != 0);
}

/// Define a entrada do controlador 1 (segundo joystick).
///
/// # Safety
/// `p` deve ser um handle válido.
#[no_mangle]
pub unsafe extern "C" fn vsmile_set_input2(p: *mut Handle, x: i32, y: i32, buttons: u32) {
    handle(p).vsmile.set_input2(joy_from_bits(x, y, buttons));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cartucho sintético que acende tudo em vermelho na paleta e desenha (base
    /// do teste da PPU) — aqui só precisamos de uma ROM válida que rode.
    fn cart() -> Vec<u8> {
        // ROM mínima: vetor de reset → 0x8000, laço fechado.
        let mut rom = vec![0xFFFFu16; 0x10000];
        rom[0xfff7] = 0x8000;
        rom[0x8000] = 0xEE41; // jmp self
        // Converte para bytes little-endian (como um arquivo .bin).
        let mut bytes = Vec::with_capacity(rom.len() * 2);
        for w in rom {
            bytes.push((w & 0xff) as u8);
            bytes.push((w >> 8) as u8);
        }
        bytes
    }

    #[test]
    fn ciclo_de_vida_e_quadro() {
        let bytes = cart();
        unsafe {
            // Aloca, copia a ROM, cria.
            let ptr = vsmile_alloc(bytes.len());
            assert!(!ptr.is_null());
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            let h = vsmile_new(ptr, bytes.len(), 0xe, 0);
            vsmile_free_buf(ptr, bytes.len());
            assert!(!h.is_null(), "vsmile_new deveria ter sucesso");

            // Entrada + um quadro.
            vsmile_set_input(h, 5, 0, B_RED);
            vsmile_run_frame(h);

            // Buffers de saída coerentes.
            assert_eq!(vsmile_rgba_len(), 320 * 240 * 4);
            assert_eq!(vsmile_width(), 320);
            assert_eq!(vsmile_height(), 240);
            let rgba = vsmile_rgba_ptr(h);
            assert!(!rgba.is_null());
            // Alpha do primeiro pixel deve ser 0xff (framebuffer_rgba preenche).
            let first_alpha = *rgba.add(3);
            assert_eq!(first_alpha, 0xff);
            // Áudio: um quadro gera amostras.
            assert!(vsmile_audio_len(h) > 0, "um quadro deve gerar áudio");

            vsmile_free(h);
        }
    }

    #[test]
    fn rom_invalida_devolve_nulo() {
        let bad = b"PK\x03\x04 not a real zip";
        unsafe {
            let ptr = vsmile_alloc(bad.len());
            std::ptr::copy_nonoverlapping(bad.as_ptr(), ptr, bad.len());
            let h = vsmile_new(ptr, bad.len(), 0xe, 0);
            vsmile_free_buf(ptr, bad.len());
            assert!(h.is_null(), "zip inválido deve devolver nulo, não travar");
        }
    }
}
