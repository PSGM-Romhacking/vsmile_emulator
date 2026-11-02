//! Persistent configuration and the app's folders. Everything lives in one folder,
//! `$XDG_CONFIG_HOME/<id>/` (default `~/.config/<id>/`; on Windows `%APPDATA%\<id>\`;
//! `<id>` comes from `brand`): `config.json`, `states/`, `cheats/`, `covers/` (the user's
//! own covers), `languages/` and `cache/` (downloaded covers). Screenshots and recordings
//! go to the user's Pictures and Videos folders. Each folder can be changed
//! (Emulation › Folders…); see [`Folder`].
//!
//! **Portable mode:** with a file named `portable.txt` next to the program (the AppImage
//! or the `.exe`), the app's folder is `<program's folder>/<id>_data/` instead, screenshots and
//! recordings included, and the paths inside it are saved relative to the program's
//! folder, so the whole thing can move (a USB stick mounted elsewhere, another PC).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// Interface preferences; a field missing from the file = default value.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub muted: bool,
    pub volume: u32,
    pub scale: u32,
    pub integer_scale: bool,
    pub ff_speed: u32,
    pub rewind: bool,
    pub slot: u32,
    /// Video filter: "none", "scanlines" or "crt".
    pub filter: String,
    pub open_last: bool,
    /// Low-pass filter on the audio (smooths the harshness of the square wave).
    pub soft_audio: bool,
    /// Interface language ("en", "pt", "es").
    pub language: String,
    pub pause_unfocused: bool,
    pub show_fps: bool,
    /// Library image: "boxart", "title" or "snap".
    pub library_art: String,
    /// Frame around the game: "none", "cover" or the key of an app bezel ("tv"…).
    pub frame: String,
    /// Couch mode: fullscreen, no menu bar, the library and a quick menu driven by the
    /// controller (see app/nav.rs).
    pub couch: bool,
    /// Select + Start held together open and close the quick menu (besides its own button).
    pub menu_combo: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            muted: true, // the core's sound is loud
            volume: 30,
            scale: 3,
            integer_scale: false,
            ff_speed: 4,
            rewind: true,
            slot: 1,
            filter: "none".into(),
            open_last: false,
            soft_audio: true,
            language: crate::i18n::DEFAULT.into(),
            pause_unfocused: true,
            show_fps: false,
            library_art: "boxart".into(),
            frame: "none".into(),
            couch: false,
            menu_combo: true,
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Last ROM folder used.
    #[serde(default)]
    pub rom_dir: Option<PathBuf>,
    /// Sound, volume, scale, theme, fast forward, rewind and slot.
    #[serde(default)]
    pub prefs: Prefs,
    /// Recently opened games (newest first).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent: Vec<PathBuf>,
    /// The core's settings (`Core::options`): key → value. Absent = the default value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub core_options: BTreeMap<String, String>,
    /// Mapping for each controller, by the name the driver reports:
    /// action ("p1_up", "p1_b", "save_state"…) → inputs ("b3", "a1+"…). Absent = default map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pads: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// Keys for each action ("save_state" → ["F2"]…). Empty = default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, Vec<String>>,
    /// Folders changed by the user: `Folder::key` → path. Absent = the default folder.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dirs: BTreeMap<String, PathBuf>,
    /// Changed folders that could not be found at startup (an unplugged disk…): the
    /// default is used in this session, without forgetting the user's choice.
    #[serde(skip)]
    pub unreachable: Vec<PathBuf>,
}

/// The app's folder, with everything in it: `$XDG_CONFIG_HOME/<id>` or `%APPDATA%\<id>`;
/// in portable mode, `<program's folder>/<id>_data` (not `<id>`: on Linux that is the
/// name of the bare executable).
pub fn app_dir() -> Option<PathBuf> {
    match portable() {
        Portable::On(base) => Some(base.join(portable_name())),
        _ => Some(crate::platform::config_base()?.join(crate::brand::get().id)),
    }
}

/// Whether the program runs in portable mode.
#[derive(Clone, PartialEq, Debug)]
pub enum Portable {
    Off,
    /// On, with the program's folder.
    On(PathBuf),
    /// `portable.txt` is there but its folder can't be written to (a CD, a read-only
    /// share): the normal folder is used, and the reason is shown at startup.
    ReadOnly(PathBuf),
}

/// The program's folder: the AppImage's (`$APPIMAGE`; the executable itself runs from a
/// temporary mount) or the executable's.
fn program_dir() -> Option<PathBuf> {
    let exe = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| std::env::current_exe().ok())?;
    Some(exe.parent()?.to_path_buf())
}

fn portable_name() -> String {
    format!("{}_data", crate::brand::get().id)
}

/// Checked once per run.
pub fn portable() -> Portable {
    static P: std::sync::OnceLock<Portable> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        let Some(dir) = program_dir().filter(|d| d.join("portable.txt").is_file()) else { return Portable::Off };
        let app = dir.join(portable_name());
        let probe = app.join(".write_test");
        let writable = std::fs::create_dir_all(&app).and_then(|_| std::fs::write(&probe, b"")).is_ok();
        let _ = std::fs::remove_file(&probe);
        if writable { Portable::On(dir) } else { Portable::ReadOnly(dir) }
    })
    .clone()
}

/// In portable mode, a path inside the program's folder becomes relative to it (when
/// saving) and back (when loading); everything else is kept as it is.
fn portable_path(p: &Path, saving: bool) -> PathBuf {
    let Portable::On(base) = portable() else { return p.to_path_buf() };
    match (saving, p.strip_prefix(&base)) {
        (true, Ok(rel)) => rel.to_path_buf(),
        (false, _) if p.is_relative() => base.join(p),
        _ => p.to_path_buf(),
    }
}

fn path() -> Option<PathBuf> {
    Some(app_dir()?.join("config.json"))
}

/// Downloaded covers: `<app folder>/cache/covers` (can be deleted at will).
pub fn cache_dir() -> Option<PathBuf> {
    Some(app_dir()?.join("cache"))
}

/// A folder the user can change.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Folder {
    States,
    Cheats,
    /// The user's own covers, in the libretro-thumbnails layout (`Named_Boxarts/<game>.png`…);
    /// they win over the downloaded ones.
    Covers,
    /// Extra translations: `<code>.json` (see `i18n::load_user`).
    Languages,
    Screenshots,
    Recordings,
    /// Battery saves of the cartridges (`<game>.sav`), for systems that have them.
    Saves,
    /// System files (BIOS), for systems that need them (`SystemInfo::bios`).
    Bios,
}

impl Folder {
    pub const ALL: [Folder; 8] = [Folder::States, Folder::Cheats, Folder::Covers, Folder::Languages, Folder::Screenshots, Folder::Recordings, Folder::Saves, Folder::Bios];

    /// Key in `Config::dirs`.
    pub fn key(self) -> &'static str {
        match self {
            Folder::States => "states",
            Folder::Cheats => "cheats",
            Folder::Covers => "covers",
            Folder::Languages => "languages",
            Folder::Screenshots => "screenshots",
            Folder::Recordings => "recordings",
            Folder::Saves => "saves",
            Folder::Bios => "bios",
        }
    }

    /// Name shown in the Folders dialog (goes through tr()).
    pub fn label(self) -> &'static str {
        match self {
            Folder::States => "Save states",
            Folder::Cheats => "Cheats",
            Folder::Covers => "Own covers",
            Folder::Languages => "Languages",
            Folder::Screenshots => "Screenshots",
            Folder::Recordings => "Recordings",
            Folder::Saves => "Battery saves",
            Folder::Bios => "BIOS",
        }
    }

    /// Where it is when the user has not changed it.
    pub fn default_dir(self) -> Option<PathBuf> {
        let portable = matches!(portable(), Portable::On(_));
        match self {
            Folder::Screenshots if !portable => user_dir("PICTURES", "Pictures"),
            Folder::Recordings if !portable => user_dir("VIDEOS", "Videos"),
            f => Some(app_dir()?.join(f.key())),
        }
    }

    /// The folder in use: the user's, if set and reachable, or the default.
    pub fn dir(self) -> Option<PathBuf> {
        let custom = CUSTOM.read().unwrap_or_else(|e| e.into_inner()).get(self.key()).cloned();
        custom.or_else(|| self.default_dir())
    }
}

/// The user's folders in use (set by [`Config::apply_dirs`]).
static CUSTOM: RwLock<BTreeMap<String, PathBuf>> = RwLock::new(BTreeMap::new());

/// Screenshots folder: `<Pictures>/<app name>` (XDG folders), unless changed.
pub fn pictures_dir() -> Option<PathBuf> {
    Folder::Screenshots.dir()
}

/// Recordings folder (GIF and MP4): `<Videos>/<app name>`, unless changed.
pub fn videos_dir() -> Option<PathBuf> {
    Folder::Recordings.dir()
}

fn user_dir(kind: &str, fallback: &str) -> Option<PathBuf> {
    Some(crate::platform::user_dir(kind, fallback)?.join(crate::brand::get().name))
}

/// Moves what is in `src` into `dst`, merging folders. A file that already exists in
/// `dst` is not overwritten (the one in `src` stays where it was). Folders left empty in
/// `src` are removed. Works across disks (copy and delete when renaming fails).
pub fn move_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.starts_with(src) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "destination inside the source"));
    }
    for e in std::fs::read_dir(src)?.flatten() {
        let (from, to) = (e.path(), dst.join(e.file_name()));
        if e.file_type()?.is_dir() {
            move_tree(&from, &to)?;
        } else if !to.exists() {
            std::fs::create_dir_all(dst)?;
            if std::fs::rename(&from, &to).is_err() {
                std::fs::copy(&from, &to)?;
                std::fs::remove_file(&from)?;
            }
        }
    }
    let _ = std::fs::remove_dir(src); // only if it was emptied
    Ok(())
}

/// Development builds before 0.9 split the data XDG-style: saves and cheats in `$XDG_DATA_HOME/<id>`
/// and the covers in `$XDG_CACHE_HOME/<id>` (on Windows, `%LOCALAPPDATA%\<id>`). Brings
/// them into the app's folder.
fn migrate() {
    if matches!(portable(), Portable::On(_)) {
        return; // a portable copy starts on its own
    }
    let Some(app) = app_dir() else { return };
    let id = crate::brand::get().id;
    let old = |base: Option<PathBuf>| base.map(|b| b.join(id)).filter(|o| *o != app && o.is_dir());
    if let Some(o) = old(crate::platform::legacy_data_base()) {
        let _ = move_tree(&o, &app);
    }
    if let Some(o) = old(crate::platform::legacy_cache_base()) {
        let _ = move_tree(&o, &app.join("cache"));
    }
}

impl Config {
    /// Reads the configuration (missing or invalid file = default), after bringing the
    /// data of older versions into the app's folder, and puts the user's folders in use.
    pub fn load() -> Self {
        migrate();
        let mut cfg: Self = path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        cfg.map_paths(false);
        cfg.apply_dirs();
        cfg
    }

    /// The paths of the file through [`portable_path`].
    fn map_paths(&mut self, saving: bool) {
        let f = |p: &mut PathBuf| *p = portable_path(p, saving);
        self.rom_dir.iter_mut().for_each(f);
        self.recent.iter_mut().for_each(f);
        self.dirs.values_mut().for_each(f);
    }

    /// Puts `dirs` in use; the ones that are not an existing folder go to `unreachable`.
    pub fn apply_dirs(&mut self) {
        let (ok, bad): (BTreeMap<_, _>, BTreeMap<_, _>) = self.dirs.iter().map(|(k, p)| (k.clone(), p.clone())).partition(|(_, p)| p.is_absolute() && p.is_dir());
        self.unreachable = bad.into_values().collect();
        *CUSTOM.write().unwrap_or_else(|e| e.into_inner()) = ok;
    }

    /// Writes atomically (temporary file + rename).
    pub fn save(&self) {
        let Some(p) = path() else { return };
        let res = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(p.parent().unwrap())?;
            let tmp = p.with_extension("json.tmp");
            let mut out = self.clone();
            out.map_paths(true);
            std::fs::write(&tmp, serde_json::to_string_pretty(&out).unwrap_or_default())?;
            std::fs::rename(&tmp, &p)
        })();
        if let Err(e) = res {
            eprintln!("config: could not save {}: {e}", p.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_tree_merges_without_overwriting() {
        let base = std::env::temp_dir().join(format!("emu_frontend_move_{}", std::process::id()));
        let (src, dst) = (base.join("src"), base.join("dst"));
        std::fs::create_dir_all(src.join("states")).unwrap();
        std::fs::create_dir_all(src.join("empty")).unwrap();
        std::fs::create_dir_all(dst.join("states")).unwrap();
        std::fs::write(src.join("states/a.state"), b"new a").unwrap();
        std::fs::write(src.join("states/b.state"), b"b").unwrap();
        std::fs::write(dst.join("states/a.state"), b"old a").unwrap();
        move_tree(&src, &dst).unwrap();
        assert_eq!(std::fs::read(dst.join("states/a.state")).unwrap(), b"old a", "existing file kept");
        assert_eq!(std::fs::read(dst.join("states/b.state")).unwrap(), b"b");
        assert!(src.join("states/a.state").exists(), "the conflicting file stays in the source");
        assert!(!src.join("empty").exists() && !dst.join("empty").exists(), "empty folders vanish");
        assert!(move_tree(&base, &dst).is_err(), "destination inside the source");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Without `portable.txt` next to the test binary, paths are kept as they are.
    #[test]
    fn paths_untouched_when_not_portable() {
        assert_eq!(portable(), Portable::Off);
        let p = Path::new("/media/stick/roms");
        assert_eq!(portable_path(p, true), p);
        assert_eq!(portable_path(p, false), p);
    }
}
