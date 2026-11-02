//! What differs between operating systems: the base folders, local time and opening a folder
//! in the file manager. Linux and the BSDs follow the XDG conventions; Windows uses
//! `%APPDATA%`, `%LOCALAPPDATA%` and the user's profile folders.

use std::path::PathBuf;
use std::process::Command;

/// A `Command` that doesn't pop up a console window on Windows (the app has no console of
/// its own, so every child process would otherwise flash a cmd window).
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// An absolute path from an environment variable.
fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute())
}

#[cfg(not(windows))]
fn home_join(sub: &str) -> Option<PathBuf> {
    env_dir("HOME").map(|h| h.join(sub))
}

/// Base of the config folder: `$XDG_CONFIG_HOME` (`~/.config`) or `%APPDATA%`.
pub fn config_base() -> Option<PathBuf> {
    #[cfg(windows)]
    return env_dir("APPDATA");
    #[cfg(not(windows))]
    return env_dir("XDG_CONFIG_HOME").or_else(|| home_join(".config"));
}

/// Where development builds before 0.9 kept saves and cheats: `$XDG_DATA_HOME` (`~/.local/share`)
/// or `%APPDATA%` (only to migrate them; see `config::migrate`).
pub fn legacy_data_base() -> Option<PathBuf> {
    #[cfg(windows)]
    return env_dir("APPDATA");
    #[cfg(not(windows))]
    return env_dir("XDG_DATA_HOME").or_else(|| home_join(".local/share"));
}

/// Where development builds before 0.9 kept the downloaded covers: `$XDG_CACHE_HOME` (`~/.cache`)
/// or `%LOCALAPPDATA%` (only to migrate them).
pub fn legacy_cache_base() -> Option<PathBuf> {
    #[cfg(windows)]
    return env_dir("LOCALAPPDATA");
    #[cfg(not(windows))]
    return env_dir("XDG_CACHE_HOME").or_else(|| home_join(".cache"));
}

/// The user's Pictures or Videos folder. `kind` is the XDG name ("PICTURES"),
/// `fallback` the usual folder name ("Pictures").
pub fn user_dir(kind: &str, fallback: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let _ = kind;
        env_dir("USERPROFILE").map(|h| h.join(fallback))
    }
    #[cfg(not(windows))]
    {
        let home = env_dir("HOME");
        std::process::Command::new("xdg-user-dir")
            .arg(kind)
            .output()
            .ok()
            .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
            .filter(|p| p.is_absolute() && p.is_dir() && Some(p) != home.as_ref())
            .or_else(|| home.map(|h| h.join(fallback)))
    }
}

/// Local date and time.
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// Converts seconds since 1970 (UTC) to the local date and time.
pub fn local_time(secs: u64) -> LocalTime {
    let t = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    #[cfg(windows)]
    unsafe {
        libc::localtime_s(&mut tm, &t)
    };
    #[cfg(not(windows))]
    unsafe {
        libc::localtime_r(&t, &mut tm)
    };
    LocalTime {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
        second: tm.tm_sec as u32,
    }
}

/// Opens a folder in the system's file manager.
pub fn open_folder(dir: &std::path::Path) {
    open_with_system(dir.as_os_str());
}

/// Opens a web address in the default browser. (egui's links only ask the integration
/// to open them, and eframe does that only with its optional `webbrowser` feature.)
pub fn open_url(url: &str) {
    open_with_system(std::ffi::OsStr::new(url));
}

/// What the system does when a file, folder or address is double-clicked.
fn open_with_system(target: &std::ffi::OsStr) {
    #[cfg(windows)]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(any(windows, target_os = "macos")))]
    let opener = "xdg-open";
    let _ = command(opener).arg(target).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}
