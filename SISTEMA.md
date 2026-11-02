# Ficha do sistema

Preencha esta ficha no clone, antes de pedir à IA para montar o emulador. A IA lê este arquivo
primeiro (ver `CLAUDE.md`) e transforma cada campo em código (`SystemInfo`, `core.rs`, app).
**Campo que você não sabe: escreva `?`.** A IA pergunta ou confere no hardware/núcleo, e não
inventa nem copia valor de outro console. Exemplo preenchido: `docs/exemplo-SISTEMA-toy.md`.

## 1. Identidade
| Campo | Valor | Vira |
|---|---|---|
| Nome do emulador | | `brand::Brand.name`, título da janela |
| Slug (minúsculas, dígitos e `-`) | | nome dos crates (`<slug>-core`, app), pasta de config |
| Nome do sistema | | `SystemInfo.name` |
| Nome curto | | `SystemInfo.short_name` |
| Versão inicial | `0.1.0` | `[workspace.package] version` |
| Ícone (PNG 256) | | `Brand.icon_png` |
| Repositório | | `Brand.repo_url` |
| Onde conseguir jogos legais/gratuitos (rótulo + URL) | | `Brand.free_games` (tela de boas-vindas) |

## 2. O núcleo
| Campo | Valor |
|---|---|
| Tipo | `crate Rust` / `C/C++ (FFI)` / `core libretro (.so/.dll)` / outro |
| Caminho ou origem (pasta, repo, arquivo) | |
| Licença do núcleo (a IA para e avisa se for `?`, não comercial, proprietária ou incompatível com GPL-3.0) | |
| É só o `.so` pronto? (então vale tentar o `libretro_player` antes de adaptar) | sim / não |
| Precisa de BIOS ou arquivos de sistema? Quais? (nome, tamanho, CRC32, obrigatório ou não; vira `SystemInfo::bios` + `Core::set_bios`) | |
| O que o conteúdo é: arquivo único, vários arquivos (um por chip: `SystemInfo::multi_file_roms`), disco (não suportado: avisar)? | |
| Já foi validado contra alguma referência (outro emulador, FPGA)? | |

## 3. Conteúdo (ROM)
| Campo | Valor | Vira |
|---|---|---|
| Extensões aceitas (sem ponto) | | `SystemInfo.extensions` |
| Tamanho mínimo/máximo da ROM | | validação em `Core::load` |
| Banco de jogos (DAT, No-Intro...) | | `game_info`, identificação por CRC32 |

## 4. Vídeo
| Campo | Valor | Vira |
|---|---|---|
| Largura × altura visíveis (px) | | `width`, `height` |
| Aspecto na tela (largura/altura, não o dos pixels) | | `aspect` |
| A resolução muda durante o jogo? | | (hoje não suportado pelo contrato: avisar) |
| Formato do pixel do núcleo (paleta, RGB565...) | | conversão para XRGB8888 em `core.rs` |
| FPS exato (fração, ex.: clock ÷ ciclos por quadro) | | `fps_num` / `fps_den` |
| Existem modelos/regiões com FPS diferente? | | `Core::fps()` + `options()` |

## 5. Áudio
| Campo | Valor | Vira |
|---|---|---|
| Taxa nativa (Hz) | | reamostragem em `take_audio` |
| Canais (mono/estéreo) | | saída sempre estéreo `f32` |
| Canais que valem mutar no debugger | | `sound_channels` |

## 6. Controles
| Campo | Valor | Vira |
|---|---|---|
| Número de jogadores | | `players` |
| Botões reais do controle (e o nome de cada um no sistema) | | `buttons` (só os que existem) |
| Qual botão do RetroPad cada um usa (B = botão de baixo, A = direita...) | | mapeamento em `run_frame` |
| Periféricos fora do RetroPad (teclado, mouse, light gun, paddle, analógico) | | (não suportado ainda: avisar) |

## 7. Memória
| Campo | Valor | Vira |
|---|---|---|
| RAM principal (tamanho) | | `system_ram()` / `system_ram_mut()` |
| Endereço da CPU do primeiro byte da RAM | | `ram_address` (debugger e cheats) |
| Tem save de bateria (SRAM do cartucho)? Tamanho? | | `Core::battery_ram()` / `load_battery()` (a interface grava `.sav`) |
| Tem mais de uma RAM exposta (debugger, cheats)? | | (não suportado ainda: avisar) |

## 8. Capas e interface
| Campo | Valor | Vira |
|---|---|---|
| Pasta no libretro-thumbnails ("Fabricante - Sistema") | | `thumbnails` |
| Formato da caixa: retrato (~0,66) / quadrada / larga (~1,3) | | `cover_aspect` |
| Moldura (bezel) de TV ou console? | | `Brand.bezels` |
| Opções do núcleo (modelo, região, imagem...) | | `Core::options()` / `set_option()` |
| Idiomas além de en, pt, es | | `i18n::add_catalog` |

## 9. Saída esperada
- [ ] App desktop (`cargo run -p <slug>`)
- [ ] Core libretro (`--libretro` no gerador)
- [ ] Pacotes de release (AppImage, `.exe`)
- [ ] Outro: 

## 10. Observações
Particularidades do sistema, armadilhas conhecidas, links de documentação do hardware, o que
não deve ser alterado no núcleo.
