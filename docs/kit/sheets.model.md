<!--
  Kit template (see docs/kit-docs.md). Replace the {{PLACEHOLDERS}} with tools/new-docs.sh and
  resolve every TODO(...). Delete this comment. Part A (the texts) is the same for every
  emulator that uses emu-frontend; part B (the data file) is the core's own, and this is the
  format Mil Capivaras uses.
  If the system has no game database, delete this file and the "Game info" bits of the README.
-->
# Game database

<!-- TODO(translate): language switcher line (see docs/kit-docs.md) -->

The info sheet (right-click the game › **Game info…**, or **Help › Game info…**) and the
game's name and box art in the library come from the **game database**, embedded in the
program. It is kept apart from the emulator's own texts (menus, buttons), which are the
interface translations in `assets/i18n/`.

| File | What it has |
|---|---|
| `assets/games/games.json` | the data of each game, the same in every language (part B) |
| `assets/games/games.en.json` | the texts of each game in English, the source text (part A) |
| `assets/games/games.<code>.json` | the same texts in another language (part A) |

<!-- TODO(system): where the database came from (a DAT, MAME's software list, No-Intro...), its license, and how it is kept up to date. -->

## A. The texts: `games.<code>.json`

Read by the interface. The core hands them over in `SystemInfo::game_texts`
(`&[("en", include_str!(…)), …]`), keyed by the game's id (`GameInfo::id`).

```json
{
  "game-id": {
    "genre": "Platform",
    "description": "What the game is about…",
    "notes": "…"
  }
}
```

| Field | Notes |
|---|---|
| key (`game-id`) | the game's id: lower case, no spaces; the same key in `games.json` and in every text file |
| `genre` | a word or two, translated in each file; shown on the sheet and in the library's list view |
| `description` | shown on the sheet |
| `notes` | shown at the end of the sheet |

All fields are optional. Without the text in the current language, the sheet shows
English. An unknown field (a typo such as `descripton`) is an error. A new language is a new
`games.<code>.json` with the same games ([translating.md](translating.md)).

## B. The data: `games.json`

Read by the core (`games.rs`), which answers `SystemInfo::game_info(rom)` with a `GameInfo`
(`emu-api`): `id`, `title`, `alt_title`, `year`, `publisher`, `developer`, `serial`,
`players`, `thumbnail`. A format that works, as used by Mil Capivaras:

```json
{
  "game-id": {
    "title": "Title",
    "alt_title": "Original title",
    "year": "1983",
    "publisher": "…",
    "developer": "…",
    "serial": "…",
    "players": 2,
    "roms": [{ "name": "Title (Japan)", "crc": "01234567", "size": 8192 }]
  }
}
```

| Field | Notes |
|---|---|
| key | the game's id; the same key in the text files |
| `title` | required |
| `alt_title` | original title (e.g. in Japanese) |
| `year`, `publisher`, `developer` | |
| `serial` | cartridge or product code |
| `players` | most players at once: only the number; the phrase ("1 or 2 players") comes from the interface translation |
| `roms` | the game's ROMs: one, or one per version |
| `roms[].name` | the ROM's No-Intro name: the name of its box art, title and gameplay images, as in RetroArch ([libretro-thumbnails](https://thumbnails.libretro.com/)) |
| `roms[].crc` | CRC32 of the ROM (`emu_api::crc32`): this is how the game is recognized, whatever the file name |
| `roms[].size` | size in bytes, when known |

<!-- TODO(system): how to find a ROM's CRC in this emulator (a --identify command line option, if the app has one), and what the sheet shows for a ROM that is not in the database. -->

## Adding a game

1. Find its CRC.
2. Add an entry to `games.json`.
3. Add its texts to every `games.<code>.json`.
4. Check: `cargo test --workspace` and open the game's info sheet.

## Rules (put them in the tests)

- Every text file has **the same games and the same fields**; only the text changes.
- Every game in the text files exists in `games.json`; no two games share a CRC.
- Only verifiable content goes in: developer with a source, description from the author
  or from the game.

**Non-Latin titles:** the interface embeds a subset of Noto Sans CJK JP
(`assets/fonts/`, SIL OFL): kana, punctuation and only the kanji already in use. A title
with new kanji requires regenerating the subset.
