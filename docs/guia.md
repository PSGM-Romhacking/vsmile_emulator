# Guia: estrutura de um emulador em Rust (core × frontend)

Documento interno. Base: o projeto **Mil Capivaras** (`~/Projetos/mil_capivaras`, emulador do Casio PV-1000), cuja arquitetura foi desenhada para o frontend ser reaproveitado por outros emuladores. Levantado em 20/10/2026 a partir do código (`crates/api/src/lib.rs`, `crates/core/src/core.rs`, `crates/app/src/main.rs`, `crates/libretro/src/lib.rs`) e de `docs/architecture.md`. Se o código divergir deste texto, o código manda.

## 1. Ideia central

Cada emulador é dividido em **back** (o console emulado) e **front** (janela, menus, biblioteca, controles...). Eles só se falam por um contrato pequeno, o crate `emu-api`, que segue as convenções do **libretro**. Consequências:

- O mesmo frontend serve a qualquer sistema. Emulador novo = core novo + app fininho.
- O mesmo core vira um **core libretro** (RetroArch) sem reescrever nada.
- O core não conhece janela, áudio, arquivos de config, nem idioma. O frontend não conhece nenhum console.

## 2. Estrutura de pastas (workspace Cargo)

```
<projeto>/
├── Cargo.toml              workspace: members = ["crates/*"], versão/edição/licença compartilhadas
├── Cargo.lock
├── crates/
│   ├── api/                emu-api: contrato core ↔ frontend (zero dependências)
│   ├── core/               <sistema>-core: o console (CPU, vídeo, som, memória, banco de jogos)
│   ├── frontend/           emu-frontend: interface reutilizável (egui)
│   ├── app/                executável: core + frontend + identidade (nome, ícone, bezels)
│   └── libretro/           cdylib: core + API C do libretro
├── assets/                 icons/, fonts/, i18n/<idioma>.json, games/games*.json, bezels/
├── docs/                   manual, hardware, compatibility, accuracy, architecture, libretro, sheets, translating, lessons, references
├── reference/              a referência de validação (ex.: core FPGA simulado) + compare.sh; fora do cargo build
├── packaging/              app.desktop, build_linux.sh, build_windows.sh
├── .github/workflows/      release.yml (testes + AppImage + .exe + libretro + FreeBSD)
├── vendor/                 forks necessários (ex.: iz80 com ciclos corrigidos) via [patch.crates-io]
├── Makefile                build, install (atalho de menu), libretro, install-libretro
├── CHANGELOG.md            as notas de cada release saem daqui
└── README.md (+ pt-BR, es)
```

### Regras de dependência (nunca violar)

| Crate | Pode depender de | Nunca depende de |
|---|---|---|
| `emu-api` | nada | tudo |
| `<sistema>-core` | `emu-api` + libs de emulação (CPU etc.) | egui, `emu-frontend`, libretro |
| `emu-frontend` | `emu-api` + egui/eframe, cpal, rfd, image, gif, zip, sevenz-rust, serde, gilrs | qualquer `*-core` |
| `app` | os três acima + eframe | — (é a cola) |
| `libretro` | `emu-api` + `<sistema>-core` | `emu-frontend` |

O Cargo.toml do workspace centraliza versões em `[workspace.dependencies]`; cada crate usa `x.workspace = true`.

## 3. O contrato (`emu-api`)

Convenções herdadas do libretro, para o core virar libretro sem adaptação:

| Aspecto | Decisão |
|---|---|
| Controles | bitmask `u16` **RetroPad** por jogador (`PadButton::bit()`, mesma ordem de `RETRO_DEVICE_ID_JOYPAD_*`) |
| Vídeo | `&[u32]` **XRGB8888** (`0x00RRGGBB`), com `width`, `height`, `pitch` (em pixels) e `aspect` por quadro |
| Áudio | **estéreo intercalado `f32`**; a taxa é escolhida pelo frontend e passada em `create(sample_rate)` |
| Save state | bytes opacos. O cabeçalho do arquivo (magic `EFST`, versão, hash FNV-1a da ROM, miniatura) é do frontend |
| Memória | `system_ram()` / `system_ram_mut()` (`RETRO_MEMORY_SYSTEM_RAM`): usada por cheats, debugger e achievements |
| Opções | `options()` / `set_option()`, como *core options* do libretro (o primeiro valor é o padrão) |

### Tipos

- `PadButton`: 16 botões RetroPad. `Button { id, label }`: botão que o sistema realmente usa + nome exibido.
- `Frame<'a> { pixels, width, height, pitch, aspect }`.
- `SystemInfo`: descrição estática do sistema (nome, `short_name`, `extensions`, nome da pasta no libretro-thumbnails, `players`, `buttons`, resolução, `aspect`, `ram_address`, `fps_num/fps_den`, `game_info`, `game_texts`, `create`).
- `GameInfo`: dados de um jogo conhecido (id estável, título, ano, publisher, serial, thumbnail No-Intro...).
- `DebugView` (`Image` com grade opcional, `Table` de `DebugGroup`/`DebugValue::{Text, Flag, Color}`), `CoreOption`.
- `crc32()`: utilitário (IEEE 802.3) para identificar ROMs.

### Trait `Core` (o que o emulador novo implementa)

```rust
pub trait Core {
    fn info(&self) -> &'static SystemInfo;
    fn fps(&self) -> (u64, u64)            { /* padrão: o de info() */ }
    fn load(&mut self, rom: &[u8]) -> Result<(), String>;
    fn reset(&mut self);
    fn run_frame(&mut self, pads: &[u16]); // pads[n] = bitmask do jogador n
    fn video(&self) -> Frame<'_>;
    fn take_audio(&mut self, out: &mut Vec<f32>);   // anexa o áudio do último quadro
    fn save_state(&self) -> Vec<u8>;
    fn load_state(&mut self, data: &[u8]) -> Result<(), String>; // também atualiza o vídeo
    fn system_ram(&self) -> &[u8];
    fn system_ram_mut(&mut self) -> &mut [u8];
    // opcionais, com padrão vazio:
    fn debug_views(&self) -> Vec<DebugView>;
    fn sound_channels(&self) -> &'static [&'static str];
    fn set_channel_muted(&mut self, channel: usize, muted: bool);
    fn options(&self) -> &'static [CoreOption];
    fn set_option(&mut self, key: &str, value: &str);
}
```

Pontos de atenção:
- `run_frame` emula **exatamente um quadro**; o ritmo (pacing), rewind e fast-forward são do frontend.
- Textos de interface no core (labels, títulos do debugger, opções) são **em inglês, no código-fonte**; o frontend traduz via `tr()` com os catálogos de idioma.
- Se o timing depende de uma opção (modelo, região), sobrescreva `fps()` e `video().aspect`.
- Se o core aceita só parte do RetroPad, declare só esses botões em `SystemInfo::buttons`: a tela de controles mostra apenas eles.

## 4. Crate do core (`crates/core`, nome `<sistema>-core`)

Estrutura usada no PV-1000:

| Arquivo | Conteúdo |
|---|---|
| `lib.rs` | `pub mod`s e `pub use core::{INFO, <Sistema>Core}` |
| `core.rs` | **adaptador**: `static INFO: SystemInfo`, `struct <Sistema>Core`, `impl Core`. Converte RetroPad → entrada nativa, mono → estéreo, estado ↔ bytes |
| `machine.rs` | o console em si: barramento, portas, interrupções, vídeo, laço de quadro |
| `machine/<parte>.rs` | subsistemas (ex.: `sound.rs`, `state.rs` com `Snapshot`). Campos `pub(super)` quando o pai os usa |
| `machine/tests.rs` | testes do console |
| `memory.rs` | mapa de memória (CPU e endereços planos para achievements) |
| `games.rs` | banco de jogos (`game_info(rom) -> Option<&'static GameInfo>`) e `TEXTS` (JSON por idioma) |

Separar `machine` (fiel ao hardware, sem conhecer o contrato) de `core.rs` (adaptador fino) mantém a emulação testável sozinha e isola a tradução para o contrato num único arquivo.

Esqueleto do adaptador:

```rust
pub static INFO: SystemInfo = SystemInfo {
    name: "Fabricante Console", short_name: "Console",
    extensions: &["bin", "rom"],
    thumbnails: "Fabricante - Console",     // pasta no libretro-thumbnails
    players: 2,
    buttons: &[Button { id: PadButton::Up, label: "Up" } /* ... */],
    width: W, height: H, aspect: 4.0 / 3.0,
    ram_address: 0x0000,                    // endereço da CPU de system_ram()[0]
    fps_num: 60, fps_den: 1,                // fração exata (ex.: clock / ciclos por quadro)
    game_info: crate::games::game_info,
    game_texts: crate::games::TEXTS,
    create: |rate| Box::new(MeuCore::new(rate)),
};
```

## 5. Crate do frontend (`crates/frontend`, `emu-frontend`)

Reutilizável como está. Organização:

| Módulo | Função |
|---|---|
| `app.rs` + `app/*` | estado do app; cada arquivo em `app/` é um `impl App` (emulation, input, nav, menus, screen, library, state, recording, controls, dialogs, docshots) |
| `audio` | saída de áudio (cpal) |
| `gamepad` (+ `evdev`) | controles: evdev no Linux/FreeBSD, gilrs nos outros |
| `keys`, `menu`, `widgets`, `theme` | teclado, menus, widgets e tema |
| `config` | preferências e pastas (`~/.config/<id>`) |
| `i18n` | `tr()` / `trf()` e catálogos |
| `covers`, `sheets`, `rom` | capas, fichas dos jogos, leitura de ROM (.zip/.7z tratados aqui) |
| `recorder` | screenshots, GIF, MP4 |
| `cheats`, `debugger`, `achievements` | cheats (`.cht` do RetroArch), debugger genérico via `debug_views`, achievements |
| `brand` | identidade do app (ver abaixo) |
| `platform` | detalhes por sistema operacional |
| `usage` | tempo de jogo e histórico de sessões (`usage.json`) |
| `crashlog` | relatório de erro: um pânico grava `crashes/<data>.log` e a abertura seguinte avisa (`install()` no app) |

O formato das capas vem de `SystemInfo::cover_aspect` (largura/altura: ~0,66 para caixas em retrato, ~1,3 para estojos largos). Os textos próprios do app (botões do sistema, "Sobre", linha de comando) entram com `i18n::add_catalog("pt", include_str!(...))`, chamado logo depois de `brand::set`; os catálogos de `assets/i18n/` têm só os textos genéricos da interface.

## 6. Crate do app (`crates/app`)

Executável fino que cola as peças. Em `main.rs`:

1. `setup()` chama `brand::set(Brand { name, id, version, icon_png, about, repo_url, bezels, free_games, thumbnails_url })` **antes** de carregar a config (as pastas usam o `id`). Logo depois, `emu_frontend::crashlog::install()` (relatório de erro, ver CHANGELOG).
2. `config::Config::load()` + `i18n::set_lang(...)`.
3. Parse de argumentos: `--scale`, `--bench`, `--shot`, `--dump-ram`, `--controls`, `--memory-map`, `--identify`, `--info`, `--couch`. Os modos sem janela (`--bench`, `--shot`) servem para **validar o core contra uma referência** (comparar quadro e RAM).
4. `eframe::run_native(..., Box::new(|cc| Ok(Box::new(app::App::new(cc, &<sistema>_core::INFO, rom, scale)))))`.

Também ficam aqui: `build.rs` (ícone no Windows), ícone e bezels embutidos com `include_bytes!`, e as strings próprias do app nos catálogos de idioma.

## 7. Crate libretro (`crates/libretro`)

- `crate-type = ["cdylib"]`; depende só de `emu-api` + core.
- Exporta as funções `retro_*` da API C. A API não tem ponteiro de contexto, então o core vive num **global** (`Mutex`/`OnceLock`).
- **Todas as funções exportadas fazem `catch_unwind`** para nenhum panic cruzar para C.
- Mapeia: `SystemInfo` → `retro_system_info`/`av_info`; `options()` → variáveis do core; `save_state` → `retro_serialize`; `system_ram` → `RETRO_MEMORY_SYSTEM_RAM`; pixel format XRGB8888; áudio f32 → i16.
- Concentra os `unsafe` (65 no Mil Capivaras); fora daqui, evitar.

## 8. Idiomas, assets e dados

- `assets/i18n/<código>.json`: um catálogo por idioma com **todos** os textos (frontend, app e sistema). Chave = texto-fonte em inglês. Cada camada tem teste que confere se seus textos estão no catálogo.
- `assets/games/games.json` (dados comuns) + `games.<idioma>.json` (descrição/notas), indexados por `GameInfo::id`.
- Capas via libretro-thumbnails (`thumbnails` em `SystemInfo` + `thumbnails_url` no `Brand`).
- **ROMs comerciais nunca entram no repo** (`.gitignore`: `roms/`, `*.zip`, `*.bin`, `*.rom`). Homebrews sem licença declarada também ficam fora.

## 9. Checklist para um emulador novo

1. Copiar o workspace (ou apontar dependências para `emu-api`/`emu-frontend`). Se for criar vários emuladores, avaliar extrair `api` e `frontend` para repositório próprio e consumir por git/path.
2. Criar `crates/<sistema>-core`: `machine.rs` (hardware) + `core.rs` (adaptador, `INFO`, `impl Core`) + `memory.rs` + `games.rs`.
3. Escrever testes do `machine` e, se houver referência (MiSTer, Verilator, outro emulador), montar `reference/` com o `compare.sh` do kit (`tools/new-docs.sh --reference`) e as opções `--shot`/`--press-start`/`--dump-ram` no app. Ver "Validação com uma referência" em `docs/kit-docs.md`.
4. Criar `crates/<nome>` (app): `brand::set(...)`, args, `run_native(..., &INFO, ...)`.
5. Adicionar catálogos de idioma e ícone/bezels em `assets/`.
6. Criar `crates/libretro` quando o core estiver estável.
7. `packaging/`, `Makefile`, `release.yml` e `CHANGELOG.md`. AppImage com `cargo zigbuild` (glibc antiga): o linker do zig transforma o `strip = "debuginfo"` do perfil em strip total, e o relatório de erro perde os nomes das funções. Compilar com `CARGO_PROFILE_RELEASE_STRIP=false` e depois `objcopy --strip-debug` no executável (e no core libretro); requisito: binutils. Modelo: `packaging/build_linux.sh` do V-Bear.
8. Documentar com o kit (`tools/new-docs.sh`): README curto e um doc por assunto em `docs/`, descrevendo o estado atual (o histórico vai no `CHANGELOG.md`); `tools/check-links.py` no fim. Regras em `docs/kit-docs.md`.
9. Se o hardware tiver mais de um modelo/timing, expor como `CoreOption` e sobrescrever `fps()`.
10. Rodar `cargo test --workspace`, `cargo clippy --workspace --all-targets`; conferir que o core não importa nada de egui.

## 10. Convenções de código observadas

- Rust edição 2024, resolver 3, licença GPL-3.0-or-later.
- Perfil release: `opt-level=3`, `lto="thin"`, `codegen-units=1`, `strip=true`.
- Módulos grandes são divididos em filhos (`foo.rs` + `foo/bar.rs`), não em `mod.rs`.
- Comentários e identificadores em inglês; documentação do usuário em EN/PT-BR/ES.
- Evitar `unwrap()`/`expect()` em caminhos que dependem de entrada do usuário (ROM, config, zip).

## 11. Atalhos desta base

- **Núcleo já existe como core libretro (`.so`/`.dll`)?** Nem precisa de adaptador: `libretro_player --core CORE.so [JOGO]` (crate `libretro-player`, usa `libretro-host`). Ver o README.
- **Documentação do emulador:** `tools/new-docs.sh` instancia o kit `docs/kit/` (`--reference` inclui a referência de validação); `tools/check-links.py` confere os links. Ver `docs/kit-docs.md`.
- **Iniciar o clone / levar a interface nova:** `tools/init-project.sh` (início e `--finalizar`) e `tools/update-interface.sh` (rodado na base). Ver o README.
- **Gerar o esqueleto de um núcleo novo:** `tools/new-emulator.sh [--libretro] [--ffi-c] <slug> "<Nome>" ["<pasta libretro-thumbnails>"]`.
- **Exemplo que roda:** `crates/toy-core` (+ `toy`, `toy-libretro`) mostra um adaptador completo; é gerado pelo script e depois preenchido. `cargo run -p toy`.

## 12. Pendências conhecidas

- `update-interface.sh --dry-run` lista os arquivos que mudariam, não o que mudou dentro deles; versões de dependências diferentes só são avisadas, quem concilia é o dono ou a IA.
- `.interface-version` leva o sufixo `-dirty` quando a base tinha alterações sem commit no momento da cópia.
- `init-project.sh` substitui o histórico local por um commit (a base fica no remote `base`); não envia nada.

- O `libretro-host` suporta só RetroPad, vídeo por software e conteúdo de um arquivo (ver o cabeçalho de `crates/libretro-host/src/lib.rs`).
- O contrato não cobre disco/fita, teclado/mouse/light gun, resolução que muda no meio do jogo e várias RAMs. BIOS (`SystemInfo::bios`, `Core::set_bios`), save de bateria (`Core::battery_ram`) e ROM em vários arquivos (`SystemInfo::multi_file_roms`) entraram em 21/10/2026, vindos do V-Bear (ver `CHANGELOG.md`).
- Interface vista e aprovada em 20/10/2026 numa sessão gráfica (exemplo `toy`); nenhum problema encontrado.
- Cada emulador é um clone independente deste repo; o Mil Capivaras mantém a própria cópia da interface (decisão de 20/10/2026), e as duas divergem.
