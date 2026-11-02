//! Emulation › Controls and shortcuts…: the mapping dialog.

#[allow(unused_imports)]
use super::*;

impl App {
    /// Emulation › Controls and shortcuts…: each action has a key (global)
    /// and an input of the selected controller. Click a field and press; the right
    /// mouse button clears.
    pub(super) fn gamepad_dialog(&mut self, ctx: &egui::Context) {
        let l = self.look;
        let Dialog::Gamepad { mut dev, mut capture } = self.dialog else { return };
        let devs = self.pads.snapshot();
        let presses = self.pads.take_presses();
        if dev >= devs.len() {
            dev = 0;
            if matches!(capture, Some((_, Col::Pad))) {
                capture = None;
            }
        }
        let cur = devs.get(dev);
        let mut map = cur.map(|d| self.mapping_for(d)).unwrap_or_default();
        let mut keymap = self.keymap.clone();
        let (mut kb_changed, mut pad_changed) = (false, false);
        let was_capturing = capture.is_some();

        // receive the captured press (Esc cancels)
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        match capture {
            Some((f, Col::Key)) => {
                if let Some(k) = self.kb_captured.take() {
                    if k != Key::Escape {
                        for v in keymap.values_mut() {
                            v.retain(|&x| x != k); // one key, one action
                        }
                        keymap.insert(f, vec![k]);
                        kb_changed = true;
                    }
                    capture = None;
                }
            }
            Some((f, Col::Pad)) => {
                if esc {
                    capture = None;
                } else if let Some((_, inp)) = cur.and_then(|d| presses.iter().find(|(n, _)| *n == d.name)) {
                    for v in map.values_mut() {
                        v.retain(|&x| x != *inp);
                    }
                    map.insert(f, vec![*inp]);
                    pad_changed = true;
                    capture = None;
                }
            }
            None => {}
        }

        let (mut close, mut reset_kb, mut reset_pad) = (false, false, false);
        let (mut define, mut clear) = (None::<(Func, Col)>, None::<(Func, Col)>);
        const W_ACT: f32 = 230.0;
        const W_KEY: f32 = 170.0;
        const W_PAD: f32 = 210.0;
        let r = l.dialog(
            ctx,
            "controls_cfg",
            Vec2::new(720.0, 680.0),
            &tr("Controls and shortcuts"),
            &tr("Click a field and press the key or controller button (Esc cancels). Right-click clears."),
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(l.strong(tr("Controller")));
                    match cur {
                        None => {
                            if self.pads.no_access.load(std::sync::atomic::Ordering::Relaxed) {
                                l.hint(ui, &tr("none found — no permission to read /dev/input (see the README, FreeBSD)"));
                            } else {
                                l.hint(ui, &tr("none connected — plug one in and wait a few seconds"));
                            }
                        }
                        Some(d) => {
                            let name = |i: usize, d: &PadInfo| format!("{} — {}", trf("Player {n}", &[("n", &(i + 1))]), d.name);
                            egui::ComboBox::from_id_salt("pad_dev").width(440.0).selected_text(name(dev, d)).show_ui(ui, |ui| {
                                for (i, d) in devs.iter().enumerate() {
                                    ui.selectable_value(&mut dev, i, name(i, d));
                                }
                            });
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    for (t, w) in [("ACTION", W_ACT), ("KEYBOARD", W_KEY), ("CONTROLLER", W_PAD)] {
                        ui.add_sized([w, 18.0], egui::Label::new(RichText::new(tr(t)).font(theme::bold(l.f.sm)).color(l.p.text_section).extra_letter_spacing(1.0)).halign(egui::Align::Min));
                    }
                });
                l.separator(ui);
                egui::ScrollArea::vertical().max_height(ui.available_height() - 56.0).auto_shrink([false, false]).show(ui, |ui| {
                    let groups = (1..=self.sys.players as u8).map(|n| (trf("PLAYER {n}", &[("n", &n)]), Some(n))).chain([(tr("EMULATOR"), None)]);
                    for (title, grp) in groups {
                        ui.add_space(8.0);
                        l.section(ui, &title);
                        ui.add_space(2.0);
                        for f in Func::all(self.sys).into_iter().filter(|f| f.player() == grp) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                // action name; the small bar lights up while it is active
                                let on = keymap.get(&f).is_some_and(|v| v.iter().any(|k| self.keys_down.contains(k))) || cur.is_some_and(|d| gamepad::active(&map, d, f));
                                let (rect, _) = ui.allocate_exact_size(Vec2::new(W_ACT, 28.0), Sense::hover());
                                if on {
                                    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min + Vec2::new(0.0, 6.0), Vec2::new(3.0, 16.0)), 0.0, l.p.accent_hover);
                                }
                                ui.painter().text(rect.left_center() + Vec2::new(12.0, 0.0), egui::Align2::LEFT_CENTER, tr(f.label(self.sys)), egui::FontId::proportional(l.f.md), l.p.text_main);

                                let kb_txt = keymap.get(&f).filter(|v| !v.is_empty()).map(|v| v.iter().map(|&k| keys::label(k)).collect::<Vec<_>>().join(" / "));
                                let r = chip(ui, &l, W_KEY, kb_txt.as_deref(), capture == Some((f, Col::Key)), &tr("Press a key…"), true);
                                if r.clicked() {
                                    define = Some((f, Col::Key));
                                } else if r.secondary_clicked() {
                                    clear = Some((f, Col::Key));
                                }

                                let pad_txt = map.get(&f).filter(|v| !v.is_empty()).map(|v| v.iter().map(|i| i.label()).collect::<Vec<_>>().join(" / "));
                                let r = chip(ui, &l, W_PAD, pad_txt.as_deref(), capture == Some((f, Col::Pad)), &tr("Press a button…"), cur.is_some());
                                if r.clicked() {
                                    define = Some((f, Col::Pad));
                                } else if r.secondary_clicked() {
                                    clear = Some((f, Col::Pad));
                                }
                            });
                        }
                    }
                });
                l.footer(
                    ui,
                    |ui| {
                        if ui.button(tr("Reset keyboard")).clicked() {
                            reset_kb = true;
                        }
                        if cur.is_some() && ui.button(tr("Reset controller")).clicked() {
                            reset_pad = true;
                        }
                    },
                    |ui| close = l.primary(ui, &tr("Close")).clicked(),
                );
            },
        );

        if let Some(dc) = define {
            capture = Some(dc);
            self.pads.take_presses(); // only presses after the click count
            self.kb_captured = None;
        }
        if let Some((f, col)) = clear {
            match col {
                Col::Key => {
                    keymap.insert(f, vec![]);
                    kb_changed = true;
                }
                Col::Pad => {
                    map.insert(f, vec![]);
                    pad_changed = true;
                }
            }
        }
        if reset_kb {
            self.keymap = keys::default_keys(self.sys);
            self.cfg.keys.clear();
            self.cfg.save();
        } else if kb_changed {
            self.cfg.keys = keys::to_cfg(&keymap, self.sys);
            self.keymap = keymap;
            self.cfg.save();
        }
        if let Some(d) = cur {
            if reset_pad {
                self.cfg.pads.remove(&d.name);
                self.cfg.save();
            } else if pad_changed {
                self.cfg.pads.insert(d.name.clone(), gamepad::mapping_to_cfg(&map));
                self.cfg.save();
            }
        }
        // Esc during capture only cancels the capture, it doesn't close the window
        let closing = close || (r.should_close() && !was_capturing && capture.is_none());
        self.kb_capture_on = !closing && matches!(capture, Some((_, Col::Key)));
        self.dialog = if closing { Dialog::None } else { Dialog::Gamepad { dev, capture } };
    }
}

/// Clickable field of the controls configuration: shows the current key/button,
/// "—" if empty, or the press prompt while capturing.
pub(super) fn chip(ui: &mut egui::Ui, l: &Look, w: f32, text: Option<&str>, capturing: bool, prompt: &str, enabled: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), sense);
    let hovered = enabled && resp.hovered();
    let (fill, stroke) = if capturing {
        (l.p.sel_bg, Stroke::new(1.0, l.p.accent_hover))
    } else if hovered {
        (l.p.hover_bg, Stroke::new(1.0, l.p.text_muted))
    } else {
        (l.p.bg_main, Stroke::new(1.0, l.p.border))
    };
    let p = ui.painter();
    p.rect(rect, 0.0, fill, stroke, egui::StrokeKind::Inside);
    let (txt, color) = match (capturing, text) {
        (true, _) => (prompt, l.p.text_main),
        (false, Some(t)) if enabled => (t, l.p.text_main),
        _ => ("—", l.p.text_muted),
    };
    let clip = rect.shrink2(Vec2::new(8.0, 0.0));
    p.with_clip_rect(clip).text(clip.left_center(), egui::Align2::LEFT_CENTER, txt, egui::FontId::proportional(l.f.base), color);
    if enabled {
        resp.on_hover_text(tr("Click to set · right-click clears"))
    } else {
        resp
    }
}
