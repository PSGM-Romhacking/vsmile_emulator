# Documentação de desenvolvimento — vsmile

Documento de referência de **tudo que foi implementado** no emulador, além do
que já estava pronto (CPU unSP, PPU, SPU, periféricos, barramento). Para a visão
de usuário, veja o [`README.md`](README.md); este documento é o registro técnico
e de desenvolvimento.

- **Estado do backend:** V.Smile padrão **~99,5%** de boot (431/432 ROMs válidas),
  **0 crashes** em 569 cartuchos. Núcleo `std`-only, sem `unsafe`.
- **Testes:** 34 no workspace (30 `vsmile-core` + 2 `vsmile-gui` + 2 `vsmile-wasm`).
- **Crates:** `vsmile-core` (lib), `vsmile-runner` (bin headless), `vsmile-gui`
  (bin minifb+cpal), `vsmile-wasm` (cdylib WebAssembly).

---

## Índice

1. [Loader de `.zip` (DEFLATE próprio)](#1-loader-de-zip-deflate-próprio)
2. [NVRAM persistente (Art Studio)](#2-nvram-persistente-art-studio)
3. [Save states](#3-save-states)
4. [Áudio: interpolação/low-pass do SPU + reamostragem](#4-áudio-interpolaçãolow-pass-do-spu--reamostragem)
5. [Segundo controlador](#5-segundo-controlador)
6. [Desmontador unSP](#6-desmontador-unsp)
7. [Depurador (breakpoints, watchpoints, inspeção)](#7-depurador-breakpoints-watchpoints-inspeção)
8. [Visualizador da PPU](#8-visualizador-da-ppu)
9. [Port WebAssembly](#9-port-webassembly)
10. [Modo lote de compatibilidade](#10-modo-lote-de-compatibilidade)
11. [Auto-detecção de BIOS](#11-auto-detecção-de-bios)
12. [Compatibilidade medida](#12-compatibilidade-medida)
13. [Investigações e achados de RE](#13-investigações-e-achados-de-re)
14. [Limitações e holdouts conhecidos](#14-limitações-e-holdouts-conhecidos)
15. [Compilar, rodar e testar](#15-compilar-rodar-e-testar)
16. [Arquivos de referência](#16-arquivos-de-referência)

---

## 1. Loader de `.zip` (DEFLATE próprio)

**O que faz:** carrega cartuchos empacotados em `.zip` diretamente, sem precisar
extrair o `.bin` à mão. Mantém a filosofia de **zero dependências** no núcleo.

**Arquivos:** `vsmile-core/src/inflate.rs`, `vsmile-core/src/zip.rs`,
`vsmile-core/src/vsmile.rs` (`rom_from_file`).

**Como funciona:**
- `inflate.rs` — descompressor **DEFLATE** (RFC 1951) do zero: leitor de bits,
  Huffman canônico (fixo e dinâmico), blocos armazenado/fixo/dinâmico, cópia LZ77.
- `zip.rs` — lê o *End Of Central Directory* e o diretório central; suporta
  métodos `0` (stored) e `8` (deflate); `extract_rom` escolhe a maior entrada
  `.bin`.
- `rom_from_file(bytes)` — detecta a assinatura `PK\x03\x04` e extrai; senão trata
  como `.bin` little-endian. O núcleo não toca em disco (o frontend lê o arquivo).

**Uso:** `vsmile-run --cart JOGO.zip` (ou `.bin`). Também na GUI e no WASM.

**Verificação:** 3 testes (bloco armazenado, back-reference deflate, leitura de um
`.zip` fixture); validado abrindo *Batman* direto do `.zip` de 3,6 MB (inflando
para 8 MB) e renderizando a tela da DC Comics.

---

## 2. NVRAM persistente (Art Studio)

**O que faz:** persiste em arquivo a SRAM com bateria dos cartuchos **Art Studio**
(mapeada em `CSB2`), para o desenho sobreviver entre execuções.

**Arquivos:** `vsmile-core/src/vsmile.rs` (campo `nvram`, `enable_nvram`,
`nvram_to_save_bytes`), frontends.

**Como funciona:** opt-in — `CSB2` é roteado para uma SRAM gravável de 1 MiW só
quando a NVRAM é habilitada; cartucho comum fica **idêntico** (CSB2 lê a ROM,
escritas ignoradas). Um flag `dirty` evita regravar à toa; ao salvar, as palavras
`0xFFFF` finais são cortadas (arquivo pequeno).

**Uso:** `vsmile-run --cart ART.zip --nvram desenho.sav`. Na GUI há **autodeteção**
(nome com "art studio") e **autosave** a cada ~3 s.

**Verificação:** teste de round-trip (escrita em CSB2 → serializa → recarrega);
Art Studio dá boot limpo com a NVRAM ativa.

---

## 3. Save states

**O que faz:** salva/carrega o estado completo do console a qualquer momento.

**Arquivos:** `vsmile-core/src/save.rs` (trait `State`, `Reader`, macro
`impl_state!`); `impl_state!(...)` em cada periférico; `VSmile::save_state` /
`load_state`.

**Como funciona:** como o núcleo não tem referências cíclicas nem `unsafe`, o
estado é uma árvore de campos serializável. O trait `State` faz dump/restore
binário little-endian; o macro `impl_state!` gera a implementação a partir da
lista de campos, evitando dessincronização. **Não** entram no save: ROMs
(recarregadas), temporização de vídeo e campos de diagnóstico. Cabeçalho com
assinatura + versão.

**Uso:** GUI **F5** salva / **F7** carrega (`<cartucho>.state`);
`vsmile-run --save-state S.state` / `--load-state S.state`.

**Verificação:** teste de **determinismo** (rodar N quadros, salvar, rodar +M;
restaurar e rodar +M → estados idênticos) + idempotência; verificado **pixel a
pixel** no *Batman* (400 quadros diretos == salvar no 300 + restaurar + 100).

---

## 4. Áudio: interpolação/low-pass do SPU + reamostragem

**O que faz:** aplica os bits de controle do SPU que antes eram lidos mas
ignorados, e melhora a reamostragem do áudio no frontend.

**Arquivos:** `vsmile-core/src/spu.rs`, `vsmile-gui/src/audio.rs`.

**Como funciona:**
- **SPU:** o bit `no_interpolation` (9) alterna entre interpolação linear e
  segurador de ordem zero; o bit `low_pass_enable` (8) aplica um filtro passa-baixa
  de um polo na saída. Ambos **desligados por padrão** → áudio inalterado para os
  jogos que não os usam (zero regressão). Posições de bit confirmadas no header do
  veesem.
- **Frontend (GUI):** a reamostragem de ~281 kHz para a taxa da placa trocou a
  média de caixa por um **decimador anti-aliasing**: biquad Butterworth (RBJ) +
  interpolação linear.

**Verificação:** o teste de save-state (determinismo) cobre os novos campos do SPU;
2 testes do biquad (ganho DC unitário, atenuação de alta frequência); *Batman*
mantém estatísticas de áudio idênticas (sem regressão).

---

## 4.1. Auditoria e regressão de timing (ciclos)

**O que faz:** garante que o modelo de timing (ciclos por instrução + clocks dos
periféricos) está correto e protegido contra regressões.

**Arquivos:** `vsmile-core/src/cpu.rs` (testes `ciclos_por_instrucao_batem_com_veesem`,
`ciclos_de_entrada_de_irq`).

**O que foi auditado (contra o veesem, oráculo open source):**

| Parâmetro | Valor | Confere com veesem |
|---|---|---|
| ALU registrador / shift | 3 (5 se rd=PC) | ✓ |
| ALU imm6 | 2 | ✓ |
| ALU imm16 | 4 (5 se rd=PC) | ✓ |
| ALU `[imm16]` / store | 7 (8 se rd=PC) | ✓ |
| ALU `[BP+imm6]` | 6 | ✓ |
| ALU `[Rs]` | 6 (7 se rd=PC) | ✓ |
| ALU `[A6]` | 5 (6 se rd=PC) | ✓ |
| branch tomado / não-tomado | 4 / 2 | ✓ |
| goto / call | 5 / 9 | ✓ |
| mul / muls | 12 / 10·n+6 | ✓ |
| nop / break | 2 / 10 | ✓ |
| push/pop | 2·n+4 | ✓ |
| entrada de IRQ | 10 | ✓ |
| scanline PPU (NTSC/PAL) | 1716 / 1728 | ✓ |
| SPU sample/envelope/rampdown | 96 / 384 / 13 | ✓ |
| ADC / timer / watchdog | 16 · 1 / 27M÷32768 / 27M·¾ | ✓ |
| DMA | cópia instantânea (0 ciclos) | ✓ |

**Conclusão:** o timing é um **espelho exato do veesem**. Nenhum dos dois modela
*wait states* de memória externa. O passo que falta é a comparação **ciclo-exato
vs MAME** (nível acima do veesem), que exigiria um traço de referência.

**Verificação:** 2 testes de regressão travam 15 contagens de ciclo de instrução +
a entrada de IRQ.

---

## 5. Segundo controlador

**O que faz:** suporte opcional a um segundo joystick.

**Arquivos:** `vsmile-core/src/vsmile.rs` (`joy2`, `set_controller2_enabled`,
`set_input2`), `vsmile-gui/src/main.rs` (`--p2`).

**Como funciona:** um segundo `VSmileJoy` com toda a fiação (UART/CTS, RTS na
porta C bit 12, ext2-IRQ, timers) espelhada nas linhas do controlador 2.
**Desligado por padrão** → comportamento byte-a-byte idêntico a um V.Smile de um
controle só.

**Uso:** `vsmile-gui --cart JOGO.zip --p2` (P2 = `I/J/K/L` direcional, `1..8`
botões).

**Verificação:** teste que liga o 2º controle, confirma que o estado diverge do
caso desligado (joy2 vivo) e que o estado do joy2 entra no save; *Batman* com P2
desligado mantém saída idêntica (zero regressão). **Ressalva:** nenhum emulador de
referência implementa o 2º controle, então a corretude foi verificada por simetria
com o 1º, não contra um oráculo.

---

## 6. Desmontador unSP

**O que faz:** decodifica palavras de instrução unSP em mnemônicos legíveis.

**Arquivos:** `vsmile-core/src/disasm.rs`; `vsmile-run --disasm`.

**Como funciona:** espelha exatamente o decodor de `cpu.rs` (todos os modos de
endereçamento, grupo F, desvios condicionais, push/pop/reti). Sintaxe-expressão da
SunPlus (`Rd = Rd + [BP+3]`, `[0x1234] = R2`, `JMP $-1`).

**Uso:** `vsmile-run --cart JOGO.zip --disasm 0xC9E8 --disasm-count 24`.

**Verificação:** 4 testes (instruções conhecidas, desvios, call/goto, modos de
endereçamento); aplicado ao boot do *Batman* produz código coerente (`GOTO`,
`CALL`, laço de cópia pra RAM, escrita do DS em `[0x3D2F]`).

---

## 7. Depurador (breakpoints, watchpoints, inspeção)

**O que faz:** depurador do núcleo instrução a instrução.

**Arquivos:** `vsmile-core/src/debugger.rs`; REPL em `vsmile-run --debugger`;
anel de divergência em `cpu.rs` (`dbg_pre_unknown`).

**Como funciona:**
- **Breakpoints** de execução (`CS:PC`) — param antes de executar a instrução.
- **Watchpoints de RAM** — disparam quando uma palavra muda (leitura sem efeitos
  colaterais).
- **step / continue** com motivo de parada; **inspeção** de registradores, RAM e
  desmontagem ao vivo.
- **Anel de divergência:** a CPU guarda as **16 últimas instruções** antes da 1ª
  instrução desconhecida — mostra o fluxo que leva a qualquer execução de lixo.

**Uso (REPL):** `b`/`db` (breakpoint), `w`/`dw` (watchpoint), `s`/`c`/`f`
(passo/continue/quadro), `r` (registradores), `m` (dump RAM), `d` (desmonta).
Funciona por pipe: `printf 'b 0xADF5\nc\nr\nq\n' | vsmile-run --cart ... --debugger`.

**Verificação:** 5 testes (breakpoint para antes de executar, watchpoint na
escrita, continue/limite, step, desmontagem); validado no *Batman* (breakpoint no
alvo do `GOTO` → `INT OFF` → chute do watchdog `0x55AA` em `0x3D24`; watchpoint
flagra o teste de RAM do boot com padrão `55AA AA55 5A5A A5A5`).

---

## 8. Visualizador da PPU

**O que faz:** inspeção gráfica do estado da PPU.

**Arquivos:** `vsmile-core/src/ppu_view.rs`; acessores em `ppu.rs`;
`vsmile-run --ppu-info/--ppu-palette/--ppu-tiles`.

**Como funciona:** acessores read-only para paleta, tabela de sprites e config dos
BGs; funções puras que geram imagens RGB555 (grade de paleta 16×16; *tile sheet*
que decodifica tiles de um segmento). A decodificação de tile é a mesma da PPU
(byte-swap + mapeamento de paleta).

**Uso:** `vsmile-run --cart JOGO.zip --frames 600 --ppu-info --ppu-palette p.bmp
--ppu-tiles t.bmp`.

**Verificação:** 2 testes (dimensão/cor da imagem de paleta; decodificação de tile
pixel-a-pixel contra o cenário conhecido); no *Batman* a paleta sai com as 26 cores
do logo da DC e o BG0 como `4bpp, tiles 16×16`.

---

## 9. Port WebAssembly

**O que faz:** roda o núcleo no navegador — **sem alterar uma linha do core**
(recompensa do `std`-only + zero `unsafe`).

**Arquivos:** `vsmile-wasm/` (cdylib, ABI em C sem `wasm-bindgen`), `web/`
(frontend `<canvas>` + Web Audio).

**Como funciona:** `vsmile-wasm/src/lib.rs` expõe `vsmile_alloc`/`new`/`run_frame`/
`rgba_ptr`/`audio_ptr`/`set_input`/… O `unsafe` fica só na ponte de ponteiros do
ABI, nunca no emulador. O `.wasm` tem ~105 KB.

**Uso:** `build-web.bat` (ou `cargo build --release --target
wasm32-unknown-unknown -p vsmile-wasm`), depois sirva `web/` por HTTP.

**Verificação:** 2 testes nativos do ABI (ciclo de vida + quadro/áudio; ROM
inválida → ponteiro nulo); compila para `wasm32-unknown-unknown` sem
erros/warnings; exports confirmados no `.wasm`. O loop JS do navegador precisa de
um browser para validar de ponta a ponta (documentado em `web/README.md`).

---

## 10. Modo lote de compatibilidade

**O que faz:** roda uma pasta inteira de ROMs, isola panics e classifica cada jogo.

**Arquivos:** `vsmile-runner/src/main.rs` (`run_batch`, `run_one_rom`).

**Como funciona:** varre `.zip`/`.bin`, roda N quadros por ROM dentro de
`catch_unwind` (uma ROM problemática não derruba o lote), classifica em
boot-limpo / tela-preta / opcode-desconhecido / erro-carregamento / panic. **Ignora
automaticamente** *bad dumps* (`[b]`) e arquivos `[BIOS]`. Dá taxa de boot por
variante (padrão/Motion/Baby) e histograma de opcodes desconhecidos.

**Uso:**
```bash
vsmile-run --batch "VTech - V.Smile" --batch-frames 240 \
    --batch-list compat.csv [--batch-sysrom BIOS.zip]
```

**Verificação:** rodado sobre 569 ROMs reais (ver seção 12); 0 panics, 0 erros de
carregamento.

---

## 11. Auto-detecção de BIOS

**O que faz:** usa a BIOS do console automaticamente quando ela está na pasta,
**sem obrigar** (cai na dummy se não houver).

**Arquivos:** `vsmile-runner/src/main.rs` (`find_bios`, `load_sysrom`),
`vsmile-gui/src/main.rs` (`find_bios`).

**Como funciona:** se `--sysrom` não for passado, procura `[BIOS] VTech V.Smile
… .zip` **na pasta do cartucho** (ignora Baby/Motion, prefere a versão mais nova:
v103 > v102 > v100) e a usa. Sem BIOS na pasta → `dummy_sysrom` (o jogo dá boot
pelo cartucho).

**Por quê:** alguns jogos chamam rotinas da BIOS como biblioteca de runtime (ex.:
*Little Red Riding Hood* lê a sysrom **~987 mil vezes**). Com a dummy travam; com a
BIOS real bootam. O console real tem a BIOS embutida — isso é fidelidade, não
limitação. O projeto **não distribui** a BIOS; usa a do usuário.

**Uso:** transparente. `vsmile-run --cart "…/Little Red Riding Hood….zip"` (sem
`--sysrom`) → auto-acha a BIOS e chega ao menu.

**Verificação:** LRRH chega ao **menu completo** (`Learning Adventure / Learning
Zone / Options`, 168 cores) sem `--sysrom`; Batman (pasta sem BIOS) cai na dummy e
funciona (fallback sem regressão).

---

## 12. Compatibilidade medida

Acervo de **569 cartuchos** (`VTech - V.Smile/`), 240 quadros por ROM, dummy
sysrom. *Bad dumps* (`[b]`, 3) e arquivos `[BIOS]` (5) excluídos → **561 jogos**.

| Resultado | Qtd |
|---|---|
| Boot limpo (desenha algo) | 524 |
| Tela preta | 35 (30 Baby, 3 Motion, 2 padrão) |
| Opcode desconhecido | 7 (6 Motion, 1 padrão) |
| Erro de carregamento | 0 |
| **Panic / crash** | **0** |

**Boot limpo por variante (sem bad dumps):**

| Variante | Boot | Observação |
|---|---|---|
| **V.Smile padrão** | **428/431 (99,3%)** | 99,5% contando LRRH, que boota via BIOS |
| V.Smile Motion | 96/105 (91,4%) | bootam com joystick; 9 divergem (bug genérico) |
| V.Smile Baby | 0/30 (0%) | console diferente, sem oráculo |

Rodar com **900 quadros** dá resultado idêntico ao de 240 → as telas pretas estão
**travadas, não lentas**. Relatórios: `COMPATIBILIDADE.txt` (resumo) e
`COMPATIBILIDADE_completa.csv` (status de cada ROM).

---

## 13. Investigações e achados de RE

Feitas com o depurador/anel de divergência do próprio projeto:

- **Little Red Riding Hood** — *dependência de BIOS*. Lê a sysrom ~987 mil vezes
  (usa a BIOS como biblioteca). Resolvido pela auto-detecção de BIOS.
- **Toy Story 2 (padrão)** — executa limpo (sem opcode inválido) e progride por
  uma inicialização longa (rampas de `wave_in`, espera de canais do SPU — que o
  depurador confirma que **passa**), mas não liga o vídeo. Causa raiz adiante, não
  isolada; provável sensibilidade a timing.
- **Zayzoo (padrão)** — `CALL 0x045104` em `0x03FDC7`, e `0x045104` é **padding
  `0xFFFF` genuíno** no ROM (4 MiW carregados corretos — não é bug de loader). O
  hardware real não chega a esse `CALL`; divergência vem de antes.
- **V.Smile Motion** — *não é problema de acelerômetro.* Nenhum emulador (nem o
  MAME) emula o acelerômetro; o software Motion funciona com o joystick padrão
  (96/105 bootam). As 9 falhas divergem **antes de qualquer comunicação com o
  controle** (0 comandos enviados) → bugs genéricos de divergência, com mecanismos
  distintos (Shrek: `POP {SR,PC}` retorna a endereço nulo `0x000000`; Zayzoo: `CALL`
  pra padding). Implementar o acelerômetro não ajudaria.

---

## 14. Limitações e holdouts conhecidos

- **V.Smile padrão:** 2 holdouts reais (*Toy Story 2*, *Zayzoo*) — divergências/
  travamentos upstream, RE por jogo de retorno incerto.
- **V.Smile Baby** (30 títulos) e **V.Smile Motion** (controle de movimento) — são
  **consoles/periféricos diferentes sem referência** em nenhum emulador open source.
  Baby não dá boot; Motion boota com joystick mas 9 títulos divergem.
- **Precisão vs MAME não auditada ciclo a ciclo** — timing da CPU é aproximado
  (relacionado ao caso Toy Story 2).
- **BIOS não acompanha o projeto** — auto-detectada da pasta quando presente.
- **WASM:** loop JS do navegador não validado headless (precisa de browser).

---

## 15. Compilar, rodar e testar

```bash
cargo test                    # 34 testes (core + gui + wasm nativo)
cargo build --release

# Jogar (GUI): auto-detecta BIOS e NVRAM; F5/F7 = save state
cargo run --release --bin vsmile-gui -- --cart JOGO.zip [--p2]

# Headless → screenshot
cargo run --release --bin vsmile-run -- --cart JOGO.zip --frames 600 --out tela.bmp

# Ferramentas: --disasm, --debugger, --ppu-info/palette/tiles,
#              --batch, --batch-list, --batch-sysrom

# WebAssembly
build-web.bat   # ou cargo build --release --target wasm32-unknown-unknown -p vsmile-wasm
```

Toolchain atual da máquina: `stable-x86_64-pc-windows-msvc`; alvo
`wasm32-unknown-unknown` instalado para o WASM.

---

## 16. Arquivos de referência

| Arquivo | Conteúdo |
|---|---|
| `README.md` | Documentação de usuário (arquitetura, uso, compatibilidade) |
| `DOCUMENTACAO.md` | Este documento (registro técnico/dev) |
| `COMPATIBILIDADE.txt` | Resumo do último lote de compatibilidade |
| `COMPATIBILIDADE_completa.csv` | Status de cada uma das 569 ROMs |
| `retomar.bat` | Reabre a sessão do Claude Code neste projeto |
| `build-web.bat` | Compila o WASM e copia para `web/` |
| `web/` | Frontend de navegador (canvas + Web Audio) |
