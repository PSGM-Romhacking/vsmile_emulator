# Enviar o core libretro de um emulador para o RetroArch

Documento interno. Levantado com o **V-Bear** (Funtech Super A'Can, 2026-11-01), o primeiro emulador desta base a pedir entrada no *Core Downloader* do RetroArch. O que está aqui foi feito de fato, menos o que está marcado como **não confirmado**. Fontes no fim.

## Visão geral

Para o core aparecer em *Online Updater › Core Downloader*, a libretro pede quatro coisas, em repositórios diferentes:

| # | O quê | Onde | Obrigatório |
|---|---|---|---|
| 1 | `<id>_libretro.info` | PR em `libretro/libretro-super`, pasta `dist/info/` | sim |
| 2 | `.gitlab-ci.yml` | raiz do repositório do emulador | sim |
| 3 | **espelho** do repositório no GitLab da libretro | criado por um mantenedor, a pedido (Discord) | sim, e **não está na documentação oficial** |
| 4 | página do core | PR em `libretro/docs` (`core-template.md`) | recomendado |
| 5 | ícone do sistema; jogos no banco | `libretro/retroarch-assets`, `libretro/libretro-database` | opcional |

Sem o item 3 o buildbot nunca compila o core, mesmo com o PR do `.info` aceito.

## Ordem que funcionou

1. Deixar o core pronto e a release publicada (links estáveis para o PR).
2. Escrever o `.info` e o `.gitlab-ci.yml` (modelos abaixo) e publicar no repositório.
3. Abrir o PR do `.info` em `libretro-super`.
4. Pedir o espelho no Discord da libretro, canal `#programming`, com o link do PR. (`#showcase` serve para apresentar o projeto; o pedido de espelho é de `#programming`.) Fazer o PR primeiro dá ao pedido um link concreto.
5. Esperar o espelho e o primeiro build. O CI **só roda na infraestrutura deles**, então espere uma rodada de correções.
6. Abrir o PR da página em `libretro/docs`.

## Depois que entra: como as atualizações chegam

- O espelho copia o repositório do GitHub sozinho, a cada commit.
- O buildbot compila a `main` (em geral toda noite). **Não segue tags nem releases do GitHub**: um commit quebrado na `main` chega aos usuários do RetroArch.
- O `.info` mora em `libretro-super`; só precisa de novo PR se mudar algo nele (versão, BIOS, recursos). Mudar só o código do core não exige PR.

## O que o core precisa cumprir

Antes de pedir, conferir (cada item já custou tempo no V-Bear):

- **Tamanho do estado fixo.** `retro_serialize_size` não pode crescer enquanto um jogo está carregado: o RetroArch pergunta uma vez e dimensiona o buffer de rewind pela resposta. Se o estado real varia de um quadro para outro (CPU salva como texto, por exemplo), reportar um tamanho fixo acima do máximo medido, preencher o resto com zeros e recusar estado maior. Medir nos jogos de verdade, não só no primeiro quadro.
- **`savestate_features = "serialized"` no `.info`.** Com `"basic"` o RetroArch recusa o rewind ("Rewind unavailable because this core lacks serialized save state support"). Nota: o `new-emulator.sh` ainda gera `"basic"`.
- **Sem threads e sem chamadas à API fora de `retro_run`.** A API não é reentrante nem segura entre threads; usar várias threads é responsabilidade do core. O V-Bear não cria threads e guarda o estado global num `Mutex`.
- **`needs_fullpath`:** `"true"` se o core abre o arquivo ele mesmo (necessário quando o jogo vem em vários arquivos dentro de `.zip`/`.7z`); `database_match_archive_member = "true"` se o playlist deve guardar o arquivo compactado.
- **`core_options = "false"`** se o core não expõe opções (não marcar `true` por padrão).
- **Nenhum panic cruza para C** e nenhuma biblioteca de sistema que o runtime do RetroArch (Flatpak) não tenha.

## `.info`

Modelo comentado: `crates/v-bear-libretro/v_bear_libretro.info` do V-Bear. Campos que pedem decisão:

- `database`: tem que ser **igual ao nome do `.rdb`** em `libretro/libretro-database/rdb/` (sem a extensão). Conferir lá antes do PR: `gh api repos/libretro/libretro-database/contents/rdb --jq '.[].name' | grep -i <sistema>`.
- `systemid`, `systemname`, `manufacturer`, `display_name` (`"<Fabricante> - <Sistema> (<Emulador>)"`).
- `license`: SPDX curto, igual à do repositório.
- Firmware: `firmwareN_desc`, `_path`, `_opt`; um `notes` com os CRC32 (`(!) arquivo (crc): xxxxxxxx`) e avisos (`(^)`).
- Para testar localmente: copiar para a pasta `info/` do RetroArch, **apagar o `core_info.cache`** (senão ele mostra o core sem descrição e esconde as extensões) e abrir Informação › Informação do core.

## `.gitlab-ci.yml`

Usa os modelos de `libretro-infrastructure/ci-templates` (`rust-linux-x64.yml`, `rust-windows-x64.yml`; esperam `lib${CORENAME}.so`). Substituir `{{ID}}` (id com `_`, ex.: `v_bear`) e `{{SLUG}}` (nome do crate, ex.: `v-bear`):

```yaml
# DESCRIPTION: GitLab CI/CD for libretro (NOT FOR GitLab-proper)

# Core definitions
.core-defs:
  variables:
    CORENAME: {{ID}}

include:
  - project: 'libretro-infrastructure/ci-templates'
    file: '/rust-linux-x64.yml'
  - project: 'libretro-infrastructure/ci-templates'
    file: '/rust-windows-x64.yml'

stages:
  - build-prepare
  - build-shared
  - build-static

libretro-build-linux-x64:
  extends:
    - .libretro-rust-linux-x64-default
    - .core-defs
  script:
    - cargo build --release --target ${TARGET_ARCH} --package {{SLUG}}-libretro
  after_script:
    - mv target/${TARGET_ARCH}/release/lib{{ID}}_libretro.so ${CORENAME}_libretro.so
    - if [ $STRIP_CORE_LIB -eq 1 ]; then $STRIP --strip-unneeded ${CORENAME}_libretro.so; fi

libretro-build-windows-x64:
  extends:
    - .libretro-rust-windows-x64-default
    - .core-defs
  script:
    - cargo build --release --target ${TARGET_ARCH} --package {{SLUG}}-libretro
  after_script:
    - mv target/${TARGET_ARCH}/release/{{ID}}_libretro.dll ${CORENAME}_libretro.dll
    - if [ $STRIP_CORE_LIB -eq 1 ]; then $STRIP --strip-unneeded ${CORENAME}_libretro.dll; fi
```

Compila só o crate libretro (o app precisa de ALSA/egui). Não existe modelo para FreeBSD: esse build continua na release do próprio repositório. **Nunca foi executado** (só roda lá); o que se sabe vem de cores de outros autores.

Riscos conhecidos de um core Rust no buildbot (**não confirmados**: só aparecem no primeiro build):

- precisa de acesso ao crates.io e de um Rust estável recente;
- um `rust-toolchain.toml` com `components = ["rustfmt", "clippy"]` pode falhar se o ambiente não os tiver nem puder instalá-los; só servem para formatar e analisar, não para compilar. Se reclamar, apagar o arquivo (ou a linha `components`); o `ci.yml` do GitHub instala o `clippy` por conta própria.

## Texto do pedido de espelho (Discord, `#programming`)

```
Hi! I'm adding a new core, {{NAME}} ({{SYSTEM}}, written in Rust, {{LICENSE}}). I followed the docs:

- Info file PR: {{PR_URL}}
- Core repo, with .gitlab-ci.yml in the root: {{REPO_URL}}

Could someone create the GitLab mirror of the repo so the buildbot can build it?

Also, is there any review or requirement before a core goes on the buildbot? I didn't find anything about it in the documentation.

Thank you, everyone!
```

## Texto do PR (`libretro/libretro-super`)

Título: `add {{ID}}.` Arquivo: `dist/info/{{ID}}_libretro.info`. Corpo (inglês, sem marca de IA nem `Co-Authored-By`, como manda o `git.md`):

```
Adds the info file for {{NAME}}, a {{SYSTEM}} emulator and libretro core.

## What it is
<console, ano, fabricante, CPUs>. {{NAME}} is written in Rust, based on <referências>, and also ships as a standalone app. The libretro core is built from the same code.
- Repository / Website / Release with the cores / License

## Status
<quantos jogos jogáveis; saves e states; rewind>. <Plataformas que compilam e onde foi realmente executado>. Dizer só o que foi testado.

## About the info file
<uma linha por decisão: core name, systemid/database, extensões, needs_fullpath, savestate/serialized, libretro_saves, core_options, firmware e CRC32, jogos em vários arquivos>

## Build
.gitlab-ci.yml is in the root of the core repository (rust-linux-x64 and rust-windows-x64 templates from ci-templates). It has not been run yet, since it only runs on libretro's infrastructure.
What the build needs: only `cargo build --release --package <crate>`; no system libraries; crates.io access; a recent stable Rust.
<se houver rust-toolchain.toml com components: avisar que rustfmt/clippy não são usados na compilação e oferecer remover>

## What is missing
- The GitLab mirror of the repository, so the buildbot can use that file.
- The libretro-docs page, which will come in a follow-up PR.
```

Abrir o PR pela conta dona do projeto (no V-Bear, `PSGM-Romhacking`): `gh repo fork libretro/libretro-super --clone=false`, clonar o fork, criar o branch `add-<id>`, copiar o `.info`, commit `add <id>.` (autor `Péricles S.`), `git push`, `gh pr create --repo libretro/libretro-super --head <conta>:add-<id> --body-file ...`. O fork cria um repositório público: a IA não deve fazê-lo sem o dono (o ambiente bloqueia); deixar os comandos prontos.

## Nada disso está confirmado

- Se a libretro revisa o código ou a licença do core antes de aceitar: as fontes não dizem. Perguntar no pedido (o texto acima já pergunta).
- Se o buildbot aceita o CI de Rust sem ajustes.
- Quanto tempo o espelho leva.

## Fontes

- <https://docs.libretro.com/development/cores/developing-cores/#add-your-core-to-libretro-infrastructure>: os passos oficiais (não citam o espelho nem o canal).
- <https://docs.libretro.com/development/cores/developing-cores/#thread-safety>
- <https://docs.libretro.com/development/cores/developing-cores/#retro_serialize_size-retro_serialize-and-retro_unserialize>: o tamanho do estado.
- <https://clownacy.wordpress.com/2025/08/24/how-i-added-a-new-libretro-core-to-retroarch/>: relato de ponta a ponta (indicado por um mantenedor no Discord); descreve o espelho, o `#programming` e os erros do primeiro build.
- Exemplos de PR do `.info`: `libretro/libretro-super` #2175 e #2176 (V-Bear).
