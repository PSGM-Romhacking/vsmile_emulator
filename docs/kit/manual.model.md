<!--
  Kit template (see docs/kit-docs.md). Replace the {{PLACEHOLDERS}} with tools/new-docs.sh and
  resolve every TODO(...). Delete this comment. The interface is the same for every emulator: most of this is written; adjust the details.
-->
# Manual

<!-- TODO(translate): language switcher line (see docs/kit-docs.md) -->

Everything the emulator does and how to use it. Download, build and default controls are in
the [README](../README.md).

## Emulator features

<!-- Screenshots: generate with the app itself (docs/kit-docs.md, "Screenshots"). -->
| | |
|---|---|
| ![Library: gameplay view](../screenshots/library-screens.png) | ![Game info sheet](../screenshots/game-info.png) |
| Library: gameplay view | Game info sheet |
| ![Debugger: memory](../screenshots/debugger-memory.png) | ![Cheats](../screenshots/cheats.png) |
| Debugger: memory | Cheats |

### Library

It is what the window shows when no game is open:
- grid with the **box art** or with the **title screen / gameplay** images;
- search, change folder, and click to open;
- **F5** (or **File › Refresh game list**) reads the ROM folder again, for games added,
  removed or renamed with the emulator open;
- **first run:** when the folder has no games, a welcome screen with the **Choose the ROM
  folder…** button (or the controller's A) and links to **free, legal games**
  (`Brand.free_games`);
- right-click: **Open** and **Game info…**;
- games are recognized **by content** (CRC32), so the file name does not matter;
- box art comes from **libretro-thumbnails**, downloaded in the background and cached,
  looked up by the ROM's No-Intro name, as in RetroArch.

Accepted formats: `.{{EXT}}`, and those same ones inside `.zip` or `.7z`.
<!-- TODO(system): why these extensions; the box shape in the library (SystemInfo::cover_aspect). -->

### Game info

Title, original title, year, publisher, developer, product code, file name and full path,
and a description in the current language, from the project's game database
(`assets/games/`; format in [`docs/sheets.md`](sheets.md)).
<!-- TODO(system): how many games have a sheet and where the database came from. Delete if the system has no game database. -->

### Playing

| Feature | Detail |
|---|---|
| **Save states** | 9 slots per game, with a thumbnail of the saved moment and date/time; the file stores the ROM's fingerprint and rejects a state from another game |
| **Rewind** | one state every 2 frames, up to ~30 s; hold Backspace or L2 <!-- TODO(system): memory it takes, from the size of the core's state --> |
| **Fast forward** | hold (Space / R2) or toggle on and off; 2x, 3x, 4x or 8x |
| **Pause** | P or Guide; pauses by itself when unfocused (optional) and with a menu or dialog open |
| **Players** | the 1st controller is P1, the 2nd is P2…; the keyboard has P1 and P2 (P2 on WASD) <!-- TODO(system): the number of players --> |
| **Remappable controls** | each action has a key and one input per controller, saved by driver name |

### Video

| Feature | Detail |
|---|---|
| Window 1x–6x | N screen pixels per game pixel, exact even on Wayland |
| Integer scaling | integer multiples only (uniform pixels) or filling the window |
| Filters | None, Scanlines, CRT |
| **Frames** | fill the sides in fullscreen: <!-- TODO(system): the bezels the app registers (Brand.bezels), plus "Game cover" (blurred and darkened), which is always there --> |
| Fullscreen | F11; the menu bar disappears and comes back when the mouse touches the top |
| FPS and speed | optional indicator |

### Sound

<!-- TODO(system): the default volume and mute, the audio rate, any smoothing/filters, the channels the debugger can mute. -->

### Recording

| Feature | Detail |
|---|---|
| Screenshot | F12: PNG in `~/Pictures/{{NAME}}/` |
| GIF | F9 |
| MP4 with sound | F10: H.264 + AAC via `ffmpeg`, at the exact frame rate (`Core::fps()`) |

Recordings go to `~/Videos/{{NAME}}/` (the XDG Pictures and Videos folders; the localized
name follows the system). Encoding runs in a separate thread, and closing the program in
the middle of a recording finishes the file before exiting.

### Tools

- **Debugger** (separate window): live RAM as a hex editor, with the bytes that changed in
  the last frame highlighted and an inspector with editing; plus whatever the core adds
  through `Core::debug_views()` (<!-- TODO(system): e.g. tiles, registers, video state, sound channels with a mute for each -->).
  Run state: running / paused, pause, advance 1 frame.
- **Cheats**, in two tabs:
  - **Codes:** the game's codes (editable value, live current value, **Write now**,
    **Remove** and **Hold**, which locks the value every frame) and the **new code** form;
  - **Search the RAM:** start the search; play until the value changes and say what
    happened (Increased, Decreased, Changed, Did not change, Is equal to), repeating until
    few addresses are left; **Use** takes the chosen address to the Codes tab.
  - Values in decimal or hexadecimal with `0x`. Saved per game in RetroArch's `.cht`
    format, which also works in RetroArch with the libretro core.
  - <!-- TODO(system): a tested example (game, address, value), and quirks such as scores stored in BCD. -->

### Controller and couch mode

To play from the couch, on a TV or a handheld, with no keyboard or mouse:

- **Quick menu, the controller's "Esc":** opens and closes anywhere with the
  **Home/Guide** button, or **Select + Start held for half a second** (for controllers
  without Home; optional), or **Esc** in couch mode. It pauses the game and offers:
  Continue, Save state, Load state, Reset, Back to the library, **Couch mode** and Quit.
- **Library by controller:** the d-pad (or the left stick, or the arrow keys) moves a
  selection between the covers; the bottom button opens the game; **Select tapped alone**
  changes the image (cover, title, gameplay).
- **Couch mode** (**Video › Couch mode**, or `--couch`): fullscreen, no menu bar, bigger
  covers and a line of button hints. It stays saved until it is turned off in the quick menu.
- **The button doesn't "leak" into the game:** while Select + Start are held together, the
  game doesn't get them; and after opening a game or closing the menu with the controller,
  game input is ignored until every button is released.

### Interface

- Desktop-style menu bar (File, Emulation, Audio, Video, Recording, Tools, Help).
- **Languages:** English (default), Portuguese and Spanish, switched live; more languages
  need only a JSON file ([`docs/translating.md`](translating.md)).
- Preferences, ROM folder, recent games, keys and controllers are saved.
- The window never opens bigger than the screen (a large scale on a small screen is shrunk to fit).
- <!-- TODO(system): its own icon and the entry in the applications menu (make install), if the Makefile has it. -->

## Installation and usage

### What it takes to run

Just the executable: icon, translations, game database, fonts, theme and frames are
embedded. When run, it creates:

| Folder | Contents |
|---|---|
| `~/.config/{{ID}}/config.json` | preferences, ROM folder, recent games, keys and controllers |
| `~/.config/{{ID}}/states/` | save states and thumbnails |
| `~/.config/{{ID}}/cheats/` | cheats (`.cht`) |
| `~/.config/{{ID}}/covers/` | your own box art (optional) |
| `~/.config/{{ID}}/languages/` | your own translations (optional) |
| `~/.config/{{ID}}/cache/covers/` | downloaded box art (can be deleted at will) |
| `~/.config/{{ID}}/usage.json` | play time and play history |
| `~/.config/{{ID}}/crashes/` | error reports, only if the program ever closed because of an error |
| `~/Pictures/{{NAME}}/`, `~/Videos/{{NAME}}/` | screenshots and recordings |

Everything except screenshots and recordings is in **one folder**, so a backup or a sync
needs only `~/.config/{{ID}}/`. On Windows it is `%APPDATA%\{{ID}}\`, with the same layout.

### Portable mode

Create an empty file named **`portable.txt`** next to the executable, and everything
(preferences, saves, cheats, box art, languages, screenshots and recordings) goes into a
**`{{ID}}_data/`** folder next to it instead of your user folder. Paths inside the program's
folder are saved relative to it, so the folder keeps working on another PC or a USB stick.
If the folder can't be written to, the emulator says so at startup and uses the normal one.

### Reporting a problem

If the emulator closes because of an error, it writes a report to
`~/.config/{{ID}}/crashes/<date>.log` (version, system, the game that was open, the error and
where in the code it happened), and the next start shows its path with an **Open folder**
button. Attach that file when reporting the problem. It contains the game's path, which may
include your user name.

### Folders, your own box art and new languages

**Emulation › Folders…** lists every folder (ROMs, save states, cheats, own box art,
languages, screenshots, recordings), each with **Open**, **Change…** and **Default**.

**Own box art.** Put PNGs in the box art folder with the libretro-thumbnails layout:
`Named_Boxarts/` (cover), `Named_Titles/` (title screen) and `Named_Snaps/` (gameplay). The
file name is the ROM's No-Intro name. Your images win over the downloaded ones.

**New languages.** **Help › Language / Idioma › Export translation template…** saves a JSON
file with every text of the interface; translate the values, name the file after the
language code (`fr.json`), put it in the languages folder and restart. The full guide, with
the rules and **how to send a translation**, is in [`docs/translating.md`](translating.md).

### Command line

```bash
{{ID}}                       # opens the library
{{ID}} game.{{EXT}}          # opens a game (also .zip, .7z)
{{ID}} /path/to/folder/      # opens the library in that folder
{{ID}} game.{{EXT}} --scale 4
```

<!-- TODO(system): the other options the app implements (--couch, --identify, --info, --controls, --memory-map, --bench, --shot, --dump-ram…). The ones that are not in main.rs do not exist. -->
