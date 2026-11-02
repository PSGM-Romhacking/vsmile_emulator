//! The other dialogs: about, game info, controls summary, save slots…

#[allow(unused_imports)]
use super::*;

impl App {
    /// Icon texture (the same PNG embedded as the window icon).
    pub(super) fn about_icon(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if self.about_icon.is_none() {
            let img = image::load_from_memory(crate::brand::get().icon_png).ok()?.to_rgba8();
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            self.about_icon = Some(ctx.load_texture("icon", ci, egui::TextureOptions::LINEAR));
        }
        self.about_icon.clone()
    }

    /// Tools › Join game parts…: asks where to save and writes the game as one file, the parts
    /// joined (largest first). `None` if the person cancelled.
    fn export_joined_rom(&self, src: &Path) -> Option<Result<String, String>> {
        let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "rom".into());
        // the system's own extension first (the one the file is named with), the others as alternatives
        let ext = self.sys.extensions.first().copied().unwrap_or("bin");
        let mut dlg = rfd::FileDialog::new().set_title(tr("Join game parts…")).set_file_name(format!("{stem}.{ext}"));
        for e in self.sys.extensions {
            dlg = dlg.add_filter(format!(".{e}"), &[*e]);
        }
        let mut dest = dlg.save_file()?;
        if dest.extension().is_none() {
            dest.set_extension(ext);
        }
        let rom = match crate::rom::export_joined(src, &dest) {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
        let saved = trf("Saved {file} ({size} bytes, CRC32 {crc})", &[("file", &dest.display()), ("size", &rom.len()), ("crc", &format!("{:08x}", emu_api::crc32(&rom)))]);
        let known = match self.sheets.for_rom(&rom) {
            Some(s) => trf("Recognized as {title}", &[("title", &s.title)]),
            None => tr("Not in the game database"),
        };
        Some(Ok(format!("{saved}. {known}.")))
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        let l = self.look;
        let icon = if self.dialog == Dialog::About { self.about_icon(ctx) } else { None };
        let cover = match &self.dialog {
            Dialog::Info(_, Some(p), _) => {
                let p = p.clone();
                self.cover_texture(ctx, &p, crate::covers::Art::Boxart)
            }
            _ => None,
        };
        let close = match &self.dialog {
            Dialog::None => return,
            Dialog::Gamepad { .. } => {
                self.gamepad_dialog(ctx);
                return;
            }
            Dialog::Folders { .. } => {
                self.folders_dialog(ctx);
                return;
            }
            Dialog::Controls => {
                let devs = self.pads.snapshot();
                let p1 = devs.first().map(|d| (d.name.clone(), self.mapping_for(d)));
                let sub = match &p1 {
                    Some((n, _)) => trf("Controller: {name}. To change: Emulation › Controls and shortcuts…", &[("name", n)]),
                    None if self.pads.no_access.load(std::sync::atomic::Ordering::Relaxed) => tr("No controller found: no permission to read /dev/input (see the README, FreeBSD)."),
                    None => tr("No controller connected. To change: Emulation › Controls and shortcuts…"),
                };
                let rows: Vec<(Func, String, String)> = Func::all(self.sys)
                    .into_iter()
                    .map(|f| {
                        let pad = p1.as_ref().and_then(|(_, m)| m.get(&f)).filter(|v| !v.is_empty()).map(|v| v.iter().map(|i| i.label()).collect::<Vec<_>>().join(" / "));
                        (f, self.key_text(f), pad.unwrap_or_default())
                    })
                    .collect();
                let r = l.dialog(ctx, "controls", Vec2::new(620.0, 640.0), &tr("Controls"), &sub, |ui| {
                    egui::ScrollArea::vertical().max_height(ui.available_height() - 56.0).show(ui, |ui| {
                        egui::Grid::new("ctl").num_columns(3).spacing([28.0, 8.0]).striped(true).show(ui, |ui| {
                            for t in ["ACTION", "KEYBOARD", "CONTROLLER"] {
                                ui.label(RichText::new(tr(t)).font(theme::bold(l.f.sm)).color(l.p.text_section).extra_letter_spacing(1.0));
                            }
                            ui.end_row();
                            for (f, k, p) in &rows {
                                ui.label(match f.player() {
                                    Some(n) => trf("P{n} · {action}", &[("n", &n), ("action", &tr(f.label(self.sys)))]),
                                    None => tr(f.label(self.sys)),
                                });
                                ui.label(if k.is_empty() { RichText::new("—").color(l.p.text_muted) } else { RichText::new(k) });
                                ui.label(RichText::new(if p.is_empty() { "—" } else { p }).color(l.p.text_muted));
                                ui.end_row();
                            }
                            ui.label(tr("Open game / Close game / Quit"));
                            ui.label("Ctrl+O / Ctrl+W / Ctrl+Q");
                            ui.label(RichText::new(tr("fixed")).color(l.p.text_muted));
                            ui.end_row();
                        });
                    });
                    let mut c = false;
                    l.footer(ui, |_| {}, |ui| c = l.primary(ui, &tr("Close")).clicked());
                    c
                });
                r.inner || r.should_close()
            }
            Dialog::About => {
                let brand = crate::brand::get();
                let r = l.dialog(ctx, "about", Vec2::new(600.0, 245.0), brand.name, "", |ui| {
                    ui.horizontal_top(|ui| {
                        if let Some(t) = &icon {
                            ui.image((t.id(), Vec2::splat(112.0)));
                            ui.add_space(12.0);
                        }
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 12.0;
                            l.hint(ui, &trf("version {v}", &[("v", &brand.version)]));
                            ui.add(egui::Label::new(tr(brand.about)).wrap());
                            // placeholder until the official repository exists: text; with a real URL, a link
                            if brand.repo_url.contains('<') {
                                l.hint(ui, brand.repo_url);
                            } else if !brand.repo_url.is_empty()
                                && ui.link(brand.repo_url).clicked() {
                                    crate::platform::open_url(brand.repo_url);
                                }
                        });
                    });
                    let mut c = false;
                    l.footer(ui, |_| {}, |ui| c = l.primary(ui, &tr("Close")).clicked());
                    c
                });
                r.inner || r.should_close()
            }
            Dialog::Info(sheet, _, game) => {
                let file = game.file_name().map(|f| f.to_string_lossy().into_owned());
                // the folder only: the file name is on the line above
                let full = game.parent().map(|d| d.display().to_string());
                // play time and last session, only once the game has been played
                let key = game.to_string_lossy();
                // "40min  ·  21/10/2026 17:05 (1 h ago)": total time, then the last session
                let played = self.usage.last_played(&key).map(|ts| {
                    let total = crate::usage::play_time(self.usage.played(&key));
                    let when = format!("{} {} ({})", crate::usage::date(ts), crate::usage::time(ts), crate::usage::relative(ts));
                    if total.is_empty() { when } else { format!("{total}  ·  {when}") }
                });
                let sheet = sheet.clone();
                // tall enough for the cover (a bit larger than in the library) plus title and footer
                let info_h = (COVER_H * 1.25 + 250.0).max(460.0);
                let r = l.dialog(ctx, "info", Vec2::new(820.0, info_h), &sheet.title, sheet.alt_title.as_deref().unwrap_or(""), |ui| {
                    let body_h = ui.available_height() - 56.0;
                    egui::ScrollArea::vertical().max_height(body_h).show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            if let Some(t) = &cover {
                                // never taller than the room there is: the cover never makes it scroll
                                let sz = t.size_vec2();
                                let h = (COVER_H * 1.25).min(body_h - 4.0);
                                ui.image((t.id(), sz * (cover_w(self.sys) * 1.25 / sz.x).min(h / sz.y)));
                                ui.add_space(16.0);
                            }
                            // values column: what's left next to the cover, minus the labels column
                            let text_w = (ui.available_width() - 130.0).max(160.0);
                            egui::Grid::new("info_data").num_columns(2).spacing([20.0, 8.0]).show(ui, |ui| {
                                let fields = [
                                    ("Genre", &sheet.genre),
                                    ("Year", &sheet.year),
                                    ("Developer", &sheet.developer),
                                    ("Publisher", &sheet.publisher),
                                    ("Cartridge", &sheet.serial),
                                    ("File", &file),
                                    ("Path", &full),
                                    ("Description", &sheet.description),
                                    ("Players", &sheet.players),
                                    ("Played", &played),
                                ];
                                for (k, v) in fields {
                                    if let Some(v) = v {
                                        ui.label(RichText::new(tr(k)).color(l.p.text_muted));
                                        ui.scope(|ui| {
                                            ui.set_max_width(text_w);
                                            ui.add(egui::Label::new(v).wrap());
                                        });
                                        ui.end_row();
                                    }
                                }
                            });
                        });
                        if let Some(n) = &sheet.notes {
                            ui.add_space(14.0);
                            l.hint(ui, n);
                        }
                    });
                    let mut c = false;
                    l.footer(ui, |_| {}, |ui| c = l.primary(ui, &tr("Close")).clicked());
                    c
                });
                r.inner || r.should_close()
            }
            Dialog::MissingBios(msg, game) => {
                let (msg, game) = (msg.clone(), game.clone());
                let mut pick = false;
                let r = l.dialog(ctx, "bios", Vec2::new(540.0, 260.0), &tr("Could not open"), "", |ui| {
                    ui.add(egui::Label::new(msg).wrap());
                    let mut c = false;
                    l.footer(ui, |ui| c = ui.button(tr("Close")).clicked(), |ui| pick = l.primary(ui, &tr("Choose BIOS folder…")).clicked());
                    c
                });
                if pick {
                    self.dialog = Dialog::None;
                    self.choose_bios_folder(game.as_deref());
                    return;
                }
                r.inner || r.should_close()
            }
            Dialog::History(game) => {
                use crate::usage::{date, play_time, time};
                let key = game.as_ref().map(|g| g.to_string_lossy().into_owned());
                // a game's name as the library shows it (title from the database, else the file name)
                let name_of = |p: &str| {
                    let path = PathBuf::from(p);
                    self.ids.get(&path).and_then(|i| i.title.clone()).unwrap_or_else(|| stem(&path))
                };
                let dur = |s: f64| {
                    let t = play_time(s);
                    if t.is_empty() { tr("<1 min") } else { t }
                };
                let sessions = self.usage.sessions(key.as_deref());
                let total: f64 = sessions.iter().map(|s| s.duration).sum();
                let title = key.as_deref().map_or_else(|| tr("Play history"), name_of);
                let sub = if sessions.is_empty() { String::new() } else { trf("{n} sessions  •  {t} in total", &[("n", &sessions.len()), ("t", &dur(total))]) };
                let rows: Vec<[String; 4]> = sessions.iter().map(|s| [date(s.start), time(s.start), name_of(&s.path), dur(s.duration)]).collect();
                let all = key.is_none();
                let r = l.dialog(ctx, "history", Vec2::new(if all { 720.0 } else { 520.0 }, 480.0), &title, &sub, |ui| {
                    if rows.is_empty() {
                        l.hint(ui, &tr("No sessions recorded yet."));
                    } else {
                        egui::ScrollArea::vertical().max_height(ui.available_height() - 56.0).show(ui, |ui| {
                            egui::Grid::new("history").num_columns(if all { 4 } else { 3 }).spacing([28.0, 10.0]).show(ui, |ui| {
                                ui.label(l.column(&tr("Date")));
                                ui.label(l.column(&tr("Time")));
                                if all {
                                    ui.label(l.column(&tr("Game")));
                                }
                                ui.label(l.column(&tr("Duration")));
                                ui.end_row();
                                for [d, h, game, len] in &rows {
                                    ui.label(d);
                                    ui.label(l.hint_rt(h));
                                    if all {
                                        ui.label(game);
                                    }
                                    ui.label(len);
                                    ui.end_row();
                                }
                            });
                        });
                    }
                    let mut c = false;
                    l.footer(ui, |_| {}, |ui| c = l.primary(ui, &tr("Close")).clicked());
                    c
                });
                r.inner || r.should_close()
            }
            Dialog::Crash(report) => {
                let report = report.clone();
                let r = l.dialog(ctx, "crash", Vec2::new(520.0, 240.0), &tr("Error report"), "", |ui| {
                    ui.add(egui::Label::new(tr("The program closed because of an error last time. A report was saved; please attach it when reporting the problem.")).wrap());
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(egui::RichText::new(report.display().to_string()).monospace()).wrap());
                    let mut c = false;
                    l.footer(
                        ui,
                        |ui| {
                            if ui.button(tr("Open folder")).clicked()
                                && let Some(d) = report.parent()
                            {
                                crate::platform::open_folder(d);
                            }
                        },
                        |ui| c = l.primary(ui, &tr("Close")).clicked(),
                    );
                    c
                });
                r.inner || r.should_close()
            }
            Dialog::ExportRom { src, result } => {
                let (src, result) = (src.clone(), result.clone());
                let (mut choose, mut save) = (false, false);
                let r = l.dialog(ctx, "export_rom", Vec2::new(600.0, 420.0), &tr("Join the game parts into one file"), "", |ui| {
                    ui.add(egui::Label::new(tr("Some games come in more than one file (for example, inside a .zip or a .7z), because they are on more than one chip in the cartridge. The emulator joins the parts by itself when it opens the game: you do not need this to play.")).wrap());
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(tr("This saves the parts as a single file, the largest first, for other emulators that expect one.")).wrap());
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(tr("Keep the original files: game databases list each part with its own checksum, so tools that identify games by checksum (the RetroArch scanner, for example) will not recognize the joined file.")).wrap());
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("{}:", tr("File")));
                        ui.label(src.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| tr("No file chosen")));
                        choose = ui.button(tr("Choose file…")).clicked();
                    });
                    if let Some(res) = &result {
                        ui.add_space(8.0);
                        match res {
                            Ok(m) | Err(m) => ui.add(egui::Label::new(m.as_str()).wrap()),
                        };
                    }
                    let mut c = false;
                    l.footer(ui, |ui| c = ui.button(tr("Close")).clicked(), |ui| save = ui.add_enabled_ui(src.is_some(), |ui| l.primary(ui, &tr("Save as…")).clicked()).inner);
                    c
                });
                let (mut src2, mut result2) = (src.clone(), result);
                if choose {
                    let mut exts: Vec<&str> = self.sys.extensions.to_vec();
                    exts.extend(["zip", "7z"]);
                    if let Some(p) = rfd::FileDialog::new().set_title(tr("Choose the game's file…")).add_filter("ROM", &exts).pick_file() {
                        src2 = Some(p);
                        result2 = None;
                    }
                }
                if save && let Some(s) = &src && let Some(res) = self.export_joined_rom(s) {
                    result2 = Some(res);
                }
                self.dialog = Dialog::ExportRom { src: src2, result: result2 };
                r.inner || r.should_close()
            }
            Dialog::Error(msg) => {
                let msg = msg.clone();
                let r = l.dialog(ctx, "error", Vec2::new(480.0, 220.0), &tr("Could not open"), "", |ui| {
                    ui.add(egui::Label::new(msg).wrap());
                    let mut c = false;
                    l.footer(ui, |_| {}, |ui| c = l.primary(ui, &tr("Close")).clicked());
                    c
                });
                r.inner || r.should_close()
            }
        };
        if close {
            self.dialog = Dialog::None;
        }
    }
}
