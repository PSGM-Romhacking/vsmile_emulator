# Exemplo de ficha preenchida: Toy Console

(Sistema inventado que vive em `crates/toy-core`; serve de modelo de preenchimento. Formato: `SISTEMA.md`.)

## 1. Identidade
Nome do emulador: Toy · slug: `toy` · sistema: Toy Console · nome curto: Toy · versão inicial: `0.1.0` · ícone: nenhum · repositório: nenhum · jogos gratuitos: nenhum (qualquer arquivo serve).

## 2. O núcleo
Tipo: crate Rust (`crates/toy-core`) · licença: GPL-3.0-or-later · BIOS: não · conteúdo: arquivo único, ignorado pelo núcleo · validado contra referência: não (sistema inventado; testes unitários).

## 3. Conteúdo
Extensões: `toy`, `bin` · tamanho: qualquer · banco de jogos: nenhum.

## 4. Vídeo
160 × 120 · aspecto 4:3 (1,333) · resolução fixa · pixel já em XRGB8888 · 60/1 fps · sem modelos.

## 5. Áudio
Taxa: a que o frontend pede (gera onda quadrada de 440 Hz no próprio ritmo) · saída estéreo · sem canais mutáveis.

## 6. Controles
1 jogador · botões: Up, Down, Left, Right, "Button 1" (B), "Button 2" (A), Start · sem periféricos extras.

## 7. Memória
16 bytes de RAM (posição, contador de quadros, fase do tom), `ram_address` 0 · sem outras RAMs nem bateria.

## 8. Capas e interface
Pasta de capas: "Example - Toy" · caixa retrato (0,66) · sem moldura · uma opção: Background (gradient/solid) · só idiomas en/pt/es.

## 9. Saída esperada
App desktop e core libretro (`tools/new-emulator.sh --libretro toy "Toy Console" "Example - Toy"`).

## 10. Observações
Todo o estado vive na RAM, então save state, cheats e debugger funcionam sem código extra.
