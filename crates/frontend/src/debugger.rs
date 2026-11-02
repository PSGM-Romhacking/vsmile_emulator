//! Debugger (Tools › Debugger…): a separate window, next to the game, with the
//! system RAM live (bytes changed in the last frame highlighted, editable), the
//! views the core describes (tiles, CPU, ports…), pausing, frame advance and
//! muting sound channels.
//!
//! Layout as in the usual emulator debuggers (Mesen, bsnes-plus) and hex editors
//! (ImHex, imgui_memory_editor): a toolbar with the run state, tabs, and in the memory
//! view a column header, the bytes in two groups of 8, an ASCII column and an inspector
//! for the selected byte; the core's tables come as cards, the tile set with a grid.

use std::collections::HashMap;

use egui::{Align, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use emu_api::{Core, DebugGroup, DebugValue, DebugView, SystemInfo};

use crate::i18n::{tr, trf};
use crate::theme::bold;
use crate::widgets::Look;

/// Bytes per row of the memory view.
const ROW: usize = 16;

#[derive(Default)]
pub struct Debugger {
    pub open: bool,
    /// 0 = memory; n = the core's view n - 1.
    pub(crate) tab: usize,
    /// RAM as of the last frame shown, to highlight what changed.
    prev: Vec<u8>,
    changed: Vec<bool>,
    selected: Option<usize>,
    edit: String,
    textures: HashMap<&'static str, egui::TextureHandle>,
    muted: Vec<bool>,
}

/// Actions the debugger asks of the app (pausing and stepping live there).
#[derive(Default)]
pub struct Requests {
    pub toggle_pause: bool,
    pub step: bool,
}

impl Debugger {
    /// Shows the window (if open). `core` is `None` without a game.
    pub fn show(&mut self, ctx: &egui::Context, look: &Look, sys: &SystemInfo, core: Option<&mut dyn Core>, paused: bool) -> Requests {
        let mut req = Requests::default();
        if !self.open {
            return req;
        }
        let builder = egui::ViewportBuilder::default().with_title(tr("Debugger")).with_inner_size([760.0, 620.0]).with_min_inner_size([560.0, 400.0]);
        let mut core = core;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("debugger"), builder, |ui, _| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                self.open = false;
            }
            self.body(ui, look, sys, &mut core, paused, &mut req);
        });
        // keep refreshing while the game runs, so the values follow it
        ctx.request_repaint_of(egui::ViewportId::from_hash_of("debugger"));
        req
    }

    /// The window's contents. The documentation screenshots draw it in the main window,
    /// since a separate window can't be captured.
    pub fn body(&mut self, ui: &mut egui::Ui, look: &Look, sys: &SystemInfo, core: &mut Option<&mut dyn Core>, paused: bool, req: &mut Requests) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(look.p.bg_main)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            let Some(core) = core.as_deref_mut() else {
                egui::Frame::new().inner_margin(16).show(ui, |ui| look.hint(ui, &tr("No game loaded")));
                return;
            };
            let views = core.debug_views();
            look.strip(ui, |ui| {
                self.toolbar(ui, look, core, paused, req);
                ui.add_space(4.0);
                let titles: Vec<&str> = std::iter::once("Memory")
                    .chain(views.iter().map(|v| match v {
                        DebugView::Image { title, .. } | DebugView::Table { title, .. } => *title,
                    }))
                    .collect();
                look.tabs(ui, &titles, &mut self.tab);
            });
            self.track_changes(core.system_ram());
            egui::Frame::new().inner_margin(Margin::same(14)).show(ui, |ui| match self.tab.checked_sub(1).and_then(|i| views.get(i)) {
                None => {
                    self.tab = 0;
                    self.memory(ui, look, sys, core);
                }
                Some(DebugView::Image { title, width, height, pixels, cell }) => self.image(ui, look, title, *width, *height, pixels, *cell),
                Some(DebugView::Table { groups, .. }) => cards(ui, look, groups),
            });
        });
    }

    /// Run state, pause / frame advance and the sound channels as toggles.
    fn toolbar(&mut self, ui: &mut egui::Ui, look: &Look, core: &mut dyn Core, paused: bool, req: &mut Requests) {
        ui.horizontal(|ui| {
            let (color, state) = if paused { (look.p.warning, "Paused") } else { (look.p.success, "Running") };
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            ui.painter().rect_filled(dot, 0.0, color);
            ui.label(RichText::new(tr(state)).font(bold(look.f.base)).color(look.p.text_main));
            ui.add_space(8.0);
            if ui.button(tr(if paused { "Continue" } else { "Pause" })).clicked() {
                req.toggle_pause = true;
            }
            if ui.add_enabled(paused, egui::Button::new(tr("Advance 1 frame"))).clicked() {
                req.step = true;
            }
            let channels = core.sound_channels();
            if channels.is_empty() {
                return;
            }
            self.muted.resize(channels.len(), false);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                for i in (0..channels.len()).rev() {
                    let on = !self.muted[i];
                    let text = RichText::new(tr(channels[i])).size(look.f.base);
                    let text = if on { text.color(look.p.text_main) } else { text.color(look.p.text_muted).strikethrough() };
                    let b = egui::Button::new(text).fill(if on { look.p.bg_main } else { Color32::TRANSPARENT }).stroke(Stroke::new(1.0, look.p.border));
                    if ui.add(b).on_hover_text(tr("Click to mute or unmute")).clicked() {
                        self.muted[i] = on;
                        core.set_channel_muted(i, self.muted[i]);
                    }
                }
                look.hint(ui, &tr("Sound:"));
            });
        });
    }

    fn track_changes(&mut self, ram: &[u8]) {
        if self.prev.len() != ram.len() {
            self.prev = ram.to_vec();
            self.changed = vec![false; ram.len()];
            return;
        }
        if self.prev != ram {
            for (i, (p, &v)) in self.prev.iter_mut().zip(ram).enumerate() {
                self.changed[i] = *p != v;
                *p = v;
            }
        }
    }

    /// Hex view: address, 16 bytes in two groups of 8, ASCII; inspector below.
    fn memory(&mut self, ui: &mut egui::Ui, look: &Look, sys: &SystemInfo, core: &mut dyn Core) {
        let base = sys.ram_address as usize;
        let ram = core.system_ram().to_vec();
        let p = look.p;
        let mono = FontId::monospace(look.f.base);
        let cw = ui.painter().layout_no_wrap("0".into(), mono.clone(), p.text_main).size().x;
        let row_h = (ui.fonts_mut(|f| f.row_height(&mono)) + 6.0).round();
        // x of each column, from the left edge of the view
        let pad = 12.0;
        let byte_x = |i: usize| pad + cw * (4.0 + 3.0 + 3.0 * i as f32 + if i >= 8 { 1.0 } else { 0.0 });
        let ascii_x = byte_x(ROW) + cw * 2.0;
        let width = (ascii_x + cw * ROW as f32 + pad).max(ui.available_width() - 2.0);

        // title and legend
        ui.horizontal(|ui| {
            let end = base + ram.len().saturating_sub(1);
            look.section(ui, &format!("RAM  {base:04X}–{end:04X}"));
            look.hint(ui, &format!("· {} KB", ram.len() / 1024));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                look.hint(ui, &tr("Changed in the last frame"));
                look.swatch(ui, p.text_accent, p.text_accent);
            });
        });

        let inspector_h = 44.0;
        let panel = egui::Frame::new().fill(p.bg_sidebar).stroke(Stroke::new(1.0, p.border));
        panel.show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            // column header
            let (head, _) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::hover());
            let painter = ui.painter();
            for i in 0..ROW {
                painter.text(Pos2::new(head.left() + byte_x(i), head.center().y), egui::Align2::LEFT_CENTER, format!("{i:02X}"), mono.clone(), p.text_section);
            }
            painter.text(Pos2::new(head.left() + ascii_x, head.center().y), egui::Align2::LEFT_CENTER, "ASCII", mono.clone(), p.text_section);
            painter.hline(head.x_range(), head.bottom(), Stroke::new(1.0, p.border));

            let rows = ram.len().div_ceil(ROW);
            let max_h = (ui.available_height() - inspector_h - 16.0).max(row_h * 4.0);
            egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show_rows(ui, row_h, rows, |ui, range| {
                for r in range {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::click());
                    let painter = ui.painter();
                    if r % 2 == 1 {
                        painter.rect_filled(rect, 0.0, p.card_bg);
                    }
                    let y = rect.center().y;
                    painter.text(Pos2::new(rect.left() + pad, y), egui::Align2::LEFT_CENTER, format!("{:04X}", base + r * ROW), mono.clone(), p.text_muted);
                    let hover = resp.hover_pos().map(|pos| pos.x - rect.left());
                    let mut hovered = None;
                    for c in 0..ROW {
                        let i = r * ROW + c;
                        let Some(&v) = ram.get(i) else { break };
                        let x = rect.left() + byte_x(c);
                        let cell = Rect::from_min_size(Pos2::new(x - cw * 0.5, rect.top() + 1.0), Vec2::new(cw * 3.0, row_h - 2.0));
                        let over = hover.is_some_and(|hx| (cell.left() - rect.left()..cell.right() - rect.left()).contains(&hx));
                        if over {
                            hovered = Some(i);
                        }
                        let changed = self.changed.get(i).copied().unwrap_or(false);
                        let color = if self.selected == Some(i) {
                            painter.rect_filled(cell, 0.0, p.border_focus);
                            p.text_main
                        } else {
                            if over {
                                painter.rect_filled(cell, 0.0, p.hover_bg);
                            }
                            if changed {
                                p.text_accent
                            } else if v == 0 {
                                p.text_muted
                            } else {
                                p.text_main
                            }
                        };
                        painter.text(Pos2::new(x, y), egui::Align2::LEFT_CENTER, format!("{v:02X}"), mono.clone(), color);
                        let ch = if (0x20..0x7F).contains(&v) { v as char } else { '·' };
                        let ac = if self.selected == Some(i) { p.text_accent } else if ch == '·' { p.border } else { p.text_muted };
                        painter.text(Pos2::new(rect.left() + ascii_x + cw * c as f32, y), egui::Align2::LEFT_CENTER, ch.to_string(), mono.clone(), ac);
                    }
                    if resp.clicked()
                        && let Some(i) = hovered
                    {
                        self.selected = Some(i);
                        self.edit = format!("{:02X}", ram[i]);
                    }
                    if hovered.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            });
        });

        // inspector of the selected byte
        egui::Frame::new().fill(p.bg_sidebar).stroke(Stroke::new(1.0, p.border)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(width - 24.0);
            ui.horizontal(|ui| match self.selected.and_then(|i| ram.get(i).map(|&v| (i, v))) {
                Some((i, v)) => {
                    ui.label(RichText::new(format!("{:04X}", base + i)).font(bold(look.f.md)).color(p.text_accent));
                    ui.add_space(6.0);
                    for (name, value) in [("Hex", format!("{v:02X}")), ("Dec", v.to_string()), ("Signed", (v as i8).to_string()), ("Bin", format!("{:04b} {:04b}", v >> 4, v & 15))] {
                        look.hint(ui, &tr(name));
                        ui.label(RichText::new(value).font(FontId::monospace(look.f.base)).color(p.text_main));
                        ui.add_space(6.0);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let write = ui.button(tr("Write")).clicked();
                        let r = ui.add(egui::TextEdit::singleline(&mut self.edit).desired_width(34.0).char_limit(2).font(egui::TextStyle::Monospace));
                        if (write || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                            && let (Ok(v), Some(b)) = (u8::from_str_radix(self.edit.trim(), 16), core.system_ram_mut().get_mut(i))
                        {
                            *b = v;
                        }
                    });
                }
                None => {
                    look.hint(ui, &tr("Click a byte to inspect and edit it."));
                }
            });
        });
    }

    /// The image enlarged by an integer factor; with `cell`, a grid and the number of
    /// the item under the mouse.
    #[allow(clippy::too_many_arguments)]
    fn image(&mut self, ui: &mut egui::Ui, look: &Look, title: &'static str, w: usize, h: usize, pixels: &[u32], cell: usize) {
        let rgb: Vec<u8> = pixels.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        let img = egui::ColorImage::from_rgb([w, h], &rgb);
        let tex = self.textures.entry(title).or_insert_with(|| ui.ctx().load_texture(format!("debug_{title}"), img.clone(), egui::TextureOptions::NEAREST));
        tex.set(img, egui::TextureOptions::NEAREST);
        let p = look.p;
        let caption_h = 28.0;
        // largest integer scale that fits
        let k = ((ui.available_width() - 24.0) / w as f32).min((ui.available_height() - caption_h - 24.0) / h as f32).floor().max(1.0);
        let size = Vec2::new(w as f32 * k, h as f32 * k);
        let mut caption = match (w.checked_div(cell), h.checked_div(cell)) {
            (Some(cols), Some(rows)) => trf("{cols} × {rows} of {n} × {n} pixels · {k}×", &[("cols", &cols.to_string()), ("rows", &rows.to_string()), ("n", &cell.to_string()), ("k", &k.to_string())]),
            _ => format!("{w} × {h} · {k}×"),
        };
        ui.vertical_centered(|ui| {
            egui::Frame::new().fill(p.bg_sidebar).stroke(Stroke::new(1.0, p.border)).inner_margin(Margin::same(10)).show(ui, |ui| {
                let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
                let painter = ui.painter();
                painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                if cell > 0 {
                    let step = cell as f32 * k;
                    let grid = Stroke::new(1.0, Color32::from_white_alpha(18));
                    for i in 1..w / cell {
                        painter.vline(rect.left() + i as f32 * step, rect.y_range(), grid);
                    }
                    for i in 1..h / cell {
                        painter.hline(rect.x_range(), rect.top() + i as f32 * step, grid);
                    }
                    if let Some(pos) = resp.hover_pos() {
                        let (cx, cy) = (((pos.x - rect.left()) / step) as usize, ((pos.y - rect.top()) / step) as usize);
                        let r = Rect::from_min_size(rect.min + Vec2::new(cx as f32 * step, cy as f32 * step), Vec2::splat(step));
                        painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(2.0, p.text_accent), StrokeKind::Outside);
                        caption = trf("Tile {n}", &[("n", &format!("{:02X}", cy * (w / cell) + cx))]);
                    }
                }
            });
            ui.add_space(4.0);
            look.hint(ui, &caption);
        });
    }
}

/// The core's groups as cards of the same width, side by side while they fit.
fn cards(ui: &mut egui::Ui, look: &Look, groups: &[DebugGroup]) {
    let p = look.p;
    // as many columns as fit (cards at least 190 px), sharing the width; 26 = margins + outline
    let (gap, extra) = (12.0, 26.0);
    let avail = ui.available_width() - 2.0;
    let cols = (((avail + gap) / (190.0 * look.scale.max(1.0) + extra + gap)).floor() as usize).clamp(1, groups.len().max(1));
    let width = (avail - gap * (cols - 1) as f32) / cols as f32 - extra;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.with_layout(Layout::left_to_right(Align::Min).with_main_wrap(true), |ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(gap);
            for (gi, g) in groups.iter().enumerate() {
                egui::Frame::new().fill(p.bg_sidebar).stroke(Stroke::new(1.0, p.border)).inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.set_width(width);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        look.section(ui, &tr(g.title));
                        ui.add_space(2.0);
                        egui::Grid::new(("debug_group", gi)).num_columns(2).spacing([16.0, 7.0]).min_col_width(width * 0.42).show(ui, |ui| {
                            for (name, value) in &g.rows {
                                ui.label(RichText::new(tr(name)).size(look.f.base).color(p.text_muted));
                                value_cell(ui, look, value);
                                ui.end_row();
                            }
                        });
                    });
                });
            }
        });
    });
}

fn value_cell(ui: &mut egui::Ui, look: &Look, value: &DebugValue) {
    let p = look.p;
    let mono = FontId::monospace(look.f.base);
    match value {
        DebugValue::Text(t) => {
            let t = if t == "muted" { tr("muted") } else { t.clone() };
            ui.label(RichText::new(t).font(mono).color(p.text_main));
        }
        DebugValue::Flag(on) => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                look.swatch(ui, if *on { p.success } else { Color32::TRANSPARENT }, if *on { p.success } else { p.text_muted });
                ui.label(RichText::new(tr(if *on { "on" } else { "off" })).size(look.f.base).color(if *on { p.text_main } else { p.text_muted }));
            });
        }
        DebugValue::Color(rgb, t) => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                look.swatch(ui, Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, *rgb as u8), p.text_muted);
                ui.label(RichText::new(t).font(mono).color(p.text_main));
            });
        }
    }
}
