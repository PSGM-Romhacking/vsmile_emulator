//! Save states: slots 1–9, their thumbnails and the state file (header with the ROM
//! fingerprint, then the core's bytes).

#[allow(unused_imports)]
use super::*;

impl App {

    /// Cheats of a game: `<cheats folder>/<game>.cht` (RetroArch's format).
    pub(super) fn cheats_path(&self, game: &Path) -> Option<PathBuf> {
        Some(crate::config::Folder::Cheats.dir()?.join(format!("{}.cht", stem(game))))
    }

    pub(super) fn state_path(&self, slot: u32) -> Option<PathBuf> {
        let stem = self.cur_game.as_ref()?.file_stem()?.to_string_lossy().into_owned();
        Some(crate::config::Folder::States.dir()?.join(format!("{stem}.slot{slot}.state")))
    }

    pub(super) fn save_state(&mut self) {
        if !self.loaded {
            return;
        }
        let Some(p) = self.state_path(self.slot) else { return };
        let bytes = state_file(self.rom_hash, &self.core.save_state());
        let res = std::fs::create_dir_all(p.parent().unwrap()).and_then(|_| {
            let tmp = p.with_extension("tmp");
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(&tmp, &p)
        });
        // thumbnail of the saved moment, next to the state (<game>.slotN.png)
        if res.is_ok() {
            let _ = self.frame_image(1).save(p.with_extension("png"));
        }
        match res {
            Ok(()) => self.notify(trf("State saved to slot {n}", &[("n", &self.slot)])),
            Err(e) => self.notify(trf("Save error: {error}", &[("error", &e)])),
        }
    }

    pub(super) fn load_state(&mut self) {
        if !self.loaded {
            return;
        }
        let Some(p) = self.state_path(self.slot) else { return };
        let res = std::fs::read(&p).map_err(|_| trf("Slot {n} is empty", &[("n", &self.slot)])).and_then(|b| {
            let st = state_payload(&b, self.rom_hash)?;
            self.core.load_state(st)
        });
        match res {
            Ok(()) => {
                self.cheevos.state_loaded();
                self.history.clear();
                self.audio.clear();
                self.next_frame = self.now();
                self.dirty_frame = true;
                self.notify(trf("State loaded from slot {n}", &[("n", &self.slot)]));
            }
            Err(e) => self.notify(e),
        }
    }

    /// Slot thumbnail (loads from the PNG and caches it; reloads if the file changed).
    pub(super) fn thumb(&mut self, ctx: &egui::Context, slot: u32) -> Option<egui::TextureId> {
        let p = self.state_path(slot)?.with_extension("png");
        let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
        if let Some((t, tex)) = self.thumbs.get(&p)
            && *t == mtime {
                return Some(tex.id());
            }
        let img = image::open(&p).ok()?.to_rgb8();
        let ci = egui::ColorImage::from_rgb([img.width() as usize, img.height() as usize], img.as_raw());
        let tex = ctx.load_texture(p.to_string_lossy(), ci, egui::TextureOptions::LINEAR);
        let id = tex.id();
        self.thumbs.insert(p, (mtime, tex));
        Some(id)
    }

    pub(super) fn set_slot(&mut self, s: u32) {
        self.slot = s.clamp(1, 9);
        let info = self.slot_info(self.slot);
        self.notify(format!("{}{info}", trf("Slot {n}", &[("n", &self.slot)])));
    }

    /// " — 16/10 21:40" if the slot has a saved state, " — empty" if not.
    pub(super) fn slot_info(&self, s: u32) -> String {
        let t = self.state_path(s).and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
        match t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) {
            Some(d) => {
                let t = crate::platform::local_time(d.as_secs());
                format!(" — {:02}/{:02} {:02}:{:02}", t.day, t.month, t.hour, t.minute)
            }
            None => tr(" — empty"),
        }
    }
}
