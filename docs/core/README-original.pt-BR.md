# vsmile — emulador do VTech V.Smile em Rust

Emulador **independente** (escrito do zero) do console educativo **VTech V.Smile**,
com foco em rodar no **Linux** — mas que também compila e roda no Windows e no
macOS. O hardware emulado é o SoC **SunPlus SPG200**, cuja CPU é a **unSP** de
16 bits.

> **Estado atual:** o emulador **roda jogos comerciais e é jogável, com vídeo,
> áudio e controle por teclado**. Renderiza corretamente a abertura de *Batman —
> Gotham City Rescue* (logo da Warner Bros + tela de título em cores) e reproduz
> a música e os efeitos do jogo.

![estado](https://img.shields.io/badge/estado-jog%C3%A1vel-success) ![testes](https://img.shields.io/badge/testes-34%20passando-success) ![licença](https://img.shields.io/badge/licen%C3%A7a-MIT-blue)

> 📖 Registro técnico/dev de **tudo que foi implementado** (cada funcionalidade,
> uso, verificação, investigações de RE): [`DOCUMENTACAO.md`](DOCUMENTACAO.md).

---

## Índice

- [Por que Rust](#por-que-rust)
- [Bibliotecas empregadas](#bibliotecas-empregadas)
- [Arquitetura](#arquitetura)
- [Como compilar e rodar](#como-compilar-e-rodar)
- [Controles](#controles)
- [Portar / rodar no Linux](#portar--rodar-no-linux)
- [O que foi implementado](#o-que-foi-implementado)
- [Compatibilidade](#compatibilidade)
- [Bugs e limitações conhecidos](#bugs-e-limitações-conhecidos)
- [Planos futuros](#planos-futuros)
- [Como funciona (visão técnica)](#como-funciona-visão-técnica)
- [A caçada ao bug do DS](#a-caçada-ao-bug-do-ds)
- [Créditos e licenças](#créditos-e-licenças)

---

## Por que Rust

O projeto é escrito em **Rust (edição 2021)**, organizado como um *workspace*
Cargo com quatro crates. Motivos da escolha:

- **Segurança de memória sem coletor de lixo.** Um emulador manipula milhões de
  acessos a memória por segundo; Rust garante ausência de *buffer overflows* e
  *use-after-free* em tempo de compilação, sem o custo de um GC.
- **Zero `unsafe`.** Todo o núcleo é Rust seguro — nenhum bloco `unsafe`. Isso é
  possível graças a uma decisão de projeto (ver [Arquitetura](#arquitetura)) que
  evita referências cíclicas (`Rc<RefCell>`) entre CPU, barramento e periféricos.
- **Desempenho nativo.** Compilado em *release* com LTO, roda o console a 60 FPS
  folgado em qualquer PC moderno.
- **Portabilidade.** O núcleo depende apenas da biblioteca padrão (`std`), então
  compila para Linux, Windows, macOS e além sem mudança de código.
- **Testabilidade.** O emulador é uma biblioteca *headless* com testes
  determinísticos — dá para rodar uma ROM e verificar a saída sem abrir janela.

Padrões de Rust usados de forma idiomática: *traits* para abstrair o barramento e
a I/O, *closures* para quebrar dependências cíclicas, *enums* e *pattern matching*
para o decodificador de instruções, e empréstimos disjuntos de campos para deixar
CPU e periféricos conversarem sem `unsafe`.

---

## Bibliotecas empregadas

A filosofia foi **mínimo de dependências**. O emulador em si (núcleo + runner)
não usa **nenhuma** biblioteca externa — é Rust puro. Só o frontend gráfico puxa
duas crates, ambas leves e multiplataforma.

| Crate do projeto | Tipo | Dependências externas                     |
|------------------|------|-------------------------------------------|
| `vsmile-core`    | lib  | **nenhuma** (só a `std`)                  |
| `vsmile-runner`  | bin  | **nenhuma**                               |
| `vsmile-gui`     | bin  | `minifb 0.27`, `cpal 0.15`                |
| `vsmile-wasm`    | cdylib | **nenhuma** (ABI em C, sem `wasm-bindgen`) |

### `minifb` 0.27 — janela e teclado
[minifb](https://crates.io/crates/minifb) ("mini framebuffer") abre uma janela e
deixa você escrever um *buffer* de pixels direto nela, além de ler o teclado. É
deliberadamente simples: nada de engine de UI, shaders ou GPU. Perfeito para um
emulador 2D — basta converter o framebuffer 320×240 do console para `0x00RRGGBB`
e mandar para a tela a cada quadro. Multiplataforma (Win32, X11/Wayland, Cocoa).

### `cpal` 0.15 — áudio
[cpal](https://crates.io/crates/cpal) ("Cross-Platform Audio Library") dá acesso
de baixo nível à placa de som em Windows (WASAPI), Linux (ALSA), macOS
(CoreAudio) e outros. Ele roda um *callback* de áudio numa thread separada que
puxa amostras de um buffer compartilhado; o loop de emulação empurra as amostras
do SPU (reamostradas de ~281 kHz para a taxa do dispositivo) nesse buffer.

### Ferramentas usadas apenas como **referência** (não dependências)
O comportamento do hardware foi estudado a partir de emuladores open source — mas
**nenhum código foi copiado**; é uma reimplementação *clean-room*:
- [`veesem`](https://github.com/sp1187/veesem) (C++, licença ISC)
- [`VFrown`](https://github.com/Schnert0/VFrown) (C, licença MIT)
- [MAME](https://github.com/mamedev/mame) (C++, BSD-3)

Usar **dois** emuladores independentes como oráculos foi fundamental: quando as
flags, os desvios e os modos de endereçamento batem entre veesem **e** VFrown, há
alta confiança de que a minha implementação está correta.

---

## Arquitetura

```
vsmile/
├── Cargo.toml                     # workspace
└── crates/
    ├── vsmile-core/               # o emulador (biblioteca, headless, testável)
    │   └── src/
    │       ├── lib.rs             # fachada + testes de integração
    │       ├── cpu.rs             # CPU unSP (SunPlus, 16 bits)
    │       ├── bus.rs             # trait Bus + FlatBus (testes)
    │       ├── memory.rs          # Spg200Mem: mapa de memória do SPG200
    │       ├── io.rs              # trait SystemIo + Region (ROM/RAM externa)
    │       ├── extmem.rs          # decodificação de chip-select (ROMCSB/CSB1-3)
    │       ├── ppu.rs             # vídeo: BGs, sprites, paleta, blending
    │       ├── spu.rs             # áudio: 16 canais PCM/ADPCM, envelopes
    │       ├── adpcm.rs           # decodificador ADPCM (IMA/DVI)
    │       ├── irq.rs             # controlador de interrupções (8 IRQ + FIQ)
    │       ├── timer.rs           # timers e base de tempo (32768 Hz)
    │       ├── uart.rs            # UART (serial do controlador)
    │       ├── adc.rs             # conversor A/D (bateria etc.)
    │       ├── gpio.rs            # portas A/B/C
    │       ├── dma.rs             # DMA genérico
    │       ├── watchdog.rs        # watchdog
    │       ├── random.rs          # LFSRs do RNG
    │       ├── clock.rs           # divisores de clock dos periféricos
    │       ├── spg.rs             # Spg200: junta CPU + barramento + periféricos
    │       ├── joy.rs             # controlador (joystick) via UART
    │       ├── vsmile.rs          # VSmile: cola do console (I/O concreta)
    │       ├── inflate.rs         # descompressor DEFLATE (RFC 1951) puro
    │       ├── zip.rs             # leitor ZIP mínimo (carrega cartucho .zip)
    │       ├── save.rs            # serialização de save states (trait State)
    │       ├── disasm.rs          # desmontador unSP
    │       ├── debugger.rs        # breakpoints, watchpoints, step, inspeção
    │       ├── ppu_view.rs        # inspeção da PPU: paleta, sprites, tiles
    │       └── settings.rs        # PpuViewSettings (debug)
    ├── vsmile-runner/             # runner headless → screenshot BMP/PPM
    │   └── src/main.rs
    ├── vsmile-gui/                # janela interativa (vídeo + áudio + teclado)
    │   ├── src/main.rs
    │   └── src/audio.rs           # saída cpal + reamostragem
    └── vsmile-wasm/               # binding WebAssembly (ABI em C)
        └── src/lib.rs
web/                               # frontend de navegador (canvas + Web Audio)
├── index.html
└── README.md
```

### Decisão de projeto central — sem referências cíclicas

No hardware, CPU, barramento e periféricos formam um grafo cíclico (a CPU lê o
barramento; os periféricos levantam IRQ de volta na CPU; a PPU lê gráficos do
barramento que a contém). Em C++ isso se resolve com ponteiros mútuos; em Rust
seguro, não.

A solução adotada:

- **CPU e barramento são campos separados** do `Spg200`. Assim
  `cpu.step(&mut mem)` usa *empréstimos disjuntos* — a CPU mexe nela mesma e
  recebe o barramento emprestado, sem ciclo.
- **Periféricos que leem o barramento** (PPU e DMA de sprite) recebem um
  **closure de leitura**, construído desestruturando os campos do `Spg200Mem`
  (dando acesso a RAM/ROM sem tocar no próprio periférico).
- **O controlador** não chama o SoC de volta: ele acumula suas saídas (bytes para
  a UART, bordas de RTS que geram IRQ) em filas que o barramento **drena**.
- **As IRQs** são recalculadas a cada passo e *empurradas* para a CPU, em vez de
  os periféricos escreverem direto nela.

O resultado é 100% Rust seguro, sem `unsafe`, `Rc` ou `RefCell`.

---

## Como compilar e rodar

Requer o toolchain Rust (via [rustup](https://rustup.rs)).

```bash
cargo test            # 30 testes do núcleo
cargo build --release
```

### Jogar (janela interativa, com som)

```bash
cargo run --release --bin vsmile-gui -- --cart JOGO.bin
```

Opções: `--sysrom BIOS.bin` (opcional — jogos dão boot pelo cartucho),
`--region 0xE`, `--pal`, `--scale 2|4`.

### Rodar headless (gera screenshot)

```bash
cargo run --release --bin vsmile-run -- --cart JOGO.bin --frames 1500 --out tela.bmp
```

`--out` aceita `.bmp` (abre em qualquer visualizador) ou `.ppm`. `--debug` imprime
diagnósticos (PC da CPU, backgrounds ativos, instruções desconhecidas,
estatísticas de áudio etc.).

### Desmontar e depurar

```bash
# Desmonta a partir de um endereço (em palavras):
cargo run --release --bin vsmile-run -- --cart JOGO.bin --disasm 0xC9E8 --disasm-count 24

# REPL do depurador (breakpoints, watchpoints, step, inspeção):
cargo run --release --bin vsmile-run -- --cart JOGO.bin --debugger

# Visualizar a PPU (após rodar alguns quadros):
cargo run --release --bin vsmile-run -- --cart JOGO.bin --frames 600 \
    --ppu-info --ppu-palette paleta.bmp --ppu-tiles tiles.bmp

# Teste de compatibilidade em lote (roda toda uma pasta, isola crashes):
cargo run --release --bin vsmile-run -- --batch "VTech - V.Smile" --batch-frames 240
```

No REPL: `b <addr>`/`db <addr>` (breakpoint), `w <addr>`/`dw <addr>` (watchpoint de
RAM), `s [n]` (passo), `c [max]` (continue), `f [n]` (avança quadros), `r`
(registradores), `m <addr> [n]` (dump de RAM), `d [addr] [n]` (desmonta), `h`
(ajuda), `q` (sair). Os comandos também funcionam via *pipe* (`printf '...' | ...`).

### ROMs

As ROMs do V.Smile são de 16 bits **little-endian**. O cartucho contém os vetores
de boot (o console dá boot pelo cartucho, não pela BIOS). O loader aceita tanto
`.bin` cru quanto `.zip` — basta apontar `--cart JOGO.zip` que o emulador extrai a
ROM de dentro (descompressão DEFLATE própria, sem dependências). **O projeto não
acompanha ROMs/BIOS.**

---

## Controles

| Tecla         | Função                                              |
|---------------|-----------------------------------------------------|
| **Setas**     | Direcional                                          |
| **Enter**     | OK / Enter                                          |
| **Backspace** | Voltar                                              |
| **H**         | Ajuda                                               |
| **A**         | ABC / Zona de aprendizado                           |
| **Z X C V**   | Botões coloridos (vermelho / verde / azul / amarelo)|
| **O / P / R** | Ligar (ON) / Desligar (OFF) / Restart               |
| **F5 / F7**   | Salvar / carregar *save state* (`<cartucho>.state`) |
| **Esc**       | Sair                                                |

Com `--p2` (segundo controlador): **I/J/K/L** = direcional, **1–4** = botões
coloridos (vermelho/verde/azul/amarelo), **5/6/7/8** = Enter/Voltar/Ajuda/ABC.

---

## Portar / rodar no Linux

O projeto foi escrito **sem código específico de plataforma**, então no Linux é
basicamente "clonar e compilar".

### 1. Toolchain
No Linux, o toolchain padrão do rustup (`x86_64-unknown-linux-gnu`) usa o linker
do sistema (`cc`/`gcc`), presente na maioria das distros — **sem** o problema de
linker que existe no Windows (ver nota no fim).

```bash
curl https://sh.rustup.rs -sSf | sh
rustup default stable
```

### 2. `vsmile-core` + `vsmile-runner` — nada a instalar
São **Rust puro, sem dependências de sistema**. Compilam e rodam de imediato:

```bash
cargo build --release -p vsmile-core -p vsmile-runner
cargo run --release --bin vsmile-run -- --cart JOGO.bin --out tela.ppm
```

### 3. `vsmile-gui` — libs de janela (X11/Wayland) e áudio (ALSA)
A GUI usa `minifb` (janela) e `cpal` (áudio, via **ALSA** no Linux). Instale os
headers de desenvolvimento:

**Debian/Ubuntu:**
```bash
sudo apt install libxkbcommon-dev libwayland-dev libxcb1-dev libasound2-dev
```
**Fedora:**
```bash
sudo dnf install libxkbcommon-devel wayland-devel libxcb-devel libX11-devel alsa-lib-devel
```
**Arch:**
```bash
sudo pacman -S libxkbcommon wayland libxcb libx11 alsa-lib
```

Depois: `cargo run --release --bin vsmile-gui -- --cart JOGO.bin`

> Num servidor headless/CI, use só `vsmile-core` + `vsmile-runner` (não exigem
> nada do sistema). Se o `cpal` não achar dispositivo de áudio, a GUI roda **sem
> som** em vez de travar.

### Nota sobre o build no Windows (desta máquina)
Nesta máquina faltava o linker MSVC (`link.exe`), então o workspace usa o
toolchain **GNU** (`stable-x86_64-pc-windows-gnu`), fixado via `rustup override`.
**No Linux isso não se aplica.**

---

## O que foi implementado

### CPU unSP (SunPlus, 16 bits) — `cpu.rs`
8 registradores (SP, R1–R4, BP, SR, PC), flags N/Z/S/C, segmentos CS/DS (22 bits
de endereço), **todos os modos de endereçamento** ([BP+imm6], imm6, imm16,
[imm16], [Rs] com pré/pós inc/dec e DS, registrador, [A6]), ALU completa,
shifts/rotates (ASR/LSL/LSR/ROL/ROR) com o nibble de shift, push/pop,
call/goto/retf, mul/muls (multiplica-acumula de filtros FIR), IRQ/FIQ com vetores,
break/nop/reti. Flags e semântica validadas contra **veesem e VFrown**.

### Vídeo (PPU) — `ppu.rs`
Dois planos de fundo (tile maps), até 256 sprites, 4 camadas de profundidade,
paleta de 256 cores (RGB555 + transparência), 2/4/6/8/16 bits por pixel, modos
bitmap/wallpaper/register, scroll e compressão por linha, blending, fade, DMA de
sprites, IRQs de vblank/posição. Gera framebuffer 320×240 RGB555.

### Áudio (SPU) — `spu.rs`, `adpcm.rs`
16 canais com formas de onda PCM 8/16 bits e **ADPCM** (IMA/DVI), fase/pitch,
envelopes de volume programáveis com reload automático via ponteiro na memória,
pitch-bend, rampdown, mixagem estéreo com pan e volume mestre, beat IRQ e IRQ de
canal. **Interpolação linear** entre amostras (desligável pelo bit
`no_interpolation`) e **filtro passa-baixa** de saída (bit `low_pass_enable`).
Gera amostras estéreo a ~281,25 kHz; o frontend reamostra para a placa com um
**decimador anti-aliasing** (biquad Butterworth + interpolação linear) via `cpal`.

### Barramento e memória — `memory.rs`, `extmem.rs`, `io.rs`
Mapa completo do SPG200 (RAM 10 KiW, registradores de PPU/SPU/GPIO/timer/sistema,
DMA, memória externa). Decodificação de chip-selects (ROMCSB/CSB1/CSB2/CSB3) por
`address_decode`. Trait `SystemIo` abstrai ROM/cartucho, portas, ADC e UART.

### Periféricos
IRQ controller (8 linhas + FIQ selecionável) · timers (base 32768 Hz + timer A/B) ·
UART · ADC (4 canais) · GPIO (portas A/B/C) · DMA genérico · watchdog · RNG (LFSRs).

### Console V.Smile — `vsmile.rs`, `joy.rs`, `spg.rs`
Cola do console (sysrom/cartucho nos chip-selects, botões, código de região,
bateria), o **controlador (joystick)** completo com handshake serial RTS/CTS
(incluindo um **segundo controlador** opcional nas linhas do controle 2), e o
topo `Spg200` que roda o sistema instrução a instrução e entrega as interrupções.

### Frontends
`vsmile-runner` (headless → screenshot BMP/PPM) e `vsmile-gui` (janela interativa
em tempo real com **vídeo, áudio e teclado**). Ambos carregam cartucho `.bin` ou
`.zip` (descompressão DEFLATE própria) e persistem a **NVRAM** dos cartuchos Art
Studio (`--nvram FILE.sav`, com autodeteção pelo nome e autosave na GUI).

### Save states — `save.rs`
Serialização binária do estado completo do console (CPU, RAM, PPU, SPU e todos os
periféricos) em Rust puro, sem dependências. Como o núcleo não tem referências
cíclicas nem `unsafe`, o estado é uma árvore de campos serializável; um trait
`State` + um macro de lista-de-campos geram a escrita/leitura sem risco de
dessincronização. As ROMs ficam de fora (recarregadas do arquivo). Verificado
rodando *Batman* por 400 quadros direto **vs.** salvar no quadro 300, restaurar e
rodar mais 100 — os framebuffers saem **idênticos pixel a pixel**.

### WebAssembly — `vsmile-wasm` + `web/`
O núcleo compila para `wasm32-unknown-unknown` **sem alteração** (é a recompensa
do `std`-only + zero `unsafe`). O crate `vsmile-wasm` é um `cdylib` com uma ABI
em C enxuta (`vsmile_new`/`run_frame`/`rgba_ptr`/`audio_ptr`/`set_input`…), sem
`wasm-bindgen` — o `unsafe` fica só na ponte de ponteiros, nunca no emulador. A
ABI é testada **nativamente** (`cargo test -p vsmile-wasm`: ciclo de vida, geração
de quadro/áudio, ROM inválida → ponteiro nulo). O frontend `web/index.html` carrega
o `.wasm` (~105 KB), desenha num `<canvas>` e toca pela Web Audio, com seletor de
ROM (`.bin`/`.zip`) e teclado. Build: `build-web.bat` ou
`cargo build --release --target wasm32-unknown-unknown -p vsmile-wasm`.

### Desmontador — `disasm.rs`
Decodifica palavras de instrução unSP em mnemônicos legíveis (sintaxe-expressão
da SunPlus), espelhando exatamente o decodor da CPU. Exposto via
`vsmile-run --disasm 0xC9E8 [--disasm-count N]`. Aplicado ao boot do *Batman*,
produz código coerente: `GOTO`, `CALL` para bancos altos, laços de cópia para a
RAM (`R2 = DS:[R4++]` → `[R3++] = R2`) e a escrita do DS em `[0x3D2F]`.

### Depurador — `debugger.rs`
Núcleo do depurador (puro, *headless*, testável): **breakpoints** de execução
(`CS:PC`), **watchpoints** de RAM (dispara quando uma palavra muda, com leitura
sem efeitos colaterais), **passo a passo** (`step`), **continue** até parar, e
**inspeção** de registradores/RAM/desmontagem ao vivo. Um REPL fino no runner
(`vsmile-run --debugger`) expõe tudo: `b`/`db` (breakpoints), `w`/`dw`
(watchpoints), `s`/`c`/`f` (passo/continue/quadro), `r` (registradores),
`m` (dump de RAM), `d` (desmonta). Validado no *Batman*: um breakpoint no alvo do
`GOTO` inicial cai em `INT OFF` → chute do watchdog (`0x55AA` em `0x3D24`) →
leitura do extmem; e um watchpoint flagra o **teste de RAM** do boot escrevendo o
padrão `55AA AA55 5A5A A5A5`.

### Visualizador da PPU — `ppu_view.rs`
Inspeção gráfica do estado da PPU: **paleta** (grade 16×16 → BMP), **tabela de
sprites** (posição, tile, tamanho, paleta, profundidade, flips), **configuração
dos BGs** (bpp, tamanho de tile, scroll, ponteiros) e um **tile sheet** que
decodifica os tiles de um segmento em imagem. A decodificação de tile é a mesma
da PPU (byte-swap + mapeamento de paleta), fixada por um teste pixel-a-pixel
contra o cenário conhecido de renderização. Exposto no runner por `--ppu-info`
(texto), `--ppu-palette OUT.bmp` e `--ppu-tiles OUT.bmp`. No *Batman*, a paleta
sai com as 26 cores do logo da DC e o BG0 aparece como `4bpp, tiles 16×16`.

### Testes
28 testes: ALU/flags/branch/push-pop da CPU, boot por ROM pelo barramento, desenho
de tile + vblank na PPU, máscara de registrador de sistema, teste ponta-a-ponta de
IRQ (vblank → handler → `reti`), DEFLATE (bloco armazenado + back-reference),
leitura de `.zip`, round-trip da NVRAM, determinismo/idempotência do save state,
opt-in do segundo controlador, 4 testes do desmontador (instruções conhecidas,
desvios, call/goto, modos de endereçamento), 5 do depurador (breakpoint para antes
de executar, watchpoint dispara na escrita, continue/limite, step, desmontagem) e 2
do visualizador da PPU (imagem de paleta, decodificação de tile pixel-a-pixel).
Mais 2 testes no `vsmile-wasm` (ciclo de vida da ABI + quadro/áudio, e ROM inválida
→ ponteiro nulo), totalizando **32** no workspace.

---

## Compatibilidade

Há um modo de **teste em lote** que roda toda uma pasta de ROMs, isola panics e
classifica cada jogo. Rodando sobre o acervo de **569 cartuchos** (900 quadros por
jogo — resultado idêntico ao de 240 quadros, ou seja, as telas pretas estão
**travadas, não lentas**):

```bash
vsmile-run --batch "VTech - V.Smile" --batch-frames 900 --batch-list compat.csv
```

O lote **ignora automaticamente *bad dumps*** (ROMs corrompidas, marcadas `[b]`/
`[b1]`… na convenção No-Intro) — 3 no acervo —, deixando **566 ROMs válidas**:

| Resultado                     | Qtd | Observação                               |
|-------------------------------|-----|------------------------------------------|
| **Boot limpo (desenha algo)** | 524 | de 566 válidas                            |
| Tela preta                    | 35  | **30 são V.Smile Baby**, 3 Motion, 2 padrão |
| Opcode desconhecido           | 7   | 6 Motion, 1 padrão (*Zayzoo*)             |
| Erro de carregamento          | 0   | o loader `.zip`/`.bin` abriu todos        |
| **Panic (crash)**             | **0** | **nenhum jogo derruba o emulador**      |

**Boot limpo por variante de console (sem *bad dumps*):**

| Variante        | Boot limpo    | Observação                                    |
|-----------------|---------------|-----------------------------------------------|
| **V.Smile padrão** | **428/431 (99,3%)** | 3 holdouts reais                          |
| V.Smile Motion  | 96/105 (91,4%) | bootam sem o acelerômetro; 9 travam esperando-o |
| V.Smile Baby    | 0/30 (0%)      | console diferente — não dá boot no padrão      |

As falhas se concentram em **duas variantes de console diferentes**, que nenhum
emulador de referência (nem o veesem) implementa:

- **V.Smile Baby** (30 títulos) — console simplificado com protocolo de controle
  próprio; **nenhum** dá boot no V.Smile padrão.
- **V.Smile Motion** (105 títulos) — mesmo SoC, mas com controle de movimento
  (acelerômetro); a maioria (96) dá boot, 9 travam esperando o periférico.

### BIOS e os holdouts do padrão

Alguns jogos chamam rotinas da **BIOS do console** (que o hardware real tem) como
biblioteca de runtime. Exemplo medido com o depurador: *Little Red Riding Hood*
lê a sysrom **~987 mil vezes** — ela é essencial, não dá para "fingir" com alguns
valores. Com a `dummy_sysrom` ela trava; com a BIOS real chega ao **menu completo**
(`Learning Adventure / Learning Zone / Options`).

**Auto-detecção (sem obrigar):** se o usuário **não** passar `--sysrom`, o
frontend procura uma BIOS de V.Smile padrão (`[BIOS] VTech V.Smile … .zip`) **na
pasta do cartucho** e a usa automaticamente; se não houver nenhuma, cai na
`dummy_sysrom` e o jogo dá boot pelo cartucho. Assim os jogos que dependem da BIOS
funcionam quando ela está presente, **sem forçar** quem não a tem. O projeto
**não acompanha** a BIOS — o usuário fornece a sua.

Descontando esse caso, sobram **2 holdouts** reais do V.Smile padrão, ambos
caracterizados com o depurador do projeto:

- **Toy Story 2 (USA)** — executa limpo (sem opcode inválido) e progride por uma
  inicialização longa (rampas de `wave_in`, espera de canais do SPU pararem — que
  o depurador confirma que **passa**), mas nunca liga o vídeo. A causa raiz está
  mais adiante e não foi isolada; provável sensibilidade a timing ou espera de um
  evento de periférico.
- **Zayzoo (USA)** — a RE (com o anel de 16 instruções antes da divergência) achou
  o ponto exato: o jogo executa `CALL 0x045104` em `0x03FDC7`, e `0x045104` é
  **padding `0xFFFF` genuíno** no ROM (os 4 MiW carregam corretos — não é bug de
  loader). No hardware real o jogo **não chega** a esse `CALL`; a divergência vem
  de bem antes (um valor/flag calculado errado, ou execução de dados como código),
  além da janela instrumentada. Isolar exige bisecção extensa.

Nenhum dos dois depende da BIOS (falham igual com e sem).

Relatório completo em [`COMPATIBILIDADE.txt`](COMPATIBILIDADE.txt); status de
**cada ROM** (CSV) em [`COMPATIBILIDADE_completa.csv`](COMPATIBILIDADE_completa.csv).

## Bugs e limitações conhecidos

### Resolvido (histórico)
- **Corrupção de DS na entrada de IRQ** *(resolvido)* — a sincronização do
  segmento de dados reescrevia o DS da CPU após toda instrução, corrompendo o SR
  quando a própria CPU o alterava. Era o que impedia o vídeo de aparecer. Ver
  [A caçada ao bug do DS](#a-caçada-ao-bug-do-ds).

### Limitações atuais
- **Segundo controlador não validado contra jogo 2-players.** O suporte existe
  (opt-in via `--p2`), espelhando a fiação do controlador 1 nas linhas do 2, mas
  nenhum emulador de referência (nem o veesem) implementa o 2º controle, então
  não há oráculo — a corretude foi verificada só por simetria e ausência de
  regressão no 1º controle.
- **Mapeamento de input não afinado.** As teclas funcionam, mas o mapeamento
  direcional/botões ainda não foi validado contra o layout real do controle — a
  direção dos eixos pode precisar de ajuste.
- **Instruções desconhecidas viram NOP.** Em vez de abortar, opcodes não
  reconhecidos são contados e tratados como NOP (`cpu.unknown_ops`). Com software
  real esse contador fica em **0**; se subir, é sinal de bug a investigar.
- **Watchdog aproximado.** É verificado uma vez por quadro (como no veesem), não a
  cada instrução — suficiente na prática, mas não ciclo-exato.
- **Timing espelha o veesem exatamente, mas não é ciclo-exato vs MAME.** O modelo
  de timing foi **auditado contra o veesem** e bate em todos os parâmetros —
  ciclos por instrução (todos os modos de endereçamento, grupo F, entrada de IRQ),
  período de scanline da PPU (1716 NTSC / 1728 PAL), clocks do SPU (96/384/13),
  ADC (16/1), timer (27M/32768), watchdog (27M·3/4) e DMA (cópia instantânea).
  Nenhum dos dois modela *wait states* de memória. Há **testes de regressão** que
  travam os ciclos por instrução. O que falta é a comparação **ciclo-exato contra
  um traço do MAME** — um nível acima do veesem.
- **Variantes V.Smile Baby e V.Smile Motion.** São consoles diferentes (controle
  próprio / acelerômetro). Sem referência em nenhum emulador open source; os
  títulos dessas linhas não bootam ou travam esperando o periférico.

---

## Planos futuros

### Curto prazo
- [ ] Afinar o mapeamento de teclas do controle e validar a direção dos eixos.
- [x] Suporte a `.zip` direto no loader (sem extrair à mão). *(DEFLATE próprio, sem deps)*
- [x] Persistir a NVRAM dos cartuchos Art Studio em arquivo. *(`--nvram`, autosave)*
- [x] Filtro passa-baixa / interpolação do SPU e reamostragem de áudio melhor.
      *(bits `no_interpolation`/`low_pass` + decimador biquad no frontend)*

### Médio prazo
- [x] Suporte ao segundo controlador. *(opt-in `--p2`, sem regressão no 1º)*
- [ ] Testes de precisão automatizados comparando com traços do MAME.
- [x] Salvar/carregar *save states* (o estado é serializável por construção).
      *(F5/F7 na GUI, `--save-state`/`--load-state` no runner; verificado
      pixel-a-pixel contra uma execução direta)*
- [ ] Opções de vídeo: escala inteira, filtro, *fullscreen*.

### Longo prazo
- [x] **Debugger** integrado: disassembler unSP, breakpoints, watchpoints de RAM,
      inspeção de RAM/registradores (`debugger.rs` + REPL `vsmile-run --debugger`)
      e visualizador de tiles/sprites/paleta da PPU (`ppu_view.rs` +
      `--ppu-info`/`--ppu-palette`/`--ppu-tiles`).
- [x] *Port* para **WebAssembly** — o núcleo (`std`-only, sem `unsafe`) compila
      para `wasm32-unknown-unknown` **sem mudar uma linha**; o crate `vsmile-wasm`
      expõe uma ABI em C (sem `wasm-bindgen`) e o frontend `web/` roda no
      navegador com `<canvas>` + Web Audio. `.wasm` de ~105 KB.
- [ ] Suporte às variantes da família: **V.Smile Baby** (controle próprio) e
      **V.Smile Motion** (acelerômetro) — 135 títulos no acervo, sem referência em
      emulador open source (engenharia reversa do zero). O SPG200 já é
      compartilhado, então é trabalho de periférico/controle, não de CPU.
- [ ] Empacotamento para Linux (AppImage/Flatpak) e Windows.

---

## Como funciona (visão técnica)

Um quadro é executado por `Spg200::run_frame`, que repete `step()` até a PPU
completar a scanline 239:

1. sincroniza o segmento de dados DS entre a CPU e o registrador `0x3D2F`;
2. `cpu.step(&mut mem)` executa uma instrução (lendo/escrevendo no barramento) e
   devolve os ciclos consumidos;
3. os periféricos (I/O, ADC, UART, timers, **SPU**, PPU) avançam por esses ciclos;
4. as 8 linhas de IRQ e a de FIQ são recalculadas e empurradas para a CPU.

O frontend coleta o framebuffer (`framebuffer()`) e o áudio (`take_audio()`) a
cada quadro; o áudio é reamostrado de ~281 kHz para a taxa da placa e tocado via
`cpal`.

---

## A caçada ao bug do DS

O momento mais instrutivo do projeto. Depois de CPU, PPU e periféricos prontos, o
jogo **executava milhões de instruções mas a tela ficava preta** — e após ~300
quadros a CPU "desgarrava" para endereços inválidos.

Diagnóstico (com instrumentação temporária): a CPU rodava **100% limpa** por ~300
quadros e só então saltava para lixo — padrão clássico de *estado corrompido*, não
de bug de decodificação. Comparando a semântica de flags e endereçamento com
veesem **e** VFrown, tudo batia. O problema estava na **cola**, não na CPU:

O segmento de dados **DS** vive no registrador `SR` da CPU e é espelhado no
registrador de I/O `0x3D2F`. Como o barramento não enxerga a CPU, eu sincronizava
o DS nos dois sentidos a cada passo. Só que isso reescrevia o DS na CPU **depois
de toda instrução** — inclusive quando a própria CPU acabara de mudar o `SR`:
a entrada de uma IRQ zera o `SR` (DS → 0) e o `reti` o restaura. Minha
sincronização sobrescrevia essas mudanças, corrompendo o DS dentro dos
*handlers* de interrupção e, com o tempo, levando a ponteiros errados e saltos
para o nada.

A correção foi cirúrgica: **só reescrever o DS na CPU quando o jogo de fato
escreve em `0x3D2F`** (um flag `ds_written`). Com isso, as instruções desconhecidas
caíram para **zero** e a tela de título surgiu em cores.

---

## Créditos e licenças

- Projeto licenciado sob **MIT**.
- Referências de comportamento (não copiadas): `veesem` (ISC, © Simon Eriksson),
  `VFrown` (MIT, © Ian), MAME (BSD-3).
- Bibliotecas de frontend: `minifb` e `cpal` (ambas MIT/Apache-2.0).
- **V.Smile**, **VTech** e os jogos são marcas de seus respectivos donos. Este é um
  projeto independente de fins educativos, sem afiliação com a VTech, e **não
  acompanha ROMs ou BIOS**.
