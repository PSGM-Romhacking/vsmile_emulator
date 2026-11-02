//! Controllers through evdev (`/dev/input/eventX`): the reader on FreeBSD, whose kernel
//! offers the same interface as Linux's (drivers `hgame` for generic HID gamepads and
//! `xb360gp` for Xbox 360 ones). Each controller is presented as Linux's joystick API
//! presents it: buttons in joydev's order (BTN_JOYSTICK and up, then BTN_MISC…), with their
//! codes, and axes in ABS code order, scaled to ±32767. So the default map and the saved
//! mappings are the same as on Linux. On Linux it is only used for testing
//! (`cargo run --example pads -- --evdev`).
//!
//! On FreeBSD the event nodes are `root:wheel 0600` by default; without permission the
//! reader sets [`Pads::no_access`] and the UI says so (the README explains the devfs rule).

use super::*;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::atomic::Ordering;

const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const KEY_CNT: usize = 0x300;
const ABS_CNT: usize = 0x40;
const BTN_MISC: usize = 0x100;
const BTN_JOYSTICK: usize = 0x120;

/// `_IOC(read, 'E', nr, len)`, encoded the way each kernel does it.
const fn ioc_read(nr: u64, len: u64) -> u64 {
    #[cfg(target_os = "linux")]
    return (2 << 30) | (len << 16) | ((b'E' as u64) << 8) | nr;
    #[cfg(not(target_os = "linux"))]
    return 0x4000_0000 | ((len & 0x1fff) << 16) | ((b'E' as u64) << 8) | nr;
}

const fn eviocgname(len: u64) -> u64 {
    ioc_read(0x06, len)
}

const fn eviocgbit(ev: u64, len: u64) -> u64 {
    ioc_read(0x20 + ev, len)
}

const fn eviocgabs(abs: u64) -> u64 {
    ioc_read(0x40 + abs, std::mem::size_of::<AbsInfo>() as u64)
}

/// `struct input_absinfo`.
#[repr(C)]
#[derive(Default)]
struct AbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

/// `struct input_event`: a `timeval`, then type, code and value.
const EVENT_SIZE: usize = std::mem::size_of::<libc::timeval>() + 8;

struct Dev {
    fd: i32,
    info: PadInfo,
    /// Key code → button index.
    keys: HashMap<u16, usize>,
    /// ABS code → (axis index, minimum, maximum).
    abs: HashMap<u16, (usize, i32, i32)>,
}

enum Opened {
    Pad(Box<Dev>),
    NotPad,
    /// No permission to read the node (FreeBSD's default for /dev/input).
    Denied,
    Fail,
}

fn bit(bits: &[u8], n: usize) -> bool {
    bits.get(n / 8).is_some_and(|b| b & (1 << (n % 8)) != 0)
}

/// The value scaled to ±32767, linearly between the axis' minimum and maximum (as joydev
/// does): a hat (-1…1) becomes -32767…32767, a trigger (0…255) rests at -32767.
fn scale(v: i32, min: i32, max: i32) -> i16 {
    if max <= min {
        return 0;
    }
    ((v as i64 - min as i64) * 65534 / (max as i64 - min as i64) - 32767).clamp(-32767, 32767) as i16
}

/// Where an axis rests, by its code, not by its position when opened (a sleeping wireless
/// controller reports stale values then). Triggers rest at the minimum: ABS_GAS/ABS_BRAKE
/// always, ABS_Z/ABS_RZ on the Xbox layout (the one with ABS_RX/ABS_RY, where the right
/// stick is RX/RY). Everything else (sticks, d-pad hats, and Z/RZ as the right stick on
/// DInput controllers) rests at the center.
fn rest_of(code: usize, xbox_layout: bool) -> i16 {
    const ABS_Z: usize = 0x02;
    const ABS_RZ: usize = 0x05;
    const ABS_GAS: usize = 0x09;
    const ABS_BRAKE: usize = 0x0a;
    let trigger = matches!(code, ABS_GAS | ABS_BRAKE) || (xbox_layout && matches!(code, ABS_Z | ABS_RZ));
    if trigger { -32767 } else { 0 }
}

/// Button codes in joydev's order: BTN_JOYSTICK up to the last key, then BTN_MISC up to
/// BTN_JOYSTICK (keyboard keys, below BTN_MISC, are not buttons).
fn button_codes(key_bits: &[u8]) -> Vec<u16> {
    (BTN_JOYSTICK..KEY_CNT).chain(BTN_MISC..BTN_JOYSTICK).filter(|&c| bit(key_bits, c)).map(|c| c as u16).collect()
}

/// The codes as Linux would give them. FreeBSD's `hgame` numbers a gamepad's HID buttons
/// from BTN_TRIGGER (0x120, the joystick range), where Linux uses BTN_GAMEPAD (0x130:
/// BTN_SOUTH, BTN_EAST…), and the default map goes by those. So a controller with
/// joystick codes and no gamepad ones gets them moved up by 0x10, as Linux does for a
/// device whose HID application is a gamepad. Only for [`PadInfo::codes`]: the events
/// keep the kernel's codes.
fn gamepad_codes(codes: &[u16]) -> Vec<u16> {
    let joystick = codes.iter().any(|c| (0x120..=0x12F).contains(c)) && !codes.iter().any(|c| (0x130..=0x13F).contains(c));
    if !cfg!(target_os = "freebsd") || !joystick {
        return codes.to_vec();
    }
    codes.iter().map(|&c| if (0x120..=0x12F).contains(&c) { c + 0x10 } else { c }).collect()
}

/// Game controllers only: some BTN_JOYSTICK/BTN_GAMEPAD button (0x120–0x13F) or
/// BTN_TRIGGER_HAPPY (≥ 0x2C0). Keyboards and mice have their own event nodes too.
fn is_gamepad(codes: &[u16]) -> bool {
    codes.iter().any(|&c| (0x120..=0x13F).contains(&c) || c >= 0x2C0)
}

fn open(path: &str) -> Opened {
    let Ok(c) = CString::new(path) else { return Opened::Fail };
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
    if fd < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EACCES) | Some(libc::EPERM) => Opened::Denied,
            _ => Opened::Fail, // retry on the next scan
        };
    }
    let mut key_bits = [0u8; KEY_CNT / 8];
    let mut abs_bits = [0u8; ABS_CNT / 8];
    let mut name = [0u8; 128];
    let ok = unsafe { libc::ioctl(fd, eviocgbit(EV_KEY as u64, key_bits.len() as u64) as _, key_bits.as_mut_ptr()) } >= 0;
    let codes = if ok { button_codes(&key_bits) } else { vec![] };
    if !is_gamepad(&codes) {
        unsafe { libc::close(fd) };
        return Opened::NotPad;
    }
    unsafe {
        if libc::ioctl(fd, eviocgbit(EV_ABS as u64, abs_bits.len() as u64) as _, abs_bits.as_mut_ptr()) < 0 {
            abs_bits = [0; ABS_CNT / 8];
        }
        libc::ioctl(fd, eviocgname(name.len() as u64) as _, name.as_mut_ptr());
    }
    let mut abs = HashMap::new();
    let (mut axes, mut rest) = (vec![], vec![]);
    let xbox_layout = bit(&abs_bits, 0x03) && bit(&abs_bits, 0x04); // ABS_RX and ABS_RY
    for code in (0..ABS_CNT).filter(|&c| bit(&abs_bits, c)) {
        let mut ai = AbsInfo::default();
        if unsafe { libc::ioctl(fd, eviocgabs(code as u64) as _, &mut ai as *mut AbsInfo) } < 0 {
            continue;
        }
        abs.insert(code as u16, (axes.len(), ai.minimum, ai.maximum));
        let r = rest_of(code, xbox_layout);
        axes.push(r); // the real position comes with the first events
        rest.push(r);
    }
    let name = String::from_utf8_lossy(&name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())]).trim().to_string();
    let info = PadInfo {
        name: if name.is_empty() { path.to_string() } else { name },
        buttons: vec![false; codes.len()],
        axes,
        rest,
        codes: gamepad_codes(&codes),
        axis_codes: (0..ABS_CNT).filter(|&c| abs.contains_key(&(c as u16))).map(|c| c as u16).collect(),
    };
    let keys = codes.iter().enumerate().map(|(i, &c)| (c, i)).collect();
    Opened::Pad(Box::new(Dev { fd, info, keys, abs }))
}

fn event_paths() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir("/dev/input")
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("event"))
                .map(|n| format!("/dev/input/{n}"))
                .collect()
        })
        .unwrap_or_default();
    // event2 before event10
    v.sort_by_key(|p| (p.len(), p.clone()));
    v
}

/// Consumes pending events; `false` if the device is gone.
fn drain(dev: &mut Dev, presses: &mut Vec<(String, Input)>) -> bool {
    let mut buf = [0u8; EVENT_SIZE * 64];
    loop {
        let r = unsafe { libc::read(dev.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if r < 0 {
            return std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock;
        }
        if r == 0 {
            return false;
        }
        let t = EVENT_SIZE - 8;
        for ev in buf[..r as usize].as_chunks::<EVENT_SIZE>().0 {
            let etype = u16::from_ne_bytes([ev[t], ev[t + 1]]);
            let code = u16::from_ne_bytes([ev[t + 2], ev[t + 3]]);
            let value = i32::from_ne_bytes([ev[t + 4], ev[t + 5], ev[t + 6], ev[t + 7]]);
            let i = &mut dev.info;
            if etype == EV_KEY
                && let Some(&n) = dev.keys.get(&code)
            {
                let was = i.buttons[n];
                i.buttons[n] = value != 0; // 2 = auto-repeat, still held
                if !was && value != 0 {
                    presses.push((i.name.clone(), Input::Button(n as u8)));
                }
            } else if etype == EV_ABS
                && let Some(&(n, min, max)) = dev.abs.get(&code)
            {
                let before = i.axes[n] as i32 - i.rest[n] as i32;
                i.axes[n] = scale(value, min, max);
                let after = i.axes[n] as i32 - i.rest[n] as i32;
                if after > AXIS_ON && before <= AXIS_ON {
                    presses.push((i.name.clone(), Input::Axis(n as u8, true)));
                } else if after < -AXIS_ON && before >= -AXIS_ON {
                    presses.push((i.name.clone(), Input::Axis(n as u8, false)));
                }
            }
        }
        if (r as usize) < buf.len() {
            return true;
        }
    }
}

pub(super) fn run(pads: &Pads) {
    let mut devs: BTreeMap<String, Box<Dev>> = BTreeMap::new();
    let mut ignored: HashSet<String> = Default::default(); // keyboards, mice…
    let mut denied: HashSet<String> = Default::default();
    let mut last_scan = std::time::Instant::now() - std::time::Duration::from_secs(10);
    loop {
        let mut changed = false;
        // hot-plug: rescan /dev/input every 1.5 s (and retry the denied nodes, in case the
        // permissions were fixed)
        if last_scan.elapsed().as_millis() >= 1500 {
            last_scan = std::time::Instant::now();
            let paths = event_paths();
            ignored.retain(|p| paths.contains(p));
            denied.retain(|p| paths.contains(p));
            for p in &paths {
                if !devs.contains_key(p) && !ignored.contains(p) {
                    match open(p) {
                        Opened::Pad(d) => {
                            denied.remove(p);
                            devs.insert(p.clone(), d);
                            changed = true;
                        }
                        Opened::NotPad => {
                            denied.remove(p);
                            ignored.insert(p.clone());
                        }
                        Opened::Denied => {
                            denied.insert(p.clone());
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
            pads.no_access.store(!denied.is_empty(), Ordering::Relaxed);
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
            && let Ok(mut d) = pads.devs.lock()
        {
            *d = devs.values().map(|d| d.info.clone()).collect();
        }
        if !presses.is_empty()
            && let Ok(mut q) = pads.presses.lock()
        {
            q.extend(presses);
            while q.len() > 64 {
                q.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_like_joydev() {
        assert_eq!(scale(-1, -1, 1), -32767, "hat");
        assert_eq!(scale(0, -1, 1), 0);
        assert_eq!(scale(1, -1, 1), 32767);
        assert_eq!(scale(0, 0, 255), -32767, "trigger at rest");
        assert_eq!(scale(-32768, -32768, 32767), -32767, "stick");
        assert_eq!(scale(5, 3, 3), 0, "no range");
    }

    #[test]
    fn rest_by_axis_kind() {
        // Xbox layout: Z/RZ are the triggers
        assert_eq!(rest_of(0x02, true), -32767);
        assert_eq!(rest_of(0x05, true), -32767);
        assert_eq!(rest_of(0x03, true), 0, "right stick X");
        // DInput (no RX/RY): Z/RZ are the right stick, GAS/BRAKE the triggers
        assert_eq!(rest_of(0x02, false), 0);
        assert_eq!(rest_of(0x09, false), -32767);
        assert_eq!(rest_of(0x10, false), 0, "d-pad hat");
    }

    #[test]
    fn buttons_in_joydev_order() {
        let mut bits = [0u8; KEY_CNT / 8];
        for c in [0x110usize, 0x130, 0x131, 0x13B, 0x2C0, 0x1E] {
            bits[c / 8] |= 1 << (c % 8);
        }
        // gamepad buttons and BTN_TRIGGER_HAPPY first, mouse buttons after, no keyboard keys
        assert_eq!(button_codes(&bits), vec![0x130, 0x131, 0x13B, 0x2C0, 0x110]);
        assert!(is_gamepad(&button_codes(&bits)));
        assert!(!is_gamepad(&[0x110, 0x111]), "a mouse");
    }

    #[test]
    fn hgame_codes_become_gamepad_codes() {
        let hgame = [0x120, 0x121, 0x12B, 0x2C0];
        let out = gamepad_codes(&hgame);
        if cfg!(target_os = "freebsd") {
            assert_eq!(out, vec![0x130, 0x131, 0x13B, 0x2C0]);
        } else {
            assert_eq!(out, hgame.to_vec(), "Linux keeps what the kernel says");
        }
        assert_eq!(gamepad_codes(&[0x130, 0x131]), vec![0x130, 0x131], "already gamepad codes");
    }

    /// The ioctl numbers, checked against the C headers of each system.
    #[test]
    fn ioctl_numbers() {
        #[cfg(target_os = "linux")]
        {
            assert_eq!(eviocgname(128), 0x8080_4506);
            assert_eq!(eviocgabs(0), 0x8018_4540);
        }
        #[cfg(target_os = "freebsd")]
        {
            assert_eq!(eviocgname(128), 0x4080_4506);
            assert_eq!(eviocgabs(0), 0x4018_4540);
        }
    }
}
