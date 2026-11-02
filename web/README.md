# V.Smile no navegador (WebAssembly)

O núcleo do emulador (`vsmile-core`) é `std`-only e **sem `unsafe`**, então compila
para `wasm32-unknown-unknown` sem alteração. O crate `vsmile-wasm` expõe uma ABI
em C mínima (sem `wasm-bindgen`); esta página (`index.html`) é o frontend em
`<canvas>` + Web Audio.

## Compilar e rodar

```bash
# 1. Gera web/vsmile_wasm.wasm
#    cargo build --release --target wasm32-unknown-unknown -p vsmile-wasm
#               cp target/wasm32-unknown-unknown/release/vsmile_wasm.wasm web/

# 2. Sirva a pasta por HTTP (o navegador não carrega .wasm de file://)
python -m http.server --directory web 8000
```

Abra <http://localhost:8000>, clique em **Abrir ROM** e escolha um `.bin` ou `.zip`
do V.Smile. Clique em **Ativar som** (navegadores exigem um gesto do usuário para
iniciar o áudio).

## Controles

Setas = direcional · Enter = OK · Backspace = Voltar · H = Ajuda · A = ABC ·
`Z X C V` = botões coloridos · O/P/R = ligar/desligar/restart.

## Notas

- O emulador **não acompanha ROMs/BIOS**.
- O `.wasm` tem ~105 KB (todo o emulador: CPU, PPU, SPU, periféricos).
- A ABI está em `crates/vsmile-wasm/src/lib.rs` e é testada nativamente
  (`cargo test -p vsmile-wasm`); o loop de navegador (este HTML) precisa de um
  browser para validar de ponta a ponta.
