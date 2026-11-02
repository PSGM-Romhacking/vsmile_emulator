//! Gamepads via the Linux joystick API (`/dev/input/jsX`), on a dedicated thread;
//! on FreeBSD via evdev (`gamepad/evdev.rs`); on other systems
//! via `gilrs`. The last two present the controllers as the first does, so the same
//! mappings and defaults apply. The thread only publishes the raw state — buttons and axes of
//! each controller — and the "presses" (for the settings screen). Which button becomes
//! which RetroPad button or action is decided by a [`Mapping`], configurable per controller.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use emu_api::{PadButton, SystemInfo};

/// Axis displacement, from rest, to count as pressed.
const AXIS_ON: i32 = 16000;

/// State of a connected controller.
#[derive(Clone, Default)]
pub struct PadInfo {
    /// Name reported by the driver (e.g. "Flydigi Vader 3 Pro").
    pub name: String,
    pub buttons: Vec<bool>,
    pub axes: Vec<i16>,
    /// Rest value of each axis (analog triggers rest at -32767).
    pub rest: Vec<i16>,
    /// Kernel code of each button (BTN_SOUTH…), when the driver reports it.
    pub codes: Vec<u16>,
    /// Kernel code of each axis (ABS_X…), when the reader knows it (evdev); empty
    /// otherwise, and then the d-pad is assumed on axes 6/7, as xpad lays it out.
    pub axis_codes: Vec<u16>,
}

impl PadInfo {
    fn pressed(&self, inp: Input) -> bool {
        match inp {
            Input::Button(n) => self.buttons.get(n as usize).copied().unwrap_or(false),
            Input::Axis(n, pos) => {
                let (Some(&v), Some(&r)) = (self.axes.get(n as usize), self.rest.get(n as usize)) else { return false };
                let d = v as i32 - r as i32;
                if pos { d > AXIS_ON } else { d < -AXIS_ON }
            }
        }
    }
}

/// A physical input: button N, or axis N in one direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Button(u8),
    Axis(u8, bool),
}

impl Input {
    /// Form saved in the config: "b3", "a1+", "a7-".
    pub fn to_text(self) -> String {
        match self {
            Input::Button(n) => format!("b{n}"),
            Input::Axis(n, p) => format!("a{n}{}", if p { '+' } else { '-' }),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        if let Some(n) = s.strip_prefix('b') {
            return n.parse().ok().map(Input::Button);
        }
        let a = s.strip_prefix('a')?;
        let (n, sign) = a.split_at(a.len().checked_sub(1)?);
        Some(Input::Axis(n.parse().ok()?, match sign { "+" => true, "-" => false, _ => return None }))
    }

    /// Text for the UI.
    pub fn label(self) -> String {
        use crate::i18n::trf;
        match self {
            Input::Button(n) => trf("Button {n}", &[("n", &n)]),
            Input::Axis(n, p) => format!("{} {}", trf("Axis {n}", &[("n", &n)]), if p { "+" } else { "−" }),
        }
    }
}

/// Remappable actions: each player's RetroPad buttons (the ones the system
/// uses) and the emulator commands. Each can have keys (global) and
/// controller inputs (per controller).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Func {
    /// A player's RetroPad button (0 = player 1).
    Pad(u8, PadButton),
    Pause,
    /// Opens and closes the quick menu (app/nav.rs); Home/Guide by default.
    QuickMenu,
    Reset,
    SaveState,
    LoadState,
    SlotPrev,
    SlotNext,
    FastForward,
    FastForwardToggle,
    Rewind,
    Mute,
    VolumeUp,
    VolumeDown,
    Fullscreen,
    Screenshot,
    RecordGif,
    RecordVideo,
    OpenCaptures,
    OpenRecordings,
    /// Reads the ROM folder again (files added or removed while the app is open).
    RefreshGames,
}

impl Func {
    /// Emulator commands (the same for any system).
    pub const EMU: [Func; 20] = [
        Func::Pause,
        Func::QuickMenu,
        Func::Reset,
        Func::SaveState,
        Func::LoadState,
        Func::SlotPrev,
        Func::SlotNext,
        Func::FastForward,
        Func::FastForwardToggle,
        Func::Rewind,
        Func::Mute,
        Func::VolumeUp,
        Func::VolumeDown,
        Func::Fullscreen,
        Func::Screenshot,
        Func::RecordGif,
        Func::RecordVideo,
        Func::OpenCaptures,
        Func::OpenRecordings,
        Func::RefreshGames,
    ];

    /// All of the system's actions: each player's buttons + emulator commands.
    pub fn all(sys: &SystemInfo) -> Vec<Func> {
        (0..sys.players as u8).flat_map(|p| sys.buttons.iter().map(move |b| Func::Pad(p, b.id))).chain(Self::EMU).collect()
    }

    /// Player who owns the button (1, 2…); `None` = emulator command.
    pub fn player(self) -> Option<u8> {
        match self {
            Func::Pad(p, _) => Some(p + 1),
            _ => None,
        }
    }

    pub fn is_game(self) -> bool {
        self.player().is_some()
    }

    /// Actions active while held (the others fire on press).
    pub fn is_hold(self) -> bool {
        self.is_game() || matches!(self, Func::FastForward | Func::Rewind)
    }

    /// Stable name in the config ("p1_b", "save_state"…).
    pub fn key(self) -> String {
        match self {
            Func::Pad(p, b) => format!("p{}_{}", p + 1, b.key()),
            f => f.emu_key().to_string(),
        }
    }

    fn emu_key(self) -> &'static str {
        match self {
            Func::Pad(..) => "",
            Func::Pause => "pause",
            Func::QuickMenu => "quick_menu",
            Func::Reset => "reset",
            Func::SaveState => "save_state",
            Func::LoadState => "load_state",
            Func::SlotPrev => "slot_prev",
            Func::SlotNext => "slot_next",
            Func::FastForward => "fastforward",
            Func::FastForwardToggle => "fastforward_toggle",
            Func::Rewind => "rewind",
            Func::Mute => "mute",
            Func::VolumeUp => "volume_up",
            Func::VolumeDown => "volume_down",
            Func::Fullscreen => "fullscreen",
            Func::Screenshot => "screenshot",
            Func::RecordGif => "record_gif",
            Func::RecordVideo => "record_video",
            Func::OpenCaptures => "open_captures",
            Func::OpenRecordings => "open_recordings",
            Func::RefreshGames => "refresh_games",
        }
    }

    /// Source text (translated with `tr`): the name the system gives the button, or the command's.
    pub fn label(self, sys: &SystemInfo) -> &'static str {
        match self {
            Func::Pad(_, b) => sys.buttons.iter().find(|x| x.id == b).map_or(b.key(), |x| x.label),
            f => f.emu_label(),
        }
    }

    pub fn emu_label(self) -> &'static str {
        match self {
            Func::Pad(..) => "",
            Func::Pause => "Pause",
            Func::QuickMenu => "Quick menu",
            Func::Reset => "Reset",
            Func::SaveState => "Save state",
            Func::LoadState => "Load state",
            Func::SlotPrev => "Previous slot",
            Func::SlotNext => "Next slot",
            Func::FastForward => "Fast forward (hold)",
            Func::FastForwardToggle => "Fast forward (toggle)",
            Func::Rewind => "Rewind (hold)",
            Func::Mute => "Sound on/off",
            Func::VolumeUp => "Volume +",
            Func::VolumeDown => "Volume −",
            Func::Fullscreen => "Fullscreen",
            Func::Screenshot => "Screenshot",
            Func::RecordGif => "Record GIF",
            Func::RecordVideo => "Record video (MP4)",
            Func::OpenCaptures => "Open screenshots folder",
            Func::OpenRecordings => "Open recordings folder",
            Func::RefreshGames => "Refresh game list",
        }
    }
}

/// Function → inputs that trigger it (any of them works).
pub type Mapping = BTreeMap<Func, Vec<Input>>;

/// Default map, positional like RetroArch's: bottom button = B, right = A,
/// left = Y, top = X, L/R/L2/R2/L3/R3, Select and Start — by the semantic
/// code the driver reports (BTN_SOUTH…), never by fixed index. Directions
/// from the left stick (axes 0/1) and the d-pad (axes 6/7). Only the
/// buttons the system uses are included; free L2/R2 become fast forward/rewind.
pub fn default_mapping(info: &PadInfo, sys: &SystemInfo) -> Mapping {
    use PadButton as P;
    let mut m = Mapping::new();
    let uses = |b: P| sys.buttons.iter().any(|x| x.id == b);
    let by_code = |code: u16| info.codes.iter().position(|&c| c == code).map(|i| Input::Button(i as u8));
    let known = !info.codes.is_empty() && info.codes.iter().any(|&c| (0x130..=0x13E).contains(&c));
    let exists = |i: &Input| matches!(i, Input::Button(n) if (*n as usize) < info.buttons.len());
    // (button, kernel code, index in the Xbox layout without codes)
    for (b, code, fallback) in [
        (P::B, 0x130, 0),      // BTN_SOUTH
        (P::A, 0x131, 1),      // BTN_EAST
        (P::Y, 0x134, 2),      // BTN_WEST
        (P::X, 0x133, 3),      // BTN_NORTH
        (P::L, 0x136, 4),      // BTN_TL
        (P::R, 0x137, 5),      // BTN_TR
        (P::Select, 0x13A, 6), // BTN_SELECT
        (P::Start, 0x13B, 7),  // BTN_START
        (P::L2, 0x138, 99),    // BTN_TL2
        (P::R2, 0x139, 99),    // BTN_TR2
        (P::L3, 0x13D, 9),     // BTN_THUMBL
        (P::R3, 0x13E, 10),    // BTN_THUMBR
    ] {
        if !uses(b) {
            continue;
        }
        let inp = if known { by_code(code) } else { Some(Input::Button(fallback)) };
        if let Some(i) = inp.filter(exists) {
            m.insert(Func::Pad(0, b), vec![i]);
        }
    }
    // Home/Guide (BTN_MODE): the quick menu, the couch-friendly way into everything
    let guide = if known { by_code(0x13C) } else { Some(Input::Button(8)) };
    if let Some(i) = guide.filter(exists) {
        m.insert(Func::QuickMenu, vec![i]);
    }
    // fast forward/rewind: R2/L2 if the system doesn't use them — button (DInput) or
    // analog trigger (XInput, axes 5 and 2, which rest at -32767)
    for (f, b, code, axis) in [(Func::FastForward, P::R2, 0x139, 5), (Func::Rewind, P::L2, 0x138, 2)] {
        if uses(b) {
            continue;
        }
        let trigger = info.rest.get(axis).is_some_and(|&r| r < -16000);
        if let Some(i) = by_code(code).or(trigger.then_some(Input::Axis(axis as u8, true))) {
            m.insert(f, vec![i]);
        }
    }
    // the d-pad: ABS_HAT0X/ABS_HAT0Y wherever they are, else axes 6/7
    let axis_of = |code: u16, default: usize| if info.axis_codes.is_empty() { Some(default) } else { info.axis_codes.iter().position(|&c| c == code) };
    let (hx, hy) = (axis_of(0x10, 6).unwrap_or(usize::MAX), axis_of(0x11, 7).unwrap_or(usize::MAX));
    for (b, axes) in [(P::Left, [(0, false), (hx, false)]), (P::Right, [(0, true), (hx, true)]), (P::Up, [(1, false), (hy, false)]), (P::Down, [(1, true), (hy, true)])] {
        let v: Vec<Input> = axes.iter().filter(|(n, _)| *n < info.axes.len()).map(|&(n, p)| Input::Axis(n as u8, p)).collect();
        if uses(b) && !v.is_empty() {
            m.insert(Func::Pad(0, b), v);
        }
    }
    m
}

pub fn active(m: &Mapping, info: &PadInfo, f: Func) -> bool {
    m.get(&f).is_some_and(|v| v.iter().any(|&i| info.pressed(i)))
}

/// This controller's RetroPad, per player: `out[n]` = buttons of the `Pad(n, …)` actions.
/// (The 1st controller uses player 1's actions as its own; the UI decides the player.)
pub fn bits(m: &Mapping, info: &PadInfo, players: usize) -> Vec<u16> {
    let mut out = vec![0u16; players];
    for (f, v) in m {
        if let Func::Pad(p, b) = *f
            && (p as usize) < players && v.iter().any(|&i| info.pressed(i)) {
                out[p as usize] |= b.bit();
            }
    }
    out
}

/// Saved mapping (text) → [`Mapping`].
/// Mappings saved before the quick menu existed had Home/Guide on Pause: it moves to the
/// quick menu (the new default), and Pause is left for the keyboard.
pub fn mapping_from_cfg(saved: &BTreeMap<String, Vec<String>>, sys: &SystemInfo) -> Mapping {
    let mut m: Mapping = Func::all(sys)
        .into_iter()
        .filter_map(|f| saved.get(&f.key()).map(|v| (f, v.iter().filter_map(|s| Input::parse(s)).collect())))
        .collect();
    if !saved.contains_key(&Func::QuickMenu.key())
        && let Some(v) = m.remove(&Func::Pause)
    {
        m.insert(Func::QuickMenu, v);
    }
    m
}

pub fn mapping_to_cfg(m: &Mapping) -> BTreeMap<String, Vec<String>> {
    m.iter().map(|(f, v)| (f.key(), v.iter().map(|i| i.to_text()).collect())).collect()
}

/// State shared with the UI.
#[derive(Default)]
pub struct Pads {
    /// Connected controllers, in /dev/input order (1st = player 1).
    pub devs: Mutex<Vec<PadInfo>>,
    /// Recent presses (controller name, input), for "Set" in the settings.
    pub presses: Mutex<VecDeque<(String, Input)>>,
    /// Some input device could not be opened for lack of permission (FreeBSD's
    /// /dev/input is root-only by default): the UI says so when no controller shows up.
    pub no_access: std::sync::atomic::AtomicBool,
}

impl Pads {
    pub fn snapshot(&self) -> Vec<PadInfo> {
        self.devs.lock().map(|d| d.clone()).unwrap_or_default()
    }

    pub fn take_presses(&self) -> Vec<(String, Input)> {
        self.presses.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default()
    }
}

pub fn start() -> Arc<Pads> {
    let pads = Arc::new(Pads::default());
    let p = pads.clone();
    #[cfg(target_os = "linux")]
    let _ = std::thread::Builder::new().name("gamepad".into()).spawn(move || linux::run(&p));
    #[cfg(target_os = "freebsd")]
    let _ = std::thread::Builder::new().name("gamepad".into()).spawn(move || evdev::run(&p));
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let _ = std::thread::Builder::new().name("gamepad".into()).spawn(move || portable::run(&p));
    pads
}

/// The evdev reader (FreeBSD's) on Linux too, to test it with real controllers
/// (`cargo run --example pads -- --evdev`).
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn start_evdev() -> Arc<Pads> {
    let pads = Arc::new(Pads::default());
    let p = pads.clone();
    let _ = std::thread::Builder::new().name("gamepad".into()).spawn(move || evdev::run(&p));
    pads
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod evdev;

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::ffi::CString;

    const JSIOCGAXES: u64 = 0x8001_6A11;
    const JSIOCGBUTTONS: u64 = 0x8001_6A12;
    const JSIOCGBTNMAP: u64 = 0x8040_6A34;
    const JSIOCGNAME_128: u64 = 0x8080_6A13; // JSIOCGNAME(128)
    const TYPE_BUTTON: u8 = 0x01;
    const TYPE_AXIS: u8 = 0x02;
    const TYPE_INIT: u8 = 0x80;

    struct Dev {
        fd: i32,
        info: PadInfo,
    }

    /// Game controllers only: some BTN_JOYSTICK/BTN_GAMEPAD button (0x120–0x13F) or
    /// BTN_TRIGGER_HAPPY (≥ 0x2C0). Virtual mice from remappers (buttons
    /// 0x110–0x117) also create /dev/input/jsX and would steal player 1's slot.
    /// Without driver codes (all zero), accept.
    fn is_gamepad(codes: &[u16]) -> bool {
        codes.iter().all(|&c| c == 0) || codes.iter().any(|&c| (0x120..=0x13F).contains(&c) || c >= 0x2C0)
    }

    enum Opened {
        Pad(Dev),
        NotPad,
        Fail,
    }

    fn open(path: &str) -> Opened {
        let Ok(c) = CString::new(path) else { return Opened::Fail };
        let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
        if fd < 0 {
            return Opened::Fail; // retry on the next scan
        }
        let (mut nb, mut na) = (0u8, 0u8);
        let mut codes = [0u16; 512];
        let mut name = [0u8; 128];
        unsafe {
            libc::ioctl(fd, JSIOCGBUTTONS as _, &mut nb as *mut u8);
            libc::ioctl(fd, JSIOCGAXES as _, &mut na as *mut u8);
            if libc::ioctl(fd, JSIOCGBTNMAP as _, codes.as_mut_ptr()) < 0 {
                codes = [0; 512];
            }
            libc::ioctl(fd, JSIOCGNAME_128 as _, name.as_mut_ptr());
        }
        let name = String::from_utf8_lossy(&name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())]).trim().to_string();
        let info = PadInfo {
            name: if name.is_empty() { path.to_string() } else { name },
            buttons: vec![false; nb as usize],
            axes: vec![0; na as usize],
            rest: vec![0; na as usize],
            codes: codes[..nb as usize].to_vec(), // zeros if the driver didn't report them
            axis_codes: vec![],
        };
        if !is_gamepad(&info.codes) {
            unsafe { libc::close(fd) };
            return Opened::NotPad;
        }
        Opened::Pad(Dev { fd, info })
    }

    fn js_paths() -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir("/dev/input")
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.starts_with("js"))
                    .map(|n| format!("/dev/input/{n}"))
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Consumes pending events; `false` if the device is gone.
    fn drain(dev: &mut Dev, presses: &mut Vec<(String, Input)>) -> bool {
        let mut buf = [0u8; 8];
        loop {
            let r = unsafe { libc::read(dev.fd, buf.as_mut_ptr() as *mut libc::c_void, 8) };
            if r < 0 {
                return std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock;
            }
            if r < 8 {
                return r != 0;
            }
            // struct js_event { u32 time; s16 value; u8 type; u8 number; }
            let value = i16::from_ne_bytes([buf[4], buf[5]]);
            let init = buf[6] & TYPE_INIT != 0; // initial state on connect
            let etype = buf[6] & !TYPE_INIT;
            let n = buf[7] as usize;
            let i = &mut dev.info;
            if etype == TYPE_BUTTON && n < i.buttons.len() {
                let was = i.buttons[n];
                i.buttons[n] = value != 0;
                if !init && !was && value != 0 {
                    presses.push((i.name.clone(), Input::Button(n as u8)));
                }
            } else if etype == TYPE_AXIS && n < i.axes.len() {
                if init {
                    i.rest[n] = value; // rest (trigger: -32767; stick: ~0)
                }
                let before = i.axes[n] as i32 - i.rest[n] as i32;
                i.axes[n] = value;
                let after = value as i32 - i.rest[n] as i32;
                if !init {
                    if after > AXIS_ON && before <= AXIS_ON {
                        presses.push((i.name.clone(), Input::Axis(n as u8, true)));
                    } else if after < -AXIS_ON && before >= -AXIS_ON {
                        presses.push((i.name.clone(), Input::Axis(n as u8, false)));
                    }
                }
            }
        }
    }

    pub(super) fn run(pads: &Pads) {
        let mut devs: BTreeMap<String, Dev> = BTreeMap::new();
        let mut ignored: std::collections::HashSet<String> = Default::default(); // jsX that aren't controllers
        let mut last_scan = std::time::Instant::now() - std::time::Duration::from_secs(10);
        loop {
            let mut changed = false;
            // hot-plug: rescan /dev/input every 1.5 s
            if last_scan.elapsed().as_millis() >= 1500 {
                last_scan = std::time::Instant::now();
                let paths = js_paths();
                ignored.retain(|p| paths.contains(p));
                for p in &paths {
                    if !devs.contains_key(p) && !ignored.contains(p) {
                        match open(p) {
                            Opened::Pad(d) => {
                                devs.insert(p.clone(), d);
                                changed = true;
                            }
                            Opened::NotPad => {
                                ignored.insert(p.clone());
                            }
                            Opened::Fail => {}
                        }
                    }
                }
                devs.retain(|p, d| {
                    let keep = paths.contains(p);
                    if !keep {
                        unsafe { libc::close(d.fd) };
                        changed = true;
                    }
                    keep
                });
            }
            let mut presses = vec![];
            if !devs.is_empty() {
                let mut pfds: Vec<libc::pollfd> = devs.values().map(|d| libc::pollfd { fd: d.fd, events: libc::POLLIN, revents: 0 }).collect();
                let n = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as _, 100) };
                if n > 0 {
                    let mut dead = vec![];
                    for (pfd, (path, dev)) in pfds.iter().zip(devs.iter_mut()) {
                        if pfd.revents == 0 {
                            continue;
                        }
                        changed = true;
                        let ok = pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) == 0 && drain(dev, &mut presses);
                        if !ok {
                            dead.push(path.clone());
                        }
                    }
                    for p in dead {
                        if let Some(d) = devs.remove(&p) {
                            unsafe { libc::close(d.fd) };
                        }
                    }
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            if changed
                && let Ok(mut d) = pads.devs.lock() {
                    *d = devs.values().map(|d| d.info.clone()).collect();
                }
            if !presses.is_empty()
                && let Ok(mut q) = pads.presses.lock() {
                    q.extend(presses);
                    while q.len() > 64 {
                        q.pop_front();
                    }
                }
        }
    }
}

/// Other systems (Windows, macOS): `gilrs`, presented as the Linux xpad driver lays an
/// Xbox controller out — buttons with their kernel codes, axes 0/1 left stick, 3/4 right
/// stick, 6/7 d-pad — so [`default_mapping`] and saved mappings work the same way.
#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
mod portable {
    use super::*;
    use gilrs::{Axis, Button, Gilrs};

    /// Buttons in index order, with the Linux code of each (BTN_SOUTH…).
    const BUTTONS: [(Button, u16); 13] = [
        (Button::South, 0x130),
        (Button::East, 0x131),
        (Button::North, 0x133),
        (Button::West, 0x134),
        (Button::LeftTrigger, 0x136),
        (Button::RightTrigger, 0x137),
        (Button::LeftTrigger2, 0x138),
        (Button::RightTrigger2, 0x139),
        (Button::Select, 0x13A),
        (Button::Start, 0x13B),
        (Button::Mode, 0x13C),
        (Button::LeftThumb, 0x13D),
        (Button::RightThumb, 0x13E),
    ];

    fn state(gp: &gilrs::Gamepad) -> PadInfo {
        let axis = |a: Axis, sign: f32| (gp.value(a) * sign * 32767.0) as i16;
        let dpad = |neg: Button, pos: Button| (gp.is_pressed(pos) as i16 - gp.is_pressed(neg) as i16) * 32767;
        PadInfo {
            name: gp.name().to_string(),
            buttons: BUTTONS.iter().map(|&(b, _)| gp.is_pressed(b)).collect(),
            // stick Y grows upwards in gilrs and downwards on Linux
            axes: vec![
                axis(Axis::LeftStickX, 1.0),
                axis(Axis::LeftStickY, -1.0),
                0,
                axis(Axis::RightStickX, 1.0),
                axis(Axis::RightStickY, -1.0),
                0,
                dpad(Button::DPadLeft, Button::DPadRight),
                dpad(Button::DPadUp, Button::DPadDown),
            ],
            rest: vec![0; 8],
            codes: BUTTONS.iter().map(|&(_, c)| c).collect(),
            axis_codes: vec![],
        }
    }

    /// Inputs that went from released to pressed between two readings.
    fn presses(before: &PadInfo, now: &PadInfo, out: &mut Vec<(String, Input)>) {
        for (n, (&b, &a)) in before.buttons.iter().zip(&now.buttons).enumerate() {
            if a && !b {
                out.push((now.name.clone(), Input::Button(n as u8)));
            }
        }
        for (n, (&b, &a)) in before.axes.iter().zip(&now.axes).enumerate() {
            let (b, a) = (b as i32, a as i32);
            if a > AXIS_ON && b <= AXIS_ON {
                out.push((now.name.clone(), Input::Axis(n as u8, true)));
            } else if a < -AXIS_ON && b >= -AXIS_ON {
                out.push((now.name.clone(), Input::Axis(n as u8, false)));
            }
        }
    }

    pub(super) fn run(pads: &Pads) {
        let Ok(mut gilrs) = Gilrs::new() else { return };
        let mut last: Vec<PadInfo> = vec![];
        loop {
            while gilrs.next_event().is_some() {} // updates the cached state
            let now: Vec<PadInfo> = gilrs.gamepads().map(|(_, gp)| state(&gp)).collect();
            if now.iter().map(|p| (&p.name, &p.buttons, &p.axes)).ne(last.iter().map(|p| (&p.name, &p.buttons, &p.axes))) {
                let mut new_presses = vec![];
                for p in &now {
                    if let Some(b) = last.iter().find(|b| b.name == p.name) {
                        presses(b, p, &mut new_presses);
                    }
                }
                if let Ok(mut d) = pads.devs.lock() {
                    *d = now.clone();
                }
                if !new_presses.is_empty() {
                    if let Ok(mut q) = pads.presses.lock() {
                        q.extend(new_presses);
                        while q.len() > 64 {
                            q.pop_front();
                        }
                    }
                }
                last = now;
            }
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu_api::{Button, Core};

    /// Test system: only B, A, Select, Start and directions (a small console).
    static SYS: SystemInfo = SystemInfo {
        name: "Test",
        short_name: "T",
        extensions: &["bin"],
        thumbnails: "",
        cover_aspect: 0.66,
        players: 2,
        buttons: &[
            Button { id: PadButton::Up, label: "Up" },
            Button { id: PadButton::Down, label: "Down" },
            Button { id: PadButton::Left, label: "Left" },
            Button { id: PadButton::Right, label: "Right" },
            Button { id: PadButton::B, label: "Fire 1" },
            Button { id: PadButton::A, label: "Fire 2" },
            Button { id: PadButton::Select, label: "Select" },
            Button { id: PadButton::Start, label: "Start" },
        ],
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

    /// Controller on the xpad driver: 11 buttons, 8 axes (2 and 5 are triggers).
    fn xpad() -> PadInfo {
        let codes = vec![0x130, 0x131, 0x133, 0x134, 0x136, 0x137, 0x13A, 0x13B, 0x13C, 0x13D, 0x13E];
        let mut rest = vec![0i16; 8];
        rest[2] = -32767;
        rest[5] = -32767;
        PadInfo { name: "Xbox".into(), buttons: vec![false; 11], axes: rest.clone(), rest, codes, axis_codes: vec![] }
    }

    /// FreeBSD's hgame with a DInput controller (the Vader 3 as tested): button codes
    /// already translated by the evdev reader, d-pad on axes 4/5.
    #[test]
    fn dpad_found_by_axis_code() {
        let codes = vec![0x130, 0x131, 0x132, 0x133, 0x134, 0x135, 0x136, 0x137, 0x138, 0x139, 0x13A, 0x13B];
        let p = PadInfo { name: "Vader".into(), buttons: vec![false; codes.len()], axes: vec![0; 6], rest: vec![0; 6], codes, axis_codes: vec![0x00, 0x01, 0x02, 0x05, 0x10, 0x11] };
        let m = default_mapping(&p, &SYS);
        assert_eq!(m[&Func::Pad(0, PadButton::Left)], vec![Input::Axis(0, false), Input::Axis(4, false)]);
        assert_eq!(m[&Func::Pad(0, PadButton::Down)], vec![Input::Axis(1, true), Input::Axis(5, true)]);
        assert_eq!(m[&Func::Pad(0, PadButton::Start)], vec![Input::Button(11)]);
    }

    #[test]
    fn default_map_uses_driver_codes() {
        let m = default_mapping(&xpad(), &SYS);
        assert_eq!(m[&Func::Pad(0, PadButton::B)], vec![Input::Button(0)]);
        assert_eq!(m[&Func::Pad(0, PadButton::Select)], vec![Input::Button(6)]);
        assert_eq!(m[&Func::Pad(0, PadButton::Start)], vec![Input::Button(7)]);
        assert_eq!(m[&Func::QuickMenu], vec![Input::Button(8)]); // Home/Guide
        assert!(!m.contains_key(&Func::Pause));
        assert_eq!(m[&Func::Pad(0, PadButton::Up)], vec![Input::Axis(1, false), Input::Axis(7, false)]);
        assert!(!m.contains_key(&Func::Pad(0, PadButton::X))); // the system doesn't use X
        // xpad: L2/R2 are analog triggers (axes 2 and 5)
        assert_eq!(m[&Func::FastForward], vec![Input::Axis(5, true)]);
        assert_eq!(m[&Func::Rewind], vec![Input::Axis(2, true)]);
    }

    /// A mapping saved before the quick menu existed had Home/Guide on Pause: it moves to
    /// the quick menu; once the quick menu is saved, Pause is left as it is.
    #[test]
    fn old_pause_mapping_becomes_quick_menu() {
        let mut saved = BTreeMap::new();
        saved.insert("pause".to_string(), vec!["b8".to_string()]);
        let m = mapping_from_cfg(&saved, &SYS);
        assert_eq!(m.get(&Func::QuickMenu), Some(&vec![Input::Button(8)]));
        assert!(!m.contains_key(&Func::Pause));
        saved.insert("quick_menu".to_string(), vec!["b10".to_string()]);
        let m = mapping_from_cfg(&saved, &SYS);
        assert_eq!((m[&Func::Pause].clone(), m[&Func::QuickMenu].clone()), (vec![Input::Button(8)], vec![Input::Button(10)]));
    }

    #[test]
    fn dinput_uses_l2_r2_as_buttons() {
        // Flydigi in DInput: TL2 = button 8 (0x138), TR2 = button 9 (0x139)
        let codes: Vec<u16> = (0x130..=0x13B).collect();
        let p = PadInfo { name: "Flydigi".into(), buttons: vec![false; codes.len()], axes: vec![0; 8], rest: vec![0; 8], codes, axis_codes: vec![] };
        let m = default_mapping(&p, &SYS);
        assert_eq!(m[&Func::FastForward], vec![Input::Button(9)]);
        assert_eq!(m[&Func::Rewind], vec![Input::Button(8)]);
    }

    #[test]
    fn no_codes_falls_back_to_xbox_layout() {
        let p = PadInfo { name: "x".into(), buttons: vec![false; 4], axes: vec![0; 2], rest: vec![0; 2], codes: vec![0; 4], axis_codes: vec![] };
        let m = default_mapping(&p, &SYS);
        assert_eq!(m[&Func::Pad(0, PadButton::A)], vec![Input::Button(1)]);
        assert!(!m.contains_key(&Func::Pad(0, PadButton::Start))); // button 7 doesn't exist
        assert_eq!(m[&Func::Pad(0, PadButton::Left)], vec![Input::Axis(0, false)]); // axis 6 doesn't exist
    }

    #[test]
    fn trigger_counts_from_rest() {
        let mut p = xpad();
        let mut m = Mapping::new();
        m.insert(Func::Pad(0, PadButton::B), vec![Input::Axis(2, true)]);
        assert_eq!(bits(&m, &p, 2)[0], 0); // at rest (-32767) it doesn't trigger
        p.axes[2] = 32767;
        assert_eq!(bits(&m, &p, 2)[0], PadButton::B.bit());
    }

    #[test]
    fn player_2_actions_on_same_controller() {
        let mut p = xpad();
        let mut m = Mapping::new();
        m.insert(Func::Pad(1, PadButton::Start), vec![Input::Button(4)]);
        p.buttons[4] = true;
        assert_eq!(bits(&m, &p, 2), vec![0, PadButton::Start.bit()]);
    }

    #[test]
    fn config_text_round_trip() {
        for i in [Input::Button(12), Input::Axis(7, true), Input::Axis(0, false)] {
            assert_eq!(Input::parse(&i.to_text()), Some(i));
        }
        let m = default_mapping(&xpad(), &SYS);
        assert_eq!(mapping_from_cfg(&mapping_to_cfg(&m), &SYS), m);
        assert_eq!(Func::Pad(1, PadButton::B).key(), "p2_b");
        assert_eq!(Input::parse("junk"), None);
    }
}
