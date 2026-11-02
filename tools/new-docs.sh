#!/usr/bin/env bash
# Instantiates the documentation kit (docs/kit/*.model.md) for an emulator: fills in the
# {{PLACEHOLDERS}} and writes README.md, CHANGELOG.md and docs/{manual,hardware,compatibility,
# accuracy,architecture,libretro,sheets,translating,lessons,references}.md, ready to be
# completed. What depends on the system is left
# as <!-- TODO(system): ... --> blocks (the outline of what to write is inside each one).
#
# Usage: tools/new-docs.sh --name "Mil Nintendos" --system "Nintendo Entertainment System" \
#          --slug nes --ext nes --repo https://github.com/Org/mil_nintendos [options]
#   --name NAME        emulator name (required)
#   --system SYSTEM    the console's name (required)
#   --slug SLUG        crate slug, as given to new-emulator.sh (required)
#   --ext EXT          main ROM extension, without the dot (required)
#   --repo URL         repository URL (default: <repository>)
#   --id ID            identifier for folders and the libretro core (default: slug with "_")
#   --version V        version (default: from [workspace.package] in Cargo.toml)
#   --license TEXT     default: "GPL-3.0 or later"
#   --core-crate NAME  default: <slug>-core
#   --date YYYY-MM-DD  default: today
#   --no-game-db       skip docs/sheets.md (the system has no game database)
#   --reference        also write reference/README.md and reference/compare.sh (the
#                      validation against a reference, see docs/kit-docs.md)
#   --verilator        with --reference: also copy the Verilator example from V-Bear
#                      (reference/{sim_top.sv,sim.h,ref_main.cpp,Makefile,compare.py}), to adapt
#   --force            overwrite files that already exist (README.md and CHANGELOG.md
#                      exist after init-project.sh)
# Afterwards: grep -rn "TODO(system)" README.md docs/ lists what is left to write.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
kit="$root/docs/kit"
name=""; system=""; slug=""; ext=""; repo="<repository>"; id=""; version=""; license="GPL-3.0 or later"
core=""; date="$(date +%F)"; gamedb=1; force=0; reference=0; verilator=0
while (( $# )); do
  case "$1" in
    --name) name="${2:?}"; shift 2 ;;
    --system) system="${2:?}"; shift 2 ;;
    --slug) slug="${2:?}"; shift 2 ;;
    --ext) ext="${2:?}"; shift 2 ;;
    --repo) repo="${2:?}"; shift 2 ;;
    --id) id="${2:?}"; shift 2 ;;
    --version) version="${2:?}"; shift 2 ;;
    --license) license="${2:?}"; shift 2 ;;
    --core-crate) core="${2:?}"; shift 2 ;;
    --date) date="${2:?}"; shift 2 ;;
    --no-game-db) gamedb=0; shift ;;
    --reference) reference=1; shift ;;
    --verilator) verilator=1; reference=1; shift ;;
    --force) force=1; shift ;;
    -h|--help) sed -n '2,27p' "$0" | sed 's/^# \?//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 1 ;;
  esac
done
[[ -n "$name" && -n "$system" && -n "$slug" && -n "$ext" ]] || { sed -n '2,27p' "$0" | sed 's/^# \?//' >&2; exit 1; }
[[ -d "$kit" ]] || { echo "docs/kit not found (is this the base or a clone made from it?)" >&2; exit 1; }
[[ -n "$id" ]] || id="${slug//-/_}"
[[ -n "$core" ]] || core="$slug-core"
[[ -n "$version" ]] || version="$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
[[ -n "$version" ]] || version="0.1.0"
anchor="$(tr 'A-Z ' 'a-z-' <<<"$system" | sed 's/[^a-z0-9-]//g')"
idup="${id^^}"

esc() { sed -e 's/[\\&|]/\\&/g' <<<"$1"; }
render() { # $1 model, $2 target
  local m="$kit/$1" t="$root/$2"
  if [[ -e "$t" && $force -eq 0 ]]; then echo "skipped (exists, use --force): $2"; return; fi
  mkdir -p "$(dirname "$t")"
  # drop the kit's header comment (first <!-- ... --> block), fill the placeholders, drop the section tags
  awk 'NR==1 && /^<!--/ {skip=1} !skip {print} skip && /-->/ {skip=0}' "$m" \
    | sed -e "s|{{NAME}}|$(esc "$name")|g" -e "s|{{SYSTEM}}|$(esc "$system")|g" -e "s|{{SYSTEM_ANCHOR}}|$(esc "$anchor")|g" \
          -e "s|{{SLUG}}|$(esc "$slug")|g" -e "s|{{ID}}|$(esc "$id")|g" -e "s|{{ID_UPPER}}|$(esc "$idup")|g" \
          -e "s|{{EXT}}|$(esc "$ext")|g" -e "s|{{REPO}}|$(esc "$repo")|g" -e "s|{{VERSION}}|$(esc "$version")|g" \
          -e "s|{{LICENSE}}|$(esc "$license")|g" -e "s|{{CORE_CRATE}}|$(esc "$core")|g" -e "s|{{DATE}}|$(esc "$date")|g" \
          -e 's/  \[\(GENERIC\|SYSTEM\|GENERIC + SYSTEM\)\]$//' > "$t"
  echo "wrote $2"
}

render README.model.md README.md
render CHANGELOG.model.md CHANGELOG.md
for d in manual hardware compatibility accuracy architecture libretro translating lessons references; do
  render "$d.model.md" "docs/$d.md"
done
(( gamedb )) && render sheets.model.md docs/sheets.md
if (( reference )); then
  render reference/README.model.md reference/README.md
  if [[ -e "$root/reference/compare.sh" && $force -eq 0 ]]; then echo "skipped (exists, use --force): reference/compare.sh"
  else install -m 755 "$kit/reference/compare.sh" "$root/reference/compare.sh"; echo "wrote reference/compare.sh"; fi
  if (( verilator )); then
    for f in sim_top.sv sim.h ref_main.cpp Makefile compare.py; do
      if [[ -e "$root/reference/$f" && $force -eq 0 ]]; then echo "skipped (exists, use --force): reference/$f"
      else install -m 644 "$kit/reference/verilator/$f" "$root/reference/$f"; echo "wrote reference/$f"; fi
    done
    echo "Verilator example copied: adapt it (see the header of each file); compare.py replaces compare.sh's loop"
  fi
fi
docs=("$root/README.md" "$root/CHANGELOG.md")
for d in manual hardware compatibility accuracy architecture libretro translating lessons references sheets; do
  [[ -e "$root/docs/$d.md" ]] && docs+=("$root/docs/$d.md")
done
[[ -e "$root/reference/README.md" ]] && docs+=("$root/reference/README.md")

left="$(grep -n '{{[A-Z_]*}}' "${docs[@]}" 2>/dev/null || true)"
[[ -z "$left" ]] || { echo "placeholders left unfilled:"; echo "$left"; }
n="$(grep -n 'TODO(system)\|TODO(translate)' "${docs[@]}" 2>/dev/null | wc -l)"
echo "$n TODO marks left. List them: grep -rn 'TODO(system)\\|TODO(translate)' README.md CHANGELOG.md docs/ reference/"
echo "Check the links when done: tools/check-links.py"
