# Changelog

All notable changes to V.Smile Emulator.

## [0.1.0] - 2026-11-02

First version on the shared base (emu-api + ORBE Shell interface).

- Core from `PericlesSavio/_vsmile` (commit 7620b41): unSP CPU, SPG200 PPU, SPU (PCM/ADPCM), DMA, timers, joystick over UART, save states, Art Studio NVRAM.
- `core.rs` adapter: RGB555 to XRGB8888, SPU audio (281 250 Hz) low-pass and resampled to the output rate, RetroPad mapping for two V.Smile joysticks, optional BIOS (v100/v102/v103, newest wins), Art Studio SRAM as battery save (recognised by CRC32), internal RAM for debugger and cheats.
- Game database: 455 games by CRC32 with No-Intro names (libretro-database), used for titles and covers.
- Desktop app, libretro core, headless runner and WebAssembly build.
- The minifb frontend of the original repository was replaced by the shared interface.
- Not yet tested with real games in this repository.
