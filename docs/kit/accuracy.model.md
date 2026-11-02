<!--
  Kit template (see docs/kit-docs.md). Replace the {{PLACEHOLDERS}} with tools/new-docs.sh and
  resolve every TODO(...). Delete this comment. Everything here depends on the system: it is
  the skeleton of Mil Capivaras' docs/accuracy.md. Write it as you validate the core, not at
  the end, and describe the current state (what changed and when goes in the CHANGELOG).
-->
# Accuracy and validation

<!-- TODO(translate): language switcher line (see docs/kit-docs.md) -->

How {{NAME}} is compared with the reference and with the real console, the result, and what
is still missing.

The references in brackets ([W1], [M1]…) are in [References](references.md).

## How the comparison is done

<!-- TODO(system), in plain words first (someone who does not know emulation must follow it):
  - What the reference is (the original core, another emulator, a simulation of the circuit,
    measurements on the real console) and why it is trustworthy.
  - How both are run with the same input: an automatic player (which buttons, at which
    frames), the same in the app (--press-start) and in the reference.
  - What is compared and when: picture (pixel by pixel), RAM (byte by byte), at which frames.
  - How a difference is reported (numbers, not impressions) and how its origin is found
    (bisect to the first differing frame, then an instruction-by-instruction trace).
  If the reference is in the repository, link it: [`reference/`](../reference/README.md). -->

## Limits of the reference

<!-- TODO(system): the part that matters most for reading the numbers. Say, with the sources:
  - Whether the reference is the console or a reconstruction (an FPGA core, another emulator).
    "Identical to the reference" then means two independent reconstructions agree: evidence
    that both are right, not proof; if they agree on a mistake, the comparison does not see it.
  - That a disagreement does not say who is wrong: it may be in the emulator, in the reference
    or in the way the reference is run. A third source (another emulator, a measurement)
    settles it.
  - How old and how tested the reference is (its history of fixes, who validated it, against
    what).
  - The limits of your own harness: fixed cartridge delay, no input, no sound, speed, what is
    not compared. -->

## Methodology (technical details)

<!-- TODO(system): the exact commands, the reference's own fixes (bugs found in the reference
  itself, fixed before comparing), and the cautions learned while comparing (e.g. snapshots
  taken in the middle of an instruction). -->

## Result

<!-- TODO(system): the numbers: how many games, which frames, how many differing pixels and
  bytes. Say which ROM versions were used (homebrews change: a stale ROM looks like an
  emulator bug). Then a table of what had to be fixed to get there: problem, where, effect. -->

## Classification

<!-- TODO(system): the level on the Emulation General Wiki scale (low / medium / high /
  cycle-accurate / subcycle), why, and honest caveats (identical to the reference is not the
  same as identical to the console). -->

## Validation against the console

<!-- TODO(system): measurements taken on a real console (by you or by others, with their test
  programs), test ROMs with photos, and what matches. Skip if none exist. -->

## Status

| # | Item | Status |
|---|---|---|
| 1 | <!-- TODO(system): a known difference from the reference or the real hardware --> | pending / **done:** the test that proves it / no change needed: why |

## How each part was validated

<!-- TODO(system): a table of the parts of the emulation (picture, windows or special layers,
  CPUs, interrupts and timing, sound, controls, battery save, ROM loading) with the proof of
  each and how strong it is (strong / partial / none). Be explicit about the parts that have
  no proof: sound with no reference, a CPU with no instruction test suite. -->

| Part | Proof | Strength |
|---|---|---|
| <!-- TODO(system) --> | | strong / partial / none |

## Each item in detail

<!-- TODO(system): one sub-section per pending item: what is approximated, what could not be
  verified, why, and what it would take. -->

## Suggested order

<!-- TODO(system): what is left, from the cheapest to the most expensive, with effort and
  gain. Example columns: item, effort, gain. -->

| # | Item | Effort | Gain |
|---|---|---|---|
| <!-- TODO(system) --> | | | |
