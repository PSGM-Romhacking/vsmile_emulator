//! Keyboard and controller: shortcuts, the actions that fire on press, and what each
//! key or button is mapped to.

#[allow(unused_imports)]
use super::*;

impl App {

    /// Fixed shortcuts (Ctrl+O, Ctrl+Q, Esc) and the remappable actions that fire
    /// on press (the "held" ones — game buttons, fast forward, rewind — are read
    /// in `emulate`).
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        let k = |m: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(m, key));
        if k(Modifiers::COMMAND, Key::O) {
            self.open_dialog();
        }
        if k(Modifiers::COMMAND, Key::Q) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        if k(Modifiers::COMMAND, Key::W) && self.dialog == Dialog::None {
            self.close_game();
        }
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if fullscreen && !self.couch && self.dialog == Dialog::None && k(Modifiers::NONE, Key::Escape) {
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        }
        let focused = ctx.input(|i| i.focused);
        let devs = self.pads.snapshot();
        let pad_now: HashSet<Func> = if focused { Func::all(self.sys).into_iter().filter(|&f| self.pad_level(&devs, f)).collect() } else { HashSet::new() };
        if self.dialog == Dialog::None {
            for f in Func::all(self.sys) {
                // the quick menu is handled in nav.rs (with the Select + Start combination)
                let pad_edge = f != Func::QuickMenu && pad_now.contains(&f) && !self.pad_prev.contains(&f);
                if !f.is_hold() && f != Func::QuickMenu && (self.kb_edge(f) || pad_edge) {
                    self.perform(ctx, f);
                }
            }
        }
        self.pad_prev = pad_now;
    }

    /// Performs a one-shot action.
    pub(super) fn perform(&mut self, ctx: &egui::Context, f: Func) {
        match f {
            Func::Pause if self.loaded => self.paused = !self.paused,
            Func::Reset if self.loaded => {
                self.reset_game();
                self.notify(tr("Game reset"));
            }
            Func::SaveState => self.save_state(),
            Func::LoadState => self.load_state(),
            Func::SlotPrev => self.set_slot(if self.slot <= 1 { 9 } else { self.slot - 1 }),
            Func::SlotNext => self.set_slot(if self.slot >= 9 { 1 } else { self.slot + 1 }),
            Func::FastForwardToggle if self.loaded => {
                self.ff_toggle = !self.ff_toggle;
                self.notify(tr(if self.ff_toggle { "Fast forward on" } else { "Fast forward off" }));
            }
            Func::Mute => {
                self.set_mute(!self.muted);
                self.notify(tr(if self.muted { "Sound off" } else { "Sound on" }));
            }
            Func::VolumeUp | Func::VolumeDown => {
                self.volume = if f == Func::VolumeUp { (self.volume + 10).min(100) } else { self.volume.saturating_sub(10) };
                let extra = if self.muted { tr(" (sound off)") } else { String::new() };
                self.notify(format!("{}{extra}", trf("Volume {n}%", &[("n", &self.volume)])));
            }
            Func::Screenshot => self.screenshot(),
            Func::RecordGif => self.toggle_record(crate::recorder::Kind::Gif),
            Func::RecordVideo => self.toggle_record(crate::recorder::Kind::Mp4),
            Func::OpenCaptures => open_folder(crate::config::pictures_dir()),
            Func::OpenRecordings => open_folder(crate::config::videos_dir()),
            Func::RefreshGames => self.refresh_games(),
            Func::Fullscreen => {
                let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!full));
            }
            _ => {}
        }
    }

    pub(super) fn kb_level(&self, f: Func) -> bool {
        self.keymap.get(&f).is_some_and(|v| v.iter().any(|k| self.keys_down.contains(k)))
    }

    pub(super) fn kb_edge(&self, f: Func) -> bool {
        self.keymap.get(&f).is_some_and(|v| v.iter().any(|k| self.key_edges.contains(k)))
    }

    pub(super) fn pad_level(&self, devs: &[PadInfo], f: Func) -> bool {
        devs.iter().any(|d| gamepad::active(&self.mapping_for(d), d, f))
    }

    /// RetroPad buttons from the keyboard, one per player.
    pub(super) fn kb_bits(&self) -> Vec<u16> {
        let mut out = vec![0u16; self.sys.players];
        for f in Func::all(self.sys) {
            if let Func::Pad(n, b) = f
                && self.kb_level(f)
                    && let Some(v) = out.get_mut(n as usize) {
                        *v |= b.bit();
                    }
        }
        out
    }

    /// Key of the action, to show in menus ("" if none).
    pub(super) fn sc(&self, f: Func) -> String {
        self.keymap.get(&f).and_then(|v| v.first()).map(|&k| keys::label(k)).unwrap_or_default()
    }

    pub(super) fn key_text(&self, f: Func) -> String {
        self.keymap.get(&f).filter(|v| !v.is_empty()).map(|v| v.iter().map(|&k| keys::label(k)).collect::<Vec<_>>().join(" / ")).unwrap_or_default()
    }

    /// Controller mapping: the one saved for this name, or the default.
    pub(super) fn mapping_for(&self, d: &PadInfo) -> Mapping {
        self.cfg.pads.get(&d.name).map(|m| gamepad::mapping_from_cfg(m, self.sys)).unwrap_or_else(|| gamepad::default_mapping(d, self.sys))
    }
}
