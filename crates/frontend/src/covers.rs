//! Game covers and screens (libretro-thumbnails), downloaded in the background with
//! `curl` (present on any distro) and stored in `<app folder>/cache/covers/`; the user's
//! own images (`config::Folder::Covers`) come first.
//! The file name is the game's (No-Intro standard), with the characters that
//! libretro replaces with "_". A missing image (e.g. homebrew) becomes a
//! `.none` marker, so it isn't downloaded again.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Sender, channel};


#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Art {
    Boxart,
    Title,
    Snap,
}

impl Art {
    pub const ALL: [Art; 3] = [Art::Boxart, Art::Title, Art::Snap];

    fn dir(self) -> &'static str {
        match self {
            Art::Boxart => "Named_Boxarts",
            Art::Title => "Named_Titles",
            Art::Snap => "Named_Snaps",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Art::Boxart => "Box art",
            Art::Title => "Title screen",
            Art::Snap => "Gameplay",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Art::Boxart => "boxart",
            Art::Title => "title",
            Art::Snap => "snap",
        }
    }

    pub fn from_key(s: &str) -> Self {
        match s {
            "title" => Art::Title,
            "snap" => Art::Snap,
            _ => Art::Boxart,
        }
    }
}

pub enum Status {
    Ready(PathBuf),
    Missing,
    Pending,
}

/// File name in libretro-thumbnails: these characters become "_".
pub fn libretro_name(game: &str) -> String {
    game.chars().map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c }).collect()
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn cache_dir() -> Option<PathBuf> {
    Some(crate::config::cache_dir()?.join("covers"))
}

/// The user's own image: `<covers folder>/<type>/<game>.png`, the layout of a
/// libretro-thumbnails repository (`Named_Boxarts/Pooyan (Japan).png`). `game` is the
/// libretro name or the name shown in the library (`Named_Boxarts/Cacorm.png`).
pub fn own(art: Art, game: &str) -> Option<PathBuf> {
    let p = crate::config::Folder::Covers.dir()?.join(art.dir()).join(format!("{}.png", libretro_name(game)));
    p.is_file().then_some(p)
}

/// The user's own image under the first of these names that has one.
pub fn own_any(art: Art, names: &[&str]) -> Option<PathBuf> {
    names.iter().find_map(|n| own(art, n))
}

/// Per-system cache: `cache/covers/<libretro system>/<type>/<game>.png`.
fn paths(system: &str, art: Art, game: &str) -> Option<(PathBuf, PathBuf)> {
    let dir = cache_dir()?.join(system).join(art.dir());
    let name = libretro_name(game);
    Some((dir.join(format!("{name}.png")), dir.join(format!("{name}.none"))))
}

pub struct Covers {
    /// The system's folder in libretro-thumbnails (e.g. "Casio - PV-1000").
    system: String,
    tx: Sender<(Art, String)>,
    asked: Mutex<HashSet<(Art, String)>>,
}

impl Covers {
    pub fn start(system: &str) -> Self {
        let (tx, rx) = channel::<(Art, String)>();
        let sys = system.to_string();
        // libretro-thumbnails, then the app's own source (homebrew), with the same layout
        let bases: Vec<String> = ["https://thumbnails.libretro.com", crate::brand::get().thumbnails_url]
            .iter()
            .filter(|b| !b.is_empty())
            .map(|b| format!("{b}/{}", url_encode(system)))
            .collect();
        let _ = std::thread::Builder::new().name("covers".into()).spawn(move || {
            for (art, game) in rx {
                let Some((png, none)) = paths(&sys, art, &game) else { continue };
                if png.exists() || none.exists() {
                    continue;
                }
                let _ = std::fs::create_dir_all(png.parent().unwrap());
                let tmp = png.with_extension("tmp");
                // 404 everywhere: the game doesn't have this image; any other failure (no
                // network, curl missing…): retry on the next run
                let mut missing = true;
                for base in &bases {
                    let url = format!("{base}/{}/{}.png", art.dir(), url_encode(&libretro_name(&game)));
                    let st = crate::platform::command("curl").args(["-fsL", "--max-time", "20", "-o"]).arg(&tmp).arg(&url).status();
                    match st {
                        Ok(s) if s.success() => {
                            let _ = std::fs::rename(&tmp, &png);
                            missing = false;
                            break;
                        }
                        Ok(s) if s.code() == Some(22) => {}
                        _ => missing = false,
                    }
                }
                let _ = std::fs::remove_file(&tmp);
                if missing && !png.exists() {
                    let _ = std::fs::write(&none, b"");
                }
            }
        });
        Self { system: system.to_string(), tx, asked: Mutex::new(HashSet::new()) }
    }

    /// The user's own image or the cached one; if neither and `fetch`, requests the
    /// download (once per run). Without `fetch`, only what is already on disk.
    pub fn get(&self, art: Art, game: &str, fetch: bool) -> Status {
        if let Some(p) = own(art, game) {
            return Status::Ready(p);
        }
        let Some((png, none)) = paths(&self.system, art, game) else { return Status::Missing };
        if png.exists() {
            return Status::Ready(png);
        }
        if !fetch || none.exists() {
            return Status::Missing;
        }
        let key = (art, game.to_string());
        if self.asked.lock().unwrap_or_else(|e| e.into_inner()).insert(key.clone()) {
            let _ = self.tx.send(key);
        }
        Status::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real download (network): `cargo test -- --ignored real_covers`.
    #[test]
    #[ignore]
    fn real_covers() {
        let dir = std::env::temp_dir().join(format!("emu_frontend_covers_{}", std::process::id()));
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };
        let c = Covers::start("Casio - PV-1000");
        let wait = |art, game: &str| {
            for _ in 0..200 {
                match c.get(art, game, true) {
                    Status::Pending => std::thread::sleep(std::time::Duration::from_millis(100)),
                    s => return s,
                }
            }
            Status::Pending
        };
        for art in Art::ALL {
            assert!(matches!(wait(art, "Dig Dug (Japan)"), Status::Ready(_)), "{art:?} for Dig Dug");
        }
        assert!(matches!(wait(Art::Boxart, "Warp & Warp (Japan)"), Status::Ready(_)), "name with &");
        assert!(matches!(wait(Art::Boxart, "cacorm"), Status::Missing), "homebrew without cover");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn libretro_names() {
        assert_eq!(libretro_name("Warp & Warp (Japan)"), "Warp _ Warp (Japan)");
        assert_eq!(url_encode("Dig Dug (Japan)"), "Dig%20Dug%20%28Japan%29");
    }
}
