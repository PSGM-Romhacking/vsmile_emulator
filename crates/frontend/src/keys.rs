//! Keys for each action (global, independent of the controller).

use std::collections::BTreeMap;

use egui::Key;

use emu_api::{PadButton, SystemInfo};

use crate::gamepad::Func;

pub type KeyMap = BTreeMap<Func, Vec<Key>>;

/// Default keys for each RetroPad button, per player.
fn pad_key(player: u8, b: PadButton) -> Option<Key> {
    use Key::*;
    use PadButton as P;
    Some(match (player, b) {
        (0, P::Up) => ArrowUp,
        (0, P::Down) => ArrowDown,
        (0, P::Left) => ArrowLeft,
        (0, P::Right) => ArrowRight,
        (0, P::B) => Z,
        (0, P::A) => X,
        (0, P::Y) => C,
        (0, P::X) => V,
        (0, P::Select) => Tab,
        (0, P::Start) => Enter,
        (1, P::Up) => W,
        (1, P::Down) => S,
        (1, P::Left) => A,
        (1, P::Right) => D,
        (1, P::B) => F,
        (1, P::A) => G,
        (1, P::Y) => H,
        (1, P::X) => J,
        (1, P::Select) => Q,
        (1, P::Start) => E,
        _ => return None,
    })
}

pub fn default_keys(sys: &SystemInfo) -> KeyMap {
    use Key::*;
    let mut m: KeyMap = Func::all(sys)
        .into_iter()
        .filter_map(|f| match f {
            Func::Pad(p, b) => pad_key(p, b).map(|k| (f, vec![k])),
            _ => None,
        })
        .collect();
    m.extend([
        (Func::Pause, vec![P]),
        (Func::Reset, vec![F1]),
        (Func::RefreshGames, vec![F5]),
        (Func::SaveState, vec![F2]),
        (Func::LoadState, vec![F4]),
        (Func::SlotPrev, vec![F6]),
        (Func::SlotNext, vec![F7]),
        (Func::FastForward, vec![Space]),
        (Func::Rewind, vec![Backspace]),
        (Func::Mute, vec![M]),
        (Func::VolumeUp, vec![Equals, Plus]),
        (Func::VolumeDown, vec![Minus]),
        (Func::Fullscreen, vec![F11]),
        (Func::Screenshot, vec![F12]),
        (Func::RecordGif, vec![F9]),
        (Func::RecordVideo, vec![F10]),
    ]);
    m
}

/// Saved ("p1_up" → ["ArrowUp"]) → [`KeyMap`]. Absent from the config = default.
pub fn from_cfg(saved: &BTreeMap<String, Vec<String>>, sys: &SystemInfo) -> KeyMap {
    if saved.is_empty() {
        return default_keys(sys);
    }
    // an action missing from the config (added in a newer version) gets its default key;
    // one the user cleared is saved empty and stays empty
    let mut m = default_keys(sys);
    for f in Func::all(sys) {
        if let Some(v) = saved.get(&f.key()) {
            m.insert(f, v.iter().filter_map(|n| Key::from_name(n)).collect());
        }
    }
    m
}

pub fn to_cfg(m: &KeyMap, sys: &SystemInfo) -> BTreeMap<String, Vec<String>> {
    // saves every action, including empty ones, so "no key" doesn't revert to the default
    Func::all(sys).into_iter().map(|f| (f.key(), m.get(&f).map(|v| v.iter().map(|k| k.name().to_string()).collect()).unwrap_or_default())).collect()
}

/// Short text for the UI.
pub fn label(k: Key) -> String {
    match k {
        Key::ArrowUp => "↑".into(),
        Key::ArrowDown => "↓".into(),
        Key::ArrowLeft => "←".into(),
        Key::ArrowRight => "→".into(),
        Key::Space => crate::i18n::tr("Space"),
        Key::Escape => "Esc".into(),
        Key::Minus => "−".into(),
        _ => k.symbol_or_name().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu_api::{Button, Core};

    static SYS: SystemInfo = SystemInfo {
        name: "Test",
        short_name: "T",
        extensions: &["bin"],
        thumbnails: "",
        cover_aspect: 0.66,
        players: 2,
        buttons: &[Button { id: PadButton::Up, label: "Up" }, Button { id: PadButton::B, label: "Fire 1" }, Button { id: PadButton::A, label: "Fire 2" }],
        width: 1,
        height: 1,
        aspect: 1.0,
        ram_address: 0,
        fps_num: 60,
        fps_den: 1,
        game_info: |_| None,
        game_texts: &[],
        bios: &[],
        multi_file_roms: false,
        create: |_| -> Box<dyn Core> { unreachable!() },
    };

    #[test]
    fn default_only_has_system_buttons() {
        let m = default_keys(&SYS);
        assert_eq!(m.get(&Func::Pad(0, PadButton::B)), Some(&vec![Key::Z]));
        assert_eq!(m.get(&Func::Pad(1, PadButton::Up)), Some(&vec![Key::W]));
        assert!(!m.contains_key(&Func::Pad(0, PadButton::Select))); // the system doesn't use it
    }

    #[test]
    fn config_round_trip() {
        let mut m = default_keys(&SYS);
        m.remove(&Func::Mute); // an action with no key stays with no key
        let back = from_cfg(&to_cfg(&m, &SYS), &SYS);
        assert_eq!(back.get(&Func::Pad(0, PadButton::Up)), Some(&vec![Key::ArrowUp]));
        assert_eq!(back.get(&Func::VolumeUp), Some(&vec![Key::Equals, Key::Plus]));
        assert!(back.get(&Func::Mute).is_none_or(|v| v.is_empty()));
    }
}
