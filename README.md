# V.Smile Emulator

Emulator built on the shared base *estrutura-novos-emuladores-rust* (internal). How the base is organised: `docs/guia.md`; the AI instructions: `CLAUDE.md`; the system sheet: `SISTEMA.md`.

## Build

```
sudo apt install pkg-config libasound2-dev   # Debian/Ubuntu (audio)
cargo build --workspace
cargo test --workspace
```
