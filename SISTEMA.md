# Ficha do sistema

Preenchida em 2026-11-02 a partir do núcleo `PericlesSavio/_vsmile` (commit 7620b41). Modelo original: preencha esta ficha no clone, antes de pedir à IA para montar o emulador. A IA lê este arquivo
primeiro (ver `CLAUDE.md`) e transforma cada campo em código (`SystemInfo`, `core.rs`, app).
**Campo que você não sabe: escreva `?`.** A IA pergunta ou confere no hardware/núcleo, e não
inventa nem copia valor de outro console. Exemplo preenchido: `docs/exemplo-SISTEMA-toy.md`.

## 1. Identidade
| Campo | Valor | Vira |
|---|---|---|
| Nome do emulador | V.Smile Emulator (provisório e neutro: o dono vai dar o nome) | `brand::Brand.name`, título da janela |
| Slug (minúsculas, dígitos e `-`) | `vsmile` | `vsmile-core`, `vsmile` (app), `vsmile-libretro`; pasta de config `vsmile` |
| Nome do sistema | VTech V.Smile | `SystemInfo.name` |
| Nome curto | V.Smile | `SystemInfo.short_name` |
| Versão inicial | `0.1.0` | `[workspace.package] version` |
| Ícone (PNG 256) | ? (ainda não há) | `Brand.icon_png` |
| Repositório | https://github.com/PSGM-Romhacking/vsmile_emulator | `Brand.repo_url` |
| Onde conseguir jogos legais/gratuitos (rótulo + URL) | ? (não se conhece homebrew) | `Brand.free_games` |

## 2. O núcleo
| Campo | Valor |
|---|---|
| Tipo | crate Rust próprio (`crates/vsmile-core`), só `std`, sem `unsafe` |
| Caminho ou origem (pasta, repo, arquivo) | `git@github.com:PericlesSavio/_vsmile.git`, commit 7620b41 (`crates/vsmile-core/src`); o runner (`vsmile-runner`) e o binding WebAssembly (`vsmile-wasm` + `web/`) vieram juntos; o frontend minifb (`vsmile-gui`) foi substituído pela interface da base |
| Licença do núcleo | era MIT (projeto do próprio dono); compatível com GPL-3.0, agora sob a GPL-3.0-or-later do workspace. Referências de comportamento, sem código copiado: veesem (ISC), VFrown (MIT), MAME (BSD-3-Clause) |
| É só o `.so` pronto? | não |
| Precisa de BIOS ou arquivos de sistema? Quais? | opcional: `vsmile_v100.bin` (205c5296), `vsmile_v102.bin` (0cd0bdf5), `vsmile_v103.bin` (387fbc24), 2 MiB cada; usa a mais nova encontrada. Sem BIOS, uma sysrom vazia: os jogos dão boot pelo cartucho, mas alguns chamam rotinas da BIOS (ex.: Little Red Riding Hood) |
| O que o conteúdo é: arquivo único, vários arquivos, disco? | cartucho, um arquivo (palavras de 16 bits little-endian) |
| Já foi validado contra alguma referência (outro emulador, FPGA)? | não quadro a quadro. Boot em lote no repo original: 524 de 566 ROMs desenham algo em 240 quadros (padrão 99,3%, Motion 91,4%, Baby 0%), ver `docs/core/COMPATIBILIDADE.txt` |

## 3. Conteúdo (ROM)
| Campo | Valor | Vira |
|---|---|---|
| Extensões aceitas (sem ponto) | `bin` (o `.zip`/`.7z` é aberto pela interface) | `SystemInfo.extensions` |
| Tamanho mínimo/máximo da ROM | até 4 MiW (8 MiB); o resto é preenchido com `$FFFF` | `VSmileIo::new` |
| Banco de jogos (DAT, No-Intro...) | 455 jogos, CRC32 do `.bin` e nome No-Intro, do `VTech - V.Smile.rdb` da libretro-database (`assets/games/vsmile.tsv`) | `crates/vsmile-core/src/games.rs` |

## 4. Vídeo
| Campo | Valor | Vira |
|---|---|---|
| Largura × altura visíveis (px) | 320 × 240 | `width`, `height` |
| Aspecto na tela | 4:3 | `aspect` |
| A resolução muda durante o jogo? | não | — |
| Formato do pixel do núcleo | RGB555, convertido para XRGB8888 em `core.rs` | `refresh` |
| FPS exato | NTSC: 27 000 000 / (1716 × 262) ≈ 60,054 | `fps_num` / `fps_den` |
| Existem modelos/regiões com FPS diferente? | sim: PAL, 1728 × 312 ciclos (≈ 50,08). O núcleo suporta, a interface ainda não oferece (fixo em NTSC, código de região `$E`) | futuro `Core::fps()` + `options()` |

## 5. Áudio
| Campo | Valor | Vira |
|---|---|---|
| Taxa nativa (Hz) | 27 MHz / 96 = 281 250 | passa-baixa Butterworth + interpolação linear em `core.rs` |
| Canais (mono/estéreo) | estéreo, `u16` com `$8000` = silêncio | — |
| Canais que valem mutar no debugger | os 16 do SPU (ainda não ligados) | `sound_channels` |

## 6. Controles
| Campo | Valor | Vira |
|---|---|---|
| Número de jogadores | 2 (o 2º controle fica plugado quando a interface manda dois pads) | `players` |
| Botões reais do controle | joystick, Enter, Exit, Help, ABC, vermelho, amarelo, azul, verde | `buttons` |
| Qual botão do RetroPad cada um usa | Start = Enter, Y/X/B/A = vermelho/amarelo/azul/verde, L = Exit, Select = Help, R = ABC (escolha provisória) | `joy_input` em `core.rs` |
| Periféricos fora do RetroPad | botões ON/OFF/Restart do console (o núcleo tem; não ligados), V.Smile Motion (sensor), teclado do V.Smile Pocket/PC Pal: não | — |

## 7. Memória
| Campo | Valor | Vira |
|---|---|---|
| RAM principal (tamanho) | 0x2800 palavras (20 KiB), exposta como bytes little-endian | `system_ram()` |
| Endereço da CPU do primeiro byte da RAM | `$0000` (endereço de palavra) | `ram_address` |
| Tem save de bateria? | só os 5 cartuchos Art Studio: SRAM em CSB2, janela de 1 MiW (2 MiB), reconhecidos por CRC32 | `battery_ram()` / `load_battery()` |
| Tem mais de uma RAM exposta? | não | — |

## 8. Capas e interface
| Campo | Valor | Vira |
|---|---|---|
| Pasta no libretro-thumbnails | `VTech - V.Smile` | `thumbnails` |
| Formato da caixa | retrato (~0,66) (não conferido) | `cover_aspect` |
| Moldura (bezel) de TV ou console? | não | `Brand.bezels` |
| Opções do núcleo | nenhuma ainda (candidatas: região/PAL, logo VTech) | — |
| Idiomas além de en, pt, es | não | — |

## 9. Saída esperada
- [x] App desktop (`cargo run -p vsmile`)
- [x] Core libretro (`crates/vsmile-libretro`)
- [ ] Pacotes de release (AppImage, `.exe`)
- [x] Outro: runner sem janela (`vsmile-run`, do repo original) e versão WebAssembly (`web/`)

## 10. Observações
- Não testado com jogos reais neste repositório (0.1.0): só testes unitários e um teste de fumaça pelo contrato.
- A RAM e a NVRAM são palavras de 16 bits; `core.rs` mantém cópias em bytes e as devolve à máquina antes do quadro seguinte quando a interface (cheats, debugger, RetroArch) escreve nelas.
- O `.sav` do Art Studio tem a janela inteira (2 MiB); o repo original cortava as palavras `$FFFF` do fim.
- Histórico e notas técnicas do núcleo (em português): `docs/core/`.
