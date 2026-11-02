//! Prints what the controller reader sees: each controller's name, pressed buttons and
//! moved axes, and every new press. For checking a controller on a new system.
//!
//!   cargo run --release -p emu-frontend --example pads            # the system's reader
//!   cargo run --release -p emu-frontend --example pads -- --evdev # Linux: FreeBSD's reader

use std::time::Duration;

fn main() {
    #[cfg(target_os = "linux")]
    let pads = if std::env::args().any(|a| a == "--evdev") { emu_frontend::gamepad::start_evdev() } else { emu_frontend::gamepad::start() };
    #[cfg(not(target_os = "linux"))]
    let pads = emu_frontend::gamepad::start();
    let mut last = String::new();
    loop {
        std::thread::sleep(Duration::from_millis(50));
        let mut now = String::new();
        for (i, p) in pads.snapshot().iter().enumerate() {
            let held: Vec<String> = p.buttons.iter().enumerate().filter(|(_, b)| **b).map(|(n, _)| format!("b{n}({:#x})", p.codes.get(n).copied().unwrap_or(0))).collect();
            let moved: Vec<String> = p.axes.iter().zip(&p.rest).enumerate().filter(|(_, (a, r))| (**a as i32 - **r as i32).abs() > 8000).map(|(n, (a, _))| format!("a{n}={a}")).collect();
            now += &format!("pad {i}: {} — {} buttons, {} axes | {} {}\n", p.name, p.buttons.len(), p.axes.len(), held.join(" "), moved.join(" "));
            if std::env::var_os("PADS_DEBUG").is_some() {
                now += &format!("  axes {:?}\n  rest {:?}\n  codes {:x?}\n", p.axes, p.rest, p.codes);
            }
        }
        if pads.snapshot().is_empty() {
            now = if pads.no_access.load(std::sync::atomic::Ordering::Relaxed) { "no controller (no permission to read some /dev/input nodes)\n".into() } else { "no controller\n".into() };
        }
        for (name, input) in pads.take_presses() {
            println!("press: {name}: {}", input.to_text());
        }
        if now != last {
            print!("{now}");
            last = now;
        }
    }
}
