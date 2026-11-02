//! Groundwork for RetroAchievements. No effect yet: these are the points where
//! `rcheevos` (the official C library, via FFI) will plug in.
//!
//! Planned integration (rc_client):
//! - `game_loaded`: `rc_client_begin_identify_and_load_game` with the ROM hash;
//!   RA identifies games by the MD5 of the cartridge file.
//! - `frame`: `rc_client_do_frame` after each emulated frame; memory is
//!   read through `read_memory` → `Core::system_ram()` (the system's flat space).
//! - paused/menu open: `rc_client_idle` (keeps the session alive without evaluating).
//! - `reset`: `rc_client_reset`.
//! - `state_loaded`: in hardcore mode, loading states, rewind and fast
//!   forward are forbidden; in softcore, `rc_client_deserialize_progress`.
//!
//! The system must be among RetroAchievements' consoles for there to be
//! official achievements; the map in `memory.rs` follows the format they use.
#![allow(dead_code)] // groundwork for RetroAchievements: used once rcheevos is in

use emu_api::Core;

#[derive(Default)]
pub struct Achievements {
    /// Future: the rc_client session. Without it, everything here is a no-op.
    active: bool,
}

impl Achievements {
    pub fn game_loaded(&mut self, _rom: &[u8]) {}

    pub fn frame(&mut self, _core: &dyn Core) {
        if !self.active {
        }
        // rc_client_do_frame(...)
    }

    pub fn reset(&mut self) {}

    pub fn state_loaded(&mut self) {}

    /// Hardcore mode: blocks loading states, rewind and fast forward.
    pub fn hardcore(&self) -> bool {
        false
    }
}
