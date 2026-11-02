<!--
  Kit template (see docs/kit-docs.md). Replace the {{PLACEHOLDERS}} with tools/new-docs.sh and
  resolve every TODO(...). Delete this comment. This guide is the same for every emulator
  that uses emu-frontend: the interface texts and the steps do not depend on the system.
-->
# Translating {{NAME}}

<!-- TODO(translate): add the language switcher line (see docs/kit-docs.md) once translations exist -->

{{NAME}} comes in English, Portuguese and Spanish. <!-- TODO(system): list the built-in languages --> Any other language can be added
**without programming and without compiling anything**: the translation is a single JSON
file that the emulator reads at startup. You can keep it for yourself or send it to be
included in the next version, so everyone gets it.

## Contents

1. [What you need](#1-what-you-need)
2. [Step by step](#2-step-by-step)
3. [The file format](#3-the-file-format)
4. [Rules that keep the interface working](#4-rules-that-keep-the-interface-working)
5. [Testing your translation](#5-testing-your-translation)
6. [Correcting an existing language](#6-correcting-an-existing-language)
7. [When a new version adds texts](#7-when-a-new-version-adds-texts)
8. [Sending it to be included](#8-sending-it-to-be-included)
9. [Game info sheets (optional)](#9-game-info-sheets-optional)
10. [Questions](#10-questions)

## 1. What you need

- {{NAME}} **{{VERSION}} or newer**.
- A text editor that saves **UTF-8** (any modern one: VS Code, Notepad++, Kate, gedit,
  micro…). Avoid word processors such as Word or LibreOffice Writer.
- About two to four hours: there are a few hundred texts, most of them short (menu items,
  buttons).

## 2. Step by step

1. **Export the template.** In the emulator, open **Help › Language / Idioma › Export
   translation template…** and save the file. It is already offered in the languages folder.
2. **Name the file after the language code**, in lower case:
   `fr.json` (French), `de.json` (German), `it.json` (Italian), `ja.json` (Japanese),
   `pt-PT.json` (European Portuguese), `zh-CN.json` (simplified Chinese)… The name is what
   tells the emulator which language it is. Use letters, digits and `-` only.
3. **Translate.** Open the file in the text editor and replace each value (the text on the
   right) with its translation. Keep the texts on the left as they are. Details in the next
   two sections.
4. **Write the language's name** in `"@name"`, the way its speakers write it: `"Français"`,
   `"Deutsch"`, `"日本語"`. This is what appears in the language menu.
5. **Put the file in the languages folder** and restart the emulator. To find the folder:
   **Emulation › Folders…**, line **Languages**, button **Open**. By default it is:

   | System | Languages folder |
   |---|---|
   | Linux, FreeBSD | `~/.config/{{ID}}/languages/` |
   | Windows | `%APPDATA%\{{ID}}\languages\` |
   | Portable mode | `{{ID}}_data/languages/`, next to the program |

6. **Choose it** in **Help › Language / Idioma**. Your language is listed after the
   built-in ones.

That is all it takes to use it yourself. To share it, see [section 8](#8-sending-it-to-be-included).

## 3. The file format

The template is a list of pairs `"original text": "translation"`. Freshly exported, each
translation is a copy of the English text:

```json
{
  "@name": "Language name",
  "Open game…": "Open game…",
  "Search {n} games…": "Search {n} games…",
  "Screenshot saved: {file}": "Screenshot saved: {file}"
}
```

Translated into French, it becomes:

```json
{
  "@name": "Français",
  "Open game…": "Ouvrir un jeu…",
  "Search {n} games…": "Rechercher parmi {n} jeux…",
  "Screenshot saved: {file}": "Capture enregistrée : {file}"
}
```

- **The left side is the key**: the English text exactly as it is in the program. Never
  change it, or the emulator will not find your translation and will show English.
- **The right side is what is shown.** It can be left as it is (identical to English) or
  **emptied** (`""`), which means "not translated yet": English is shown in its place. So an
  incomplete file already works, and you can translate in several sessions.
- **The order of the lines does not matter**, and lines can be removed (they show English).

## 4. Rules that keep the interface working

1. **Keep the fields between braces** exactly as they are: `{n}`, `{name}`, `{file}`,
   `{path}`, `{error}`, `{system}`… The program replaces them with a number, a name or a
   path. They can move inside the sentence (`"{n} jeux"`), but not be translated or removed.
2. **Keep the "…"** at the end of menu items that open a window (`"Folders…"` →
   `"Dossiers…"`). It is a convention: "…" means "opens something where you will choose".
3. **Keep the "›"** in menu paths (`Emulation › Controls and shortcuts…`) and translate the
   names on both sides the same way you translated those menus, so people can follow them.
4. **Keep the JSON valid:** each line ends with a comma except the last; quotes inside a
   text are written `\"`; a line break inside a text is `\n`. If the file has an error, the
   emulator shows a message at startup with the file name and the reason, and ignores that
   file (the others still load).
5. **Short texts stay short.** Menu items and buttons have limited room. If the
   translation is much longer than the English, look for a shorter word.
6. **Do not translate** names of things: {{NAME}}, {{SYSTEM}}, RetroArch, libretro,
   RetroAchievements, the file formats (`.{{EXT}}`, `.cht`, PNG, MP4,
   GIF), key names (`Ctrl`, `Esc`, `F2`) and the folder names `Named_Boxarts`,
   `Named_Titles` and `Named_Snaps`.
7. **Technical terms:** use what players of your language actually say. "Save state",
   "ROM", "cheat" and "rewind" are often kept in English; when in doubt, look at how
   RetroArch is translated into your language and follow it.
8. **The keyboard and controller labels** (`KEYBOARD`, `CONTROLLER`, `EMULATOR`…) are column
   headers shown in capitals: keep them in capitals.

## 5. Testing your translation

After restarting with your language chosen, walk through these screens; together they show
nearly every text:

- [ ] every menu of the menu bar, including the submenus;
- [ ] the library (search, the image tabs, right-click on a game, the welcome screen when
      the folder is empty);
- [ ] a game running: the messages at the top (save state, screenshot, fast forward);
- [ ] **Emulation › Controls and shortcuts…** and **Help › Controls**;
- [ ] **Emulation › Folders…**;
- [ ] **Tools › Debugger…** and **Tools › Cheats…** (both tabs);
- [ ] **Help › Game info…** and **Help › About**;
- [ ] the quick menu (Home/Guide button on a controller, or **Video › Couch mode** and Esc).

Look for cut words, texts that overflow their button, and English left behind (a key that
was changed by accident, or a value still empty).

## 6. Correcting an existing language

To fix a few texts of a built-in language (Portuguese or Spanish), you do not need the whole
template: create `pt.json` or `es.json` in the languages folder with **only the lines you
want to change**. Your texts take the place of the built-in ones; everything else stays.
`"@name"` is ignored for built-in languages. If the fix is right for everyone, send it too
([section 8](#8-sending-it-to-be-included)).

## 7. When a new version adds texts

New versions add and change texts. To bring a translation up to date:

1. Export the template again from the new version.
2. Compare it with your file. A text-comparison tool helps: the "compare files" view of
   VS Code, Meld, WinMerge or `diff`. Lines that exist only in the new template are the new
   texts.
3. Copy the new lines into your file and translate them. Lines that exist only in your file
   are texts the program no longer uses: they are harmless, but can be deleted.

Until you do that, the new texts simply appear in English.

## 8. Sending it to be included

Translations sent in are added to the next version, built into the program, with credit to
the translator in the README and in the release notes.

**How to send**, in order of preference:

1. **An issue on GitHub:** open
   <{{REPO}}/issues/new>, title
   `Translation: <language>` (for example `Translation: Français`), and attach the `.json`
   file (if GitHub refuses the attachment, compress it as `.zip` first). Say how you want to
   be credited: name or nickname, and optionally a link.
2. **A pull request**, if you use Git: put the file in `translations/<code>.json` and open
   the pull request. The maintainer splits it into the program's catalogs.

**Before sending**, check:

- [ ] the file name is the language code (`fr.json`);
- [ ] `"@name"` is filled in with the language's own name;
- [ ] the emulator loads it without an error message, and the screens of
      [section 5](#5-testing-your-translation) look right;
- [ ] it was exported from the **latest version** (or updated as in
      [section 7](#7-when-a-new-version-adds-texts)).

A partial translation is welcome too: say so in the issue, and the missing texts will show
in English until someone completes them.

**License:** {{NAME}} is {{LICENSE}}. By sending a translation you agree that it is
distributed under the same license, like the rest of the program.

## 9. Game info sheets (optional)

The descriptions in **Help › Game info…** (what the game is about, notes) are not in the
template: they belong to the game database, one file per language, in the repository, at
`assets/games/` (`games.en.json`, `games.pt.json`, `games.es.json`) <!-- TODO(system): only if the system has a game database; otherwise delete this section -->. For a language
without them the emulator shows them in English, so translating them is not needed for the
interface to be complete. (The number of players is not a text: its phrase, "1 or {n}
players", is in the interface template.)

To translate them as well, copy `games.en.json` to `games.<code>.json`, translate only the
values of `"description"` and `"notes"` (keep the game keys such as `"game-id"` and the
field names), and send it in the same issue. These are built into the program, so they only
appear after the next version.

## 10. Questions

Open an issue on GitHub with the title `Translation question: <language>`. Questions about
the meaning of a text are welcome: some are short and depend on the screen they are on.
