//! egui interface: menu bar, game screen and dialogs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use egui::{Color32, Key, Modifiers, RichText, Sense, Stroke, Vec2, ViewportCommand};

use crate::audio::Audio;
use crate::config::Folder;
use crate::gamepad::{self, Func, Mapping, PadInfo, Pads};
use crate::i18n::{tr, trf};
use crate::keys::{self, KeyMap};

mod controls;
mod dialogs;
mod docshots;
mod emulation;
mod folders;
mod input;
mod library;
mod menus;
mod nav;
mod recording;
mod screen;
mod state;
use emu_api::{Core, SystemInfo};
use crate::theme;
use crate::widgets::Look;

/// Library: just the box cover, in its own shape (`SystemInfo::cover_aspect`). The height
/// is fixed; the width follows the shape. They also define the initial window size.
/// Size of the library's images (covers, title and in-game screens) over the original
/// layout: 20% larger.
const LIB_SCALE: f32 = 1.2;
const COVER_H: f32 = 213.0 * LIB_SCALE;
const CARD_H: f32 = COVER_H + 6.0 + 34.0; // cover + name

/// Width of a cover in the library for this system's box shape.
fn cover_w(sys: &SystemInfo) -> f32 {
    (COVER_H * sys.cover_aspect).clamp(100.0 * LIB_SCALE, 280.0 * LIB_SCALE)
}
const CARD_GAP: f32 = 12.0;
const LIB_COLS: f32 = 7.0;
const LIB_ROWS: f32 = 2.0;
/// Title and in-game screen are landscape (256 × 192, 4:3): 6 × 3 in the same window,
/// taking the same width as the cover grid.
const SHOT_W: f32 = 168.0 * LIB_SCALE;
const SHOT_H: f32 = 126.0 * LIB_SCALE;

/// Initial window size: the library with 7 × 2 covers (14), plus header,
/// margins and the menu bar.
pub fn library_window_size(sys: &SystemInfo) -> [f32; 2] {
    let card_w = cover_w(sys) + 12.0;
    let w = 24.0 * 2.0 + LIB_COLS * card_w + (LIB_COLS - 1.0) * CARD_GAP + 14.0; // margins + slack
    let h = 28.0 + 16.0 + LIB_HEADER + LIB_ROWS * CARD_H + (LIB_ROWS - 1.0) * CARD_GAP + 8.0 + 10.0; // bar + header + grid
    [w, h]
}
const LIB_HEADER: f32 = 60.0;
/// Height left for the title bar and the taskbar when fitting the window to the screen.
const SCREEN_MARGIN_H: f32 = 90.0;

/// Rewind: keeps a state every 2 frames, up to ~30 s (~30 MB).
const REWIND_EVERY: u64 = 2;
const REWIND_MAX: usize = 900;

#[derive(Clone, Copy, PartialEq)]
enum Speed {
    Normal,
    Fast,
    Rewind,
}

/// Video filter (saved as "none", "scanlines" or "crt").
#[derive(Clone, Copy, PartialEq)]
enum Filter {
    None,
    Scanlines,
    Crt,
}

impl Filter {
    fn from_name(s: &str) -> Self {
        match s {
            "scanlines" => Filter::Scanlines,
            "crt" => Filter::Crt,
            _ => Filter::None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Filter::None => "none",
            Filter::Scanlines => "scanlines",
            Filter::Crt => "crt",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Filter::None => "None",
            Filter::Scanlines => "Scanlines",
            Filter::Crt => "CRT",
        }
    }
}

/// Column of the controls configuration.
#[derive(Clone, Copy, PartialEq)]
enum Col {
    Key,
    Pad,
}

#[derive(PartialEq)]
enum Dialog {
    None,
    /// The previous run closed with an error; the report's path.
    Crash(PathBuf),
    Controls,
    /// Configure controls: selected controller and action/column waiting for a press.
    Gamepad { dev: usize, capture: Option<(Func, Col)> },
    About,
    /// Emulation › Folders…; whether changing a folder moves its files along.
    Folders { move_files: bool },
    /// Game info sheet and the cached cover, if any.
    Info(Box<crate::sheets::Sheet>, Option<PathBuf>, PathBuf),
    /// Play history: one game's sessions, or every game's (`None`).
    History(Option<PathBuf>),
    Error(String),
    /// System files missing (message, game to retry): offers to choose the BIOS folder.
    MissingBios(String, Option<PathBuf>),
    /// Tools › Export joined ROM…: the game's file and the outcome of the last export.
    ExportRom { src: Option<PathBuf>, result: Option<Result<String, String>> },
}

/// Identification of a game file, cached by modification time.
#[derive(Clone)]
struct Ident {
    mtime: Option<std::time::SystemTime>,
    /// Title from the info sheet (MAME or sheets).
    title: Option<String>,
    /// Cover name in libretro-thumbnails, from the info sheet.
    thumbnail: Option<String>,
    /// The whole info sheet (genre, year…), for the list view; in the language of the
    /// moment it was read (the cache is emptied when the language changes).
    sheet: Option<crate::sheets::Sheet>,
}

/// A column of the library's list view (Box art / Title screen / Gameplay / List).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ListCol {
    Title,
    File,
    Genre,
    Year,
    Developer,
    Publisher,
    Players,
    PlayTime,
    LastPlayed,
}

impl ListCol {
    const ALL: [ListCol; 9] = [ListCol::Title, ListCol::File, ListCol::Genre, ListCol::Year, ListCol::Developer, ListCol::Publisher, ListCol::Players, ListCol::PlayTime, ListCol::LastPlayed];

    fn label(self) -> &'static str {
        match self {
            ListCol::Title => "Title",
            ListCol::File => "File",
            ListCol::Genre => "Genre",
            ListCol::Year => "Year",
            ListCol::Developer => "Developer",
            ListCol::Publisher => "Publisher",
            ListCol::Players => "Players",
            ListCol::PlayTime => "Play time",
            ListCol::LastPlayed => "Last played",
        }
    }
}

pub struct App {
    /// Emulated system and the core in use (replaced entirely when the game is closed).
    sys: &'static SystemInfo,
    core: Box<dyn Core>,
    audio: Audio,
    pads: Arc<Pads>,
    tex: Option<egui::TextureHandle>,
    look: Look,

    loaded: bool,
    paused: bool,
    muted: bool, // starts muted: the core's sound is loud
    volume: u32, // %
    scale: u32,
    /// integer multiples only (uniform pixels); off = fills the window
    integer_scale: bool,
    filter: Filter,
    /// Tools › Debugger and a frame advance it asked for (runs once while paused).
    debugger: crate::debugger::Debugger,
    step_frame: bool,
    /// Tools › Cheats (codes of the current game, held every frame).
    cheats: crate::cheats::Cheats,
    /// Frame around the game ("none", "cover" or a bezel key) and its textures.
    frame: String,
    bezel_tex: std::collections::HashMap<&'static str, Option<egui::TextureHandle>>,
    /// Blurred cover of the current game (game, texture), made when the game changes.
    cover_bg: Option<(PathBuf, Option<egui::TextureHandle>)>,
    open_last: bool,
    /// Smooth sound: low-pass and DC blocker state.
    soft_audio: bool,
    lp: [f32; 2],
    dc_x: [f32; 2],
    dc_y: [f32; 2],
    size_pending: bool,
    bar_h: f32,
    bar_visible: bool,
    /// frames until releasing the min/max used to force the size
    size_lock: u32,
    /// The window was checked against the screen size (once, when the monitor is known).
    screen_fitted: bool,
    menu_open: bool,
    dialog: Dialog,

    rom_dir: PathBuf,
    cfg: crate::config::Config,
    cur_game: Option<PathBuf>,
    games: Vec<PathBuf>,

    /// Keyboard: held keys, keys pressed this frame and the action map.
    keys_down: HashSet<Key>,
    key_edges: Vec<Key>,
    keymap: KeyMap,
    /// Actions triggered by the controller in the previous frame (to act only on press).
    pad_prev: HashSet<Func>,
    /// Configuration: the next key goes to the action being captured (triggers nothing).
    kb_capture_on: bool,
    kb_captured: Option<Key>,
    /// Fast forward on without having to hold.
    ff_toggle: bool,
    ff_speed: u32,
    rewind_enabled: bool,
    history: std::collections::VecDeque<Vec<u8>>,
    frame_no: u64,
    speed: Speed,
    /// Save state: current slot (1–9) and fingerprint of the loaded ROM.
    slot: u32,
    rom_hash: u64,
    /// Slot thumbnails: file → (modification time, texture).
    thumbs: std::collections::HashMap<PathBuf, (std::time::SystemTime, egui::TextureHandle)>,
    /// Auto-pause when switching windows (and whether it is active now).
    pause_unfocused: bool,
    focus_paused: bool,
    /// FPS/speed indicator: frames emulated in the last second.
    show_fps: bool,
    fps_frames: u32,
    fps_t0: f64,
    fps_value: f64,
    /// Library search (the library is what the window shows when no game is loaded).
    library_filter: String,
    library_art: crate::covers::Art,
    /// The library shows a list (a table of the games) instead of images.
    library_list: bool,
    /// List view: the column it is sorted by, and whether ascending.
    list_sort: (ListCol, bool),
    /// Old duck icon for "About" (loaded the first time).
    about_icon: Option<egui::TextureHandle>,
    /// Title and cover name of each file, identified by CRC32 (cached by modification time).
    ids: std::collections::HashMap<PathBuf, Ident>,
    /// Game info sheets (system data + embedded sheets).
    sheets: crate::sheets::Sheets,
    /// Covers/screens from libretro-thumbnails and the textures already loaded.
    covers: crate::covers::Covers,
    cover_tex: std::collections::HashMap<PathBuf, egui::TextureHandle>,
    /// Recording in progress and the ones still finishing encoding.
    recorder: Option<crate::recorder::Recorder>,
    finishing: Vec<crate::recorder::Recorder>,
    has_ffmpeg: bool,
    /// Hooks for RetroAchievements (no effect yet; see achievements.rs).
    cheevos: crate::achievements::Achievements,
    /// Quick on-screen notice (text, until when).
    toast: Option<(String, f64)>,
    /// The frame changed outside the emulation loop (e.g. state loaded while paused).
    dirty_frame: bool,
    /// Last title sent to the window.
    last_title: String,
    clock: Instant,
    next_frame: f64,
    /// Screenshot tour for the documentation (`EMU_DOCSHOTS`), if active.
    docshots: Option<docshots::DocShots>,
    /// Battery save as last read or written (`.sav`), and when it was last checked.
    battery_saved: Vec<u8>,
    battery_t: f64,
    /// Play time and history of every game (`usage.json`).
    usage: crate::usage::Usage,
    /// The game being played: (path, start, seconds actually played so far).
    session: Option<(String, f64, f64)>,
    /// Last moment the emulation ran, to add the time between frames to the session.
    session_tick: f64,
    /// Which system files (`SystemInfo::bios`) are in the BIOS folder or the ROM folder
    /// (BIOS menu, library banner); None = not checked since the folders last changed.
    bios_found: Option<Vec<bool>>,
    /// Controller navigation (library, quick menu) and couch mode (see nav.rs).
    nav: nav::Nav,
    couch: bool,
    /// Select + Start (held) also open the quick menu.
    menu_combo: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, sys: &'static SystemInfo, rom: Option<PathBuf>, scale: u32) -> Self {
        let cfg = crate::config::Config::load();
        // the user's languages are only known now (their folder comes from the config)
        let lang_errors = Folder::Languages.dir().map(|d| crate::i18n::load_user(&d)).unwrap_or_default();
        crate::i18n::set_lang(&cfg.prefs.language);
        let pr = cfg.prefs.clone();
        // a single theme, with Blender's colors
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, 1.0);
        let audio = Audio::open();
        let mut app = Self {
            sys,
            core: (sys.create)(audio.rate),
            audio,
            pads: crate::gamepad::start(),
            tex: None,
            look: Look { p: theme::palette(), f: theme::Fonts::scaled(1.0), scale: 1.0 },
            loaded: false,
            paused: false,
            muted: pr.muted,
            volume: pr.volume.min(100),
            scale,
            integer_scale: pr.integer_scale,
            filter: Filter::from_name(&pr.filter),
            frame: pr.frame.clone(),
            debugger: Default::default(),
            step_frame: false,
            cheats: Default::default(),
            bezel_tex: Default::default(),
            cover_bg: None,
            open_last: pr.open_last,
            soft_audio: pr.soft_audio,
            lp: [0.0; 2],
            dc_x: [0.0; 2],
            dc_y: [0.0; 2],
            size_pending: false, // the initial size already comes from the ViewportBuilder; locking at startup makes KWin treat the window as fixed-size
            bar_h: 0.0,
            bar_visible: true,
            size_lock: 0,
            screen_fitted: false,
            menu_open: false,
            dialog: Dialog::None,
            rom_dir: std::env::current_dir().unwrap_or_default(),
            cfg,
            cur_game: None,
            games: vec![],
            keys_down: HashSet::new(),
            key_edges: vec![],
            keymap: KeyMap::new(),
            pad_prev: HashSet::new(),
            kb_capture_on: false,
            kb_captured: None,
            ff_toggle: false,
            ff_speed: pr.ff_speed.clamp(2, 8),
            rewind_enabled: pr.rewind,
            history: Default::default(),
            frame_no: 0,
            speed: Speed::Normal,
            slot: pr.slot.clamp(1, 9),
            rom_hash: 0,
            toast: None,
            cheevos: Default::default(),
            thumbs: Default::default(),
            library_filter: String::new(),
            ids: Default::default(),
            sheets: crate::sheets::Sheets::load(sys),
            about_icon: None,
            library_art: crate::covers::Art::from_key(&pr.library_art),
            library_list: pr.library_art == "list",
            list_sort: (ListCol::Title, true),
            nav: Default::default(),
            couch: pr.couch,
            menu_combo: pr.menu_combo,
            covers: crate::covers::Covers::start(sys.thumbnails),
            cover_tex: Default::default(),
            recorder: None,
            finishing: vec![],
            has_ffmpeg: crate::recorder::has_ffmpeg(),
            pause_unfocused: pr.pause_unfocused,
            focus_paused: false,
            show_fps: pr.show_fps,
            fps_frames: 0,
            fps_t0: 0.0,
            fps_value: 0.0,
            dirty_frame: false,
            last_title: String::new(),
            clock: Instant::now(),
            next_frame: 0.0,
            docshots: docshots::DocShots::from_env(),
            battery_saved: Vec::new(),
            battery_t: 0.0,
            usage: crate::usage::Usage::load(),
            session: None,
            session_tick: 0.0,
            bios_found: None,
        };
        app.keymap = keys::from_cfg(&app.cfg.keys, sys);
        for (k, v) in &app.cfg.core_options {
            app.core.set_option(k, v);
        }
        if let Some(d) = app.cfg.rom_dir.clone().filter(|d| d.is_dir()) {
            app.rom_dir = d; // last folder used
        }
        if let Some(p) = rom {
            let p = std::fs::canonicalize(&p).unwrap_or(p);
            if p.is_dir() {
                app.set_rom_dir(p);
            } else {
                app.load_game(&p);
            }
        } else if app.open_last
            && let Some(g) = app.cfg.recent.first().filter(|g| g.is_file()).cloned() {
                app.load_game(&g);
            }
        app.games = crate::rom::list(&app.rom_dir, sys.extensions);
        let mut warn: Vec<String> = app.cfg.unreachable.iter().map(|p| trf("{path} was not found; using the default for now.", &[("path", &p.display())])).collect();
        warn.extend(lang_errors);
        if let crate::config::Portable::ReadOnly(d) = crate::config::portable() {
            warn.push(trf("portable.txt found, but {path} is read-only: using the normal folder.", &[("path", &d.display())]));
        }
        if !warn.is_empty() {
            app.toast = Some((warn.join("\n"), 6.0));
        }
        if let Some(report) = crate::crashlog::take_pending()
            && matches!(app.dialog, Dialog::None)
        {
            app.dialog = Dialog::Crash(report);
        }
        app
    }

    fn load_game(&mut self, p: &Path) {
        self.flush_battery();
        self.end_session();
        match crate::rom::read(p, self.sys.multi_file_roms) {
            Ok(rom) => {
                if !self.sys.bios.is_empty() {
                    match self.find_bios(p) {
                        Ok(files) => self.core.set_bios(&files),
                        Err(e) => {
                            self.dialog = Dialog::MissingBios(e, Some(p.to_path_buf()));
                            return;
                        }
                    }
                }
                if let Err(e) = self.core.load(&rom) {
                    self.dialog = Dialog::Error(e);
                    return;
                }
                self.core.take_audio(&mut Vec::new());
                self.history.clear();
                self.rom_hash = rom_hash(&rom);
                self.cheevos.game_loaded(&rom);
                self.audio.clear();
                self.cur_game = Some(p.to_path_buf());
                crate::crashlog::set_game(Some(p));
                self.session = Some((p.to_string_lossy().into_owned(), crate::usage::now(), 0.0));
                self.load_battery();
                self.cheats.load(self.cheats_path(p));
                let id = self.identify(p, &rom);
                self.ids.insert(p.to_path_buf(), id);
                if let Some(d) = p.parent() {
                    self.set_rom_dir(d.to_path_buf());
                }
                self.loaded = true;
                self.paused = false;
                self.next_frame = self.now();
                // recent games: newest first, no repeats, up to 10
                let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
                self.cfg.recent.retain(|g| *g != p);
                self.cfg.recent.insert(0, p);
                self.cfg.recent.truncate(10);
                self.cfg.save();
            }
            Err(e) => self.dialog = Dialog::Error(e),
        }
    }

    /// Changes the ROM folder, refreshes the list and saves it to the config.
    fn set_rom_dir(&mut self, d: PathBuf) {
        self.bios_found = None;
        self.games = crate::rom::list(&d, self.sys.extensions);
        if self.cfg.rom_dir.as_ref() != Some(&d) {
            self.cfg.rom_dir = Some(d.clone());
            self.cfg.save();
        }
        self.rom_dir = d;
    }

    /// Reads the ROM folder again: files added, removed or renamed while the app is open.
    fn refresh_games(&mut self) {
        self.bios_found = None;
        self.games = crate::rom::list(&self.rom_dir, self.sys.extensions);
        self.notify(trf("Game list updated: {n} games", &[("n", &self.games.len())]));
    }

    fn open_dialog(&mut self) {
        let exts: Vec<&str> = ["zip", "7z"].into_iter().chain(self.sys.extensions.iter().copied()).collect();
        let f = rfd::FileDialog::new()
            .set_title(tr("Open game"))
            .set_directory(&self.rom_dir)
            .add_filter(trf("{system} games", &[("system", &self.sys.short_name)]), &exts)
            .add_filter(tr("All files"), &["*"])
            .pick_file();
        if let Some(f) = f {
            self.load_game(&f);
        }
    }

    /// Saves a catalog with every text, to start a new language (Help › Language).
    fn export_template(&mut self) {
        let dir = Folder::Languages.dir();
        if let Some(d) = &dir {
            let _ = std::fs::create_dir_all(d);
        }
        let mut dlg = rfd::FileDialog::new().set_title(tr("Export translation template…")).set_file_name("xx.json").add_filter("JSON", &["json"]);
        if let Some(d) = dir.filter(|d| d.is_dir()) {
            dlg = dlg.set_directory(d);
        }
        let Some(p) = dlg.save_file() else { return };
        match std::fs::write(&p, crate::i18n::template()) {
            Ok(()) => self.toast = Some((tr("Template saved. Translate the texts, name the file after the language code (fr.json) in the languages folder and restart."), self.now() + 8.0)),
            Err(e) => self.notify(trf("Could not save: {error}", &[("error", &e)])),
        }
    }

    fn set_language(&mut self, code: &str) {
        crate::i18n::set_lang(code);
        self.ids.clear(); // the game sheets (genre, players…) are read again in the new language
        self.cfg.prefs.language = crate::i18n::lang().into();
        self.cfg.save();
    }

    fn set_mute(&mut self, m: bool) {
        self.muted = m;
        self.audio.clear();
    }

    fn now(&self) -> f64 {
        self.clock.elapsed().as_secs_f64()
    }

    /// Size of the game screen in game pixels, already with the frame's aspect ratio.
    fn display_size(&self) -> Vec2 {
        let v = self.core.video();
        let h = v.height as f32;
        Vec2::new((h * v.aspect).round(), h)
    }

    /// Current frame as an image, scaled `k` times (screenshots and thumbnails).
    fn frame_image(&self, k: u32) -> image::RgbImage {
        let v = self.core.video();
        image::RgbImage::from_fn(v.width as u32 * k, v.height as u32 * k, |x, y| {
            let p = v.pixels[(y / k) as usize * v.pitch + (x / k) as usize];
            image::Rgb([(p >> 16) as u8, (p >> 8) as u8, p as u8])
        })
    }

    fn title(&self) -> String {
        let mut t = match &self.cur_game {
            Some(g) => format!("{} — {}", crate::brand::get().name, self.ids.get(g).map_or_else(|| stem(g), |i| display(g, i))),
            None => crate::brand::get().name.into(),
        };
        if self.paused {
            t += &tr(" (paused)");
        }
        t
    }



    fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), self.now() + 2.0));
    }






    /// Saves the preferences when something changed (called every frame).
    fn save_prefs(&mut self) {
        let p = crate::config::Prefs {
            muted: self.muted,
            volume: self.volume,
            scale: self.scale,
            integer_scale: self.integer_scale,
            ff_speed: self.ff_speed,
            rewind: self.rewind_enabled,
            slot: self.slot,
            filter: self.filter.name().into(),
            open_last: self.open_last,
            soft_audio: self.soft_audio,
            language: crate::i18n::lang().into(),
            pause_unfocused: self.pause_unfocused,
            show_fps: self.show_fps,
            library_art: if self.library_list { "list".into() } else { self.library_art.key().into() },
            frame: self.frame.clone(),
            couch: self.couch,
            menu_combo: self.menu_combo,
        };
        if p != self.cfg.prefs {
            self.cfg.prefs = p;
            self.cfg.save();
        }
    }





    /// File › Close game: finishes recording, unloads and returns to the library.
    fn close_game(&mut self) {
        if !self.loaded {
            return;
        }
        self.flush_battery();
        self.end_session();
        if let Some(mut r) = self.recorder.take() {
            r.stop();
            self.finishing.push(r);
        }
        self.core = (self.sys.create)(self.audio.rate);
        self.history.clear();
        self.audio.clear();
        self.cur_game = None;
        crate::crashlog::set_game(None);
        self.cheats.load(None);
        self.loaded = false;
        self.paused = false;
        self.ff_toggle = false;
        self.tex = None;
        self.notify(tr("Game closed"));
    }

    /// The system files, from the BIOS folder or (small files only) the game's folder;
    /// Err with the list of required ones missing.
    fn find_bios(&self, game: &Path) -> Result<Vec<Option<Vec<u8>>>, String> {
        let bios_dir = Folder::Bios.dir();
        let mut dirs: Vec<(PathBuf, u64)> = bios_dir.iter().map(|d| (d.clone(), u64::MAX)).collect();
        if let Some(d) = game.parent() {
            dirs.push((d.to_path_buf(), 1 << 20));
        }
        let files = crate::rom::find_bios(self.sys.bios, &dirs);
        let missing: Vec<String> = self.sys.bios.iter().zip(&files).filter(|(b, f)| b.required && f.is_none()).map(|(b, _)| format!("{} (CRC32 {:08x})", b.name, b.crc32)).collect();
        if missing.is_empty() {
            return Ok(files);
        }
        let folder = bios_dir.map_or_else(|| "?".to_string(), |d| d.display().to_string());
        Err(trf("Missing system files (BIOS): {files}. Put them in {folder} or next to the game (loose or in a .zip).", &[("folder", &folder), ("files", &missing.join(", "))]))
    }

    /// Asks for the BIOS folder (the same setting as Emulation › Folders) and, if a game
    /// was waiting for it, tries the game again.
    fn choose_bios_folder(&mut self, game: Option<&Path>) {
        let mut d = rfd::FileDialog::new().set_title(tr("Choose BIOS folder…"));
        if let Some(cur) = Folder::Bios.dir().filter(|d| d.is_dir()) {
            d = d.set_directory(cur);
        }
        if let Some(dir) = d.pick_folder() {
            self.cfg.dirs.insert(Folder::Bios.key().to_string(), dir);
            self.cfg.apply_dirs();
            self.cfg.save();
            self.bios_found = None;
            match game {
                Some(g) => self.load_game(g),
                None => {
                    let msg = if self.missing_bios().is_empty() { tr("BIOS found") } else { tr("BIOS still incomplete") };
                    self.notify(msg);
                }
            }
        }
    }

    /// Which system files are in the BIOS folder or the ROM folder (cached), in
    /// `SystemInfo::bios` order.
    fn bios_found(&mut self) -> Vec<bool> {
        if self.bios_found.is_none() {
            let mut dirs: Vec<(PathBuf, u64)> = Folder::Bios.dir().into_iter().map(|d| (d, u64::MAX)).collect();
            dirs.push((self.rom_dir.clone(), 1 << 20));
            let files = crate::rom::find_bios(self.sys.bios, &dirs);
            self.bios_found = Some(files.iter().map(Option::is_some).collect());
        }
        self.bios_found.clone().unwrap_or_default()
    }

    /// Required system files missing from the BIOS folder and the ROM folder.
    fn missing_bios(&mut self) -> Vec<&'static str> {
        let found = self.bios_found();
        self.sys.bios.iter().zip(found).filter(|(b, f)| b.required && !f).map(|(b, _)| b.name).collect()
    }

    /// The system files as text (BIOS › Copy list): name, size, CRC32, status.
    fn bios_report(&mut self) -> String {
        let found = self.bios_found();
        let folder = Folder::Bios.dir().map_or_else(|| "?".to_string(), |d| d.display().to_string());
        let mut out = trf("{system} BIOS — folder: {folder}", &[("system", &self.sys.short_name), ("folder", &folder)]) + "\n";
        // columns padded with spaces: aligned wherever the text is shown in a monospace font
        let name_w = self.sys.bios.iter().map(|b| b.name.chars().count()).max().unwrap_or(0);
        let size_w = self.sys.bios.iter().map(|b| size_text(b.size).chars().count()).max().unwrap_or(0);
        for (b, ok) in self.sys.bios.iter().zip(found) {
            let status = if ok { tr("found") } else if b.required { tr("missing") } else { tr("missing (optional)") };
            out += &format!("{:name_w$}  {:>size_w$}  CRC32 {:08x}  {}\n", b.name, size_text(b.size), b.crc32, status);
        }
        out
    }

    /// Closes the play session of the current game into the history (play time, last
    /// played); sessions under `usage::MIN_SESSION` are left out.
    fn end_session(&mut self) {
        if let Some((path, start, played)) = self.session.take() {
            self.usage.add(&path, start, played);
        }
    }

    /// `<saves folder>/<game>.sav`.
    fn battery_path(&self) -> Option<PathBuf> {
        let stem = self.cur_game.as_ref()?.file_stem()?.to_string_lossy().into_owned();
        Some(Folder::Saves.dir()?.join(format!("{stem}.sav")))
    }

    /// Puts the game's .sav back into the core (if the core has battery RAM).
    fn load_battery(&mut self) {
        self.battery_saved.clear();
        if self.core.battery_ram().is_none() {
            return;
        }
        if let Some(data) = self.battery_path().and_then(|p| std::fs::read(p).ok()) {
            self.core.load_battery(&data);
            self.battery_saved = self.core.battery_ram().unwrap_or_default().to_vec();
            return;
        }
        // none in the saves folder yet: a <game>.sav next to the game (older versions kept it
        // there, and so do other emulators) is used, and copied into the saves folder at the
        // next write (battery_saved stays empty, so that write happens)
        let beside = self.cur_game.as_ref().map(|g| g.with_extension("sav"));
        if let Some(data) = beside.as_ref().and_then(|p| std::fs::read(p).ok()) {
            self.core.load_battery(&data);
            self.notify(trf("Battery save taken from {file}", &[("file", &beside.as_ref().unwrap().display())]));
            return;
        }
        self.battery_saved = self.core.battery_ram().unwrap_or_default().to_vec();
    }

    /// Writes the .sav if the game changed its battery RAM since the last write.
    fn flush_battery(&mut self) {
        if !self.loaded {
            return;
        }
        let Some(ram) = self.core.battery_ram() else { return };
        if ram == self.battery_saved.as_slice() {
            return;
        }
        let ram = ram.to_vec();
        let Some(p) = self.battery_path() else { return };
        let res = std::fs::create_dir_all(p.parent().unwrap()).and_then(|_| {
            let tmp = p.with_extension("tmp");
            std::fs::write(&tmp, &ram)?;
            std::fs::rename(&tmp, &p)
        });
        match res {
            Ok(()) => self.battery_saved = ram,
            Err(e) => self.notify(trf("Save error: {error}", &[("error", &e)])),
        }
    }

    fn reset_game(&mut self) {
        self.core.reset();
        self.cheevos.reset();
        self.history.clear();
    }










    // ------------------------------------------------------------ window

    /// When maximized or fullscreen, Wayland ignores the size request:
    /// go back to normal and only apply it once the window is "free".
    fn set_scale(&mut self, ctx: &egui::Context, s: u32) {
        self.scale = s;
        let (max, full) = ctx.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false)));
        // only request a state change if needed: a needless "leave fullscreen"
        // makes the compositor resend the old size over the new one
        if full {
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        }
        if max {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
        }
        self.size_pending = true;
    }

    /// Usable screen area in points (monitor minus room for the title bar and taskbar), once known.
    fn screen_room(ctx: &egui::Context) -> Option<Vec2> {
        let m = ctx.input(|i| i.viewport().monitor_size)?;
        (m.x > 0.0 && m.y > 0.0).then(|| Vec2::new(m.x - 16.0, m.y - SCREEN_MARGIN_H))
    }

    /// On a small screen (1366×768) the starting size may not fit: shrink the window once.
    fn fit_screen(&mut self, ctx: &egui::Context) {
        if self.screen_fitted {
            return;
        }
        let Some(room) = Self::screen_room(ctx) else { return };
        // on Wayland `inner_rect` never comes (winit can't know the window's position)
        let inner = ctx.input(|i| i.viewport_rect());
        if inner.width() <= 0.0 {
            return;
        }
        self.screen_fitted = true;
        let (max, full) = ctx.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false)));
        if max || full || (inner.width() <= room.x && inner.height() <= room.y) {
            return;
        }
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(inner.size().min(room)));
    }

    fn apply_pending_size(&mut self, ctx: &egui::Context) {
        self.fit_screen(ctx);
        // release the size locks a few frames after they were applied
        if self.size_lock > 0 {
            self.size_lock -= 1;
            if self.size_lock == 0 {
                ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(self.display_size()));
                ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(Vec2::splat(f32::INFINITY)));
            }
        }
        if !self.size_pending || self.bar_h <= 0.0 {
            return;
        }
        let (max, full) = ctx.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false)));
        if max || full {
            return;
        }
        let ppp = ctx.pixels_per_point();
        let game = self.display_size() * self.scale as f32 / ppp; // N physical pixels per game pixel
        let mut size = Vec2::new(game.x, game.y + self.bar_h);
        if let Some(room) = Self::screen_room(ctx) {
            size = size.min(room); // a large scale on a small screen
        }
        // On Wayland winit ignores InnerSize when KWin marks the window as
        // "tiled"; locking min = max = desired size forces the
        // compositor to resize. The locks are removed after `size_lock` frames.
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(size));
        ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(size));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        self.size_lock = 20;
        self.size_pending = false;
    }











    /// Duration of a frame now (the core's timing can depend on a setting).
    fn frame_seconds(&self) -> f64 {
        let (num, den) = self.core.fps();
        den as f64 / num as f64
    }



}

impl eframe::App for App {
    /// Keyboard read straight from the events (held keys and this frame's presses). Tab
    /// (Select) doesn't reach egui, so it doesn't move focus; during configuration
    /// capture, no key reaches it.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if !raw.focused {
            self.keys_down.clear();
        }
        for ev in &raw.events {
            if let egui::Event::Key { key, pressed, repeat: false, modifiers, .. } = ev {
                if *pressed {
                    if self.kb_capture_on {
                        self.kb_captured.get_or_insert(*key);
                        continue;
                    }
                    self.keys_down.insert(*key);
                    if !modifiers.command && !modifiers.alt {
                        self.key_edges.push(*key);
                    }
                } else {
                    self.keys_down.remove(key);
                }
            }
        }
        if self.kb_capture_on {
            raw.events.retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)));
        } else {
            raw.events.retain(|e| !matches!(e, egui::Event::Key { key: Key::Tab, .. }));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_battery();
        self.end_session();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        if self.docshots_embed(ui) {
            self.docshots_step(&ctx);
            return;
        }
        // in fullscreen the menu bar hides and comes back when the mouse touches the top
        self.nav_update(&ctx);
        let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        let y = ctx.input(|i| i.pointer.hover_pos().map(|p| p.y));
        let near = |lim: f32| y.is_some_and(|y| y <= lim);
        // couch mode: no menu bar (the quick menu replaces it)
        self.bar_visible = !self.couch && (!full || self.menu_open || near(if self.bar_visible { self.bar_h + 4.0 } else { 6.0 }));
        if self.bar_visible {
            self.menu_bar(ui);
        } else {
            self.menu_open = false;
        }
        self.apply_pending_size(&ctx);
        self.emulate(&ctx);
        // battery save: written within ~5 s of a change (and when the game closes)
        let now = self.now();
        if now - self.battery_t >= 5.0 {
            self.battery_t = now;
            self.flush_battery();
        }
        if !self.loaded {
            self.library_view(ui); // without a game, the window is the library
        } else {
            self.screen(ui);
        }
        self.dialogs(&ctx);
        self.quick_menu(&ctx);
        self.couch_hints(&ctx);
        let core: Option<&mut dyn Core> = if self.loaded { Some(self.core.as_mut()) } else { None };
        let req = self.debugger.show(&ctx, &self.look, self.sys, core, self.paused);
        if req.toggle_pause && self.loaded {
            self.paused = !self.paused;
        }
        self.step_frame |= req.step;
        let core: Option<&mut dyn Core> = if self.loaded { Some(self.core.as_mut()) } else { None };
        self.cheats.show(&ctx, &self.look, self.sys, core);
        // only when it changes: each window command requests a new frame
        let title = self.title();
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        self.docshots_step(&ctx);
        self.key_edges.clear();
        self.poll_recordings();
        self.save_prefs();
        // every frame only while emulating (or with the controls configuration, which lights
        // up actions live); when idle, 20 times per second is enough to read the controller,
        // show covers that arrived and clear notices (mouse and keyboard already trigger redraws)
        let running = self.loaded && !self.paused && !self.focus_paused && !self.menu_open && self.dialog == Dialog::None;
        if running || matches!(self.dialog, Dialog::Gamepad { .. }) {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}


/// "2026-10-16 21-55-03" (local time), for file names.
fn local_stamp() -> String {
    let t = crate::platform::local_time(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
    format!("{:04}-{:02}-{:02} {:02}-{:02}-{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second)
}

/// "4 KiB", "16 bytes"; empty when not known.
fn size_text(n: u32) -> String {
    match n {
        0 => String::new(),
        n if n >= 1024 && n % 1024 == 0 => format!("{} KiB", n / 1024),
        n => trf("{n} bytes", &[("n", &n)]),
    }
}

/// Opens the folder in the file manager (creates it if it doesn't exist yet).
fn open_folder(dir: Option<PathBuf>) {
    if let Some(d) = dir {
        let _ = std::fs::create_dir_all(&d);
        crate::platform::open_folder(&d);
    }
}

// ------------------------------------------------------------ save state file

/// Header of the frontend state file: "EFST", version (u32 LE),
/// ROM fingerprint (u64 LE) and the core's opaque bytes.
const STATE_MAGIC: &[u8; 4] = b"EFST";
const STATE_VERSION: u32 = 1;

/// ROM fingerprint (FNV-1a 64): prevents loading another game's state.
fn rom_hash(rom: &[u8]) -> u64 {
    rom.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3))
}

fn state_file(rom_hash: u64, core: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(16 + core.len());
    v.extend_from_slice(STATE_MAGIC);
    v.extend_from_slice(&STATE_VERSION.to_le_bytes());
    v.extend_from_slice(&rom_hash.to_le_bytes());
    v.extend_from_slice(core);
    v
}

/// Core bytes inside the file. Old files (without the header) go
/// straight to the core, which recognizes its own format.
fn state_payload(b: &[u8], rom_hash: u64) -> Result<&[u8], String> {
    if b.len() < 16 || &b[..4] != STATE_MAGIC {
        return Ok(b);
    }
    if u32::from_le_bytes(b[4..8].try_into().unwrap()) != STATE_VERSION {
        return Err(tr("Incompatible save state version"));
    }
    if u64::from_le_bytes(b[8..16].try_into().unwrap()) != rom_hash {
        return Err(tr("This save state belongs to another game"));
    }
    Ok(&b[16..])
}

/// Frame pixels without the extra space of each row (pitch = width).
fn packed(v: &emu_api::Frame) -> Vec<u32> {
    if v.pitch == v.width {
        return v.pixels[..v.width * v.height].to_vec();
    }
    (0..v.height).flat_map(|y| v.pixels[y * v.pitch..y * v.pitch + v.width].iter().copied()).collect()
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn state_header() {
        let f = state_file(42, b"abc");
        assert_eq!(state_payload(&f, 42).unwrap(), b"abc");
        assert!(state_payload(&f, 43).is_err());
        assert_eq!(state_payload(b"PVST old.......xx", 7).unwrap(), b"PVST old.......xx");
    }
}

/// File name without extension.
fn stem(p: &Path) -> String {
    p.file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

/// Name to display: the info sheet title, otherwise the file name.
fn display(p: &Path, id: &Ident) -> String {
    id.title.clone().unwrap_or_else(|| stem(p))
}

/// Game cover for the "Game cover" frame: reduced, blurred and darkened; drawn
/// enlarged with smoothing, it becomes a soft background.
fn blurred_cover(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?.to_rgb8();
    let w = 32;
    let h = (img.height() * w / img.width().max(1)).max(1);
    let small = image::imageops::blur(&image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle), 1.5);
    let rgb: Vec<u8> = small.as_raw().iter().map(|&c| (c as f32 * 0.4) as u8).collect();
    let ci = egui::ColorImage::from_rgb([w as usize, h as usize], &rgb);
    Some(ctx.load_texture("cover_bg", ci, egui::TextureOptions::LINEAR))
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn blurred_cover_is_small_and_dark() {
        let dir = std::env::temp_dir().join(format!("emu_frontend_cover_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("cover.png");
        image::RgbImage::from_pixel(140, 213, image::Rgb([200, 100, 50])).save(&p).unwrap();
        let ctx = egui::Context::default();
        let t = blurred_cover(&ctx, &p).expect("texture");
        assert_eq!(t.size(), [32, 48]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
