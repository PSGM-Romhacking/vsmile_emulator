//! Emulation › Folders…: where each kind of file lives, with buttons to open the folder,
//! change it and go back to the default (see `config::Folder`).

#[allow(unused_imports)]
use super::*;
use crate::config::Folder;

/// A button pressed in the dialog.
enum Action {
    OpenRoms,
    ChangeRoms,
    Open(Folder),
    Change(Folder),
    Default(Folder),
}

impl App {
    pub(super) fn folders_dialog(&mut self, ctx: &egui::Context) {
        let Dialog::Folders { mut move_files } = self.dialog else { return };
        let l = self.look;
        let mut action = None;
        // (folder, path in use, changed by the user, changed but missing)
        let info = |f: Folder| {
            let custom = self.cfg.dirs.get(f.key());
            (f, f.dir(), custom.is_some(), custom.is_some_and(|c| self.cfg.unreachable.contains(c)))
        };
        // battery saves and BIOS only for the systems that have them
        let mut progress = vec![info(Folder::States), info(Folder::Cheats), info(Folder::Saves)];
        if !self.sys.bios.is_empty() {
            progress.push(info(Folder::Bios));
        }
        let groups = [
            (tr("Games and progress"), progress, None),
            (tr("Your files"), vec![info(Folder::Covers), info(Folder::Languages)], Some(tr("Covers: Named_Boxarts, Named_Titles and Named_Snaps folders, PNGs named after the game. Languages: Help › Language › Export translation template…"))),
            (tr("Screenshots and recordings"), vec![info(Folder::Screenshots), info(Folder::Recordings)], None),
        ];
        let app = short_path(crate::config::app_dir().as_deref());
        let sub = match crate::config::portable() {
            crate::config::Portable::On(_) => trf("Portable mode (portable.txt next to the program): everything else lives in {path}.", &[("path", &app)]),
            _ => trf("Everything else lives in {path}.", &[("path", &app)]),
        };
        let rom_dir = self.rom_dir.clone();
        let r = l.dialog(ctx, "folders", Vec2::new(860.0, 600.0), &tr("Folders"), &sub, |ui| {
            // one line per folder: name, path (shortened; full on hover), buttons on the right
            let row = |ui: &mut egui::Ui, name: &str, path: Option<&Path>, missing: bool, custom: Option<bool>| -> Option<u8> {
                let mut clicked = None;
                ui.horizontal(|ui| {
                    ui.set_min_height(l.btn_h());
                    ui.allocate_ui_with_layout(Vec2::new(150.0, l.btn_h()), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_width(150.0);
                        ui.add(egui::Label::new(l.strong(name)).truncate());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // right to left: Default, Change…, Open
                        match custom {
                            Some(c) => {
                                if ui.add_enabled(c, egui::Button::new(tr("Default"))).on_hover_text(tr("Back to the default folder")).clicked() {
                                    clicked = Some(2);
                                }
                            }
                            None => {
                                ui.add_visible(false, egui::Button::new(tr("Default")));
                            }
                        }
                        if ui.button(tr("Change…")).clicked() {
                            clicked = Some(1);
                        }
                        if ui.button(tr("Open")).clicked() {
                            clicked = Some(0);
                        }
                        ui.add_space(8.0);
                        let mut text = short_path(path);
                        let color = if missing {
                            text = trf("{path} (not found; using the default)", &[("path", &text)]);
                            l.p.warning
                        } else {
                            l.p.text_muted
                        };
                        let full = path.map_or_else(String::new, |p| p.display().to_string());
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(text).size(l.f.base).color(color)).truncate()).on_hover_text(full);
                        });
                    });
                });
                clicked
            };
            // a scroll bar beside the cards, not over them (small windows)
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(ui.available_height() - l.btn_h() - 24.0).show(ui, |ui| {
                for (i, (title, folders, hint)) in groups.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(8.0);
                    }
                    l.card(ui, Some(title), |ui| {
                        if i == 0 {
                            match row(ui, &tr("ROMs"), Some(&rom_dir), false, None) {
                                Some(0) => action = Some(Action::OpenRoms),
                                Some(1) => action = Some(Action::ChangeRoms),
                                _ => {}
                            }
                        }
                        for &(f, ref dir, custom, missing) in folders {
                            match row(ui, &tr(f.label()), dir.as_deref(), missing, Some(custom)) {
                                Some(0) => action = Some(Action::Open(f)),
                                Some(1) => action = Some(Action::Change(f)),
                                Some(2) => action = Some(Action::Default(f)),
                                _ => {}
                            }
                        }
                        if let Some(h) = hint {
                            ui.add(egui::Label::new(l.hint_rt(h)).wrap());
                        }
                    });
                }
            });
            let mut c = false;
            l.footer(
                ui,
                |ui| {
                    ui.checkbox(&mut move_files, tr("When changing a folder, move its files along"));
                },
                |ui| c = l.primary(ui, &tr("Close")).clicked(),
            );
            c
        });
        let close = r.inner || r.should_close();
        self.dialog = if close { Dialog::None } else { Dialog::Folders { move_files } };
        match action {
            Some(Action::OpenRoms) => crate::platform::open_folder(&self.rom_dir),
            Some(Action::ChangeRoms) => {
                if let Some(d) = rfd::FileDialog::new().set_title(tr("Change folder…")).set_directory(&self.rom_dir).pick_folder() {
                    self.set_rom_dir(d);
                }
            }
            Some(Action::Open(f)) => open_folder(f.dir()),
            Some(Action::Change(f)) => {
                let mut d = rfd::FileDialog::new().set_title(tr(f.label()));
                if let Some(cur) = f.dir().filter(|d| d.is_dir()) {
                    d = d.set_directory(cur);
                }
                if let Some(new) = d.pick_folder() {
                    self.change_folder(f, Some(new), move_files);
                }
            }
            Some(Action::Default(f)) => self.change_folder(f, None, move_files),
            None => {}
        }
    }

    /// Changes a folder (`None` = back to the default), bringing its files along if asked.
    fn change_folder(&mut self, f: Folder, new: Option<PathBuf>, move_files: bool) {
        let (Some(old), Some(target)) = (f.dir(), new.clone().or_else(|| f.default_dir())) else { return };
        if old != target && move_files && old.is_dir() {
            if let Err(e) = crate::config::move_tree(&old, &target) {
                self.notify(trf("Could not move the files: {error}", &[("error", &e)]));
                return;
            }
            if old.is_dir() {
                self.notify(tr("Files with the same name were already in the new folder and stayed in the old one"));
            }
        }
        match new {
            Some(p) => self.cfg.dirs.insert(f.key().to_string(), p),
            None => self.cfg.dirs.remove(f.key()),
        };
        self.cfg.apply_dirs();
        self.cfg.save();
        self.bios_found = None;
        if f == Folder::Languages {
            for e in crate::i18n::load_user(&target) {
                self.notify(e);
            }
        }
    }
}

/// A path for display: the home folder as `~`.
fn short_path(p: Option<&Path>) -> String {
    let Some(p) = p else { return "—".into() };
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    match home.as_deref().and_then(|h| p.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => p.display().to_string(),
    }
}
