<!--
  Kit template (see docs/kit-docs.md): the emulator's README, short, as in Mil Capivaras:
  what it is, download, controls, build, an index of the documentation and the licenses.
  Everything else lives in docs/. Replace the {{PLACEHOLDERS}} with tools/new-docs.sh and
  resolve every TODO(...). Delete this comment.
-->
# {{NAME}}

<!-- TODO(translate): language switcher line: **English** · [Português](README.pt-BR.md) · [Español](README.es.md) -->

**{{NAME}}** is an emulator of the **{{SYSTEM}}** (<!-- TODO(system): year, maker -->) written in **Rust**.
<!-- TODO(system): in two or three lines: what it was reimplemented from, how it was validated, what it runs (counts of games), the headline numbers. -->
It has a complete graphical interface:
- library with box art;
- save states and rewind;
- GIF and MP4 recording;
- debugger and cheats;
- 3 languages.

It also ships as a **libretro core**, for RetroArch.

![Library with box art](screenshots/library-covers.png)

<!-- TODO(system): why the name, and the icon, if there is a story. -->

| | |
|---|---|
| Accuracy | <!-- TODO(system): level and how it was measured ([accuracy](docs/accuracy.md)) --> |
| Compatibility | <!-- TODO(system): N/N games, how it was checked ([list](docs/compatibility.md)) --> |
| Speed | <!-- TODO(system): frames/s on which CPU, and the multiple of real time --> |
| Platforms | Linux (Wayland and X11, AppImage) and Windows; RetroArch on any system that runs the core |
| License | {{LICENSE}} ([`LICENSE`](LICENSE)) |


## Download

<!-- TODO(system): only when there is a release pipeline. The binaries are in the [releases]({{REPO}}/releases):
| File | System |
|---|---|
| `{{SLUG}}-{{VERSION}}-linux-x86_64.AppImage` | Linux x86-64 |
| `{{SLUG}}-{{VERSION}}-windows-x86_64.exe` | Windows 10 and 11, 64-bit |
| `{{ID}}_libretro-{{VERSION}}-linux-x86_64.zip` / `-windows-x86_64.zip` | the libretro core and its `.info` |
| `SHA256SUMS` | checksums of every file |
On Linux: `chmod +x` the AppImage and run it. Each version's notes come from [`CHANGELOG.md`](CHANGELOG.md). -->

## Default controls

| Action | Keyboard | Controller |
|---|---|---|
| Directions | arrow keys | d-pad and left analog stick |
| <!-- TODO(system): the system's buttons and their keys --> | | |
| Pause / reset | P / F1 | — |
| Quick menu (open and close) | Esc (in couch mode) | Home/Guide; Select + Start held for half a second (option) |
| Save / load state; slot −/+ | F2 / F4; F6 / F7 | — |
| Fast forward / rewind (hold) | Space / Backspace | R2 / L2 |
| Sound; volume +/− | M; = / − | — |
| Fullscreen; screenshot; GIF; MP4 | F11; F12; F9; F10 | — |
| Open / close game / quit | Ctrl+O / Ctrl+W / Ctrl+Q | — |
| Refresh game list (reads the ROM folder again) | F5 | — |

Everything can be remapped in **Emulation › Controls and shortcuts…**.

## Build

```bash
cargo build --release -p {{ID}}
cargo build --release -p {{SLUG}}-libretro    # the libretro core
```

Requirements: Rust (cargo) and the ALSA audio library (`libasound2-dev` and `pkg-config`
on Debian and Ubuntu, `alsa-lib-devel` on Fedora and openSUSE, `alsa-lib` on Arch).
Optional at runtime: `ffmpeg` (MP4 recording), `curl` (downloading box art) and
`xdg-open` (opening folders). Without them, only those features are unavailable.

## Reporting a problem

If {{NAME}} closes because of an error, the next start shows where the report was saved
(`~/.config/{{ID}}/crashes/`). Attach it to the issue, with the version and what you were doing.
See [Manual › Reporting a problem](docs/manual.md#reporting-a-problem).

## Documentation

| Document | Contents |
|---|---|
| [Manual](docs/manual.md) | every feature; portable mode, folders, own box art, command line |
| [Compatibility](docs/compatibility.md) | the games tested and how |
| [Hardware](docs/hardware.md) | the {{SYSTEM}} in detail and how each emulator reproduces it |
| [Accuracy and validation](docs/accuracy.md) | how the emulator is compared with the reference; what is missing |
| [Architecture](docs/architecture.md) | the code, how the emulation works and the reasoning behind each decision |
| [libretro core](docs/libretro.md) | the core for RetroArch |
| [Game database](docs/sheets.md) | the games' info sheets: format and rules |
| [Translating](docs/translating.md) | how to translate the interface into a new language |
| [Lessons learned](docs/lessons.md) | what the project taught |
| [References](docs/references.md) | the sources cited in the documents |
| [Changelog](CHANGELOG.md) | the changes in each version |

<!-- TODO(system): drop the rows of documents the project does not have (sheets.md without a game database); add reference/README.md if there is a reference (tools/new-docs.sh --reference). -->

## Licenses

| Part | License |
|---|---|
| {{NAME}} | {{LICENSE}} ([`LICENSE`](LICENSE)) |
| Noto Sans CJK JP subset | SIL Open Font License 1.1 |
| <!-- TODO(system): the core's origin, vendored crates, game database --> | |

The ROMs are not part of the project.
