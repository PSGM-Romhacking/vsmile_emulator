#!/usr/bin/env bash
# Turns a fresh clone of estrutura-novos-emuladores-rust into the project of ONE emulator.
#
# Start (once, in the new clone):
#   tools/init-project.sh --name "Nome do Emulador" [--version 0.1.0] [--remote git@...:Org/repo.git] [--yes]
#     - git: the base becomes the remote "base" (for comparing later); `--remote` becomes
#       "origin"; the history is replaced by ONE commit. Nothing is pushed.
#     - the base's README/CHANGELOG move to docs/ (base-README.md, base-CHANGELOG.md) and the
#       project gets its own; the version goes into [workspace.package] of Cargo.toml.
#     - the toy example and libretro-player STAY: the AI uses them as a model and as a first
#       test while adapting the core. SISTEMA.md stays, to be filled in.
#
# Finish (optional, when the emulator works):
#   tools/init-project.sh --finalizar [--yes]
#     removes toy-core, toy, toy-libretro, libretro-player and libretro-host (and their CI
#     steps and workspace entries). tools/new-emulator.sh and tools/templates stay.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
name=""; version=""; remote=""; yes=0; finish=0
while (( $# )); do
  case "$1" in
    --name) name="${2:?--name needs a value}"; shift 2 ;;
    --version) version="${2:?--version needs a value}"; shift 2 ;;
    --remote) remote="${2:?--remote needs a value}"; shift 2 ;;
    --yes) yes=1; shift ;;
    --finalizar) finish=1; shift ;;
    -h|--help) sed -n '2,17p' "$0" | sed 's/^# \?//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 1 ;;
  esac
done

confirm() {
  (( yes )) && return 0
  read -r -p "$1 [y/N] " a
  [[ "$a" == y || "$a" == Y ]]
}

# ---- finish ------------------------------------------------------------------------
if (( finish )); then
  [[ ! -e .estrutura-base ]] || { echo "run the start step first (this is still the base)" >&2; exit 1; }
  gone=(crates/toy-core crates/toy crates/toy-libretro crates/libretro-player crates/libretro-host docs/exemplo-SISTEMA-toy.md)
  echo "This removes: ${gone[*]}"
  confirm "Continue?" || exit 1
  rm -rf "${gone[@]}"
  # workspace entries and CI steps that only exist for them
  sed -i '/^libretro-host = /d; /^libloading = /d' Cargo.toml
  if [[ -f .github/workflows/ci.yml ]]; then
    sed -i '/- name: libretro host against a real core/,/^$/d' .github/workflows/ci.yml
    awk '/- name: Generator/ {skip=1} !skip {print}' .github/workflows/ci.yml > .github/workflows/ci.yml.new && mv .github/workflows/ci.yml.new .github/workflows/ci.yml
  fi
  printf "\nNote: toy-core, toy, toy-libretro, libretro-player and libretro-host were removed (init-project.sh --finalizar); ignore the references to them above.\n" >> CLAUDE.md
echo "Done. Run: cargo build --workspace && cargo test --workspace (this also refreshes Cargo.lock)."
  exit 0
fi

# ---- start -------------------------------------------------------------------------
[[ -n "$name" ]] || { echo "--name is required" >&2; sed -n '2,17p' "$0" | sed 's/^# \?//'; exit 1; }
[[ -e .estrutura-base ]] || { echo "already initialised (no .estrutura-base marker)" >&2; exit 1; }
[[ -z "$version" || "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "--version must be MAJOR.MINOR.PATCH" >&2; exit 1; }

echo "Project: $name  version: ${version:-unchanged}  origin: ${remote:-<none>}"
echo "This REPLACES the local git history of $root with a single commit."
confirm "Continue?" || exit 1

# docs of the base move aside; the project gets its own README and CHANGELOG
mv README.md docs/base-README.md
mv CHANGELOG.md docs/base-CHANGELOG.md
cat > README.md <<EOF
# $name

Emulator built on the shared base *estrutura-novos-emuladores-rust* (internal). How the base is organised: \`docs/guia.md\`; the AI instructions: \`CLAUDE.md\`; the system sheet: \`SISTEMA.md\`.

## Build

\`\`\`
sudo apt install pkg-config libasound2-dev   # Debian/Ubuntu (audio)
cargo build --workspace
cargo test --workspace
\`\`\`
EOF
cat > CHANGELOG.md <<EOF
# Changelog

All notable changes to $name.

## [Unreleased]
EOF
if [[ -n "$version" ]]; then
  sed -i '/^\[workspace.package\]/,/^\[/ s/^version = ".*"/version = "'"$version"'"/' Cargo.toml
fi
rm -f .estrutura-base

# git: keep the base as a remote, replace the history
base_url="$(git remote get-url origin 2>/dev/null || true)"
rm -rf .git
git init -q -b main
[[ -z "$base_url" ]] || git remote add base "$base_url"
[[ -z "$remote" ]] || git remote add origin "$remote"
git add -A
git commit -q -m "Start $name from estrutura-novos-emuladores-rust"
echo "Done. Next: fill in SISTEMA.md, put the core in, and ask the AI to adapt it."
[[ -z "$remote" ]] || echo "Push when ready: git push -u origin main"
