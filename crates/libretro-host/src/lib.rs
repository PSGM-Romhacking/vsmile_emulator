//! Libretro host: loads a libretro core (a shared library) and presents it as an
//! `emu_api::Core`, so `emu-frontend` plays it without a per-system adapter.
//!
//! ```ignore
//! let info = libretro_host::open(Path::new("core_libretro.so"), HostConfig::default())?;
//! // `info` is the `&'static SystemInfo` for `emu_frontend::app::App::new(cc, info, ...)`
//! ```
//!
//! Limits (v1):
//! - one core per process (`SystemInfo::create` is a plain `fn`, so the core lives in a global);
//! - software rendering only (no `SET_HW_RENDER`); RetroPad only (no mouse, keyboard, analog,
//!   light gun); no disc control, subsystems, achievements or cheats;
//! - content is passed as bytes, or as one temporary file for `need_fullpath` cores, so
//!   multi-file content (cue/bin, m3u) does not work;
//! - the buttons declared to the frontend are the 16 RetroPad buttons, whatever the core uses;
//! - until a game is loaded the resolution, aspect ratio and frame rate are defaults
//!   (libretro only reports them after `retro_load_game`); the real ones come with the frames.
//!
//! libretro callbacks have no context pointer, so the state they touch is global
//! (`CTX`). Every callback catches panics: none may unwind into C.

mod convert;
mod env;

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock};

use emu_api::{Button, Core, CoreOption, Frame, GameInfo, PadButton, SystemInfo};

pub use convert::{PixelFormat, Resampler};

const API_VERSION: c_uint = 1;
const DEVICE_JOYPAD: c_uint = 1;
const ID_JOYPAD_MASK: c_uint = 256;
const MEMORY_SYSTEM_RAM: c_uint = 2;

// ---- libretro.h structs ------------------------------------------------------------

#[repr(C)]
struct RetroSystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct RetroGameGeometry {
    pub base_width: c_uint,
    pub base_height: c_uint,
    pub max_width: c_uint,
    pub max_height: c_uint,
    pub aspect_ratio: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct RetroSystemTiming {
    pub fps: f64,
    pub sample_rate: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct RetroSystemAvInfo {
    pub geometry: RetroGameGeometry,
    pub timing: RetroSystemTiming,
}

#[repr(C)]
struct RetroGameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}

type EnvironmentFn = unsafe extern "C" fn(c_uint, *mut c_void) -> bool;
type VideoRefreshFn = unsafe extern "C" fn(*const c_void, c_uint, c_uint, usize);
type AudioSampleFn = unsafe extern "C" fn(i16, i16);
type AudioSampleBatchFn = unsafe extern "C" fn(*const i16, usize) -> usize;
type InputPollFn = unsafe extern "C" fn();
type InputStateFn = unsafe extern "C" fn(c_uint, c_uint, c_uint, c_uint) -> i16;

/// The core's exported functions, resolved once.
struct Api {
    set_environment: unsafe extern "C" fn(EnvironmentFn),
    set_video_refresh: unsafe extern "C" fn(VideoRefreshFn),
    set_audio_sample: unsafe extern "C" fn(AudioSampleFn),
    set_audio_sample_batch: unsafe extern "C" fn(AudioSampleBatchFn),
    set_input_poll: unsafe extern "C" fn(InputPollFn),
    set_input_state: unsafe extern "C" fn(InputStateFn),
    init: unsafe extern "C" fn(),
    deinit: unsafe extern "C" fn(),
    get_system_av_info: unsafe extern "C" fn(*mut RetroSystemAvInfo),
    reset: unsafe extern "C" fn(),
    run: unsafe extern "C" fn(),
    serialize_size: unsafe extern "C" fn() -> usize,
    serialize: unsafe extern "C" fn(*mut c_void, usize) -> bool,
    unserialize: unsafe extern "C" fn(*const c_void, usize) -> bool,
    load_game: unsafe extern "C" fn(*const RetroGameInfo) -> bool,
    unload_game: unsafe extern "C" fn(),
    get_memory_data: unsafe extern "C" fn(c_uint) -> *mut c_void,
    get_memory_size: unsafe extern "C" fn(c_uint) -> usize,
    need_fullpath: bool,
    first_extension: String,
}

static API: OnceLock<Api> = OnceLock::new();
static SYSTEM: OnceLock<SystemInfo> = OnceLock::new();

fn api() -> &'static Api {
    API.get().expect("libretro_host::open was not called")
}

// ---- state shared with the callbacks -------------------------------------------------

pub(crate) struct Ctx {
    pub pads: Vec<u16>,
    pub format: PixelFormat,
    /// Last frame the core drew (XRGB8888) and whether it is new since `run_frame` took it.
    pub fb: Vec<u32>,
    pub fresh: bool,
    pub width: usize,
    pub height: usize,
    pub aspect: f32,
    /// Interleaved stereo at the core's own rate.
    pub audio: Vec<f32>,
    pub core_rate: f64,
    pub fps: f64,
    pub options: &'static [CoreOption],
    pub values: HashMap<String, CString>,
    pub options_updated: bool,
    pub system_dir: CString,
    pub save_dir: CString,
}

pub(crate) static CTX: LazyLock<Mutex<Ctx>> = LazyLock::new(|| {
    Mutex::new(Ctx {
        pads: Vec::new(),
        format: PixelFormat::Rgb1555,
        fb: Vec::new(),
        fresh: false,
        width: 0,
        height: 0,
        aspect: 0.0,
        audio: Vec::new(),
        core_rate: 48000.0,
        fps: 60.0,
        options: &[],
        values: HashMap::new(),
        options_updated: false,
        system_dir: CString::default(),
        save_dir: CString::default(),
    })
});

pub(crate) fn ctx() -> MutexGuard<'static, Ctx> {
    CTX.lock().unwrap_or_else(|e| e.into_inner())
}

// ---- opening a core --------------------------------------------------------------

/// Where the core may keep its BIOS files and saves, and the libretro-thumbnails folder
/// for the covers ("Sony - PlayStation"; empty = no covers).
pub struct HostConfig {
    pub system_dir: PathBuf,
    pub save_dir: PathBuf,
    pub thumbnails: String,
}

impl Default for HostConfig {
    fn default() -> Self {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(std::env::temp_dir)
            .join("libretro-host");
        Self { system_dir: base.join("system"), save_dir: base.join("saves"), thumbnails: String::new() }
    }
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

unsafe fn text(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

const BUTTONS: &[Button] = &[
    Button { id: PadButton::Up, label: "Up" },
    Button { id: PadButton::Down, label: "Down" },
    Button { id: PadButton::Left, label: "Left" },
    Button { id: PadButton::Right, label: "Right" },
    Button { id: PadButton::B, label: "B" },
    Button { id: PadButton::A, label: "A" },
    Button { id: PadButton::Y, label: "Y" },
    Button { id: PadButton::X, label: "X" },
    Button { id: PadButton::L, label: "L" },
    Button { id: PadButton::R, label: "R" },
    Button { id: PadButton::L2, label: "L2" },
    Button { id: PadButton::R2, label: "R2" },
    Button { id: PadButton::L3, label: "L3" },
    Button { id: PadButton::R3, label: "R3" },
    Button { id: PadButton::Start, label: "Start" },
    Button { id: PadButton::Select, label: "Select" },
];

fn no_game_info(_: &[u8]) -> Option<&'static GameInfo> {
    None
}

/// Loads the core's library and describes the system. Call it once, before the frontend.
pub fn open(path: &Path, cfg: HostConfig) -> Result<&'static SystemInfo, String> {
    if let Some(s) = SYSTEM.get() {
        return Ok(s);
    }
    let lib = unsafe { libloading::Library::new(path) }.map_err(|e| format!("{}: {e}", path.display()))?;
    macro_rules! sym {
        ($name:literal) => {
            *unsafe { lib.get(concat!($name, "\0").as_bytes()) }.map_err(|e| format!("{}: missing {}: {e}", path.display(), $name))?
        };
    }
    let version: unsafe extern "C" fn() -> c_uint = sym!("retro_api_version");
    let version = unsafe { version() };
    if version != API_VERSION {
        return Err(format!("{}: libretro API version {version} (expected {API_VERSION})", path.display()));
    }
    let get_system_info: unsafe extern "C" fn(*mut RetroSystemInfo) = sym!("retro_get_system_info");
    let mut si = RetroSystemInfo { library_name: std::ptr::null(), library_version: std::ptr::null(), valid_extensions: std::ptr::null(), need_fullpath: false, block_extract: false };
    unsafe { get_system_info(&mut si) };
    let name = unsafe { text(si.library_name) };
    let extensions_text = unsafe { text(si.valid_extensions) }.to_lowercase();
    let exts: Vec<&'static str> = extensions_text.split('|').filter(|e| !e.is_empty()).map(|e| leak(e.to_string())).collect();
    let api = Api {
        set_environment: sym!("retro_set_environment"),
        set_video_refresh: sym!("retro_set_video_refresh"),
        set_audio_sample: sym!("retro_set_audio_sample"),
        set_audio_sample_batch: sym!("retro_set_audio_sample_batch"),
        set_input_poll: sym!("retro_set_input_poll"),
        set_input_state: sym!("retro_set_input_state"),
        init: sym!("retro_init"),
        deinit: sym!("retro_deinit"),
        get_system_av_info: sym!("retro_get_system_av_info"),
        reset: sym!("retro_reset"),
        run: sym!("retro_run"),
        serialize_size: sym!("retro_serialize_size"),
        serialize: sym!("retro_serialize"),
        unserialize: sym!("retro_unserialize"),
        load_game: sym!("retro_load_game"),
        unload_game: sym!("retro_unload_game"),
        get_memory_data: sym!("retro_get_memory_data"),
        get_memory_size: sym!("retro_get_memory_size"),
        need_fullpath: si.need_fullpath,
        first_extension: exts.first().map_or("bin", |e| *e).to_string(),
    };
    std::mem::forget(lib); // the core stays loaded for the whole run

    std::fs::create_dir_all(&cfg.system_dir).ok();
    std::fs::create_dir_all(&cfg.save_dir).ok();
    {
        let mut c = ctx();
        c.system_dir = CString::new(cfg.system_dir.to_string_lossy().into_owned()).unwrap_or_default();
        c.save_dir = CString::new(cfg.save_dir.to_string_lossy().into_owned()).unwrap_or_default();
    }
    let _ = API.set(api);

    let exts: &'static [&'static str] = Box::leak(exts.into_boxed_slice());
    let info = SystemInfo {
        name: leak(name.clone()),
        short_name: leak(name),
        extensions: exts,
        thumbnails: leak(cfg.thumbnails),
        cover_aspect: 0.66,
        players: 2,
        buttons: BUTTONS,
        // libretro only tells the real values after a game is loaded
        width: 320,
        height: 240,
        aspect: 4.0 / 3.0,
        ram_address: 0,
        fps_num: 60,
        fps_den: 1,
        game_info: no_game_info,
        game_texts: &[],
        bios: &[],
        multi_file_roms: false,
        create: |rate| Box::new(HostCore::new(rate)),
    };
    Ok(SYSTEM.get_or_init(|| info))
}

// ---- the core as an emu_api::Core ----------------------------------------------------

/// A loaded libretro core. `retro_init` runs at `load` (the frontend creates a new one for
/// every game and drops the old one afterwards) and `retro_deinit` when it is dropped.
pub struct HostCore {
    sample_rate: u32,
    fb: Vec<u32>,
    width: usize,
    height: usize,
    aspect: f32,
    fps: f64,
    resampler: Resampler,
    initialized: bool,
    loaded: bool,
    temp_file: Option<PathBuf>,
}

impl HostCore {
    fn new(sample_rate: u32) -> Self {
        Self { sample_rate, fb: vec![0; 320 * 240], width: 320, height: 240, aspect: 4.0 / 3.0, fps: 60.0, resampler: Resampler::default(), initialized: false, loaded: false, temp_file: None }
    }

    fn shutdown(&mut self) {
        let api = api();
        if self.loaded {
            unsafe { (api.unload_game)() };
            self.loaded = false;
        }
        if self.initialized {
            unsafe { (api.deinit)() };
            self.initialized = false;
        }
        if let Some(f) = self.temp_file.take() {
            let _ = std::fs::remove_file(f);
        }
    }
}

impl Drop for HostCore {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Core for HostCore {
    fn info(&self) -> &'static SystemInfo {
        SYSTEM.get().expect("libretro_host::open was not called")
    }

    fn fps(&self) -> (u64, u64) {
        const DEN: u64 = 1_000_000;
        ((self.fps * DEN as f64).round().max(1.0) as u64, DEN)
    }

    fn load(&mut self, rom: &[u8]) -> Result<(), String> {
        self.shutdown();
        let api = api();
        {
            let mut c = ctx();
            c.audio.clear();
            c.fb.clear();
            c.fresh = false;
            c.options_updated = true;
        }
        unsafe {
            (api.set_environment)(env::environment);
            (api.set_video_refresh)(video_refresh);
            (api.set_audio_sample)(audio_sample);
            (api.set_audio_sample_batch)(audio_sample_batch);
            (api.set_input_poll)(input_poll);
            (api.set_input_state)(input_state);
            (api.init)();
        }
        self.initialized = true;

        let path = if api.need_fullpath {
            let p = std::env::temp_dir().join(format!("libretro-host-{}.{}", std::process::id(), api.first_extension));
            std::fs::write(&p, rom).map_err(|e| format!("{}: {e}", p.display()))?;
            self.temp_file = Some(p.clone());
            Some(CString::new(p.to_string_lossy().into_owned()).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let game = RetroGameInfo {
            path: path.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
            data: if api.need_fullpath { std::ptr::null() } else { rom.as_ptr().cast() },
            size: rom.len(),
            meta: std::ptr::null(),
        };
        if !unsafe { (api.load_game)(&game) } {
            self.shutdown();
            return Err("the core rejected this content".into());
        }
        self.loaded = true;

        let mut av = RetroSystemAvInfo::default();
        unsafe { (api.get_system_av_info)(&mut av) };
        let mut c = ctx();
        env::apply_av_info(&mut c, &av);
        self.fps = c.fps;
        self.width = c.width.max(1);
        self.height = c.height.max(1);
        self.aspect = c.aspect;
        self.resampler = Resampler::new(c.core_rate, self.sample_rate as f64);
        Ok(())
    }

    fn reset(&mut self) {
        if self.loaded {
            unsafe { (api().reset)() };
        }
    }

    fn run_frame(&mut self, pads: &[u16]) {
        if !self.loaded {
            return;
        }
        ctx().pads = pads.to_vec();
        unsafe { (api().run)() };
        let mut c = ctx();
        self.fps = c.fps;
        self.aspect = c.aspect;
        if c.fresh {
            c.fresh = false;
            self.width = c.width;
            self.height = c.height;
            self.fb.clone_from(&c.fb);
        }
    }

    fn video(&self) -> Frame<'_> {
        Frame { pixels: &self.fb, width: self.width, height: self.height, pitch: self.width, aspect: self.aspect }
    }

    fn take_audio(&mut self, out: &mut Vec<f32>) {
        let input = std::mem::take(&mut ctx().audio);
        self.resampler.process(&input, out);
    }

    fn save_state(&self) -> Vec<u8> {
        if !self.loaded {
            return Vec::new();
        }
        let api = api();
        let size = unsafe { (api.serialize_size)() };
        let mut data = vec![0u8; size];
        if size == 0 || !unsafe { (api.serialize)(data.as_mut_ptr().cast(), size) } {
            return Vec::new();
        }
        data
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        if !self.loaded {
            return Err("no game loaded".into());
        }
        if unsafe { (api().unserialize)(data.as_ptr().cast(), data.len()) } { Ok(()) } else { Err("the core rejected this save state".into()) }
    }

    fn system_ram(&self) -> &[u8] {
        if !self.loaded {
            return &[];
        }
        let api = api();
        let (p, n) = unsafe { ((api.get_memory_data)(MEMORY_SYSTEM_RAM), (api.get_memory_size)(MEMORY_SYSTEM_RAM)) };
        if p.is_null() { &[] } else { unsafe { std::slice::from_raw_parts(p.cast(), n) } }
    }

    fn system_ram_mut(&mut self) -> &mut [u8] {
        if !self.loaded {
            return &mut [];
        }
        let api = api();
        let (p, n) = unsafe { ((api.get_memory_data)(MEMORY_SYSTEM_RAM), (api.get_memory_size)(MEMORY_SYSTEM_RAM)) };
        if p.is_null() { &mut [] } else { unsafe { std::slice::from_raw_parts_mut(p.cast(), n) } }
    }

    fn options(&self) -> &'static [CoreOption] {
        ctx().options
    }

    fn set_option(&mut self, key: &str, value: &str) {
        let mut c = ctx();
        if c.options.iter().any(|o| o.key == key && o.values.iter().any(|(v, _)| *v == value))
            && let Ok(v) = CString::new(value) {
                c.values.insert(key.to_string(), v);
                c.options_updated = true;
            }
    }
}

// ---- callbacks ----------------------------------------------------------------------

use std::panic::catch_unwind;

unsafe extern "C" fn video_refresh(data: *const c_void, width: c_uint, height: c_uint, pitch: usize) {
    let _ = catch_unwind(|| {
        if data.is_null() {
            return; // the core asks to repeat the previous frame
        }
        let mut c = ctx();
        let (w, h) = (width as usize, height as usize);
        let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), pitch * h) };
        let format = c.format;
        convert::to_xrgb8888(format, bytes, w, h, pitch, &mut c.fb);
        c.width = w;
        c.height = h;
        c.fresh = true;
    });
}

unsafe extern "C" fn audio_sample(left: i16, right: i16) {
    let _ = catch_unwind(|| {
        ctx().audio.extend_from_slice(&[left as f32 / 32768.0, right as f32 / 32768.0]);
    });
}

unsafe extern "C" fn audio_sample_batch(data: *const i16, frames: usize) -> usize {
    catch_unwind(|| {
        if !data.is_null() {
            let samples = unsafe { std::slice::from_raw_parts(data, frames * 2) };
            ctx().audio.extend(samples.iter().map(|&s| s as f32 / 32768.0));
        }
        frames
    })
    .unwrap_or(frames)
}

unsafe extern "C" fn input_poll() {}

unsafe extern "C" fn input_state(port: c_uint, device: c_uint, _index: c_uint, id: c_uint) -> i16 {
    catch_unwind(|| {
        if device & 0xFF != DEVICE_JOYPAD {
            return 0;
        }
        let pad = ctx().pads.get(port as usize).copied().unwrap_or(0);
        match id {
            ID_JOYPAD_MASK => pad as i16,
            0..=15 => ((pad >> id) & 1) as i16,
            _ => 0,
        }
    })
    .unwrap_or(0)
}

#[allow(dead_code)]
fn _assert_types(_: c_int) {}
