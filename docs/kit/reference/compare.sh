#!/bin/sh
# Compares the emulator with the reference: both run the same ROM with the same input
# (--press-start) and the picture and the RAM are compared at each frame.
#
# Usage: reference/compare.sh EMULATOR ROM...
#   EMULATOR  the emulator's binary (e.g. target/release/<slug>)
#   ROM       the system's ROM files, or .zip/.7z (extracted with 7z)
#   FRAMES    frames to compare (default: "300 3000")
#   REF       the reference's binary (default: reference/ref)
# Both programs must accept: GAME --shot N out.ppm --press-start --dump-ram ram.bin, write a
# binary PPM (P6) and the same RAM range. Prints one line per game and frame: differing
# pixels and differing RAM bytes (0 = identical).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
ref=${REF:-$here/ref}
[ -x "$ref" ] || { echo "build the reference first (see $here/README.md)" >&2; exit 1; }
[ $# -ge 2 ] || { sed -n '4,12p' "$0" >&2; exit 1; }
emu=$1; shift
frames=${FRAMES:-300 3000}
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

printf '%-40s %6s %8s %8s\n' GAME FRAME PIXELS RAM
for rom in "$@"; do
    case $rom in
        *.zip|*.7z) rm -rf "$tmp/x"; 7z e -y -bso0 -o"$tmp/x" "$rom" >/dev/null; file=$(ls "$tmp"/x/* | head -n 1) ;;
        *) file=$rom ;;
    esac
    for n in $frames; do
        "$ref" "$file" --shot "$n" "$tmp/r.ppm" --press-start --dump-ram "$tmp/r.ram"
        "$emu" "$file" --shot "$n" "$tmp/e.ppm" --press-start --dump-ram "$tmp/e.ram" >/dev/null
        # the PPM header is 3 lines ("P6", width and height, 255), then 3 bytes per pixel
        h=$(head -n 3 "$tmp/r.ppm" | wc -c)
        px=$(cmp -l "$tmp/r.ppm" "$tmp/e.ppm" | awk -v h="$h" '{print int(($1 - 1 - h) / 3)}' | sort -u | wc -l)
        ram=$(cmp -l "$tmp/r.ram" "$tmp/e.ram" | wc -l)
        printf '%-40s %6s %8s %8s\n' "$(basename "$rom" | cut -c1-40)" "$n" "$px" "$ram"
    done
done
