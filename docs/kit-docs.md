# Kit de documentação (para cada emulador novo)

Documento interno. Os docs de um emulador seguem a estrutura do **Mil Capivaras** depois da revisão de 21/10/2026: um **README curto** (o que é, baixar, controles, compilar, índice, licenças) e o resto em `docs/`, um documento por assunto, tudo em três idiomas. Este kit transforma essa estrutura em modelos reaproveitáveis em `docs/kit/`, para cada emulador novo ter a mesma documentação sem recomeçar do zero.

## Por que essa estrutura

O README do Mil Capivaras chegou a 1.900 linhas (117 KB) juntando manual, referência de hardware, diário de laboratório e artigo técnico: quem só queria jogar passava por 870 linhas antes de achar "Baixar". Numa avaliação externa, a documentação levou **7/10** por isso, por contradições entre seções e pelo tom de diário. Depois da divisão, da limpeza e da referência de validação publicada, **9/10**. As regras abaixo são o que fez a diferença.

## Mapa: Mil Capivaras → kit

| Mil Capivaras | Modelo do kit | O que é igual / o que muda |
|---|---|---|
| `README.md` (124 linhas) | `README.model.md` | Igual na estrutura: resumo, tabela de números, baixar, controles, compilar, índice, licenças |
| `docs/manual.md` | `manual.model.md` | Quase todo escrito, porque a interface é a mesma; ajustar detalhes do sistema |
| `docs/hardware.md` | `hardware.model.md` | Só o esboço: o console, o hardware em detalhe, como as implementações diferem |
| `docs/compatibility.md` | `compatibility.model.md` | Só o esboço: como foi testado e uma tabela por grupo de jogos |
| `docs/accuracy.md` | `accuracy.model.md` | Só o esboço: como a comparação é feita, resultado, classificação, situação |
| `docs/architecture.md` | `architecture.model.md` | Estrutura, contrato `emu-api` e frontend iguais; o núcleo, o funcionamento e as decisões são a parte a escrever |
| `docs/libretro.md` | `libretro.model.md` | O core libretro é **gerado**, então o texto é quase todo igual; muda opções, `.info`, RetroAchievements. Para enviar o core ao RetroArch: [enviar-core-libretro.md](enviar-core-libretro.md) |
| `docs/translating.md` | `translating.model.md` | Igual para todos; só nome, pasta de config, extensão e URL |
| `docs/sheets.md` | `sheets.model.md` | Parte A igual; parte B (`games.json`) é do núcleo. Apague se não há banco de jogos |
| `docs/lessons.md` | `lessons.model.md` | Só o esboço |
| `docs/references.md` | `references.model.md` | Só o esboço: as fontes numeradas citadas nos outros docs |
| `reference/README.md` + `compare.sh` | `reference/README.model.md` + `reference/compare.sh` | Só com `--reference`; ver "Validação com uma referência" |
| `CHANGELOG.md` | `CHANGELOG.model.md` | Mesmo formato (as notas de cada release saem da seção da versão) |

## Como usar

No clone do emulador (depois do `init-project.sh`, com o `new-emulator.sh` já rodado):

```
tools/new-docs.sh --name "Mil Nintendos" --system "Nintendo Entertainment System" \
  --slug nes --ext nes --repo https://github.com/Org/mil_nintendos --force [--reference [--verilator]]
```

- `--reference` escreve `reference/README.md` e `reference/compare.sh`; `--verilator` copia também o exemplo do V-Bear (`sim_top.sv`, `sim.h`, `ref_main.cpp`, `Makefile`, `compare.py`) para você adaptar.
- Preenche os `{{PLACEHOLDERS}}`, tira o comentário de cabeçalho do kit e escreve `README.md`, `CHANGELOG.md` e `docs/*.md`. `--no-game-db` pula o `sheets.md`; `--reference` escreve também `reference/README.md` e `reference/compare.sh`.
- `--force` é necessário porque o `init-project.sh` já criou um `README.md` e um `CHANGELOG.md` provisórios. Sem ele, o script pula o que já existe.
- O que depende do sistema fica como `<!-- TODO(system): ... -->`, com o roteiro do que escrever dentro. `TODO(translate)` marca a linha de troca de idioma.
- Lista do que falta: `grep -rn 'TODO(system)\|TODO(translate)' README.md CHANGELOG.md docs/ reference/`.
- No fim, e depois de qualquer divisão ou renomeação: `tools/check-links.py` (confere arquivos e âncoras de todos os links).

**Ordem sugerida:** escreva enquanto constrói, não no fim. `hardware.md` antes do núcleo, as decisões (`architecture.md`) à medida que decide, `accuracy.md` à medida que valida, `compatibility.md` quando tiver a lista de jogos testados.

## Regras de escrita (aprendidas no Mil Capivaras)

1. **Descreva o estado atual, não o caminho.** Nada de "testado em 2026-10-17", "(antes: …)", "a explicação antiga estava errada", "plano aprovado", notas parciais ("modelo 9,5") ou listas de passos já feitos. O que mudou e quando vai no `CHANGELOG.md`; o que se aprendeu vai em `docs/lessons.md`. Um doc escrito como plano (ex.: "Recomendado: usar bindgen") é reescrito como descrição ("os tipos estão escritos à mão em `lib.rs`") quando o trabalho termina, conferindo no código.
2. **Um fato em um lugar.** Quando o mesmo número aparece em dois docs, um dia eles divergem (o Mil Capivaras chegou a dizer 140 e 152 comparações para a mesma regressão). Escreva o número uma vez e ligue para ele dos outros lugares. Contagens que mudam (jogos, comparações) só onde são medidas.
3. **Confira as afirmações sobre terceiros na fonte.** O README do Mil Capivaras dizia que o MAME usava a porta FE como endereço do PCG e punha a borda dos lados: o código do MAME mostrou que ele ignora a FE e põe a borda em cima e embaixo desde a 0.268. Antes de afirmar o que outro emulador faz, leia o código dele; cite a versão.
4. **Nada de "seção N" em links.** Numeração quebra ao dividir ou reordenar. O texto do link é o nome do destino: `[Compatibilidade](compatibility.md)`, `[as medições do Par](accuracy.md#...)`.
5. **Toda afirmação tem fonte ou medição.** Fontes numeradas ([W1], [M2], [N1]…) listadas em `docs/references.md`; cada doc que cita tem a linha "As referências entre colchetes … estão em Referências". Número sem fonte vira `?` e aviso ao dono.
6. **Ressalvas honestas ficam visíveis.** "Idêntico" diz a quê ("ao core do MiSTer simulado, no quadro 3000"), não "ao console". O que é aproximado ou não verificado vai para "O que falta".
7. **Comandos, opções e arquivos citados existem.** O app gerado só tem `--scale`; `--shot`, `--press-start`, `--dump-ram`, `--bench` e os outros só entram no doc se o `main.rs` do emulador os tiver.
8. **Datas só onde são o assunto.** Datas de publicação de homebrews de terceiros e "atualizado em" envelhecem e se contradizem (ROM atualizada depois da versão que a cita). O CHANGELOG tem as datas das versões; o resto não precisa.

## Idiomas

Inglês é o texto-fonte; `X.pt-BR.md` e `X.es.md` ao lado, e no topo de cada arquivo a linha `**English** · [Português](X.pt-BR.md) · [Español](X.es.md)`. Os modelos vêm só em inglês: depois que a versão em inglês estiver pronta, peça à IA para traduzir. Ao mudar um doc, mude os três no mesmo commit: não há checagem automática de que eles batem, e é por aí que as contradições voltam. Na tradução de um doc que foi dividido ou reorganizado, **mova o texto já traduzido** em vez de retraduzir.

## Validação com uma referência

O que levou o "uso do vibe coding" do Mil Capivaras de 8,5 para 9: a prova de que o emulador está certo **não depende da IA nem da máquina do autor**. A IA escreve o código; quem decide se está certo é uma comparação com uma referência independente, publicada no repositório.

- **A referência** é outra implementação confiável do mesmo hardware: o core FPGA simulado (Verilator), o núcleo original, outro emulador de precisão conhecida. Ela fica em `reference/`, fora do `cargo build`, com o código, o Makefile e as licenças.
- **A mesma entrada nos dois:** o app e a referência aceitam `JOGO --shot N saida.ppm --press-start --dump-ram ram.bin`, com o mesmo "jogador automático" (no Mil Capivaras: Start nos quadros 120–129, depois Fire 1 + direita alternando a cada 20 quadros a partir do 200). Implemente essas opções no `main.rs` do app desde o começo (modelo: `~/Projetos/mil_capivaras/crates/app/src/main.rs`).
- **`reference/compare.sh`** (do kit) roda os dois em cada ROM e imprime pixels e bytes diferentes por quadro; `REF` aponta para o binário da referência, `FRAMES` escolhe os quadros. Rode antes de cada versão.
- **Guarde as ferramentas de diagnóstico no repositório.** O rastreio instrução a instrução e a captura da RAM no fim da instrução, que explicaram as últimas diferenças do Mil Capivaras, ficaram fora do repositório e se perderam; só uma versão anterior da referência foi recuperada de um backup.
- **Confira a versão das ROMs antes de culpar o emulador.** Na revalidação, a ROM de teste dos controles e o Demônios divergiam: eram versões antigas na pasta de ROMs. O banco de jogos (CRC32) diz qual versão é a certa; o `compatibility.md` diz de onde baixar.
- **Licença:** se a referência é compilada junto com código GPL-2.0 sem "ou posterior" (cores do MiSTer costumam ser), o código próprio da pasta `reference/` fica GPL-2.0 ou posterior, não GPL-3.0 como o emulador. A referência é um programa à parte, não ligado ao emulador.
- **Diferenças explicadas não são erro, mas precisam estar escritas.** Ex.: "restos de pilha" (1 ou 2 bytes) em quadros intermediários, por gravar a RAM no meio de uma instrução que atravessa a virada do quadro.
- **Quem é a referência importa.** Um core FPGA ou outro emulador é uma **reconstrução**, não o console (o do MiSTer do Super A'Can foi feito a partir de vídeos do console e do estudo dos jogos, nunca comparado com um console de verdade). "Idêntico à referência" quer dizer que duas reconstruções concordam: é evidência, não prova. Escreva isso no `accuracy.md` ("Limits of the reference"), junto com a idade do repositório e o que a referência não cobre.
- **Uma discordância não diz quem errou.** Pode ser o emulador, a referência ou o modo de rodá-la. Use uma **terceira fonte** (outro emulador, uma medição) para decidir. No V-Bear, a janela 0 desenhada uma linha abaixo só foi decidida porque o MAME concordou com a simulação.
- **Os dois nunca ligam no mesmo ritmo.** Se a simulação lê o cartucho com um atraso fixo, cada jogo chega à mesma tela com 0 a vários quadros de diferença. Compare o quadro N da referência com uma **janela** de quadros do emulador e fique com o melhor (menos pixels diferentes; empate, menos bytes de RAM). Imprima o deslocamento. Em imagem parada o deslocamento não é confiável: a RAM é que decide. O exemplo está em `docs/kit/reference/verilator/compare.py` (`--offset`, `--window`, `--cache`).
- **Normalize o tamanho da imagem antes de comparar.** O emulador e a simulação podem ter tamanhos e modos diferentes (linhas esticadas, abertura centrada). Aplique a mesma conversão do emulador à imagem da simulação e compare cores exatas.
- **ROM em vários arquivos:** junte as partes antes de rodar. O V-Bear comparou só a primeira parte de um jogo de duas ROMs e a RAM "diferente" não significava nada, até se perceber.
- **Verilator:** não use `--public-flat-rw` num core grande (o GCC passa de 25 minutos num único arquivo); deixe público só o que precisa por comentário no próprio SystemVerilog e leia a RAM por uma porta que o core já tem. Ordem dos arquivos importa (pacotes primeiro), e alguns cores leem arquivos `.mem` do diretório atual. Funciona em cerca de 1 quadro por segundo, então planeje 300 quadros por jogo (~5 min).
- **Registre o que o teste não cobre:** sem botões, sem som, só os primeiros quadros. Uma tabela "como cada parte foi validada" (com força forte/parcial/nenhuma) no `accuracy.md` evita parecer que tudo foi provado.
- **Um teste que acha erro vale pelo erro.** No V-Bear, 10 de 12 jogos saíram idênticos no quadro 300, mas o valor da comparação foi achar a janela 0 errada em uma linha. Guarde o caso com os números (antes e depois).

## Screenshots

O README e o manual citam imagens em `screenshots/`. O frontend gera todas sozinho, com variáveis de ambiente (veja `crates/frontend/src/app/docshots.rs`):

```
EMU_DOCSHOTS=screenshots EMU_DOCSHOTS_GAME=/caminho/jogo.rom \
  XDG_CONFIG_HOME=$(mktemp -d) cargo run --release -p <slug>
```

Use um `XDG_CONFIG_HOME` descartável: o roteiro muda configurações. Ele percorre uma lista fixa de telas, salva um PNG de cada e fecha: `library-covers`, `library-screens`, `library-list`, `couch-library`, `game-tv`, `couch-menu`, `game-info`, `folders`, `debugger-memory`, `debugger-tiles`, `debugger-cpu`, `debugger-video`, `cheats`, `cheats-search`. `EMU_DOCSHOTS_SEL=<n>` escolhe a capa selecionada no modo sofá (contando de 0). As que dependem do sistema (a moldura `game-tv`, abas do debugger) só aparecem se o app registrar bezels e o núcleo der `debug_views()`. Os modelos usam só as sempre disponíveis (`library-covers`, `library-screens`, `game-info`, `debugger-memory`, `cheats`); ajuste ao que existir.

## Antes de dar o doc por pronto

- [ ] Nenhum `TODO(system)` nem `{{...}}` sobrando.
- [ ] `tools/check-links.py` sem link quebrado.
- [ ] Todo número e afirmação sobre o hardware e sobre outros emuladores tem fonte ([Xn]) ou medição citada.
- [ ] Nenhuma data de "quando foi feito", nenhum "(antes: …)", nenhum plano escrito no futuro.
- [ ] Cada número que muda aparece num lugar só.
- [ ] Comandos, opções e atalhos citados existem no app.
- [ ] Capturas de tela geradas com o app de verdade.
- [ ] Se há referência: `reference/compare.sh` rodado nos jogos da lista de compatibilidade, e o resultado bate com o que `accuracy.md` e `compatibility.md` dizem.
- [ ] Versões em português e espanhol atualizadas no mesmo commit (ou removida a linha de troca de idioma).
