//! Play time and history of each game, as in ORBE: total time played, when it was last
//! played and the most recent sessions (start and length). Kept in `<app folder>/usage.json`,
//! by the game file's path.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::{tr, trf};

/// Sessions kept, newest first; older ones drop off (the totals stay).
pub const MAX_SESSIONS: usize = 200;

/// Sessions shorter than this (seconds) are not recorded: opening a game by mistake.
pub const MIN_SESSION: f64 = 10.0;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Session {
    pub path: String,
    /// Seconds since 1970 (UTC).
    pub start: f64,
    /// Seconds actually played (paused, menus and an unfocused window don't count).
    pub duration: f64,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Usage {
    /// Game path → seconds played in total.
    #[serde(default)]
    pub play: BTreeMap<String, f64>,
    /// Game path → end of the last session (seconds since 1970).
    #[serde(default)]
    pub last: BTreeMap<String, f64>,
    #[serde(default)]
    pub sessions: Vec<Session>,
}

impl Usage {
    fn file() -> Option<PathBuf> {
        Some(crate::config::app_dir()?.join("usage.json"))
    }

    pub fn load() -> Self {
        Self::file().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    fn save(&self) {
        let Some(p) = Self::file() else { return };
        let Ok(json) = serde_json::to_string_pretty(self) else { return };
        let _ = std::fs::create_dir_all(p.parent().unwrap()).and_then(|_| {
            let tmp = p.with_extension("tmp");
            std::fs::write(&tmp, json)?;
            std::fs::rename(&tmp, &p)
        });
    }

    /// Records a session (if long enough) and saves.
    pub fn add(&mut self, path: &str, start: f64, duration: f64) {
        if self.record(path, start, duration) {
            self.save();
        }
    }

    /// Records a session in memory; false if it was too short to count.
    fn record(&mut self, path: &str, start: f64, duration: f64) -> bool {
        if duration < MIN_SESSION {
            return false;
        }
        *self.play.entry(path.to_string()).or_default() += duration;
        self.last.insert(path.to_string(), start + duration);
        self.sessions.insert(0, Session { path: path.to_string(), start, duration });
        self.sessions.truncate(MAX_SESSIONS);
        true
    }

    /// Sessions, newest first; with `path`, only that game's.
    pub fn sessions(&self, path: Option<&str>) -> Vec<&Session> {
        self.sessions.iter().filter(|s| path.is_none_or(|p| s.path == p)).collect()
    }

    pub fn played(&self, path: &str) -> f64 {
        self.play.get(path).copied().unwrap_or(0.0)
    }

    pub fn last_played(&self, path: &str) -> Option<f64> {
        self.last.get(path).copied()
    }
}

/// Seconds since 1970 (UTC).
pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// "2h 15min", "40min"; empty under a minute.
pub fn play_time(seconds: f64) -> String {
    if seconds < 60.0 {
        return String::new();
    }
    let minutes = (seconds / 60.0) as u64;
    let (hours, mins) = (minutes / 60, minutes % 60);
    if hours > 0 { format!("{hours}h {mins:02}min") } else { format!("{mins}min") }
}

/// "16/10/2026" and "21:40", local time.
pub fn date(ts: f64) -> String {
    let t = crate::platform::local_time(ts as u64);
    format!("{:02}/{:02}/{:04}", t.day, t.month, t.year)
}

pub fn time(ts: f64) -> String {
    let t = crate::platform::local_time(ts as u64);
    format!("{:02}:{:02}", t.hour, t.minute)
}

/// "now", "5 min ago", "yesterday", "3 days ago"…; the date after a year.
pub fn relative(ts: f64) -> String {
    let diff = now() - ts;
    let n = |v: f64| v as i64;
    if diff < 60.0 {
        tr("now")
    } else if diff < 3600.0 {
        trf("{n} min ago", &[("n", &n(diff / 60.0))])
    } else if diff < 86400.0 {
        trf("{n} h ago", &[("n", &n(diff / 3600.0))])
    } else {
        let days = n(diff / 86400.0);
        match days {
            1 => tr("yesterday"),
            2..7 => trf("{n} days ago", &[("n", &days)]),
            7..30 => trf("{n} wk ago", &[("n", &(days / 7))]),
            30..60 => tr("1 month ago"),
            60..365 => trf("{n} months ago", &[("n", &(days / 30))]),
            _ => date(ts),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_add_up_and_short_ones_are_skipped() {
        let mut u = Usage::default();
        assert!(u.record("a", 1000.0, 120.0));
        assert!(!u.record("b", 2000.0, 5.0));
        assert!(u.record("a", 3000.0, 60.0));
        assert_eq!(u.played("a"), 180.0);
        assert_eq!(u.played("b"), 0.0);
        assert_eq!(u.last_played("a"), Some(3060.0));
        assert_eq!(u.sessions(Some("a")).len(), 2);
        assert_eq!(u.sessions(None)[0].start, 3000.0); // newest first
        for i in 0..MAX_SESSIONS {
            u.record("c", i as f64, 30.0);
        }
        assert_eq!(u.sessions.len(), MAX_SESSIONS);
    }

    #[test]
    fn play_time_text() {
        assert_eq!(play_time(30.0), "");
        assert_eq!(play_time(40.0 * 60.0), "40min");
        assert_eq!(play_time(2.0 * 3600.0 + 5.0 * 60.0), "2h 05min");
    }
}
