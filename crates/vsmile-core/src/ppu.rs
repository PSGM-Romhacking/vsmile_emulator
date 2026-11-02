//! PPU (Picture Processing Unit) do SPG200.
//!
//! Gera a imagem de 320×240 a partir de **dois planos de fundo** (tile maps) e
//! até **256 sprites**, com 4 camadas de profundidade, paleta de 256 cores
//! (RGB555 + bit de transparência), compressão/scroll por linha, blending e
//! fade. O vídeo é desenhado scanline a scanline; ao completar a linha 239 o
//! quadro está pronto (sinal de fim de frame).
//!
//! ## Acesso ao barramento
//!
//! Ao desenhar, a PPU lê tile maps e dados gráficos da RAM ou da ROM externa.
//! Para evitar referência cíclica (a PPU é dona pelo barramento), os métodos
//! que leem recebem um **closure** `read: FnMut(u32) -> u16`. O
//! [`crate::memory::Spg200Mem`] constrói esse closure desestruturando seus
//! próprios campos, dando à PPU acesso de leitura a RAM/ROM sem tocar em si
//! mesma.

use crate::settings::PpuViewSettings;
use crate::memory::VideoTiming;

pub const SCREEN_W: usize = 320;
pub const SCREEN_H: usize = 240;

/// Sign-extend dos `bits` inferiores de `value`.
#[inline]
fn sext(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

#[inline]
fn div_round_up(dividend: u32, divisor: u32) -> u32 {
    dividend / divisor + (dividend % divisor != 0) as u32
}

// ---- cor (RGB555 + transparência no bit 15) --------------------------------

#[inline]
pub(crate) fn is_transparent(c: u16) -> bool {
    c & 0x8000 != 0
}
#[inline]
fn color_r(c: u16) -> i32 {
    ((c >> 10) & 0x1f) as i32
}
#[inline]
fn color_g(c: u16) -> i32 {
    ((c >> 5) & 0x1f) as i32
}
#[inline]
fn color_b(c: u16) -> i32 {
    (c & 0x1f) as i32
}
#[inline]
fn make_color(r: i32, g: i32, b: i32) -> u16 {
    (((r as u16) & 0x1f) << 10) | (((g as u16) & 0x1f) << 5) | ((b as u16) & 0x1f)
}

#[inline]
fn blend_interpolate(old_value: i32, new_value: i32, blend_level: i32) -> i32 {
    old_value * (4 - (blend_level + 1)) / 4 + new_value * (blend_level + 1) / 4
}

// ---- estruturas de registradores -------------------------------------------

#[derive(Clone, Copy, Default)]
struct BgData {
    xscroll: u16,
    yscroll: u16,
    attr: u16,
    ctrl: u16,
    tile_map_ptr: u16,
    attribute_map_ptr: u16,
    segment_ptr: u16,
}

impl BgData {
    // attr (máscara 0x3fff): depth(12..14) palette(8..12) vsize(6..8) hsize(4..6)
    //                        vflip(3) hflip(2) color_mode(0..2)
    fn depth(&self) -> u32 {
        ((self.attr >> 12) & 0x3) as u32
    }
    fn palette(&self) -> u32 {
        ((self.attr >> 8) & 0xf) as u32
    }
    fn vsize(&self) -> u32 {
        ((self.attr >> 6) & 0x3) as u32
    }
    fn hsize(&self) -> u32 {
        ((self.attr >> 4) & 0x3) as u32
    }
    fn vflip(&self) -> bool {
        self.attr & 0x8 != 0
    }
    fn hflip(&self) -> bool {
        self.attr & 0x4 != 0
    }
    fn color_mode(&self) -> u32 {
        (self.attr & 0x3) as u32
    }
    // ctrl (máscara 0x1ff): blend(8) hicolor(7) vcompress(6) hcompress(5)
    //                       hmovement(4) enabled(3) wallpaper(2) register(1) bitmap(0)
    fn blend(&self) -> bool {
        self.ctrl & 0x100 != 0
    }
    fn hicolor_mode(&self) -> bool {
        self.ctrl & 0x80 != 0
    }
    fn vcompress(&self) -> bool {
        self.ctrl & 0x40 != 0
    }
    fn hmovement(&self) -> bool {
        self.ctrl & 0x10 != 0
    }
    fn enabled(&self) -> bool {
        self.ctrl & 0x8 != 0
    }
    fn wallpaper_mode(&self) -> bool {
        self.ctrl & 0x4 != 0
    }
    fn register_mode(&self) -> bool {
        self.ctrl & 0x2 != 0
    }
    fn bitmap_mode(&self) -> bool {
        self.ctrl & 0x1 != 0
    }
}

#[derive(Clone, Copy, Default)]
struct SpriteData {
    ch: u16,
    xpos: u16,
    ypos: u16,
    attr: u16,
}

impl SpriteData {
    // attr (máscara 0x7fff): blend(14) depth(12..14) palette(8..12) vsize(6..8)
    //                        hsize(4..6) vflip(3) hflip(2) color_mode(0..2)
    fn blend(&self) -> bool {
        self.attr & 0x4000 != 0
    }
    fn depth(&self) -> u32 {
        ((self.attr >> 12) & 0x3) as u32
    }
    fn palette(&self) -> u32 {
        ((self.attr >> 8) & 0xf) as u32
    }
    fn vsize(&self) -> u32 {
        ((self.attr >> 6) & 0x3) as u32
    }
    fn hsize(&self) -> u32 {
        ((self.attr >> 4) & 0x3) as u32
    }
    fn vflip(&self) -> bool {
        self.attr & 0x8 != 0
    }
    fn hflip(&self) -> bool {
        self.attr & 0x4 != 0
    }
    fn color_mode(&self) -> u32 {
        (self.attr & 0x3) as u32
    }
}

// Máscaras de escrita dos registradores.
const BG_ATTR_WRITE_MASK: u16 = 0x3fff;
const BG_CTRL_WRITE_MASK: u16 = 0x01ff;
const SPRITE_ATTR_WRITE_MASK: u16 = 0x7fff;
const INTERRUPT_WRITE_MASK: u16 = 0x0007; // dma(2) pos(1) vblank(0)

const NUM_SPRITES: usize = 256;

/// Endereço da linha de um tile dentro de um segmento gráfico.
#[inline]
pub(crate) fn line_segment_addr(
    segment_ptr: u16,
    ch: u32,
    tile_y: u32,
    tile_width: u32,
    tile_height: u32,
    bits_per_pixel: u32,
) -> u32 {
    ((segment_ptr as u32) << 6) + (ch * tile_height + tile_y) * tile_width * bits_per_pixel / 16
}

/// A PPU do SPG200.
pub struct Ppu {
    framebuffer: Box<[u16]>, // SCREEN_W * SCREEN_H, RGB555
    video_timing: VideoTiming,
    cur_scanline: i32,
    scanline_counter: i32,
    scanline_period: i32,
    frame_count: i64,

    irq_ctrl: u16,
    irq_status: u16,
    irq_line: bool,

    bg_data: [BgData; 2],
    sprite_data: [SpriteData; NUM_SPRITES],
    sprite_segment_ptr: u16,
    stn_lcd_control: u16,
    blend_level: u16,
    fade_level: u16,
    vertical_compress_amount: u16,
    vertical_compress_offset: u16,
    line_scroll: [u16; 256],
    line_compress: [u16; 256],
    palette_memory: [u16; 256],

    sprite_enable: bool,
    sprite_dma_source: u16,
    sprite_dma_target: u16,
    sprite_dma_length: u16,
    irq_vpos: u16,
    irq_hpos: u16,

    view: PpuViewSettings,
}

impl Ppu {
    pub fn new(video_timing: VideoTiming) -> Self {
        let period = Self::period_for(video_timing);
        let mut ppu = Self {
            framebuffer: vec![0; SCREEN_W * SCREEN_H].into_boxed_slice(),
            video_timing,
            cur_scanline: 0,
            scanline_counter: period,
            scanline_period: period,
            frame_count: 0,
            irq_ctrl: 0,
            irq_status: 0,
            irq_line: false,
            bg_data: [BgData::default(); 2],
            sprite_data: [SpriteData::default(); NUM_SPRITES],
            sprite_segment_ptr: 0,
            stn_lcd_control: 0,
            blend_level: 0,
            fade_level: 0,
            vertical_compress_amount: 0x20,
            vertical_compress_offset: 0,
            line_scroll: [0; 256],
            line_compress: [0; 256],
            palette_memory: [0; 256],
            sprite_enable: false,
            sprite_dma_source: 0,
            sprite_dma_target: 0,
            sprite_dma_length: 0,
            irq_vpos: 0x1ff,
            irq_hpos: 0x1ff,
            view: PpuViewSettings::default(),
        };
        ppu.reset();
        ppu
    }

    fn period_for(video_timing: VideoTiming) -> i32 {
        (if video_timing == VideoTiming::Ntsc { 429 } else { 432 }) * 4
    }

    pub fn reset(&mut self) {
        self.cur_scanline = 0;
        self.scanline_period = Self::period_for(self.video_timing);
        self.scanline_counter = self.scanline_period;
        self.frame_count = 0;
        self.framebuffer.fill(0);
        self.bg_data = [BgData::default(); 2];
        self.sprite_data = [SpriteData::default(); NUM_SPRITES];
        self.sprite_segment_ptr = 0;
        self.blend_level = 0;
        self.vertical_compress_amount = 0x20;
        self.vertical_compress_offset = 0;
        self.fade_level = 0;
        self.line_scroll = [0; 256];
        self.line_compress = [0; 256];
        self.palette_memory = [0; 256];
        self.sprite_enable = false;
        self.sprite_dma_source = 0;
        self.sprite_dma_target = 0;
        self.sprite_dma_length = 0;
        self.stn_lcd_control = 0;
        self.irq_vpos = 0x1ff;
        self.irq_hpos = 0x1ff;
        self.irq_ctrl = 0;
        self.irq_status = 0;
        self.update_irq();
    }

    pub fn set_view_settings(&mut self, view: PpuViewSettings) {
        self.view = view;
    }

    /// Avança o clock de scanline por `cycles`. Retorna `true` quando um quadro
    /// completo acabou de ser desenhado. `read` lê palavras do barramento
    /// (RAM/ROM) para buscar tile maps e dados gráficos.
    pub fn run_cycles<F: FnMut(u32) -> u16>(&mut self, cycles: u32, mut read: F) -> bool {
        self.scanline_counter -= cycles as i32;
        if self.scanline_counter > 0 {
            return false;
        }
        self.scanline_counter += self.scanline_period;

        let scanlines = if self.video_timing == VideoTiming::Ntsc {
            262
        } else {
            312
        };
        let mut frame_finished = false;

        if self.cur_scanline == self.irq_vpos as i32 && (self.irq_ctrl & 0x2) != 0 {
            self.irq_status |= 0x2; // pos
            self.update_irq();
        }

        if self.cur_scanline < 240 {
            self.draw_line(self.cur_scanline, &mut read);
            if self.cur_scanline == 239 {
                if (self.irq_ctrl & 0x1) != 0 {
                    self.irq_status |= 0x1; // vblank
                    self.update_irq();
                }
                self.frame_count += 1;
                frame_finished = true;
            }
            self.cur_scanline += 1;
        } else if self.cur_scanline >= scanlines - 1 {
            self.irq_status &= !0x1; // clear vblank
            self.update_irq();
            self.cur_scanline = 0;
        } else {
            self.cur_scanline += 1;
        }

        frame_finished
    }

    fn update_irq(&mut self) {
        self.irq_line = (self.irq_ctrl & self.irq_status & INTERRUPT_WRITE_MASK) != 0;
    }

    /// Linha de IRQ agregada da PPU (conectada ao IRQ0/FIQ-select 0 da CPU).
    pub fn irq_line(&self) -> bool {
        self.irq_line
    }

    pub fn frame_counter(&self) -> i64 {
        self.frame_count
    }

    pub fn line_counter(&self) -> u16 {
        self.cur_scanline as u16
    }

    /// Framebuffer cru (RGB555, `SCREEN_W * SCREEN_H` palavras).
    pub fn framebuffer(&self) -> &[u16] {
        &self.framebuffer
    }

    // ---- inspeção (depurador) -------------------------------------------

    /// Cópia das 256 cores da paleta (RGB555).
    pub fn palette(&self) -> [u16; 256] {
        self.palette_memory
    }

    /// Ponteiro de segmento usado pelos sprites (para decodificar seus tiles).
    pub fn sprite_segment(&self) -> u16 {
        self.sprite_segment_ptr
    }

    /// Se a camada de sprites está habilitada.
    pub fn sprites_enabled(&self) -> bool {
        self.sprite_enable
    }

    /// Configuração dos dois backgrounds.
    pub fn bg_info(&self) -> [crate::ppu_view::BgInfo; 2] {
        [self.one_bg_info(0), self.one_bg_info(1)]
    }

    fn one_bg_info(&self, i: usize) -> crate::ppu_view::BgInfo {
        let bg = self.bg_data[i];
        crate::ppu_view::BgInfo {
            index: i,
            enabled: bg.enabled(),
            bitmap: bg.bitmap_mode(),
            ctrl: bg.ctrl,
            attr: bg.attr,
            xscroll: bg.xscroll,
            yscroll: bg.yscroll,
            tile_map_ptr: bg.tile_map_ptr,
            attribute_map_ptr: bg.attribute_map_ptr,
            segment_ptr: bg.segment_ptr,
            bpp: if bg.hicolor_mode() {
                16
            } else {
                (bg.color_mode() + 1) * 2
            },
            tile_w: 8 << bg.hsize(),
            tile_h: 8 << bg.vsize(),
            palette: bg.palette(),
        }
    }

    /// Tabela interna de sprites (todos os `NUM_SPRITES`; o chamador filtra os
    /// ativos, tipicamente por `ch != 0`).
    pub fn sprites_info(&self) -> Vec<crate::ppu_view::SpriteInfo> {
        self.sprite_data
            .iter()
            .enumerate()
            .map(|(i, s)| crate::ppu_view::SpriteInfo {
                index: i,
                ch: s.ch,
                x: s.xpos,
                y: s.ypos,
                attr: s.attr,
                palette: s.palette(),
                depth: s.depth(),
                tile_w: 8 << s.hsize(),
                tile_h: 8 << s.vsize(),
                bpp: (s.color_mode() + 1) * 2,
                hflip: s.hflip(),
                vflip: s.vflip(),
                blend: s.blend(),
            })
            .collect()
    }

    // ---- desenho --------------------------------------------------------

    fn draw_line<F: FnMut(u32) -> u16>(&mut self, scanline: i32, read: &mut F) {
        let y = scanline as usize;
        // Começa tudo transparente (bit 15).
        for x in 0..SCREEN_W {
            self.framebuffer[y * SCREEN_W + x] = 0x8000;
        }

        for layer in 0..4u32 {
            for bg in 0..2usize {
                if !self.view.show_bg[bg] {
                    continue;
                }
                if self.bg_data[bg].enabled() && self.bg_data[bg].depth() == layer {
                    self.draw_bg_scanline(bg, scanline, read);
                }
            }

            if self.sprite_enable
                && self.view.show_sprites
                && self.view.show_sprites_in_layer[layer as usize]
            {
                // sprites não-blended primeiro...
                for s in 0..NUM_SPRITES {
                    let sp = self.sprite_data[s];
                    if sp.ch != 0 && !sp.blend() && sp.depth() == layer {
                        self.draw_sprite_scanline(s, scanline, read);
                    }
                }
                // ...e os com blend por último.
                for s in 0..NUM_SPRITES {
                    let sp = self.sprite_data[s];
                    if sp.ch != 0 && sp.blend() && sp.depth() == layer {
                        self.draw_sprite_scanline(s, scanline, read);
                    }
                }
            }
        }

        // Pixels ainda transparentes viram preto.
        for x in 0..SCREEN_W {
            let p = &mut self.framebuffer[y * SCREEN_W + x];
            if is_transparent(*p) {
                *p = 0;
            }
        }
    }

    fn draw_bg_scanline<F: FnMut(u32) -> u16>(&mut self, bg_index: usize, screen_y: i32, read: &mut F) {
        let bg = self.bg_data[bg_index];

        let mut virtual_y = screen_y;
        if bg.vcompress() {
            let amount = self.vertical_compress_amount as i32;
            let offset = sext(self.vertical_compress_offset as u32, 13) + 128 - 128 * amount / 0x20;
            virtual_y = screen_y * amount / 0x20 + offset;
        }

        if virtual_y < 0 || virtual_y >= 240 {
            return;
        }

        let tilemap_y = (virtual_y + bg.yscroll as i32) & 0xff;
        let scroll_x = (bg.xscroll as i32
            + if bg.hmovement() {
                self.line_scroll[tilemap_y as usize] as i32
            } else {
                0
            })
            & 0x1ff;

        if bg.bitmap_mode() {
            let addr_lo = read(bg.tile_map_ptr as u32 + tilemap_y as u32);
            let shift = if tilemap_y & 1 != 0 { 8 } else { 0 };
            let addr_hi = read(bg.attribute_map_ptr as u32 + (tilemap_y as u32) / 2) >> shift;
            let addr = (addr_lo as u32) | ((addr_hi as u32) << 16);
            let bits_per_pixel = if bg.hicolor_mode() {
                16
            } else {
                (bg.color_mode() + 1) * 2
            };
            let mut screen_x = -scroll_x;
            while screen_x < 320 {
                self.draw_tile_line(
                    screen_y,
                    screen_x,
                    addr,
                    512,
                    bg.palette(),
                    false,
                    bits_per_pixel,
                    bg.blend(),
                    read,
                );
                screen_x += 512;
            }
            return;
        }

        let tile_width = 8i32 << bg.hsize();
        let tile_height = 8i32 << bg.vsize();
        let tilemap_ytile = tilemap_y / tile_height;
        let tiles_per_row = 512 >> (bg.hsize() + 3);

        let mut screen_x = -(scroll_x % tile_width);
        while screen_x < 320 {
            let tilemap_x = (screen_x + scroll_x) & 0x1ff;
            let tilemap_xtile = tilemap_x / tile_width;

            let tilemap_tilepos = if bg.wallpaper_mode() {
                0
            } else {
                tiles_per_row * tilemap_ytile + tilemap_xtile
            };

            let ch = read(bg.tile_map_ptr as u32 + tilemap_tilepos as u32);
            if ch == 0 {
                screen_x += tile_width;
                continue;
            }

            let mut palette = bg.palette();
            let mut vflip = bg.vflip();
            let mut hflip = bg.hflip();
            let mut blend = bg.blend();

            if !bg.register_mode() {
                let attr_addr = bg.attribute_map_ptr as u32 + (tilemap_tilepos as u32 >> 1);
                let attr_word = read(attr_addr);
                let attr = attr_word >> if tilemap_tilepos & 1 != 0 { 8 } else { 0 };
                // TileAttribute: blend(6) vflip(5) hflip(4) palette(0..4)
                palette = (attr & 0xf) as u32;
                vflip = attr & 0x20 != 0;
                hflip = attr & 0x10 != 0;
                blend = attr & 0x40 != 0;
            }

            let tile_y = if !vflip {
                tilemap_y % tile_height
            } else {
                tile_height - (tilemap_y % tile_height) - 1
            };
            let bits_per_pixel = (bg.color_mode() + 1) * 2;

            let addr = line_segment_addr(
                bg.segment_ptr,
                ch as u32,
                tile_y as u32,
                tile_width as u32,
                tile_height as u32,
                bits_per_pixel,
            );
            self.draw_tile_line(
                screen_y,
                screen_x,
                addr,
                tile_width,
                palette,
                hflip,
                bits_per_pixel,
                blend,
                read,
            );

            screen_x += tile_width;
        }
    }

    fn draw_sprite_scanline<F: FnMut(u32) -> u16>(
        &mut self,
        sprite: usize,
        screen_y: i32,
        read: &mut F,
    ) {
        let sp = self.sprite_data[sprite];
        let tile_width = 8i32 << sp.hsize();
        let tile_height = 8i32 << sp.vsize();
        let xpos = (160 + sext(sp.xpos as u32, 9)) - tile_width / 2;
        let ypos = (128 - sext(sp.ypos as u32, 9)) - tile_height / 2;
        let bits_per_pixel = (sp.color_mode() + 1) * 2;

        let tile_y = if !sp.vflip() {
            screen_y - ypos
        } else {
            (tile_height - 1) - (screen_y - ypos)
        };

        if tile_y < 0 || tile_y >= tile_height {
            return;
        }

        let addr = line_segment_addr(
            self.sprite_segment_ptr,
            sp.ch as u32,
            tile_y as u32,
            tile_width as u32,
            tile_height as u32,
            bits_per_pixel,
        );
        self.draw_tile_line(
            screen_y,
            xpos,
            addr,
            tile_width,
            sp.palette(),
            sp.hflip(),
            bits_per_pixel,
            sp.blend(),
            read,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_tile_line<F: FnMut(u32) -> u16>(
        &mut self,
        screen_y: i32,
        screen_x_start: i32,
        line_addr: u32,
        tile_width: i32,
        palette: u32,
        hflip: bool,
        bits_per_pixel: u32,
        blend: bool,
        read: &mut F,
    ) {
        let bpp = bits_per_pixel as i32;
        let mut pixbuf_shift: i32 = -bpp;
        let mut pixbuf: u32 = 0;
        let mut addr: u32 = line_addr.wrapping_add(if hflip {
            (tile_width as u32 * bits_per_pixel) / 16 - 1
        } else {
            0
        });

        // Pula leituras de pixels totalmente fora da tela à esquerda.
        let left_offscreen = if screen_x_start < 0 { -screen_x_start } else { 0 };
        let mut skipped_pixels = 0i32;
        if left_offscreen > 0 {
            let skipped_words = (left_offscreen as u32 * bits_per_pixel) / 16;
            if skipped_words != 0 {
                addr = if hflip {
                    addr.wrapping_sub(skipped_words)
                } else {
                    addr.wrapping_add(skipped_words)
                };
                skipped_pixels = div_round_up(skipped_words * 16, bits_per_pixel) as i32;
                pixbuf_shift -= ((skipped_pixels as u32 * bits_per_pixel) % 16) as i32;
            }
        }

        let mut screen_x = screen_x_start + skipped_pixels;
        while screen_x < screen_x_start + tile_width && screen_x < 320 {
            if pixbuf_shift < 0 {
                let mut val = read(addr);
                addr = if hflip {
                    addr.wrapping_sub(1)
                } else {
                    addr.wrapping_add(1)
                };
                if bits_per_pixel != 16 {
                    val = (val >> 8) | (val << 8);
                }
                pixbuf = if hflip {
                    ((val as u32) << 16) | (pixbuf >> 16)
                } else {
                    (pixbuf << 16) | (val as u32)
                };
                pixbuf_shift += 16;
            }

            let pixbuf_shift_flip = if hflip {
                ((16 - bpp) - pixbuf_shift) + 16
            } else {
                pixbuf_shift
            };
            let pixdata = (pixbuf >> pixbuf_shift_flip) & ((1 << bits_per_pixel) - 1);
            pixbuf_shift -= bpp;

            if screen_x < 0 {
                screen_x += 1;
                continue;
            }

            let newcolor = match bits_per_pixel {
                2 | 4 => self.palette_memory[(palette as usize * 16 + pixdata as usize) & 0xff],
                6 => self.palette_memory[((palette as usize >> 2) * 64 + pixdata as usize) & 0xff],
                8 => self.palette_memory[pixdata as usize & 0xff],
                16 => pixdata as u16,
                _ => unreachable!(),
            };

            if is_transparent(newcolor) {
                screen_x += 1;
                continue;
            }

            let fb_index = screen_y as usize * SCREEN_W + screen_x as usize;
            let final_color = if blend {
                let old = self.framebuffer[fb_index];
                if !is_transparent(old) {
                    let level = self.blend_level as i32;
                    make_color(
                        blend_interpolate(color_r(old), color_r(newcolor), level),
                        blend_interpolate(color_g(old), color_g(newcolor), level),
                        blend_interpolate(color_b(old), color_b(newcolor), level),
                    )
                } else {
                    newcolor
                }
            } else {
                newcolor
            };

            self.framebuffer[fb_index] = final_color;
            screen_x += 1;
        }
    }

    // ---- DMA de sprites -------------------------------------------------

    /// DMA que copia atributos de sprite da memória (via `read`) para a
    /// memória interna de sprites. Disparado por escrita em `0x2872`.
    pub fn start_sprite_dma<F: FnMut(u32) -> u16>(&mut self, length: u16, mut read: F) {
        self.sprite_dma_length = length;
        while self.sprite_dma_length != 0 {
            let word = read(self.sprite_dma_source as u32);
            self.sprite_dma_source = self.sprite_dma_source.wrapping_add(1);
            self.write_sprite_memory(self.sprite_dma_target, word);
            self.sprite_dma_target = (self.sprite_dma_target.wrapping_add(1)) & 0x3ff;
            self.sprite_dma_length -= 1;
        }
        if self.irq_ctrl & 0x4 != 0 {
            self.irq_status |= 0x4; // dma
            self.update_irq();
        }
    }

    fn read_sprite_memory(&self, offset: u16) -> u16 {
        let index = ((offset & 0x3ff) >> 2) as usize;
        match offset & 3 {
            0 => self.sprite_data[index].ch,
            1 => self.sprite_data[index].xpos,
            2 => self.sprite_data[index].ypos,
            _ => self.sprite_data[index].attr,
        }
    }

    fn write_sprite_memory(&mut self, offset: u16, value: u16) {
        let index = ((offset & 0x3ff) >> 2) as usize;
        match offset & 3 {
            0 => self.sprite_data[index].ch = value,
            1 => self.sprite_data[index].xpos = value & 0x1ff,
            2 => self.sprite_data[index].ypos = value & 0x1ff,
            _ => self.sprite_data[index].attr = value & SPRITE_ATTR_WRITE_MASK,
        }
    }

    // ---- dispatch de registradores (mapeados pelo Spg200Mem) ------------

    /// Lê um registrador da PPU no endereço dado. Retorna `None` se o endereço
    /// não pertence à PPU.
    pub fn read_reg(&mut self, addr: u32) -> Option<u16> {
        let v = match addr {
            0x2810 | 0x2816 => self.bg_data[((addr - 0x2810) / 6) as usize].xscroll,
            0x2811 | 0x2817 => self.bg_data[((addr - 0x2810) / 6) as usize].yscroll,
            0x2812 | 0x2818 => self.bg_data[((addr - 0x2810) / 6) as usize].attr,
            0x2813 | 0x2819 => self.bg_data[((addr - 0x2810) / 6) as usize].ctrl,
            0x2814 | 0x281a => self.bg_data[((addr - 0x2810) / 6) as usize].tile_map_ptr,
            0x2815 | 0x281b => self.bg_data[((addr - 0x2810) / 6) as usize].attribute_map_ptr,
            0x281c => self.vertical_compress_amount,
            0x281d => self.vertical_compress_offset,
            0x2820 | 0x2821 => self.bg_data[(addr - 0x2820) as usize].segment_ptr,
            0x2822 => self.sprite_segment_ptr,
            0x282a => self.blend_level,
            0x2830 => self.fade_level,
            0x2836 => self.irq_vpos,
            0x2837 => self.irq_hpos,
            0x2842 => self.sprite_enable as u16,
            0x2854 => self.stn_lcd_control,
            0x2862 => self.irq_ctrl,
            0x2863 => self.irq_status,
            0x2870 => self.sprite_dma_source,
            0x2871 => self.sprite_dma_target,
            0x2872 => self.sprite_dma_length,
            0x2900..=0x29ff => self.line_scroll[(addr & 0xff) as usize],
            0x2a00..=0x2aff => self.line_compress[(addr & 0xff) as usize],
            0x2b00..=0x2bff => self.palette_memory[(addr & 0xff) as usize],
            0x2c00..=0x2fff => self.read_sprite_memory((addr & 0x3ff) as u16),
            0x3d1c => self.cur_scanline as u16,
            _ => return None,
        };
        Some(v)
    }

    /// Escreve um registrador da PPU. Retorna `false` se o endereço não
    /// pertence à PPU. **Não** trata `0x2872` (DMA de sprite) — esse é
    /// disparado via [`Ppu::start_sprite_dma`] pelo barramento, pois precisa
    /// ler a memória.
    pub fn write_reg(&mut self, addr: u32, value: u16) -> bool {
        match addr {
            0x2810 | 0x2816 => self.bg_data[((addr - 0x2810) / 6) as usize].xscroll = value & 0x1ff,
            0x2811 | 0x2817 => self.bg_data[((addr - 0x2810) / 6) as usize].yscroll = value & 0xff,
            0x2812 | 0x2818 => {
                self.bg_data[((addr - 0x2810) / 6) as usize].attr = value & BG_ATTR_WRITE_MASK
            }
            0x2813 | 0x2819 => {
                self.bg_data[((addr - 0x2810) / 6) as usize].ctrl = value & BG_CTRL_WRITE_MASK
            }
            0x2814 | 0x281a => {
                self.bg_data[((addr - 0x2810) / 6) as usize].tile_map_ptr = value & 0x3fff
            }
            0x2815 | 0x281b => {
                self.bg_data[((addr - 0x2810) / 6) as usize].attribute_map_ptr = value & 0x3fff
            }
            0x281c => self.vertical_compress_amount = value & 0x1ff,
            0x281d => self.vertical_compress_offset = value & 0x1fff,
            0x2820 | 0x2821 => self.bg_data[(addr - 0x2820) as usize].segment_ptr = value,
            0x2822 => self.sprite_segment_ptr = value,
            0x282a => self.blend_level = value & 0x03,
            0x2830 => self.fade_level = value & 0xff,
            0x2836 => self.irq_vpos = value & 0x1ff,
            0x2837 => self.irq_hpos = value & 0x1ff,
            0x2842 => self.sprite_enable = value & 0x1 != 0,
            0x2854 => self.stn_lcd_control = value & 0x3f,
            0x2862 => {
                self.irq_ctrl = value & INTERRUPT_WRITE_MASK;
                self.update_irq();
            }
            0x2863 => {
                self.irq_status &= !(value & INTERRUPT_WRITE_MASK);
                self.update_irq();
            }
            0x2870 => self.sprite_dma_source = value & 0x3fff,
            0x2871 => self.sprite_dma_target = value & 0x3ff,
            // 0x2872 tratado externamente (start_sprite_dma)
            0x2900..=0x29ff => self.line_scroll[(addr & 0xff) as usize] = value & 0x1ff,
            0x2a00..=0x2aff => self.line_compress[(addr & 0xff) as usize] = value,
            0x2b00..=0x2bff => self.palette_memory[(addr & 0xff) as usize] = value,
            0x2c00..=0x2fff => self.write_sprite_memory((addr & 0x3ff) as u16, value),
            _ => return false,
        }
        true
    }
}

crate::impl_state!(BgData {
    xscroll, yscroll, attr, ctrl, tile_map_ptr, attribute_map_ptr, segment_ptr
});
crate::impl_state!(SpriteData { ch, xpos, ypos, attr });
crate::impl_state!(Ppu {
    framebuffer, cur_scanline, scanline_counter, scanline_period, frame_count,
    irq_ctrl, irq_status, irq_line,
    bg_data, sprite_data, sprite_segment_ptr, stn_lcd_control,
    blend_level, fade_level, vertical_compress_amount, vertical_compress_offset,
    line_scroll, line_compress, palette_memory,
    sprite_enable, sprite_dma_source, sprite_dma_target, sprite_dma_length,
    irq_vpos, irq_hpos
});
