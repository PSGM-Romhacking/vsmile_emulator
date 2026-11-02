//! Inspeção da PPU para o depurador: leitura da paleta, da tabela de sprites e
//! da configuração dos backgrounds, além da **decodificação de tiles** em
//! imagens RGB555 (para um visualizador de *tile sheet* / paleta).
//!
//! As funções de decodificação espelham o caminho sem *flip* de
//! [`crate::ppu::Ppu::draw_tile_line`] — mesma leitura byte a byte, mesmo
//! *byte-swap* para bpp ≠ 16 e o mesmo mapeamento de paleta. Os acessores
//! `palette()/sprites_info()/bg_info()` ficam em [`crate::ppu`] (os campos são
//! privados); aqui estão os tipos e as rotinas puras.

use crate::ppu::{is_transparent, line_segment_addr};

/// Descrição de um sprite da tabela interna da PPU.
#[derive(Debug, Clone, Copy)]
pub struct SpriteInfo {
    pub index: usize,
    pub ch: u16,
    pub x: u16,
    pub y: u16,
    pub attr: u16,
    pub palette: u32,
    pub depth: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub bpp: u32,
    pub hflip: bool,
    pub vflip: bool,
    pub blend: bool,
}

/// Configuração de um plano de fundo (BG0/BG1).
#[derive(Debug, Clone, Copy)]
pub struct BgInfo {
    pub index: usize,
    pub enabled: bool,
    pub bitmap: bool,
    pub ctrl: u16,
    pub attr: u16,
    pub xscroll: u16,
    pub yscroll: u16,
    pub tile_map_ptr: u16,
    pub attribute_map_ptr: u16,
    pub segment_ptr: u16,
    pub bpp: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub palette: u32,
}

/// Mapeamento de índice de pixel → cor, igual ao de `draw_tile_line`.
#[inline]
fn palette_lookup(palette_mem: &[u16], bpp: u32, palette: u32, pixdata: u32) -> u16 {
    match bpp {
        2 | 4 => palette_mem[(palette as usize * 16 + pixdata as usize) & 0xff],
        6 => palette_mem[((palette as usize >> 2) * 64 + pixdata as usize) & 0xff],
        8 => palette_mem[pixdata as usize & 0xff],
        16 => pixdata as u16,
        _ => 0,
    }
}

/// Decodifica um tile `ch` em `tile_w*tile_h` pixels RGB555 (a transparência é
/// preservada no bit 15). `read` lê palavras da memória de gráficos.
pub fn decode_tile<F: FnMut(u32) -> u16>(
    mut read: F,
    segment_ptr: u16,
    ch: u32,
    tile_w: u32,
    tile_h: u32,
    bpp: u32,
    palette: u32,
    palette_mem: &[u16],
) -> Vec<u16> {
    let mut out = vec![0u16; (tile_w * tile_h) as usize];
    let bpp_i = bpp as i32;
    for ty in 0..tile_h {
        let mut addr = line_segment_addr(segment_ptr, ch, ty, tile_w, tile_h, bpp);
        let mut pixbuf_shift: i32 = -bpp_i;
        let mut pixbuf: u32 = 0;
        for tx in 0..tile_w {
            if pixbuf_shift < 0 {
                let mut val = read(addr);
                addr = addr.wrapping_add(1);
                if bpp != 16 {
                    val = (val >> 8) | (val << 8);
                }
                pixbuf = (pixbuf << 16) | val as u32;
                pixbuf_shift += 16;
            }
            let pixdata = (pixbuf >> pixbuf_shift) & ((1 << bpp) - 1);
            pixbuf_shift -= bpp_i;
            out[(ty * tile_w + tx) as usize] = palette_lookup(palette_mem, bpp, palette, pixdata);
        }
    }
    out
}

/// Imagem (RGB555) + largura/altura. A largura/altura são em pixels.
pub type Image = (Vec<u16>, usize, usize);

/// Grade 16×16 das 256 cores da paleta, cada célula `swatch`×`swatch` pixels.
pub fn palette_image(palette_mem: &[u16], swatch: usize) -> Image {
    let w = 16 * swatch;
    let h = 16 * swatch;
    let mut img = vec![0u16; w * h];
    for idx in 0..256 {
        let c = palette_mem[idx] & 0x7fff; // ignora o bit de transparência
        let cx = (idx % 16) * swatch;
        let cy = (idx / 16) * swatch;
        for yy in 0..swatch {
            for xx in 0..swatch {
                img[(cy + yy) * w + (cx + xx)] = c;
            }
        }
    }
    (img, w, h)
}

/// Monta um *tile sheet*: `count` tiles a partir de `first_ch`, em `cols`
/// colunas, com 1 px de separação (cor `bg`). Pixels transparentes viram `bg`.
#[allow(clippy::too_many_arguments)]
pub fn tile_sheet<F: FnMut(u32) -> u16>(
    mut read: F,
    segment_ptr: u16,
    bpp: u32,
    tile_w: u32,
    tile_h: u32,
    palette: u32,
    palette_mem: &[u16],
    first_ch: u32,
    count: u32,
    cols: u32,
    bg: u16,
) -> Image {
    let cols = cols.max(1);
    let rows = count.div_ceil(cols);
    let cell_w = tile_w as usize + 1;
    let cell_h = tile_h as usize + 1;
    let w = cols as usize * cell_w + 1;
    let h = rows as usize * cell_h + 1;
    let mut img = vec![bg; w * h];
    for i in 0..count {
        let ch = first_ch + i;
        let tile = decode_tile(
            &mut read,
            segment_ptr,
            ch,
            tile_w,
            tile_h,
            bpp,
            palette,
            palette_mem,
        );
        let gx = (i % cols) as usize * cell_w + 1;
        let gy = (i / cols) as usize * cell_h + 1;
        for ty in 0..tile_h as usize {
            for tx in 0..tile_w as usize {
                let c = tile[ty * tile_w as usize + tx];
                let px = if is_transparent(c) { bg } else { c & 0x7fff };
                img[(gy + ty) * w + (gx + tx)] = px;
            }
        }
    }
    (img, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_image_dimensoes_e_cor() {
        let mut pal = [0u16; 256];
        pal[1] = 31 << 10; // vermelho puro
        let (img, w, h) = palette_image(&pal, 4);
        assert_eq!((w, h), (64, 64));
        // Célula do índice 1 (coluna 1, linha 0): canto superior-esquerdo em (4,0).
        assert_eq!(img[4], 31 << 10);
    }

    #[test]
    fn decode_tile_2bpp() {
        // Reproduz o cenário do teste da PPU: tile 8x8 @2bpp, segment=8, ch=1,
        // linha 0 = índice de pixel 1 repetido (palavra já byte-swapped na RAM).
        // A paleta mapeia índice 1 → vermelho.
        let mut mem = std::collections::HashMap::new();
        // tile ch=1 começa em (8<<6) + (1*8+0)*8*2/16 = 512 + 8 = 520 (0x208).
        let line: u16 = 0b01_01_01_01_01_01_01_01;
        let swapped = (line >> 8) | (line << 8);
        mem.insert(0x208u32, swapped);
        let mut pal = [0u16; 256];
        pal[1] = 31 << 10; // vermelho (paleta 0, índice 1)

        let read = |a: u32| *mem.get(&a).unwrap_or(&0);
        let tile = decode_tile(read, 8, 1, 8, 8, 2, 0, &pal);
        assert_eq!(tile.len(), 64);
        // Toda a primeira linha deve ser vermelha (índice 1).
        for x in 0..8 {
            assert_eq!(tile[x], 31 << 10, "pixel (0,{x}) deveria ser vermelho");
        }
    }
}
