# Instruções para a IA: juntar um núcleo de emulador com a interface

Este repo é a **base**: `emu-api` (contrato) + `emu-frontend` (interface egui do Mil Capivaras).
Fluxo do dono do projeto (cada emulador é um clone independente deste repo, sem dependência entre eles): ele traz o **núcleo de outro sistema** (já pronto, de qualquer origem)
e a IA **monta o emulador** ligando esse núcleo à interface. Não reescreva o núcleo nem a
interface; escreva só a cola. Visão geral: `README.md`. Guia completo da estrutura: `docs/guia.md`.

## Atualizar a interface de um emulador já existente
Só quando o dono pedir (normalmente junto com uma melhoria do núcleo). A fonte é este repo: rodar, **a partir do checkout da base**, `tools/update-interface.sh <caminho-do-emulador> --dry-run` e, se estiver certo, sem `--dry-run` (o emulador precisa estar com o git limpo). Ele copia `crates/api`, `crates/frontend`, `assets/fonts` e os catálogos pt/es, sem mexer em `<sistema>-core`, no app e no libretro, e grava `.interface-version`. Reconcilie à mão as diferenças de `[workspace.dependencies]` que ele mostrar. Depois compilar e corrigir só o que o contrato mudou (campos novos de `SystemInfo`, métodos de `Core`, assinaturas como `app::library_window_size`). Se este repo mudou o contrato, a mudança deve estar descrita no `CHANGELOG.md` daqui.

## Início do clone
O clone novo é iniciado pelo dono com `tools/init-project.sh --name ... --version ... --remote ...` (ver README). Se `.estrutura-base` ainda existir na raiz, o clone **não** foi iniciado: avise o dono, não rode scripts de adaptação. Com o emulador pronto, `tools/init-project.sh --finalizar` remove o exemplo; não faça isso por conta própria.

## Regras
- Versão: este repo não tem versão. Ao criar um emulador, ajuste `[workspace.package] version` (semver; `0.x.y` enquanto em desenvolvimento) do clone, que é a versão do emulador como um todo. Não crie tags nem número de versão para a base.
- O núcleo **não** importa egui nem `emu-frontend`. A interface **não** importa o núcleo.
  Só o crate `app` conhece os dois.
- Não modificar `crates/api` nem `crates/frontend` para acomodar um sistema, a menos que não
  haja outro jeito. Se precisar (ex.: capas em outro formato), fazer de forma genérica
  (campo em `SystemInfo`), nunca com `if sistema == ...`, e avisar o dono.
- Respeitar o sistema: botões, resolução, FPS exato, aspecto, taxa de áudio, tamanho de ROM,
  extensões e RAM vêm do hardware real do console, não de valores copiados do PV-1000.
- Textos de interface no código em **inglês**; traduções via `i18n::add_catalog` (app) ou `assets/i18n/<idioma>.json` (só a interface genérica).
- Nunca commitar ROMs, BIOS, cores de terceiros (`.so`/`.dll`) ou jogos comerciais. O `.gitignore` bloqueia as extensões e pastas comuns (`roms/`, `bios/`, `cores/`, `*.zip`, `*.bin`, `*.so`...): **não use `git add -f` nem tire entradas do `.gitignore`** para fazer um desses arquivos entrar; antes de qualquer commit, confira `git status` e avise o dono se aparecer algum.
- Licença do núcleo: leia o campo "Licença do núcleo" do `SISTEMA.md` antes de integrar. Se estiver vazio (`?`), for não comercial, proprietária ou incompatível com GPL-3.0, **pare e avise o dono**: ele decide se segue (uso só pessoal) ou não.
- Git: commit e push direto na `main`, sem `Co-Authored-By`.

## Procedimento

**Passo zero: ler `SISTEMA.md`** (a ficha do sistema, preenchida pelo dono no clone). Ela diz o que o sistema é; cada campo diz em que vira no código. Campo com `?` ou vazio: pergunte ao dono ou confira no núcleo, nunca use valor de outro console. Se a ficha disser que o sistema precisa de algo que o contrato não cobre (ver "Se o sistema não cabe no contrato"), pare e avise. Modelo preenchido: `docs/exemplo-SISTEMA-toy.md`.

0. **Já é um core libretro compilado (`.so`/`.dll`)?** Então não escreva adaptador: `cargo run --release -p libretro-player -- --core CORE.so [JOGO]`
   (crate `libretro-host`; limites no cabeçalho de `crates/libretro-host/src/lib.rs`: só RetroPad, vídeo por software, conteúdo de um
   arquivo). Só vá ao passo 1 se o host não der conta, ou se o dono quiser um emulador próprio (com ícone, opções, capas).
1. **Entender o núcleo recebido.** Descobrir: linguagem (Rust, C, C++), se é um crate, um
   código-fonte avulso ou um core libretro (`.so`/`.dll`); como carrega ROM; como roda um quadro;
   formato do vídeo (pixel format, largura/altura, pitch); formato do áudio (taxa, canais, tipo);
   entrada (botões/periféricos); como salva/restaura estado; onde está a RAM.
2. **Escolher a ligação**:
   - núcleo em Rust → depender dele como crate;
   - C/C++ → FFI (`bindgen` ou `extern "C"` manual) dentro de um crate `<sistema>-core`;
   - só existe como core libretro compilado → crate que carrega a biblioteca e fala a API
     libretro (o contrato `emu-api` já espelha libretro: RetroPad, XRGB8888, estado em bytes).
3. **Gerar o esqueleto** (não escrever do zero): `tools/new-emulator.sh [--libretro] [--ffi-c] <slug> "<Nome>" "<pasta libretro-thumbnails>"`
   (`--libretro` gera também o core libretro, sem nada a editar; `--ffi-c` acrescenta `build.rs` com a crate `cc` e `src/ffi.rs` para núcleos em C/C++).
   Modelo preenchido e funcionando: `crates/toy-core` (veja `cargo run -p toy`).
   cria `crates/<slug>-core` (adaptador com marcas `TODO(core)`) e `crates/<slug>` (app que já abre com tela preta).
   Depois, preencher os `TODO(core)` em `core.rs` e o ícone/links em `main.rs`. O esqueleto gerado (core e app) compila.
   O adaptador (`core.rs`): `static INFO: SystemInfo` e
   `struct ...Core` implementando `emu_api::Core`. Conversões típicas:
   - entrada: bitmask RetroPad → entrada nativa (declarar em `SystemInfo::buttons` só os botões reais);
   - vídeo: qualquer formato → `u32` XRGB8888 (`0x00RRGGBB`) em `Frame`;
   - áudio: qualquer formato → estéreo intercalado `f32`; reamostrar se a taxa não for a pedida em `create(sample_rate)`;
   - estado: serialização do núcleo ↔ `Vec<u8>`;
   - RAM: `system_ram()` / `system_ram_mut()`.
4. **Criar `crates/<nome>` (app)**: `brand::set(...)` antes de carregar a config, `config::Config::load()`,
   `i18n::set_lang`, `eframe::run_native(..., App::new(cc, &INFO, rom, scale))`. Modelo: o `main.rs` do
   Mil Capivaras (`~/Projetos/mil_capivaras/crates/app/src/main.rs`). Incluir ícone e, se houver, bezels.
5. **Idiomas e dados**: os textos próprios do app entram com `i18n::add_catalog("pt", include_str!(...))` (os catálogos de `assets/i18n/` têm só a interface genérica); `SystemInfo::cover_aspect` define o formato das capas; criar `games.rs` (identificação por
   CRC32 via `emu_api::crc32`) só se houver banco de jogos; senão retornar `None`.
6. **Libretro**: `--libretro` do gerador (ou `crates/toy-libretro` como exemplo). O Mil Capivaras tem uma versão específica do PV-1000.
7. **Documentação**: rode `tools/new-docs.sh` (parâmetros no `--help`) e escreva os `TODO(system)` a partir do que você sabe do sistema e do núcleo, sem inventar: número ou afirmação sem fonte ou medição vira `?` e aviso ao dono. Siga as **regras de escrita** de `docs/kit-docs.md`: estado atual (nada de datas de quando foi feito, "(antes: …)" ou planos no futuro; o histórico vai no `CHANGELOG.md`), um fato num lugar só, afirmação sobre outro emulador conferida no código dele, links sem "seção N". Termine com `tools/check-links.py`. Modelo de profundidade: `docs/` do Mil Capivaras (`~/Projetos/mil_capivaras`, só leitura). Os modelos estão em inglês; as versões pt-BR/es só depois que o inglês estiver pronto.
8. **Validar**: `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets`;
   rodar um jogo de verdade (vídeo, áudio, controles, save state, rewind). Se existir uma referência (núcleo
   original, core FPGA simulado, outro emulador), a validação é a comparação com ela, não a sua avaliação do código:
   o app ganha `--shot N`, `--press-start` e `--dump-ram` (modelo: `main.rs` do Mil Capivaras), a referência
   fica em `reference/` com o `compare.sh` do kit (`tools/new-docs.sh --reference`), e o resultado (pixels e bytes
   diferentes por jogo e quadro) vai para `docs/accuracy.md`. Ferramentas de diagnóstico (rastreio, log de
   interrupções) ficam no repositório, nunca só numa pasta temporária. Antes de culpar o emulador por uma
   diferença, confira se a ROM é a versão do banco de jogos (CRC32). Detalhes: "Validação com uma referência"
   em `docs/kit-docs.md`.
9. **Fechar**: `CHANGELOG.md` atualizado e uma nota curta sobre o que o sistema tem de particular.

## Se o sistema não cabe no contrato
O contrato cobre: 1 ou mais jogadores com RetroPad, vídeo XRGB8888, áudio estéreo, estado em bytes,
uma RAM, opções de núcleo, arquivos de sistema/BIOS (`SystemInfo::bios` + `Core::set_bios`; a interface
procura por CRC32 e avisa o que falta), save de bateria (`Core::battery_ram`/`load_battery`, gravado em
`.sav`) e ROM em vários arquivos (`SystemInfo::multi_file_roms`). **Não cobre ainda**: disco/fita/troca
de mídia, teclado/mouse/light gun/analógico, resolução que muda no meio do jogo, várias RAMs. Se o núcleo recebido
precisa de algo disso: **pare, diga o que falta e proponha a extensão** (novo método com padrão vazio em `Core`
ou novo campo em `SystemInfo`, sem quebrar os cores existentes) e espere o dono aprovar.

## Pendências conhecidas da base
- Build, testes e clippy verificados em 20/10/2026 (precisa de `pkg-config` e `libasound2-dev`). Se algum teste do frontend falhar, confirme se é da base antes de culpar o núcleo novo.
- Interface aprovada pelo dono em 20/10/2026, numa sessão gráfica, com o exemplo `toy` (nada quebrado). Num sistema novo, confira de novo vídeo, áudio, controles, save state e rewind.
- O contrato não cobre disco/fita, periféricos fora do RetroPad, resolução variável, várias RAMs; o host libretro idem. BIOS e save de bateria: cobertos desde 21/10/2026 (ver `CHANGELOG.md`); o host libretro ainda não os repassa.
- O Mil Capivaras tem a própria cópia da interface, de propósito (decisão de 20/10/2026): não sincronizar, não criar dependência entre os repos.
