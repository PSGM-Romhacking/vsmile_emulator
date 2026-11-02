#!/usr/bin/env bash
# Brings the base's interface into an existing emulator. Run it FROM THE BASE checkout:
#   tools/update-interface.sh /path/to/emulator [--dry-run] [--force]
#
# Copies crates/api, crates/frontend, assets/fonts and the base's language catalogs
# (assets/i18n/pt.json, es.json) into the emulator. It never touches the emulator's own
# crates (<system>-core, app, libretro), its other assets or its Cargo.toml: it only
# shows how [workspace.dependencies] differ, so you can reconcile them.
# Afterwards: cargo build, then fix what the contract changed (see CHANGELOG.md of the base).
# The base commit copied is recorded in the emulator's .interface-version.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
[[ -e "$root/.estrutura-base" ]] || { echo "run this from the base checkout, not from an emulator's clone" >&2; exit 1; }
dry=0; force=0; target=""
for a in "$@"; do
  case "$a" in
    --dry-run) dry=1 ;;
    --force) force=1 ;;
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \?//'; exit 0 ;;
    --*) echo "unknown option: $a" >&2; exit 1 ;;
    *) target="$a" ;;
  esac
done
[[ -n "$target" && -d "$target/crates" && -f "$target/Cargo.toml" ]] || { echo "usage: $0 /path/to/emulator [--dry-run] [--force]" >&2; exit 1; }
target="$(cd "$target" && pwd)"
[[ "$target" != "$root" ]] || { echo "the target is the base itself" >&2; exit 1; }

if git -C "$target" rev-parse --git-dir >/dev/null 2>&1 && [[ -n "$(git -C "$target" status --porcelain)" ]] && (( ! force && ! dry )); then
  echo "the emulator has uncommitted changes; commit them first (or use --force)" >&2
  exit 1
fi

rs=(rsync -rlc --itemize-changes)
(( dry )) && rs+=(--dry-run)
echo "== crates/api, crates/frontend (replaced, files removed in the base are removed here)"
for d in crates/api crates/frontend; do
  mkdir -p "$target/$d"
  "${rs[@]}" --delete --exclude target "$root/$d/" "$target/$d/"
done
echo "== assets/fonts, assets/i18n/pt.json, es.json"
mkdir -p "$target/assets/fonts" "$target/assets/i18n"
"${rs[@]}" "$root/assets/fonts/" "$target/assets/fonts/"
"${rs[@]}" "$root/assets/i18n/pt.json" "$root/assets/i18n/es.json" "$target/assets/i18n/"

echo "== [workspace.dependencies]: base (<) vs emulator (>)"
deps() { sed -n '/^\[workspace.dependencies\]/,/^\[/p' "$1" | grep -v '^\[' | grep -v '^#' | grep -v '^$' | sed 's/ *= */ = /' | sort; }
diff <(deps "$root/Cargo.toml") <(deps "$target/Cargo.toml") || true
echo "(lines only in the emulator are its own crates; lines that differ in version are for you to reconcile)"

new="$(git -C "$root" rev-parse --short HEAD 2>/dev/null || echo unknown)"
[[ -z "$(git -C "$root" status --porcelain 2>/dev/null)" ]] || new="$new-dirty"
old=""
[[ ! -f "$target/.interface-version" ]] || old="$(sed -n 's/^base: //p' "$target/.interface-version" | head -1)"
if [[ -n "$old" && "$old" != "$new" ]] && git -C "$root" cat-file -e "${old%-dirty}" 2>/dev/null; then
  echo "== base commits touching the interface since $old"
  git -C "$root" log --oneline "${old%-dirty}..HEAD" -- crates/api crates/frontend assets CHANGELOG.md || true
fi
if (( dry )); then
  echo "(dry run: nothing was changed)"
else
  printf 'base: %s\ndate: %s\n' "$new" "$(date +%F)" > "$target/.interface-version"
  echo "Done ($old -> $new). Next: cd $target && cargo build --workspace, then read CHANGELOG.md of the base."
fi
