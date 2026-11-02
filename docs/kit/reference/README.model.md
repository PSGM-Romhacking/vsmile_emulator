<!--
  Kit template (see docs/kit-docs.md, "Validation with a reference"). Written by
  tools/new-docs.sh --reference into reference/README.md, together with reference/compare.sh.
  Resolve every TODO(...). Delete this comment. Model: Mil Capivaras' reference/README.md (the
  MiSTer core simulated with Verilator).
-->
# Reference: <!-- TODO(system): what it is, e.g. "the MiSTer core simulated with Verilator" -->

<!-- TODO(translate): language switcher line (see docs/kit-docs.md) -->

The tool that validates {{NAME}}. <!-- TODO(system): what the reference is (link to its
origin), why it is trustworthy, and that it runs a ROM with the same input as the emulator
and saves the picture and the RAM, so the two can be compared byte by byte. --> The method
and the results are in [Accuracy and validation](../docs/accuracy.md).

It is not part of the emulator: nothing here is built by `cargo build`.

## Before you use it: what this reference is and is not

<!-- TODO(system): the same three points as "Limits of the reference" in accuracy.md, short:
  a reconstruction is not the console; a disagreement does not say who is wrong; how old and how
  tested the reference is. Then the limits of the harness itself (fixed cartridge delay, no
  input, no sound, speed, what the RAM comparison means). -->

## Use

<!-- TODO(system): requirements and how to build the reference into reference/ref (or set REF). -->

```bash
cargo build --release
reference/compare.sh target/release/{{SLUG}} roms/*.{{EXT}}
```

`compare.sh` runs each ROM in both, with `--press-start` (<!-- TODO(system): the scripted
input, identical in the app and in the reference -->), and prints the differing pixels and
RAM bytes at each frame (0 = identical). The frames are 300 and 3000 by default; another
list goes in `FRAMES`, e.g. `FRAMES="2 50 300 1000 3000"`. <!-- TODO(system): the measured
time per game. -->

## Alignment

<!-- TODO(system): the emulator and the reference rarely reach the same frame at the same time
  (a fixed cartridge delay in the simulation changes the boot time of each game). Say how it is
  handled: compare frame N of the reference with a window of the emulator's frames and keep the
  best (fewest differing pixels, then fewest differing RAM bytes), and print the offset. On a
  still picture many frames give 0 pixels, so the offset is not reliable; the RAM settles it.
  The Verilator example in docs/kit/reference/verilator/compare.py does this (--offset,
  --window, --cache). -->

## What to expect

<!-- TODO(system): the result on the games of the compatibility list (in the ROM versions
listed there), and every known, explained difference (e.g. stack leftovers at intermediate
frames), with a link to where it is explained. -->

## Files

| File | What it is |
|---|---|
| `compare.sh` | runs the reference and the emulator and compares them |
| <!-- TODO(system): every file, and the changes made to third-party code (as a patch) --> | |

## Licenses

| Part | License |
|---|---|
| <!-- TODO(system): the reference's own license (a GPL-2.0-only core forces GPL-2.0-or-later on the code compiled with it) --> | |

The reference is a separate program: it is not linked to the emulator.
