//! Screenshots for the documentation: with `EMU_DOCSHOTS=<folder>` and
//! `EMU_DOCSHOTS_GAME=<rom>` (and optionally `EMU_DOCSHOTS_SEL=<n>`, the cover selected in
//! couch mode, counting from 0), the app walks through a fixed list of screens by itself
//! (library, game with each frame, info sheet, folders, debugger, cheats), saves the main window as
//! a PNG for each and closes. Only the app's own window is captured; the debugger and
//! cheats contents are drawn in it, since eframe can't capture separate windows.
//! Run it with a throwaway `XDG_CONFIG_HOME`/`XDG_DATA_HOME`, since it changes settings.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{ViewportCommand, ViewportId};
use emu_api::PadButton;

use super::{App, Dialog};

/// The screens, in order: (PNG name, what to set up).
const SCENES: [(&str, Scene); 14] = [
    ("library-covers", Scene::Library(crate::covers::Art::Boxart)),
    ("library-screens", Scene::Library(crate::covers::Art::Snap)),
    ("library-list", Scene::List),
    ("couch-library", Scene::Couch(false)),
    ("game-tv", Scene::Game("tv")),
    ("couch-menu", Scene::Couch(true)),
    ("game-info", Scene::Info),
    ("folders", Scene::Folders),
    ("debugger-memory", Scene::Debugger(0)),
    ("debugger-tiles", Scene::Debugger(1)),
    ("debugger-cpu", Scene::Debugger(2)),
    ("debugger-video", Scene::Debugger(3)),
    ("cheats", Scene::Cheats(false)),
    ("cheats-search", Scene::Cheats(true)),
];

#[derive(Clone, Copy)]
enum Scene {
    Library(crate::covers::Art),
    /// The library as a list.
    List,
    Game(&'static str),
    Info,
    /// Emulation › Folders…
    Folders,
    /// The debugger on a tab (0 = memory, then the core's views).
    Debugger(usize),
    /// The cheats on a tab (true = the RAM search).
    Cheats(bool),
    /// Couch mode (without going fullscreen): the library with a cover selected, or the
    /// quick menu over the game, with no frame (true).
    Couch(bool),
}

/// Frames emulated before the game screenshots: Start, then playing with fire held.
const PLAY_FRAMES: u32 = 1500;
/// Passes to wait after setting a screen up (covers arrive, windows open and settle).
const SETTLE: u32 = 90;

pub(super) struct DocShots {
    dir: PathBuf,
    game: PathBuf,
    scene: usize,
    wait: u32,
    /// Screenshot requested: set once it is saved.
    shooting: Option<Arc<AtomicBool>>,
}

impl DocShots {
    pub(super) fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("EMU_DOCSHOTS")?);
        let game = PathBuf::from(std::env::var_os("EMU_DOCSHOTS_GAME")?);
        std::fs::create_dir_all(&dir).ok()?;
        Some(Self { dir, game, scene: 0, wait: 0, shooting: None })
    }
}

impl App {
    /// One step of the screenshot tour; called at the end of every pass.
    pub(super) fn docshots_step(&mut self, ctx: &egui::Context) {
        let Some(mut ds) = self.docshots.take() else { return };
        ctx.request_repaint();
        let Some(&(name, scene)) = SCENES.get(ds.scene) else {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        };
        let window = ViewportId::ROOT;
        if let Some(done) = &ds.shooting {
            if done.load(Ordering::Acquire) {
                ds.shooting = None;
                ds.scene += 1;
                ds.wait = 0;
            }
        } else if ds.wait == 0 {
            self.docshots_setup(ctx, scene, &ds.game);
            ds.wait = 1;
        } else if ds.wait < SETTLE {
            ds.wait += 1;
        } else {
            let done = Arc::new(AtomicBool::new(false));
            let (flag, path, repaint) = (done.clone(), ds.dir.join(format!("{name}.png")), ctx.clone());
            let save = egui::ScreenshotCallback::new(move |img| {
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                match image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba).map(|i| i.save(&path)) {
                    Some(Ok(())) => println!("{}", path.display()),
                    _ => eprintln!("{}: could not save", path.display()),
                }
                flag.store(true, Ordering::Release);
                repaint.request_repaint();
            });
            ctx.send_viewport_cmd_to(window, ViewportCommand::Screenshot(save));
            ds.shooting = Some(done);
        }
        self.docshots = Some(ds);
    }

    fn docshots_setup(&mut self, ctx: &egui::Context, scene: Scene, game: &std::path::Path) {
        self.pause_unfocused = false;
        self.couch = false;
        self.nav.quick = None;
        self.library_list = false;
        match scene {
            Scene::Library(art) => self.library_art = art,
            Scene::List => self.library_list = true,
            Scene::Couch(menu) => {
                self.couch = true;
                self.library_art = crate::covers::Art::Boxart;
                self.nav.active = true;
                self.nav.sel = std::env::var("EMU_DOCSHOTS_SEL").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
                self.nav.moved = true;
                if menu {
                    self.frame = "none".into();
                    self.nav.quick = Some(1);
                }
            }
            Scene::Game(frame) => {
                if !self.loaded {
                    self.load_game(game);
                    let (start, fire) = (PadButton::Start.bit(), PadButton::B.bit() | PadButton::Right.bit());
                    for f in 0..PLAY_FRAMES {
                        let pad = if (120..130).contains(&f) { start } else if f >= 200 && (f / 20) % 2 == 1 { fire } else { 0 };
                        let mut input = vec![0; self.sys.players];
                        input[0] = pad;
                        self.core.run_frame(&input);
                        self.core.take_audio(&mut Vec::new());
                    }
                    self.upload(ctx);
                }
                self.frame = frame.into();
            }
            Scene::Info => {
                self.frame = "tv".into();
                if let Some(g) = self.cur_game.clone() {
                    self.open_info(&g);
                }
            }
            Scene::Folders => self.dialog = Dialog::Folders { move_files: true },
            // separate windows can't be captured: their contents fill the main window
            Scene::Debugger(tab) => {
                self.dialog = Dialog::None;
                self.debugger.tab = tab;
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(760.0, 620.0)));
            }
            Scene::Cheats(search) => {
                self.cheats.searching = search;
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(720.0, 560.0)));
            }
        }
    }

    /// Draws the debugger or cheats contents in the main window when the tour is on one of
    /// them; returns whether it did.
    pub(super) fn docshots_embed(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(ds) = &self.docshots else { return false };
        if ds.wait == 0 && ds.shooting.is_none() {
            return false;
        }
        let mut core: Option<&mut dyn emu_api::Core> = Some(self.core.as_mut());
        match SCENES.get(ds.scene).map(|s| s.1) {
            Some(Scene::Debugger(_)) => self.debugger.body(ui, &self.look, self.sys, &mut core, self.paused, &mut Default::default()),
            Some(Scene::Cheats(_)) => self.cheats.body(ui, &self.look, self.sys, &mut core),
            _ => return false,
        }
        true
    }
}
