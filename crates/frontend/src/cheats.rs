//! Cheats (Tools › Cheats…), in three steps: find where a value lives in the RAM
//! (start a search, play, say whether it increased, decreased…), change it once or
//! save it as a code, and hold saved codes every frame if wanted. Codes are saved per game in RetroArch's `.cht`
//! format (RAM cheats: handler 1, set to value, 8 bits, address = offset in the
//! system RAM), so the same file works in RetroArch with the libretro core.

use std::path::PathBuf;

use egui::{Align, Color32, Layout, Margin, RichText, Vec2};
use emu_api::{Core, SystemInfo};

use crate::i18n::{tr, trf};
use crate::theme::bold;
use crate::widgets::Look;

/// Candidates listed at most (the count is always shown).
const LIST_MAX: usize = 100;

#[derive(Clone, PartialEq, Debug)]
pub struct Code {
    pub desc: String,
    /// Offset in the system RAM (`Core::system_ram`).
    pub offset: usize,
    pub value: u8,
    pub enabled: bool,
}

struct Search {
    /// RAM at the last step, to compare with now.
    last: Vec<u8>,
    candidates: Vec<usize>,
}

#[derive(Default)]
pub struct Cheats {
    pub open: bool,
    codes: Vec<Code>,
    /// `.cht` file of the current game.
    file: Option<PathBuf>,
    search: Option<Search>,
    /// "Equal to" field of the search.
    equal: String,
    // chosen address: address, value and name of a new code
    addr: String,
    value: String,
    desc: String,
    /// Result of the last action, shown under the editor (text, is it an error).
    message: Option<(String, bool)>,
    /// Tab shown: the saved codes, or the RAM search.
    pub(crate) searching: bool,
}

/// A byte value typed by the user: decimal (0–255), or hexadecimal with `0x`/`$`.
fn parse_value(s: &str) -> Option<u8> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).or_else(|| s.strip_prefix('$')) {
        Some(h) => u8::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// A CPU address typed by the user, always hexadecimal (`BB22`, `0xBB22`, `$BB22`).
fn parse_addr(s: &str) -> Option<usize> {
    let s = s.trim();
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).or_else(|| s.strip_prefix('$')).unwrap_or(s);
    usize::from_str_radix(h, 16).ok()
}

/// A byte in decimal and hexadecimal: "64 (0x40)".
fn show(v: u8) -> String {
    format!("{v} (0x{v:02X})")
}

impl Cheats {
    /// Loads the codes of a game (`None` = no game).
    pub fn load(&mut self, file: Option<PathBuf>) {
        self.codes = file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).map(|t| parse(&t)).unwrap_or_default();
        self.file = file;
        self.search = None;
        self.message = None;
    }

    fn save(&self) {
        let Some(f) = &self.file else { return };
        let res = std::fs::create_dir_all(f.parent().unwrap_or(f)).and_then(|_| {
            if self.codes.is_empty() {
                std::fs::remove_file(f).or(Ok(()))
            } else {
                std::fs::write(f, to_cht(&self.codes))
            }
        });
        if let Err(e) = res {
            eprintln!("{}: {e}", f.display());
        }
    }

    /// Holds the codes marked "Hold"; called before every emulated frame.
    pub fn apply(&self, core: &mut dyn Core) {
        let ram = core.system_ram_mut();
        for c in self.codes.iter().filter(|c| c.enabled) {
            if let Some(b) = ram.get_mut(c.offset) {
                *b = c.value;
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, look: &Look, sys: &SystemInfo, core: Option<&mut dyn Core>) {
        if !self.open {
            return;
        }
        let builder = egui::ViewportBuilder::default().with_title(tr("Cheats")).with_inner_size([720.0, 560.0]).with_min_inner_size([560.0, 420.0]);
        let mut core = core;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("cheats"), builder, |ui, _| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                self.open = false;
            }
            self.body(ui, look, sys, &mut core);
        });
        ctx.request_repaint_of(egui::ViewportId::from_hash_of("cheats"));
    }

    /// The window's contents. The documentation screenshots draw it in the main window,
    /// since a separate window can't be captured. Same layout as the debugger: a strip
    /// with the summary and the tabs, then cards.
    pub fn body(&mut self, ui: &mut egui::Ui, look: &Look, sys: &SystemInfo, core: &mut Option<&mut dyn Core>) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(look.p.bg_main)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            let Some(core) = core.as_deref_mut() else {
                egui::Frame::new().inner_margin(16).show(ui, |ui| look.hint(ui, &tr("No game loaded")));
                return;
            };
            look.strip(ui, |ui| {
                ui.horizontal(|ui| {
                    let held = self.codes.iter().filter(|c| c.enabled).count();
                    look.swatch(ui, if held > 0 { look.p.success } else { Color32::TRANSPARENT }, if held > 0 { look.p.success } else { look.p.text_muted });
                    ui.label(RichText::new(trf("{n} codes · {held} held", &[("n", &self.codes.len()), ("held", &held)])).font(bold(look.f.base)).color(look.p.text_main));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        look.hint(ui, &tr("Saved per game in RetroArch's .cht format"));
                    });
                });
                ui.add_space(4.0);
                let mut tab = self.searching as usize;
                look.tabs(ui, &["Codes", "Search the RAM"], &mut tab);
                self.searching = tab == 1;
            });
            egui::Frame::new().inner_margin(Margin::same(14)).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    if self.searching {
                        self.search_ui(ui, look, sys, core.system_ram());
                    } else {
                        self.codes_ui(ui, look, sys, core);
                        self.editor_ui(ui, look, sys, core);
                    }
                });
            });
        });
    }

    /// "Search the RAM" tab: a card per step, then the candidates.
    fn search_ui(&mut self, ui: &mut egui::Ui, look: &Look, sys: &SystemInfo, ram: &[u8]) {
        let base = sys.ram_address as usize;
        let mut filter: Option<Box<dyn Fn(u8, u8) -> bool>> = None;
        // 1. start
        step_card(ui, look, "1", "Begin", |ui| {
            ui.horizontal(|ui| {
                if ui.button(tr(if self.search.is_some() { "Restart search" } else { "Start search" })).clicked() {
                    self.search = Some(Search { last: ram.to_vec(), candidates: (0..ram.len()).collect() });
                }
                match &self.search {
                    Some(s) => ui.label(RichText::new(trf("{n} possible addresses", &[("n", &s.candidates.len())])).color(look.p.text_main)),
                    None => look.hint(ui, &tr("every RAM address is a candidate")),
                };
            });
        });
        // 2. play and say what happened
        step_card(ui, look, "2", "Play", |ui| {
            look.hint(ui, &tr("Play until the value changes (lose a life, score points), then say what happened to it:"));
            ui.add_enabled_ui(self.search.is_some(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, f) in [
                        ("Increased", Box::new(|old: u8, new: u8| new > old) as Box<dyn Fn(u8, u8) -> bool>),
                        ("Decreased", Box::new(|old, new| new < old)),
                        ("Changed", Box::new(|old, new| new != old)),
                        ("Did not change", Box::new(|old, new| new == old)),
                    ] {
                        if ui.button(tr(label)).clicked() {
                            filter = Some(f);
                        }
                    }
                    ui.add_space(8.0);
                    if ui.button(tr("Is equal to")).clicked() {
                        match parse_value(&self.equal) {
                            Some(v) => filter = Some(Box::new(move |_, new| new == v)),
                            None => self.message = Some((tr("Invalid value: use 0 to 255, or hexadecimal with 0x (0x40)."), true)),
                        }
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.equal).hint_text("3").desired_width(44.0).font(egui::TextStyle::Monospace));
                });
            });
            look.hint(ui, &tr("Repeat until few addresses are left."));
            if let Some((m, true)) = &self.message {
                ui.label(RichText::new(m).color(look.p.danger_text));
            }
        });
        // 3. pick one
        if let Some(s) = &mut self.search
            && let Some(f) = filter
        {
            s.candidates.retain(|&i| ram.get(i).is_some_and(|&new| f(s.last[i], new)));
            s.last = ram.to_vec();
        }
        let mut pick = None;
        step_card(ui, look, "3", "Choose", |ui| {
            look.hint(ui, &tr("Use takes the address to the Codes tab, to change it there."));
            let Some(s) = &self.search else { return };
            if s.candidates.is_empty() {
                look.hint(ui, &tr("None left: the value may be stored differently (see the tip below). Start again."));
            } else if s.candidates.len() > LIST_MAX {
                look.hint(ui, &tr("Too many to list: keep playing and narrowing down."));
            } else {
                egui::Grid::new("cheat_search").num_columns(4).spacing([28.0, 6.0]).striped(true).show(ui, |ui| {
                    for t in ["Address", "Before", "Now", ""] {
                        ui.label(look.column(&tr(t)));
                    }
                    ui.end_row();
                    for &i in &s.candidates {
                        ui.label(RichText::new(format!("{:04X}", base + i)).font(mono(look)).color(look.p.text_accent));
                        ui.label(RichText::new(show(s.last[i])).font(mono(look)).color(look.p.text_muted));
                        ui.label(RichText::new(show(ram[i])).font(mono(look)).color(look.p.text_main));
                        if ui.small_button(tr("Use")).clicked() {
                            pick = Some(i);
                        }
                        ui.end_row();
                    }
                });
            }
        });
        if let Some(i) = pick {
            self.addr = format!("{:04X}", base + i);
            self.value = ram[i].to_string();
            self.message = None;
            self.searching = false;
        }
        look.card(ui, None, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr("Tip: scores")).color(look.p.text_section)).show(ui, |ui| {
                ui.add(egui::Label::new(look.hint_rt(tr("Tip: scores are often stored as BCD (two decimal digits per byte: 400 points can be 0x04 0x00 or 0x40), so search with Increased / Did not change instead of the number on screen. Example, Pooyan: score in BB22 (low) and BB23 (high), in tens of points."))).wrap());
            });
        });
    }

    /// "Codes" tab, bottom: a new code (or a one-off change).
    fn editor_ui(&mut self, ui: &mut egui::Ui, look: &Look, sys: &SystemInfo, core: &mut dyn Core) {
        let base = sys.ram_address as usize;
        let len = core.system_ram().len();
        let offset = parse_addr(&self.addr).and_then(|a| a.checked_sub(base)).filter(|&o| o < len);
        let (mut write, mut add) = (false, false);
        look.card(ui, Some(&tr("NEW CODE")), |ui| {
            // label over each field (a Grid would cap the fields at the labels' width)
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let field = |ui: &mut egui::Ui, label: &str, edit: egui::TextEdit| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.label(look.column(&tr(label)));
                        ui.add(edit);
                    });
                };
                field(ui, "Address", egui::TextEdit::singleline(&mut self.addr).hint_text(format!("{base:04X}")).desired_width(64.0).font(egui::TextStyle::Monospace));
                field(ui, "Value", egui::TextEdit::singleline(&mut self.value).hint_text("99").desired_width(48.0).font(egui::TextStyle::Monospace));
                field(ui, "Name", egui::TextEdit::singleline(&mut self.desc).hint_text(tr("e.g. Lives")).desired_width(240.0));
            });
            match offset {
                Some(o) => look.hint(ui, &trf("now: {value}", &[("value", &show(core.system_ram()[o]))])),
                None => look.hint(ui, &trf("Address in hexadecimal, from {from} to {to}; value from 0 to 255 (or hexadecimal with 0x).", &[("from", &format!("{base:04X}")), ("to", &format!("{:04X}", base + len - 1))])),
            };
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                add = look.primary(ui, &tr("Save as code")).on_hover_text(tr("Keeps it in the list above, to use again or hold every frame.")).clicked();
                write = ui.add(egui::Button::new(tr("Write now")).min_size(Vec2::new(0.0, look.btn_h()))).on_hover_text(tr("Changes the value once; the game goes on from there.")).clicked();
                if let Some((m, err)) = &self.message {
                    ui.add_space(10.0);
                    ui.label(RichText::new(m).color(if *err { look.p.danger_text } else { look.p.success }));
                }
            });
        });
        if write || add {
            let value = parse_value(&self.value);
            self.message = match (offset, value) {
                (None, _) => Some((tr("Invalid address."), true)),
                (_, None) => Some((tr("Invalid value: use 0 to 255, or hexadecimal with 0x (0x40)."), true)),
                (Some(o), Some(v)) => {
                    if write {
                        core.system_ram_mut()[o] = v;
                    }
                    if add {
                        let desc = if self.desc.trim().is_empty() { format!("{:04X} = {v}", base + o) } else { self.desc.trim().to_string() };
                        self.codes.push(Code { desc, offset: o, value: v, enabled: false });
                        self.desc.clear();
                        self.save();
                    }
                    Some((tr(if add { "Code saved." } else { "Written. The screen may only show it when the game redraws that part." }), false))
                }
            };
        }
    }

    /// "Codes" tab, top: the codes of this game.
    fn codes_ui(&mut self, ui: &mut egui::Ui, look: &Look, sys: &SystemInfo, core: &mut dyn Core) {
        let base = sys.ram_address as usize;
        let mut remove = None;
        let mut changed = false;
        look.card(ui, Some(&tr("CODES OF THIS GAME")), |ui| {
            if self.codes.is_empty() {
                look.hint(ui, &tr("No codes yet: create one below, or find the address in Search the RAM."));
                return;
            }
            egui::Grid::new("cheat_codes").num_columns(6).spacing([18.0, 8.0]).striped(true).show(ui, |ui| {
                for t in ["Hold", "Name", "Address", "Value", "Now", ""] {
                    ui.label(look.column(&tr(t)));
                }
                ui.end_row();
                for (i, c) in self.codes.iter_mut().enumerate() {
                    changed |= ui.checkbox(&mut c.enabled, "").on_hover_text(tr("Holds the value every frame (the game cannot change it).")).changed();
                    ui.label(RichText::new(&c.desc).color(look.p.text_main));
                    ui.label(RichText::new(format!("{:04X}", base + c.offset)).font(mono(look)).color(look.p.text_accent));
                    changed |= ui.add(egui::DragValue::new(&mut c.value).range(0..=255)).changed();
                    let now = core.system_ram().get(c.offset).copied().unwrap_or(0);
                    ui.label(RichText::new(show(now)).font(mono(look)).color(if c.enabled && now == c.value { look.p.success } else { look.p.text_muted }));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Write now")).clicked()
                            && let Some(b) = core.system_ram_mut().get_mut(c.offset)
                        {
                            *b = c.value;
                        }
                        if ui.add(egui::Button::new(RichText::new(tr("Remove")).color(look.p.danger_text))).clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.end_row();
                }
            });
        });
        if let Some(i) = remove {
            self.codes.remove(i);
            changed = true;
        }
        if changed {
            self.save();
        }
    }
}

fn mono(look: &Look) -> egui::FontId {
    egui::FontId::monospace(look.f.base)
}

/// A step of the search: a numbered badge and its title, then the contents.
fn step_card(ui: &mut egui::Ui, look: &Look, n: &str, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    look.card(ui, None, |ui| {
        ui.horizontal(|ui| {
            let galley = ui.painter().layout_no_wrap(n.to_string(), bold(look.f.base), look.p.text_main);
            let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 0.0, look.p.border_focus);
            ui.painter().galley(r.center() - galley.size() / 2.0, galley, look.p.text_main);
            ui.label(RichText::new(tr(title)).font(bold(look.f.md)).color(look.p.text_main));
        });
        body(ui);
    });
}

// ------------------------------------------------------------ .cht files

/// Reads RetroArch's `.cht` (only RAM cheats that set an 8-bit value; others are
/// skipped).
pub fn parse(text: &str) -> Vec<Code> {
    let kv: std::collections::HashMap<&str, &str> = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim().trim_matches('"')))
        .collect();
    let n: usize = kv.get("cheats").and_then(|v| v.parse().ok()).unwrap_or(0);
    (0..n)
        .filter_map(|i| {
            let get = |k: &str| kv.get(format!("cheat{i}_{k}").as_str()).copied();
            let num = |k: &str| get(k).and_then(|v| v.parse::<u64>().ok());
            if num("handler") != Some(1) || num("cheat_type") != Some(1) || num("memory_search_size") != Some(3) {
                return None;
            }
            Some(Code { desc: get("desc").unwrap_or("").to_string(), offset: num("address")? as usize, value: num("value")? as u8, enabled: get("enable") == Some("true") })
        })
        .collect()
}

/// Writes the codes as RetroArch writes RAM cheats.
pub fn to_cht(codes: &[Code]) -> String {
    let mut out = format!("cheats = \"{}\"\n\n", codes.len());
    for (i, c) in codes.iter().enumerate() {
        let desc = c.desc.replace('"', "'");
        out += &format!(
            "cheat{i}_desc = \"{desc}\"\ncheat{i}_code = \"\"\ncheat{i}_enable = \"{}\"\ncheat{i}_big_endian = \"false\"\ncheat{i}_handler = \"1\"\ncheat{i}_memory_search_size = \"3\"\ncheat{i}_cheat_type = \"1\"\ncheat{i}_value = \"{}\"\ncheat{i}_address = \"{}\"\ncheat{i}_address_bit_position = \"0\"\ncheat{i}_repeat_count = \"1\"\ncheat{i}_repeat_add_to_value = \"0\"\ncheat{i}_repeat_add_to_address = \"1\"\n\n",
            c.enabled, c.value, c.offset
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_in_decimal_or_hex() {
        assert_eq!(parse_value("64"), Some(64));
        assert_eq!(parse_value(" 0x40 "), Some(0x40));
        assert_eq!(parse_value("$ff"), Some(255));
        assert_eq!(parse_value("400"), None); // doesn't fit a byte
        assert_eq!(parse_value("abc"), None);
        assert_eq!(parse_addr("BB22"), Some(0xBB22));
        assert_eq!(parse_addr("0xbb22"), Some(0xBB22));
        assert_eq!(parse_addr("$BB22"), Some(0xBB22));
        assert_eq!(show(0x40), "64 (0x40)");
    }

    #[test]
    fn cht_round_trip() {
        let codes = vec![
            Code { desc: "Infinite lives".into(), offset: 0x312, value: 3, enabled: true },
            Code { desc: "Say \"hi\"".into(), offset: 0, value: 0xFF, enabled: false },
        ];
        let back = parse(&to_cht(&codes));
        assert_eq!(back[0], codes[0]);
        assert_eq!(back[1].desc, "Say 'hi'");
        assert_eq!((back[1].offset, back[1].value, back[1].enabled), (0, 0xFF, false));
    }

    #[test]
    fn skips_other_cheat_kinds() {
        // a code-based cheat (handler 0) and a 16-bit one are not ours
        let t = "cheats = 2\ncheat0_handler = 0\ncheat0_code = \"ABC\"\ncheat1_handler = \"1\"\ncheat1_cheat_type = \"1\"\ncheat1_memory_search_size = \"4\"\ncheat1_address = \"5\"\ncheat1_value = \"1\"\n";
        assert!(parse(t).is_empty());
    }
}
