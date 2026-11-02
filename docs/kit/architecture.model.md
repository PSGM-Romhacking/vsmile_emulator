<!--
  Kit template (see docs/kit-docs.md). Replace the {{PLACEHOLDERS}} with tools/new-docs.sh and
  resolve every TODO(...). Delete this comment. The structure is the base's and is the same
  for every emulator; the parts about the system's own core are marked.
-->
# Architecture and how it works

<!-- TODO(translate): language switcher line (see docs/kit-docs.md) -->

How the code is organized, how the emulation works and the reasoning behind each decision.
{{NAME}} is a Cargo workspace with the console separate from the
interface; they only know each other through a small contract (`emu-api`).

## Structure

```
crates/api/             core ↔ frontend contract (no dependencies)
crates/{{SLUG}}-core/   {{SYSTEM}}: machine, memory, game database, tests
crates/frontend/        reusable frontend (egui, menus, library, controllers,
                        save states, rewind, recording, covers, languages, config)
crates/{{SLUG}}/        executable: core + frontend + branding (name, icon, About)
crates/{{SLUG}}-libretro/   {{ID}}_libretro.so = core + libretro API
assets/                 icons, fonts, bezels, i18n/<code>.json, games/
docs/                   the documentation (list in the README)
```

- `{{CORE_CRATE}}` → only `emu-api` (+ the libraries the emulation needs).
- `emu-frontend` → only `emu-api`: it knows no console.
- `crates/{{SLUG}}` ties the two together; the libretro core uses the core and `emu-api`,
  never the frontend.

<!-- TODO(system): the real core's dependencies (CPU crate, vendored forks and why). -->

## Contract (`emu-api`), following libretro conventions

| Aspect | Decision |
|---|---|
| Controls | **RetroPad** bitmask per player (B, Y, Select, Start, ↑ ↓ ← →, A, X, L, R, L2, R2, L3, R3); the system declares which ones it uses and the name of each |
| Video | `&[u32]` **XRGB8888** + width/height/aspect ratio per frame |
| Audio | **interleaved stereo f32**, rate chosen by the frontend |
| Save state | opaque bytes (`retro_serialize`); the file header (magic, ROM hash, thumbnail) belongs to the frontend |
| Memory | `system_ram()` (`RETRO_MEMORY_SYSTEM_RAM`) |
| Metadata | `SystemInfo`: name, extensions, libretro-thumbnails name, cover shape, buttons, FPS, resolution, identification by hash |
| Frame rate | `Core::fps()` (default: the one in `SystemInfo`) |
| Options | `Core::options()` / `set_option()`, like libretro *core options* |
| Debugging | `debug_views()`: images (with optional grid) and tables in groups |

<!-- TODO(system): what {{SYSTEM}} does with each row: how RetroPad maps to the native input, how the picture/audio are converted, the state format, the RAM exposed, the options, the exact FPS and where it comes from. -->

## The core crate

| File | Contents |
|---|---|
| `lib.rs` | `pub mod`s and `pub use core::{INFO, …Core}` |
| `core.rs` | the **adapter**: `static INFO: SystemInfo` and `impl emu_api::Core` (RetroPad → native input, picture → XRGB8888, audio → stereo f32, state ↔ bytes) |
| `machine.rs` + `machine/…` | the console itself: bus, ports, interrupts, video, the frame loop |
| `memory.rs` | memory map (CPU and flat addresses) |
| `games.rs` | game database and the games' texts |

<!-- TODO(system): the real file list and what each part emulates. -->

## The frontend

`emu-frontend` is shared as is. Each file in `app/` is an `impl App` block (emulation loop,
input, controller navigation, menus, screen, library, save states, recording, controls,
dialogs). It reads everything about the system from `SystemInfo`; the only system-specific
bits are what the app passes in: `brand::set(Brand { name, id, version, icon_png, about,
repo_url, bezels, free_games, thumbnails_url })` and the app's own language texts
(`i18n::add_catalog`).

## Tests

<!-- TODO(system): the core's tests (deterministic save state, memory map, frame comparison against the reference), the libretro C API test, how to run them, and which ones need ROMs that are not in the repository. -->

## How the emulation works

<!-- TODO(system): the heart of the emulation, as the code does it: time base, one frame, the
  CPU (which core/crate and what was fixed), video pixel by pixel, sound, save states.
  Link to the code (`file:line`) where it helps. -->

## The reasoning behind each decision

<!-- TODO(system): a table "Decision | Why". Every choice that someone would question:
  the reference chosen, language/libraries, resolution, frame rate, aspect ratio,
  identification database, what is embedded. Say what each choice costs. -->
