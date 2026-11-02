//! Languages: the source text in the code is English; each
//! other language has a catalog `assets/i18n/<code>.json` (source text → translation),
//! embedded in the binary. Without a translation, the source text is shown. Default: English.
//! The user's files in the languages folder ([`load_user`]) add languages or correct the
//! built-in ones.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{LazyLock, RwLock};

/// Built-in languages: (code, native name).
pub const LANGS: [(&str, &str); 3] = [("en", "English"), ("pt", "Português (Brasil)"), ("es", "Español")];

/// Key of a user catalog that holds the language's own name, not a translation.
pub const NAME_KEY: &str = "@name";
pub const DEFAULT: &str = "en";

/// The frontend's generic catalogs: (code, JSON).
const BUILTIN: [(&str, &str); 2] = [("pt", include_str!("../../../assets/i18n/pt.json")), ("es", include_str!("../../../assets/i18n/es.json"))];

fn parse(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).expect("invalid language catalog")
}

static CATALOGS: LazyLock<RwLock<HashMap<&'static str, HashMap<String, String>>>> =
    LazyLock::new(|| RwLock::new(BUILTIN.iter().map(|&(c, j)| (c, parse(j))).collect()));

static CURRENT: RwLock<&'static str> = RwLock::new(DEFAULT);

/// Languages added by the user's files: (code, native name).
static USER: RwLock<Vec<(&'static str, &'static str)>> = RwLock::new(Vec::new());

/// The languages to choose from: the built-in ones, then the user's.
pub fn langs() -> Vec<(&'static str, &'static str)> {
    LANGS.into_iter().chain(USER.read().unwrap_or_else(|e| e.into_inner()).iter().copied()).collect()
}

/// Adds the app's own texts (the system's buttons, "About", command line...) to a built-in
/// language: `json` is a catalog like `assets/i18n/<code>.json` (English text → translation).
/// Call it right after `brand::set`, once per language. Unknown codes and invalid JSON are
/// ignored (the texts stay in English).
pub fn add_catalog(code: &str, json: &str) {
    let Some(&(code, _)) = LANGS.iter().find(|(c, _)| *c == code) else { return };
    let Ok(cat) = serde_json::from_str::<HashMap<String, String>>(json) else { return };
    CATALOGS.write().unwrap_or_else(|e| e.into_inner()).entry(code).or_default().extend(cat);
}

/// Reads the user's catalogs, `<dir>/<code>.json`: the same format as the built-in ones
/// (English text → translation), plus `"@name"` with the language's name ("Français").
/// The code is the file name ("fr", "pt-PT"); a built-in code corrects that language.
/// Can be called again (after changing the folder): what is new is added. Returns the
/// files that could not be read, with the reason.
pub fn load_user(dir: &Path) -> Vec<String> {
    let mut errors = vec![];
    let Ok(rd) = std::fs::read_dir(dir) else { return errors };
    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    for f in files {
        let code = f.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let name = f.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if code.is_empty() || code.len() > 16 || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            errors.push(format!("{name}: the file name must be the language code (fr.json, pt-PT.json)"));
            continue;
        }
        let mut cat: HashMap<String, String> = match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{name}: {e}"));
                continue;
            }
        };
        let native = cat.remove(NAME_KEY).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| code.clone());
        cat.retain(|_, v| !v.is_empty()); // an empty value = not translated yet
        let known = langs().into_iter().find(|(c, _)| *c == code).map(|(c, _)| c);
        let code: &'static str = known.unwrap_or_else(|| Box::leak(code.into_boxed_str()));
        if known.is_none() {
            USER.write().unwrap_or_else(|e| e.into_inner()).push((code, Box::leak(native.into_boxed_str())));
        }
        CATALOGS.write().unwrap_or_else(|e| e.into_inner()).entry(code).or_default().extend(cat);
    }
    errors
}

/// A catalog to start a new language from: every known text, each one mapped to itself
/// (the English text), and `"@name"`. Saved by Help › Export translation template….
pub fn template() -> String {
    let c = CATALOGS.read().unwrap_or_else(|e| e.into_inner());
    let keys: BTreeSet<&String> = c.iter().filter(|(code, _)| LANGS.iter().any(|(l, _)| l == *code)).flat_map(|(_, m)| m.keys()).collect();
    let mut out = serde_json::Map::new();
    out.insert(NAME_KEY.into(), "Language name".into());
    for k in keys {
        out.insert(k.clone(), k.clone().into());
    }
    serde_json::to_string_pretty(&out).unwrap_or_default()
}

pub fn set_lang(code: &str) {
    let code = langs().into_iter().find(|(c, _)| *c == code).map_or(DEFAULT, |(c, _)| c);
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = code;
}

pub fn lang() -> &'static str {
    *CURRENT.read().unwrap_or_else(|e| e.into_inner())
}

/// Translates into the current language.
pub fn tr(text: &str) -> String {
    let c = CATALOGS.read().unwrap_or_else(|e| e.into_inner());
    c.get(lang()).and_then(|c| c.get(text)).cloned().unwrap_or_else(|| text.to_string())
}

/// Translates and fills in the `{name}` fields.
pub fn trf(text: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = tr(text);
    for (k, v) in args {
        out = out.replace(&format!("{{{k}}}"), &v.to_string());
    }
    out
}

/// Texts passed literally to tr()/trf() in the folder's `.rs` files.
#[doc(hidden)]
pub fn source_keys(dir: &Path) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.file_name() == "i18n.rs" || e.path().extension().is_none_or(|x| x != "rs") {
            continue; // this very file contains "tr(\"" as text
        }
        let src = std::fs::read_to_string(e.path()).unwrap();
        for marker in ["tr(\"", "trf(\""] {
            let mut rest = src.as_str();
            while let Some(i) = rest.find(marker) {
                let before = rest[..i].chars().last();
                rest = &rest[i + marker.len()..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue; // another identifier ending in "tr("
                }
                let end = rest.find('"').unwrap();
                keys.insert(rest[..end].replace("\\\"", "\""));
            }
        }
    }
    keys
}

/// Checks that every key has a translation in every built-in language, with the same {…} fields
/// (uses the catalogs already registered).
#[doc(hidden)]
pub fn assert_translated(keys: &BTreeSet<String>) {
    let fields = |s: &str| s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(k, _)| k.to_string())).collect::<BTreeSet<_>>();
    let cats = CATALOGS.read().unwrap_or_else(|e| e.into_inner());
    // only the built-in languages (a user's file may be partial)
    for (code, cat) in cats.iter().filter(|(c, _)| LANGS.iter().any(|(l, _)| l == *c)) {
        let missing: Vec<_> = keys.iter().filter(|k| !cat.contains_key(*k)).collect();
        assert!(missing.is_empty(), "{code}.json missing translation for: {missing:#?}");
        for k in keys {
            assert_eq!(fields(k), fields(&cat[k]), "{code}.json: different fields in {k:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A user file adds a language with its name and can correct a built-in one.
    #[test]
    fn user_languages() {
        let dir = std::env::temp_dir().join(format!("emu_frontend_langs_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("fr.json"), r#"{"@name": "Français", "Quit": "Quitter", "Pause": ""}"#).unwrap();
        std::fs::write(dir.join("broken.json"), "{").unwrap();
        std::fs::write(dir.join("bad name.json"), "{}").unwrap();
        let errors = load_user(&dir);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(langs().contains(&("fr", "Français")));
        let c = CATALOGS.read().unwrap();
        assert_eq!(c["fr"]["Quit"], "Quitter");
        assert!(!c["fr"].contains_key("Pause"), "empty = not translated");
        drop(c);
        assert!(template().contains("\"@name\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every frontend text passed to tr()/trf() has a translation in the generic catalog.
    #[test]
    fn catalogs_complete() {
        let mut keys = source_keys(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")));
        // texts that reach tr() through a variable (the scanner above does not see them)
        for f in crate::gamepad::Func::EMU {
            keys.insert(f.emu_label().to_string());
        }
        for a in crate::covers::Art::ALL {
            keys.insert(a.label().to_string());
        }
        for f in crate::config::Folder::ALL {
            keys.insert(f.label().to_string());
        }
        for k in [
            "None", "Scanlines", "CRT", "No frame", "Game cover", "Memory", "on", "off", "muted", "Continue", "Pause", "EMULATOR", "ACTION", "KEYBOARD", "CONTROLLER",
            "Fast forward on", "Fast forward off", "Sound off", "Sound on", "custom", "default",
            "Stop video recording", "Record video (MP4)", "Stop GIF", "Record GIF",
            "Year", "Publisher", "Developer", "Cartridge", "File", "Path", "Description", "Players", "Restart search", "Start search", "Codes", "Search the RAM", "Begin", "Play", "Choose", "Increased", "Decreased", "Changed", "Did not change", "Before", "Now", "Hold", "Value", "Address", "Name",
        ] {
            keys.insert(k.to_string());
        }
        assert_translated(&keys);
    }
}
