//! Using the emulator from the couch: the library and a quick menu driven by the
//! controller (or the arrow keys), and the couch mode (fullscreen, no menu bar, bigger
//! covers, a line of button hints).
//!
//! Buttons, by position (Xbox names in the hints, the most familiar ones): the bottom
//! button (A on Xbox, RetroPad B) confirms, the right one (B on Xbox) goes back, Select
//! (tapped alone) changes the library image. The quick menu — the controller's "Esc" —
//! opens and closes with its own action (Home/Guide by default, remappable in Controls and
//! shortcuts) and with Select + Start held together for half a second (optional, on by
//! default), anywhere (library or game, couch mode or not); Esc also opens it in couch
//! mode. Start alone stays a game button, so it is never used for navigation.

use std::collections::{HashMap, HashSet};

use egui::{Color32, Key, RichText, Stroke, Vec2};
use emu_api::PadButton;

use super::{App, Dialog};
use crate::gamepad::{self, Func};
use crate::i18n::tr;
use crate::theme::bold;

/// A held direction repeats after this long, then every `REPEAT_EVERY` seconds.
const REPEAT_AFTER: f64 = 0.4;
const REPEAT_EVERY: f64 = 0.11;
/// How long Select + Start must be held together to open the quick menu.
const COMBO_HOLD: f64 = 0.5;
/// While playing, the button hints show only this long (after opening a game or closing
/// the menu), so they don't cover the picture.
const HINT_SECONDS: f64 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum NavKey {
    Up,
    Down,
    Left,
    Right,
    Ok,
    Back,
    Alt,
    Menu,
}

/// Quick menu entries.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    Continue,
    SaveState,
    LoadState,
    Reset,
    Library,
    ImageType,
    CouchMode,
    Quit,
}

impl Entry {
    fn label(self, couch: bool) -> &'static str {
        match self {
            Entry::Continue => "Continue",
            Entry::SaveState => "Save state",
            Entry::LoadState => "Load state",
            Entry::Reset => "Reset",
            Entry::Library => "Back to the library",
            Entry::ImageType => "Library image",
            Entry::CouchMode if couch => "Leave couch mode",
            Entry::CouchMode => "Couch mode",
            Entry::Quit => "Quit",
        }
    }
}

#[derive(Default)]
pub(super) struct Nav {
    /// Selected card in the library (index in the filtered list).
    pub sel: usize,
    /// The selection is shown (hidden again when the mouse moves).
    pub active: bool,
    /// The selection moved this pass: scroll the library to it.
    pub moved: bool,
    /// Keys pressed this pass (with repeat for the directions).
    pressed: HashSet<NavKey>,
    held_since: HashMap<NavKey, f64>,
    next_repeat: HashMap<NavKey, f64>,
    /// Quick menu: open, and the selected entry.
    pub quick: Option<usize>,
    combo_since: Option<f64>,
    /// The combo fired: it fires again only after Select and Start are released.
    combo_fired: bool,
    /// Select and Start are both held (their bits are kept from the game meanwhile).
    pub combo_held: bool,
    /// Select is held and Start hasn't joined it: on release it changes the image.
    select_solo: Option<bool>,
    /// After a controller action (opening a game, closing the menu), game input is
    /// ignored until every button is released, so the press doesn't reach the game.
    pub block_input: bool,
    /// While playing, show the button hints until this time.
    pub hint_until: f64,
}

impl Nav {
    pub(super) fn pressed(&self, k: NavKey) -> bool {
        self.pressed.contains(&k)
    }
}

impl App {
    /// Reads the controllers and the keyboard for navigation; called once per pass,
    /// before the library or the game are drawn.
    pub(super) fn nav_update(&mut self, ctx: &egui::Context) {
        let now = self.now();
        let focused = ctx.input(|i| i.focused);
        let devs = self.pads.snapshot();
        let typing = ctx.egui_wants_keyboard_input();
        let pad = |f: Func| focused && devs.iter().any(|d| gamepad::active(&self.mapping_for(d), d, f));
        let key = |k: Key| focused && !typing && ctx.input(|i| i.key_down(k));
        let p1 = |b: PadButton| pad(Func::Pad(0, b));
        // the arrow keys and Enter move the selection only where they aren't game input
        let browsing = !self.loaded || self.nav.quick.is_some();
        let mut down: HashSet<NavKey> = HashSet::new();
        for (k, b, kb) in [
            (NavKey::Up, PadButton::Up, Key::ArrowUp),
            (NavKey::Down, PadButton::Down, Key::ArrowDown),
            (NavKey::Left, PadButton::Left, Key::ArrowLeft),
            (NavKey::Right, PadButton::Right, Key::ArrowRight),
        ] {
            if browsing && (p1(b) || key(kb)) {
                down.insert(k);
            }
        }
        if browsing && (p1(PadButton::B) || key(Key::Enter)) {
            down.insert(NavKey::Ok);
        }
        if browsing && (p1(PadButton::A) || (self.nav.quick.is_some() && key(Key::Escape))) {
            down.insert(NavKey::Back);
        }
        // the Quick menu action (Home/Guide by default, remappable, also on the keyboard);
        // Esc only in couch mode, where it doesn't leave fullscreen
        let kb_menu = focused && !typing && self.kb_level(Func::QuickMenu);
        if pad(Func::QuickMenu) || kb_menu || (self.couch && self.nav.quick.is_none() && key(Key::Escape)) {
            down.insert(NavKey::Menu);
        }
        let (select, start) = (p1(PadButton::Select), p1(PadButton::Start));

        // press edges, and repeats for held directions
        self.nav.pressed.clear();
        for k in &down {
            let first = !self.nav.held_since.contains_key(k);
            if first {
                self.nav.held_since.insert(*k, now);
                self.nav.next_repeat.insert(*k, now + REPEAT_AFTER);
                self.nav.pressed.insert(*k);
            } else if matches!(k, NavKey::Up | NavKey::Down | NavKey::Left | NavKey::Right) && now >= self.nav.next_repeat[k] {
                self.nav.next_repeat.insert(*k, now + REPEAT_EVERY);
                self.nav.pressed.insert(*k);
            }
        }
        self.nav.held_since.retain(|k, _| down.contains(k));
        if !self.nav.pressed.is_empty() {
            self.nav.active = true;
        }
        if ctx.input(|i| i.pointer.delta() != Vec2::ZERO) {
            self.nav.active = false;
        }

        // Select alone, on release: change the library image (Start joining in cancels it,
        // so opening the menu doesn't also change the image)
        self.nav.select_solo = match (select, self.nav.select_solo) {
            (true, None) => Some(!start),
            (true, Some(solo)) => Some(solo && !start),
            (false, Some(true)) => {
                if browsing && self.nav.quick.is_none() {
                    self.nav.pressed.insert(NavKey::Alt);
                    self.nav.active = true;
                }
                None
            }
            (false, _) => None,
        };

        // Select + Start held for half a second, anywhere: open or close the quick menu,
        // once per hold
        self.nav.combo_held = self.menu_combo && select && start;
        if !(select || start) {
            self.nav.combo_fired = false;
        }
        match (self.nav.combo_held && !self.nav.combo_fired, self.nav.combo_since) {
            (true, None) => self.nav.combo_since = Some(now),
            (true, Some(t)) if now - t >= COMBO_HOLD => {
                self.nav.combo_since = None;
                self.nav.combo_fired = true;
                self.nav.pressed.insert(NavKey::Menu);
            }
            (false, _) => self.nav.combo_since = None,
            _ => {}
        }

        if self.nav.pressed(NavKey::Menu) && self.dialog == Dialog::None {
            self.nav.quick = if self.nav.quick.is_some() { None } else { Some(0) };
            self.nav.block_input = true;
        }
    }

    /// The quick menu's entries in the current situation.
    fn quick_entries(&self) -> Vec<Entry> {
        if self.loaded {
            vec![Entry::Continue, Entry::SaveState, Entry::LoadState, Entry::Reset, Entry::Library, Entry::CouchMode, Entry::Quit]
        } else {
            vec![Entry::Continue, Entry::ImageType, Entry::CouchMode, Entry::Quit]
        }
    }

    /// Draws the quick menu (if open) and runs the chosen entry.
    pub(super) fn quick_menu(&mut self, ctx: &egui::Context) {
        let Some(mut sel) = self.nav.quick else { return };
        let entries = self.quick_entries();
        sel = sel.min(entries.len() - 1);
        if self.nav.pressed(NavKey::Up) {
            sel = if sel == 0 { entries.len() - 1 } else { sel - 1 };
        }
        if self.nav.pressed(NavKey::Down) {
            sel = (sel + 1) % entries.len();
        }
        let mut chosen = self.nav.pressed(NavKey::Ok).then_some(entries[sel]);
        let mut close = self.nav.pressed(NavKey::Back);
        let l = self.look;
        let k = if self.couch { 1.5 } else { 1.0 };
        egui::Area::new(egui::Id::unique("quick_menu")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            egui::Frame::new().fill(l.p.bg_sidebar).stroke(Stroke::new(1.0, l.p.border)).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
                ui.set_width(220.0 * k);
                ui.label(RichText::new(tr("Menu")).font(bold(l.f.lg * k)).color(l.p.text_section));
                ui.add_space(6.0);
                for (i, e) in entries.iter().enumerate() {
                    let text = RichText::new(tr(e.label(self.couch))).size(l.f.md * k).color(l.p.text_main);
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0 * k), egui::Sense::click());
                    if i == sel {
                        ui.painter().rect_filled(rect, 0.0, l.p.sel_bg);
                        ui.painter().rect_stroke(rect, 0.0, Stroke::new(2.0, l.p.border_focus), egui::StrokeKind::Inside);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 0.0, l.p.hover_bg);
                    }
                    let galley = ui.painter().layout_no_wrap(text.text().to_string(), egui::FontId::proportional(l.f.md * k), l.p.text_main);
                    ui.painter().galley(rect.left_center() + Vec2::new(12.0, -galley.size().y / 2.0), galley, l.p.text_main);
                    if resp.hovered() && ctx.input(|i| i.pointer.delta() != Vec2::ZERO) {
                        sel = i;
                    }
                    if resp.clicked() {
                        chosen = Some(*e);
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new(tr("A: choose · B or Home: back")).size(l.f.sm * k).color(l.p.text_muted));
                if self.menu_combo {
                    ui.label(RichText::new(tr("Select + Start (hold): close")).size(l.f.sm * k).color(l.p.text_muted));
                }
            });
        });
        self.nav.quick = Some(sel);
        if let Some(e) = chosen {
            close = true;
            match e {
                Entry::Continue => {}
                Entry::SaveState => self.save_state(),
                Entry::LoadState => self.load_state(),
                Entry::Reset => {
                    self.reset_game();
                    self.notify(tr("Game reset"));
                }
                Entry::Library => self.close_game(),
                Entry::ImageType => self.next_library_art(),
                Entry::CouchMode => self.set_couch(ctx, !self.couch),
                Entry::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
        if close {
            self.nav.quick = None;
            self.nav.block_input = true;
            self.nav.hint_until = self.now() + HINT_SECONDS;
        }
    }

    /// Couch mode on or off: fullscreen and no menu bar, or back to the window.
    pub(super) fn set_couch(&mut self, ctx: &egui::Context, on: bool) {
        self.couch = on;
        self.nav.hint_until = self.now() + HINT_SECONDS;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
        let msg = if !on { "Couch mode off" } else if self.menu_combo { "Couch mode: Home or Select + Start (hold) opens the menu" } else { "Couch mode: Home opens the menu" };
        self.notify(tr(msg));
    }

    /// Next view of the library: box art › title screen › gameplay › list › box art.
    pub(super) fn next_library_art(&mut self) {
        let all = crate::covers::Art::ALL;
        if self.library_list {
            self.library_list = false;
            self.library_art = all[0];
        } else {
            let i = all.iter().position(|&a| a == self.library_art).unwrap_or(0);
            if i + 1 == all.len() {
                self.library_list = true;
            } else {
                self.library_art = all[i + 1];
            }
        }
        self.nav.moved = true;
    }

    /// The line of button hints at the bottom of the screen: while Select + Start are held
    /// (any mode), and in couch mode (while playing, only for a moment).
    pub(super) fn couch_hints(&self, ctx: &egui::Context) {
        let text = if self.nav.combo_since.is_some() {
            tr("Keep holding Select + Start…")
        } else if !self.couch || self.nav.quick.is_some() || (self.loaded && self.now() > self.nav.hint_until) {
            return;
        } else if self.loaded {
            tr(if self.menu_combo { "Home or Select + Start (hold): menu" } else { "Home: menu" })
        } else if self.menu_combo {
            tr("D-pad: choose · A: play · Select: image · Home or Select + Start (hold): menu")
        } else {
            tr("D-pad: choose · A: play · Select: image · Home: menu")
        };
        let l = self.look;
        egui::Area::new(egui::Id::unique("couch_hints")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -10.0)).interactable(false).show(ctx, |ui| {
            egui::Frame::new().fill(Color32::from_black_alpha(170)).inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
                // one line, never wrapped in the middle of a hint
                ui.add(egui::Label::new(RichText::new(text).size(l.f.md * 1.2).color(l.p.text_main)).wrap_mode(egui::TextWrapMode::Extend));
            });
        });
    }

    /// A game was opened: show the hints for a moment.
    pub(super) fn nav_game_opened(&mut self) {
        self.nav.hint_until = self.now() + HINT_SECONDS;
    }

    /// Game input after a controller action: nothing until every button is released.
    pub(super) fn filter_input(&mut self, input: &mut [u16]) {
        if self.nav.combo_held
            && let Some(p) = input.first_mut()
        {
            *p &= !(PadButton::Select.bit() | PadButton::Start.bit());
        }
        if self.nav.block_input {
            if input.iter().all(|&b| b == 0) {
                self.nav.block_input = false;
            } else {
                input.iter_mut().for_each(|b| *b = 0);
            }
        }
    }
}
