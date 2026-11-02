//! Error report: when the program panics, writes `<app folder>/crashes/<date>.log` with the
//! message, where it happened, the version, the system and the game that was running, so a
//! tester can attach it. The next start shows where the report is (`take_pending`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

static GAME: Mutex<Option<String>> = Mutex::new(None);

fn dir() -> Option<PathBuf> {
    Some(crate::config::app_dir()?.join("crashes"))
}

/// Marker left by a crash, holding the report's path; read on the next start.
fn marker() -> Option<PathBuf> {
    Some(dir()?.join("pending"))
}

/// Installs the panic hook; call once, after `brand::set`. The default hook still runs
/// (message on stderr).
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = write(info); // a failure here must not hide the original panic
        previous(info);
    }));
}

/// The game running now (None when closed), for the report.
pub fn set_game(path: Option<&Path>) {
    if let Ok(mut g) = GAME.lock() {
        *g = path.map(|p| p.display().to_string());
    }
}

fn write(info: &std::panic::PanicHookInfo) -> std::io::Result<()> {
    let dir = dir().ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::create_dir_all(&dir)?;
    let t = crate::platform::local_time(crate::usage::now() as u64);
    let stamp = format!("{:04}-{:02}-{:02}_{:02}-{:02}-{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second);
    let message = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_else(|| "(no message)".into());
    let brand = crate::brand::get();
    // try_lock: the panic may have happened while the lock was held
    let game = GAME.try_lock().ok().and_then(|g| g.clone()).unwrap_or_else(|| "(none)".into());
    let text = format!(
        "{} {}\nDate: {stamp}\nSystem: {} {}\nThread: {}\nGame: {game}\nError: {message}\nWhere: {}\n\nBacktrace:\n{}\n",
        brand.name,
        brand.version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::current().name().unwrap_or("?"),
        info.location().map_or("?".into(), |l| l.to_string()),
        std::backtrace::Backtrace::force_capture(),
    );
    let file = dir.join(format!("{stamp}.log"));
    std::fs::write(&file, text)?;
    std::fs::write(marker().ok_or(std::io::ErrorKind::NotFound)?, file.display().to_string())
}

/// The report left by a crash in the previous run, once (the marker is removed).
pub fn take_pending() -> Option<PathBuf> {
    let m = marker()?;
    let path = std::fs::read_to_string(&m).ok()?;
    let _ = std::fs::remove_file(&m);
    Some(PathBuf::from(path.trim())).filter(|p| p.is_file())
}
