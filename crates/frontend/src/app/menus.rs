//! The menu bar.

#[allow(unused_imports)]
use super::*;

impl App {

    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        use crate::menu::{self, Mark};
        let ctx = ui.ctx().clone();
        let l = self.look;
        let mut open = false;
        let mut info: Option<PathBuf> = None; // Help › Game info
        let r = egui::Panel::top("menubar")
            .frame(egui::Frame::new().fill(l.p.bg_sidebar).inner_margin(egui::Margin::symmetric(4, 2)))
            .show_separator_line(false)
            .show(ui, |ui| {
                egui::MenuBar::new().style(menu::style).config(egui::containers::menu::MenuConfig::new().style(menu::style)).ui(ui, |ui| {
                    // a little room before and after each title (also widens the hover area)
                    ui.spacing_mut().button_padding.x = 10.0;
                    open |= ui
                        .menu_button(tr("File"), |ui| {
                            menu::begin(ui);
                            if menu::item(ui, &l, &tr("Open game…"), "Ctrl+O", Mark::None, true) {
                                self.open_dialog();
                            }
                            let mut pick = None;
                            menu::submenu(ui, &l, &tr("Games in folder"), true, |ui| {
                                if self.games.is_empty() {
                                    menu::note(ui, &l, &tr("No games in this folder"));
                                }
                                egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                                    for g in &self.games {
                                        let name = g.file_stem().unwrap_or_default().to_string_lossy();
                                        let cur = self.cur_game.as_ref() == Some(g);
                                        if menu::item(ui, &l, &name, "", Mark::Radio(cur), true) {
                                            pick = Some(g.clone());
                                        }
                                    }
                                });
                            });
                            if let Some(g) = pick {
                                self.load_game(&g);
                            }
                            if menu::item(ui, &l, &tr("Refresh game list"), &self.sc(Func::RefreshGames), Mark::None, true) {
                                self.refresh_games();
                            }
                            if menu::item(ui, &l, &tr("Play history…"), "", Mark::None, true) {
                                self.dialog = Dialog::History(None);
                            }
                            let recent: Vec<PathBuf> = self.cfg.recent.iter().filter(|g| g.is_file()).cloned().collect();
                            let mut pick = None;
                            let mut clear = false;
                            menu::submenu(ui, &l, &tr("Recent games"), !recent.is_empty(), |ui| {
                                for g in &recent {
                                    let name = g.file_stem().unwrap_or_default().to_string_lossy();
                                    let cur = self.cur_game.as_ref() == Some(g);
                                    if menu::item(ui, &l, &name, "", Mark::Radio(cur), true) {
                                        pick = Some(g.clone());
                                    }
                                }
                                menu::separator(ui, &l);
                                if menu::item(ui, &l, &tr("Clear list"), "", Mark::None, true) {
                                    clear = true;
                                }
                            });
                            if let Some(g) = pick {
                                self.load_game(&g);
                            }
                            if clear {
                                self.cfg.recent.clear();
                                self.cfg.save();
                            }
                            if menu::item(ui, &l, &tr("Open last game on startup"), "", Mark::Check(self.open_last), true) {
                                self.open_last = !self.open_last;
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &tr("Close game"), "Ctrl+W", Mark::None, self.loaded) {
                                self.close_game();
                            }
                            if menu::item(ui, &l, &tr("Quit"), "Ctrl+Q", Mark::None, true) {
                                ctx.send_viewport_cmd(ViewportCommand::Close);
                            }
                        })
                        .inner
                        .is_some();
                    open |= ui
                        .menu_button(tr("Emulation"), |ui| {
                            menu::begin(ui);
                            if menu::item(ui, &l, &tr("Pause"), &self.sc(Func::Pause), Mark::Check(self.paused), self.loaded) {
                                self.paused = !self.paused;
                            }
                            if menu::item(ui, &l, &tr("Pause when unfocused"), "", Mark::Check(self.pause_unfocused), true) {
                                self.pause_unfocused = !self.pause_unfocused;
                            }
                            if menu::item(ui, &l, &tr("Reset"), &self.sc(Func::Reset), Mark::None, self.loaded) {
                                self.reset_game();
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &tr("Save state"), &self.sc(Func::SaveState), Mark::None, self.loaded) {
                                self.save_state();
                            }
                            let has_state = self.state_path(self.slot).is_some_and(|p| p.is_file());
                            if menu::item(ui, &l, &tr("Load state"), &self.sc(Func::LoadState), Mark::None, self.loaded && has_state) {
                                self.load_state();
                            }
                            let mut pick = None;
                            let thumbs: Vec<Option<egui::TextureId>> = (1..=9).map(|n| if self.loaded { self.thumb(&ctx, n) } else { None }).collect();
                            menu::submenu(ui, &l, &trf("Slot ({n})", &[("n", &self.slot)]), true, |ui| {
                                egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                                    for n in 1..=9 {
                                        let txt = if self.loaded { format!("{}{}", trf("Slot {n}", &[("n", &n)]), self.slot_info(n)) } else { trf("Slot {n}", &[("n", &n)]) };
                                        if menu::item_thumb(ui, &l, thumbs[n as usize - 1], &txt, Mark::Radio(self.slot == n)) {
                                            pick = Some(n);
                                        }
                                    }
                                });
                            });
                            if let Some(n) = pick {
                                self.set_slot(n);
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &tr("Fast forward"), &self.sc(Func::FastForwardToggle), Mark::Check(self.ff_toggle), self.loaded) {
                                self.ff_toggle = !self.ff_toggle;
                            }
                            menu::submenu(ui, &l, &trf("Fast-forward speed ({n}x)", &[("n", &self.ff_speed)]), true, |ui| {
                                for v in [2, 3, 4, 8] {
                                    if menu::item(ui, &l, &format!("{v}x"), "", Mark::Radio(self.ff_speed == v), true) {
                                        self.ff_speed = v;
                                    }
                                }
                            });
                            if menu::item(ui, &l, &tr("Allow rewind"), "", Mark::Check(self.rewind_enabled), true) {
                                self.rewind_enabled = !self.rewind_enabled;
                                if !self.rewind_enabled {
                                    self.history.clear();
                                }
                            }
                            // the core's settings (e.g. the hardware model), one submenu each
                            for o in self.core.options() {
                                let cur = self.cfg.core_options.get(o.key).map_or(o.values[0].0, String::as_str).to_string();
                                let cur_label = o.values.iter().find(|v| v.0 == cur).map_or(o.values[0].1, |v| v.1);
                                let mut chosen = None;
                                menu::submenu(ui, &l, &format!("{} ({})", tr(o.label), tr(cur_label)), true, |ui| {
                                    for &(value, label) in o.values {
                                        if menu::item(ui, &l, &tr(label), "", Mark::Radio(cur == value), true) {
                                            chosen = Some(value);
                                        }
                                    }
                                });
                                if let Some(v) = chosen {
                                    self.core.set_option(o.key, v);
                                    self.cfg.core_options.insert(o.key.to_string(), v.to_string());
                                    self.cfg.save();
                                    self.history.clear(); // rewinding across models would mix them
                                }
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &tr("Controls and shortcuts…"), "", Mark::None, true) {
                                self.pads.take_presses();
                                self.dialog = Dialog::Gamepad { dev: 0, capture: None };
                            }
                            if menu::item(ui, &l, &tr("Select + Start (hold) opens the menu"), "", Mark::Check(self.menu_combo), true) {
                                self.menu_combo = !self.menu_combo;
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &tr("Folders…"), "", Mark::None, true) {
                                self.dialog = Dialog::Folders { move_files: true };
                            }
                        })
                        .inner
                        .is_some();
                    // systems with a BIOS: where it is and what is missing
                    if !self.sys.bios.is_empty() {
                        open |= ui
                            .menu_button(tr("BIOS"), |ui| {
                                menu::begin(ui);
                                if menu::item(ui, &l, &tr("Choose BIOS folder…"), "", Mark::None, true) {
                                    self.choose_bios_folder(None);
                                }
                                if menu::item(ui, &l, &tr("Open BIOS folder"), "", Mark::None, true) {
                                    open_folder(Folder::Bios.dir());
                                }
                                if menu::item(ui, &l, &tr("Check again"), "", Mark::None, true) {
                                    self.bios_found = None;
                                }
                                menu::separator(ui, &l);
                                let folder = Folder::Bios.dir().map_or_else(|| "?".to_string(), |d| d.display().to_string());
                                menu::note(ui, &l, &trf("Folder: {folder}", &[("folder", &folder)]));
                                let found = self.bios_found();
                                let n = found.iter().filter(|&&f| f).count();
                                menu::note(ui, &l, &trf("{n} of {total} files found (✓); click one to copy its name and CRC32", &[("n", &n), ("total", &found.len())]));
                                menu::separator(ui, &l);
                                let mut copy = None;
                                // size right-aligned in a fixed width, so the CRC32 column lines up
                                let size_w = self.sys.bios.iter().map(|b| size_text(b.size).chars().count()).max().unwrap_or(0);
                                for (b, ok) in self.sys.bios.iter().zip(&found) {
                                    let state = if *ok { String::new() } else if b.required { format!(" — {}", tr("missing")) } else { format!(" — {}", tr("missing (optional)")) };
                                    let detail = format!("{:>size_w$}  CRC32 {:08x}", size_text(b.size), b.crc32);
                                    if menu::item_mono(ui, &l, &format!("{}{state}", b.name), &detail, Mark::Check(*ok)) {
                                        copy = Some(format!("{} {:08x}", b.name, b.crc32));
                                    }
                                }
                                menu::separator(ui, &l);
                                if menu::item(ui, &l, &tr("Copy list"), "", Mark::None, true) {
                                    copy = Some(self.bios_report());
                                }
                                if let Some(text) = copy {
                                    ctx.copy_text(text);
                                    self.notify(tr("Copied to the clipboard"));
                                }
                            })
                            .inner
                            .is_some();
                    }
                    open |= ui
                        .menu_button(tr("Audio"), |ui| {
                            menu::begin(ui);
                            if menu::item(ui, &l, &tr("Sound on"), &self.sc(Func::Mute), Mark::Check(!self.muted), true) {
                                self.set_mute(!self.muted);
                            }
                            if menu::item(ui, &l, &tr("Smooth sound"), "", Mark::Check(self.soft_audio), true) {
                                self.soft_audio = !self.soft_audio;
                            }
                            menu::submenu(ui, &l, &trf("Volume ({n}%)", &[("n", &self.volume)]), true, |ui| {
                                for v in (10..=100).rev().step_by(10) {
                                    if menu::item(ui, &l, &format!("{v}%"), "", Mark::Radio(self.volume == v), true) {
                                        self.volume = v;
                                    }
                                }
                            });
                        })
                        .inner
                        .is_some();
                    open |= ui
                        .menu_button(tr("Video"), |ui| {
                            menu::begin(ui);
                            let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                            for s in 1..=6u32 {
                                if menu::item(ui, &l, &trf("Window {n}x", &[("n", &s)]), "", Mark::Radio(!full && self.scale == s), true) {
                                    self.set_scale(&ctx, s);
                                }
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &tr("Integer scaling"), "", Mark::Check(self.integer_scale), true) {
                                self.integer_scale = !self.integer_scale;
                            }
                            let mut filt = None;
                            menu::submenu(ui, &l, &trf("Filter ({name})", &[("name", &tr(self.filter.label()))]), true, |ui| {
                                for f in [Filter::None, Filter::Scanlines, Filter::Crt] {
                                    if menu::item(ui, &l, &tr(f.label()), "", Mark::Radio(self.filter == f), true) {
                                        filt = Some(f);
                                    }
                                }
                            });
                            if let Some(f) = filt {
                                self.filter = f;
                                self.dirty_frame = true; // resend the texture with the right smoothing
                            }
                            // frames: none, the app's bezels and the game cover
                            let frames: Vec<(&str, &str)> = [("none", "No frame")].into_iter().chain(crate::brand::get().bezels.iter().map(|b| (b.key, b.label))).chain([("cover", "Game cover")]).collect();
                            let current = frames.iter().find(|(k, _)| *k == self.frame).map_or("No frame", |(_, l)| *l);
                            let mut chosen = None;
                            menu::submenu(ui, &l, &trf("Frame ({name})", &[("name", &tr(current))]), true, |ui| {
                                for (k, label) in &frames {
                                    if menu::item(ui, &l, &tr(label), "", Mark::Radio(self.frame == *k), true) {
                                        chosen = Some(k.to_string());
                                    }
                                }
                            });
                            if let Some(k) = chosen {
                                self.frame = k;
                            }
                            if menu::item(ui, &l, &tr("Fullscreen"), &self.sc(Func::Fullscreen), Mark::Check(full), true) {
                                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!full));
                            }
                            if menu::item(ui, &l, &tr("Show FPS and speed"), "", Mark::Check(self.show_fps), true) {
                                self.show_fps = !self.show_fps;
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &tr("Couch mode"), "", Mark::Check(self.couch), true) {
                                let on = !self.couch;
                                self.set_couch(&ctx, on);
                            }
                        })
                        .inner
                        .is_some();
                    open |= ui
                        .menu_button(tr("Recording"), |ui| {
                            use crate::recorder::Kind;
                            menu::begin(ui);
                            if menu::item(ui, &l, &tr("Screenshot"), &self.sc(Func::Screenshot), Mark::None, self.loaded) {
                                self.screenshot();
                            }
                            menu::separator(ui, &l);
                            let rec = self.recorder.as_ref().map(|r| r.kind);
                            let (txt, on) = if rec == Some(Kind::Mp4) { ("Stop video recording", true) } else { ("Record video (MP4)", false) };
                            if menu::item(ui, &l, &tr(txt), &self.sc(Func::RecordVideo), Mark::Check(on), self.loaded && self.has_ffmpeg) {
                                self.toggle_record(Kind::Mp4);
                            }
                            let (txt, on) = if rec == Some(Kind::Gif) { ("Stop GIF", true) } else { ("Record GIF", false) };
                            if menu::item(ui, &l, &tr(txt), &self.sc(Func::RecordGif), Mark::Check(on), self.loaded) {
                                self.toggle_record(Kind::Gif);
                            }
                            if !self.has_ffmpeg {
                                menu::note(ui, &l, &tr("MP4 video needs ffmpeg installed"));
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &tr("Open recordings folder"), &self.sc(Func::OpenRecordings), Mark::None, true) {
                                open_folder(crate::config::videos_dir());
                            }
                            if menu::item(ui, &l, &tr("Open screenshots folder"), &self.sc(Func::OpenCaptures), Mark::None, true) {
                                open_folder(crate::config::pictures_dir());
                            }
                        })
                        .inner
                        .is_some();
                    open |= ui
                        .menu_button(tr("Tools"), |ui| {
                            menu::begin(ui);
                            if menu::item(ui, &l, &tr("Debugger…"), "", Mark::Check(self.debugger.open), true) {
                                self.debugger.open = !self.debugger.open;
                            }
                            if menu::item(ui, &l, &tr("Cheats…"), "", Mark::Check(self.cheats.open), true) {
                                self.cheats.open = !self.cheats.open;
                            }
                            // only for a system with games on several ROM chips (their files come in parts)
                            if self.sys.multi_file_roms {
                                menu::separator(ui, &l);
                                if menu::item(ui, &l, &tr("Join game parts…"), "", Mark::None, true) {
                                    self.dialog = Dialog::ExportRom { src: self.cur_game.clone(), result: None };
                                }
                            }
                        })
                        .inner
                        .is_some();
                    open |= ui
                        .menu_button(tr("Help"), |ui| {
                            menu::begin(ui);
                            // always bilingual, so it can be found even without understanding the current language
                            let mut lang = None;
                            menu::submenu(ui, &l, "Language / Idioma", true, |ui| {
                                for (code, name) in crate::i18n::langs() {
                                    if menu::item(ui, &l, name, "", Mark::Radio(crate::i18n::lang() == code), true) {
                                        lang = Some(code);
                                    }
                                }
                                menu::separator(ui, &l);
                                if menu::item(ui, &l, &tr("Export translation template…"), "", Mark::None, true) {
                                    self.export_template();
                                }
                                if menu::item(ui, &l, &tr("Open languages folder"), "", Mark::None, true) {
                                    open_folder(Folder::Languages.dir());
                                }
                            });
                            if let Some(c) = lang {
                                self.set_language(c);
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &tr("Controls"), "", Mark::None, true) {
                                self.dialog = Dialog::Controls;
                            }
                            if menu::item(ui, &l, &tr("Game info…"), "", Mark::None, self.loaded) {
                                info = self.cur_game.clone();
                            }
                            menu::separator(ui, &l);
                            if menu::item(ui, &l, &trf("About {name}", &[("name", &crate::brand::get().name)]), "", Mark::None, true) {
                                self.dialog = Dialog::About;
                            }
                        })
                        .inner
                        .is_some();
                });
            });
        self.bar_h = r.response.rect.height();
        self.menu_open = open;
        if let Some(g) = info {
            self.open_info(&g);
        }
    }
}
