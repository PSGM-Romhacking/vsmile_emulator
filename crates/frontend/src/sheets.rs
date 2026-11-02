//! Game sheets: the system's game database (`SystemInfo::game_info`: title, year,
//! publisher, players…) plus each game's texts in the current language
//! (`SystemInfo::game_texts`). The emulator's own texts (menus, labels) are not here:
//! they are the i18n catalogs. Format: `docs/sheets.md`.

use std::collections::BTreeMap;

use emu_api::SystemInfo;
use serde::Deserialize;

use crate::i18n::{tr, trf};

/// A game's texts in one language.
#[derive(Deserialize, Default, Clone)]
#[serde(default, deny_unknown_fields)]
struct Texts {
    /// "Fighting", "Role-playing"…: a word or two, in this language.
    genre: Option<String>,
    description: Option<String>,
    notes: Option<String>,
}

/// Sheet ready to display, already in the current language.
#[derive(Clone, PartialEq, Default)]
pub struct Sheet {
    pub title: String,
    pub alt_title: Option<String>,
    /// Game name in libretro-thumbnails ("Pooyan (Japan)"); without it, there is no cover to download.
    pub thumbnail: Option<String>,
    pub year: Option<String>,
    pub publisher: Option<String>,
    pub developer: Option<String>,
    pub serial: Option<String>,
    /// In the current language ("Fighting").
    pub genre: Option<String>,
    /// "1 or 2 players", in the current language.
    pub players: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    /// The game's key in the database ("pooyan").
    pub key: String,
}

pub struct Sheets {
    sys: &'static SystemInfo,
    /// language → game → texts
    texts: BTreeMap<String, BTreeMap<String, Texts>>,
}

/// The number of players as a phrase: the number is data, the words are the language's.
fn players_text(n: u8) -> String {
    match n {
        0 | 1 => tr("1 player"),
        2 => trf("1 or {n} players", &[("n", &n)]),
        _ => trf("1 to {n} players", &[("n", &n)]),
    }
}

impl Sheets {
    pub fn load(sys: &'static SystemInfo) -> Self {
        let texts = sys
            .game_texts
            .iter()
            .map(|&(code, json)| {
                let m = serde_json::from_str(json).unwrap_or_else(|e| {
                    eprintln!("invalid game texts ({code}): {e}");
                    BTreeMap::new()
                });
                (code.to_string(), m)
            })
            .collect();
        Self { sys, texts }
    }

    /// The game's texts in the current language; failing that, in English.
    fn texts(&self, key: &str) -> Option<&Texts> {
        [crate::i18n::lang(), "en"].iter().find_map(|l| self.texts.get(*l)?.get(key))
    }

    /// The ROM's sheet; `None` for a ROM that is not in the game database.
    pub fn for_rom(&self, rom: &[u8]) -> Option<Sheet> {
        let g = (self.sys.game_info)(rom)?;
        let t = self.texts(&g.id).cloned().unwrap_or_default();
        Some(Sheet {
            title: g.title.clone(),
            alt_title: g.alt_title.clone(),
            thumbnail: g.thumbnail.clone(),
            year: g.year.clone(),
            publisher: g.publisher.clone(),
            developer: g.developer.clone(),
            serial: g.serial.clone(),
            genre: t.genre,
            players: g.players.map(players_text),
            description: t.description,
            notes: t.notes,
            key: g.id.clone(),
        })
    }
}

/// Problems in a system's game texts (for the tests): invalid JSON or an unknown field,
/// a missing language, or a game or field present in one language and absent in another.
pub fn check(sys: &SystemInfo) -> Vec<String> {
    let mut out = vec![];
    let mut langs: Vec<(&str, BTreeMap<String, Texts>)> = vec![];
    for &(code, json) in sys.game_texts {
        match serde_json::from_str(json) {
            Ok(m) => langs.push((code, m)),
            Err(e) => out.push(format!("{code}: {e}")),
        }
    }
    for (code, _) in crate::i18n::LANGS {
        if !sys.game_texts.iter().any(|(c, _)| *c == code) {
            out.push(format!("missing game texts \"{code}\""));
        }
    }
    let shape = |t: &Texts| (t.genre.is_some(), t.description.is_some(), t.notes.is_some());
    if let Some((first, base)) = langs.first() {
        for (code, m) in &langs {
            for (game, t) in m {
                match base.get(game) {
                    None => out.push(format!("{code}.{game}: does not exist in {first}")),
                    Some(b) if shape(b) != shape(t) => out.push(format!("{code}.{game}: fields differ from {first}")),
                    _ => {}
                }
            }
            for game in base.keys().filter(|g| !m.contains_key(*g)) {
                out.push(format!("{code}: missing {game}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu_api::{GameInfo, PadButton};

    static GAME: std::sync::LazyLock<GameInfo> = std::sync::LazyLock::new(|| GameInfo {
        id: "game".into(),
        title: "Game".into(),
        alt_title: None,
        year: Some("1983".into()),
        publisher: None,
        developer: None,
        serial: None,
        players: Some(2),
        thumbnail: Some("Game (World)".into()),
    });

    const BASE: SystemInfo = SystemInfo {
        name: "Test",
        short_name: "T",
        extensions: &["bin"],
        thumbnails: "",
        cover_aspect: 0.66,
        players: 1,
        buttons: &[emu_api::Button { id: PadButton::B, label: "Fire 1" }],
        width: 1,
        height: 1,
        aspect: 1.0,
        ram_address: 0,
        fps_num: 60,
        fps_den: 1,
        game_info: |rom| (rom == b"123456789").then(|| &*GAME),
        game_texts: &[
            ("en", r#"{"game": {"description": "A game."}}"#),
            ("pt", r#"{"game": {"description": "Um jogo."}}"#),
            ("es", r#"{"game": {"description": "Un juego."}}"#),
        ],
        bios: &[],
        multi_file_roms: false,
        create: |_| unreachable!(),
    };
    static SYS: SystemInfo = BASE;

    #[test]
    fn sheet_in_language() {
        let s = Sheets::load(&SYS);
        crate::i18n::set_lang("es");
        let f = s.for_rom(b"123456789").expect("sheet");
        crate::i18n::set_lang("en");
        assert_eq!((f.title.as_str(), f.thumbnail.as_deref()), ("Game", Some("Game (World)")));
        assert_eq!(f.description.as_deref(), Some("Un juego."));
        assert_eq!(f.players.as_deref(), Some("1 o 2 jugadores"));
        assert!(s.for_rom(b"other rom").is_none());
        assert!(check(&SYS).is_empty(), "{:?}", check(&SYS));
    }

    #[test]
    fn check_reports_differences() {
        static BAD: SystemInfo = SystemInfo {
            game_texts: &[
                ("en", r#"{"x": {"description": "A", "notes": "N"}}"#),
                ("pt", r#"{"x": {"descripton": "B"}}"#),
            ],
            ..BASE
        };
        let p = check(&BAD);
        assert!(p.iter().any(|m| m.starts_with("pt:") && m.contains("descripton")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("\"es\"")), "{p:?}");
    }
}
