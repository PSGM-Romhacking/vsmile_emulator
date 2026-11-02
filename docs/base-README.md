# Estrutura para novos emuladores em Rust

Base compartilhada para emuladores em Rust: o **contrato** entre núcleo e interface (`emu-api`), a **interface** reutilizável, chamada de **ORBE Shell** (`emu-frontend`, egui) e ferramentas para ligar um núcleo de outro sistema a ela. Nasceu do [Mil Capivaras](https://github.com/PSGM-Romhacking/mil_capivaras) (emulador do Casio PV-1000), de onde a interface foi copiada em 20/10/2026.

Documento interno. Guia completo da arquitetura: [`docs/guia.md`](docs/guia.md). Instruções para a IA que monta emuladores aqui: [`CLAUDE.md`](CLAUDE.md).

## O que tem aqui

| Pasta | O que é |
|---|---|
| `crates/api` | `emu-api`: contrato núcleo ↔ interface (trait `Core`, `SystemInfo`...), no estilo libretro |
| `crates/frontend` | `emu-frontend`: a interface (menus, biblioteca com capas, controles, save states, rewind, gravação, debugger, cheats, idiomas) |
| `crates/libretro-host` | roda **qualquer core libretro** (`.so`/`.dll`) como um `emu_api::Core`, sem adaptador por sistema |
| `crates/libretro-player` | executável: `libretro_player --core CORE.so [JOGO]` |
| `crates/toy-core`, `toy`, `toy-libretro` | exemplo completo e funcional (um "console de brinquedo"), gerado pelo script e preenchido |
| `SISTEMA.md` | ficha do sistema, a preencher em cada clone |
| `docs/kit/` | modelos de documentação: README curto, `docs/` com um documento por assunto, CHANGELOG e a referência de validação (`reference/`) |
| `tools/new-docs.sh` | instancia o kit para um emulador (ver `docs/kit-docs.md`) |
| `tools/check-links.py` | confere arquivos e âncoras dos links da documentação |
| `tools/init-project.sh` | inicia o clone como projeto de um emulador (`--finalizar` limpa o exemplo) |
| `tools/update-interface.sh` | leva a interface da base para um emulador existente (rodar na base) |
| `tools/new-emulator.sh` | gera o esqueleto de um emulador novo |
| `tools/templates/` | template do core libretro gerado com `--libretro` |
| `assets/` | fonte japonesa e catálogos de idioma (pt, es) embutidos na interface |
| `docs/guia.md` | o guia: estrutura, contrato, cada crate, checklist |

## Fluxo (resumo)

Cada emulador novo é um **clone deste repo**, independente dos outros (decidido em 20/10/2026; o Mil Capivaras também segue independente, com a própria cópia da interface):

1. Clonar este repo com o nome do projeto novo e iniciá-lo:
   ```
   git clone git@github-psgm:PSGM-Romhacking/_estrutura-novos-emuladores-rust.git meu-emulador && cd meu-emulador
   tools/init-project.sh --name "Meu Emulador" --version 0.1.0 --remote git@github-psgm:PSGM-Romhacking/meu-emulador.git
   ```
   O script troca o histórico por um commit só, deixa a base como remote `base`, dá ao projeto o próprio README/CHANGELOG e a versão. **Mantém** o `toy` e o `libretro-player`, que a IA usa como modelo e primeiro teste. Nada é enviado: o `git push -u origin main` é seu.
2. Colocar dentro o núcleo (código ou `.so`) e preencher a ficha do sistema, [`SISTEMA.md`](SISTEMA.md) (nome, extensões, botões, resolução, FPS, pasta de capas...). Exemplo preenchido: [`docs/exemplo-SISTEMA-toy.md`](docs/exemplo-SISTEMA-toy.md).
3. Adaptar (a IA segue o `CLAUDE.md`): rodar o gerador, preencher os `TODO(core)`.
4. Compilar e testar.
5. Documentação: `tools/new-docs.sh --name ... --system ... --slug ... --ext ... --repo ... --force` gera `README.md`, `CHANGELOG.md` e `docs/` a partir do kit (estrutura do Mil Capivaras), com `TODO(system)` onde o sistema exige texto próprio; `--reference` acrescenta `reference/` (validação contra uma referência). No fim, `tools/check-links.py`. Guia: [`docs/kit-docs.md`](docs/kit-docs.md).
6. (Opcional, com o emulador pronto) `tools/init-project.sh --finalizar` remove o `toy`, o player e o host libretro, e os passos de CI que dependem deles.

**Levar a interface nova a um emulador que já existe** (só quando valer a pena): a partir do checkout **da base**, `tools/update-interface.sh ~/Projetos/meu-emulador [--dry-run]`. Copia `crates/api`, `crates/frontend`, `assets/fonts` e os catálogos pt/es; não toca no código do emulador; mostra as diferenças de `[workspace.dependencies]`; grava o commit da base em `.interface-version` do emulador. Depois: compilar e ajustar o que o contrato mudou (ver `CHANGELOG.md`).

**Versão:** este repo não tem versão (é interno e privado). A versão (`MAJOR.MINOR.PATCH`) é de cada emulador, em `[workspace.package] version` do `Cargo.toml` do clone; os crates gerados a herdam. O valor `0.1.0` que vem aqui é só o ponto de partida: ajuste ao criar o emulador. O Makefile, o CHANGELOG e as releases do emulador usam esse número.

**Evolução da interface:** este repo é a fonte da interface (considerada madura em 20/10/2026). Uma melhoria feita aqui **não** é replicada nos emuladores já criados; cada emulador só recebe a interface nova quando o dono for mexer no núcleo dele e quiser aproveitar. Nesse caso, copiar de volta `crates/api`, `crates/frontend`, `assets/fonts` e `assets/i18n` deste repo para o emulador e ajustar o que o contrato mudou (`SystemInfo`, `Core`); o código do próprio emulador (`<sistema>-core`, app, libretro) não é tocado.

## Três jeitos de jogar um núcleo

1. **O núcleo já é um core libretro compilado** (qualquer sistema do RetroArch com RetroPad):
   ```
   cargo run --release -p libretro-player -- --core /caminho/core_libretro.so [JOGO.zip | PASTA] [--thumbnails "Pasta - Sistema"]
   ```
2. **O núcleo é código seu em Rust** (ou C/C++ via FFI): gerar o esqueleto e preencher os `TODO(core)`:
   ```
   tools/new-emulator.sh --libretro nes "Mil Nintendos" "Nintendo - Nintendo Entertainment System"
   # C/C++: acrescente --ffi-c
   cargo run -p mil_nintendos -- JOGO
   ```
3. **Só ver a interface funcionando:** `cargo run -p toy`.

## Desenvolvimento

```
sudo apt install pkg-config libasound2-dev   # Debian/Ubuntu (áudio)
cargo build --workspace
cargo test --workspace
```

Testar o host contra um core real (pula se a variável não existir):
```
LIBRETRO_TEST_CORE=/caminho/core_libretro.so cargo test -p libretro-host --test real_core -- --nocapture
```

## Licença e conteúdo de terceiros

GPL-3.0 ou posterior ([`LICENSE`](LICENSE)).

- **O núcleo que você trouxer tem licença própria** e ela tem de ser compatível com a GPL-3.0 se o emulador for distribuído (código estático ou FFI, com ele dentro do mesmo binário). Um core libretro carregado pelo host (`.so`/`.dll`) mantém a licença dele e é carregado como arquivo separado. Anote a licença no campo correspondente do `SISTEMA.md` antes de integrar. Em dúvida (licença não comercial, sem licença, ou proprietária), não distribua.
- **ROMs, BIOS e cores de terceiros nunca entram no git.** O `.gitignore` bloqueia `roms/`, `bios/`, `cores/`, `*.zip`, `*.7z`, `*.bin`, `*.rom`, `*.iso`, `*.cue`, `*.chd`, `*.so`, `*.dll` e `*.dylib`. Guarde-os nessas pastas (ou fora do repo). Se um arquivo legítimo com uma dessas extensões precisar ser versionado (um recurso do próprio projeto), use `git add -f` de propósito.
