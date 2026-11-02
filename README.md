# V.Smile Emulator

Emulator of the **VTech V.Smile** (SunPlus SPG200 SoC, unSP 16-bit CPU), written in Rust. The name is provisional.

**Status: 0.1.0, early.** The core boots most standard cartridges (see [`docs/core/COMPATIBILIDADE.txt`](docs/core/COMPATIBILIDADE.txt), measured in the original core repository), but this build has not been played through with real games yet. V.Smile Baby games do not boot; V.Smile Motion games boot without the motion sensor.

ROMs and BIOS are not included.

## What is here

| Crate | What it is |
|---|---|
| `crates/vsmile-core` | the machine (CPU, PPU, SPU, DMA, timers, UART/joystick) and `core.rs`, its adapter to the `emu-api` contract |
| `crates/vsmile` | the desktop app (ORBE Shell interface: library with covers, save states, rewind, debugger, cheats) |
| `crates/vsmile-libretro` | the same core as a libretro core for RetroArch |
| `crates/vsmile-runner` | headless runner: N frames to a screenshot, batch compatibility, disassembler, debugger |
| `crates/vsmile-wasm` + `web/` | the core in the browser (WebAssembly) |
| `crates/api`, `crates/frontend` | the shared contract and interface (from the internal base) |

## Run

```
cargo run --release -p vsmile -- GAME.bin      # or .zip / .7z, or a folder
cargo run --release -p vsmile-runner --bin vsmile-run -- --cart GAME.bin --frames 600 --out shot.bmp
```

**BIOS (optional):** `vsmile_v100.bin`, `vsmile_v102.bin` or `vsmile_v103.bin` (MAME set `vsmile`), found by CRC32 in the BIOS folder. Without it games boot from the cartridge, but a few call routines in the BIOS.

**Controls (RetroPad):** stick = D-pad, Enter = Start, red/yellow/blue/green = Y/X/B/A, Exit = L, Help = Select, ABC = R. Two controllers.

**Art Studio** cartridges keep their drawings in battery-backed SRAM, saved as `.sav`.

## Build

```
sudo apt install pkg-config libasound2-dev   # Debian/Ubuntu (audio)
cargo build --workspace
cargo test --workspace
```

The libretro core: `cargo build --release -p vsmile-libretro` (with `crates/vsmile-libretro/vsmile_libretro.info`).

## Documentation

- [`SISTEMA.md`](SISTEMA.md): the system sheet (resolution, frame rate, audio, controls, memory).
- [`docs/core/`](docs/core/): history and technical notes of the core (Portuguese), from the original repository.
- [`docs/guia.md`](docs/guia.md): how the shared base is organised.

## License

GPL-3.0-or-later ([`LICENSE`](LICENSE)). Hardware behaviour was studied in veesem (ISC), VFrown (MIT) and MAME (BSD-3-Clause); no code was copied. V.Smile and VTech are trademarks of their owners; this project is not affiliated with VTech.
