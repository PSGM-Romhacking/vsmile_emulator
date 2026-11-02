//! Descompressor **DEFLATE** (RFC 1951) em Rust puro, sem dependências.
//!
//! Implementado do zero para que o loader de cartuchos consiga abrir ROMs
//! empacotadas em `.zip` (método 8 = *deflate*) sem puxar nenhuma crate
//! externa — mantendo a filosofia de dependência mínima do projeto.
//!
//! Suporta os três tipos de bloco do DEFLATE:
//! - `00` armazenado (*stored*, sem compressão);
//! - `01` Huffman fixo;
//! - `10` Huffman dinâmico.
//!
//! Não é uma implementação de *zlib* (não lê cabeçalho/Adler-32); recebe o
//! fluxo DEFLATE cru, que é o que o ZIP guarda.

/// Leitor de bits little-endian (ordem de bits do DEFLATE).
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u32, // 0..8, bits já consumidos do byte atual
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Lê um único bit.
    #[inline]
    fn bit(&mut self) -> Result<u32, String> {
        let byte = *self
            .data
            .get(self.byte_pos)
            .ok_or("deflate: fim inesperado dos dados")?;
        let b = (byte >> self.bit_pos) & 1;
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        Ok(b as u32)
    }

    /// Lê `n` bits (0..=16), com o primeiro bit lido como o menos significativo.
    #[inline]
    fn bits(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0u32;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Ok(v)
    }

    /// Descarta os bits restantes do byte atual (alinha no byte).
    fn align(&mut self) {
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
    }
}

/// Árvore de Huffman canônica, decodificada por tabela de comprimentos.
///
/// Guarda, para cada código possível, o símbolo correspondente. A decodificação
/// percorre bit a bit acumulando o código (MSB primeiro, como manda o DEFLATE
/// para a *leitura* dos códigos de Huffman) e consulta as fronteiras por
/// comprimento (algoritmo canônico do RFC 1951, seção 3.2.2).
struct Huffman {
    /// Para cada comprimento L (1..=MAX_BITS): primeiro código canônico de L.
    first_code: [u32; MAX_BITS + 1],
    /// Para cada comprimento L: índice em `symbols` do primeiro símbolo de L.
    first_symbol: [usize; MAX_BITS + 1],
    /// Quantos códigos existem de cada comprimento.
    count: [u32; MAX_BITS + 1],
    /// Símbolos ordenados por (comprimento, valor).
    symbols: Vec<u16>,
}

const MAX_BITS: usize = 15;

impl Huffman {
    /// Constrói a partir dos comprimentos de código de cada símbolo
    /// (`code_lengths[s]` = nº de bits do símbolo `s`, 0 = ausente).
    fn new(code_lengths: &[u8]) -> Result<Self, String> {
        let mut count = [0u32; MAX_BITS + 1];
        for &l in code_lengths {
            let l = l as usize;
            if l > MAX_BITS {
                return Err("deflate: comprimento de código inválido".into());
            }
            count[l] += 1;
        }
        count[0] = 0; // comprimento 0 = símbolo ausente

        // Primeiro código canônico de cada comprimento.
        let mut first_code = [0u32; MAX_BITS + 1];
        let mut first_symbol = [0usize; MAX_BITS + 1];
        let mut code = 0u32;
        let mut sym_idx = 0usize;
        for l in 1..=MAX_BITS {
            code = (code + count[l - 1]) << 1;
            first_code[l] = code;
            first_symbol[l] = sym_idx;
            sym_idx += count[l] as usize;
        }

        // Símbolos ordenados por (comprimento, valor de símbolo).
        let total: usize = count.iter().map(|&c| c as usize).sum();
        let mut symbols = vec![0u16; total];
        let mut offsets = first_symbol;
        for (sym, &l) in code_lengths.iter().enumerate() {
            if l != 0 {
                let l = l as usize;
                symbols[offsets[l]] = sym as u16;
                offsets[l] += 1;
            }
        }

        Ok(Self {
            first_code,
            first_symbol,
            count,
            symbols,
        })
    }

    /// Decodifica um símbolo do fluxo.
    #[inline]
    fn decode(&self, br: &mut BitReader) -> Result<u16, String> {
        let mut code = 0u32;
        for l in 1..=MAX_BITS {
            code = (code << 1) | br.bit()?;
            let cnt = self.count[l];
            if cnt != 0 {
                let first = self.first_code[l];
                if code >= first && code < first + cnt {
                    let idx = self.first_symbol[l] + (code - first) as usize;
                    return Ok(self.symbols[idx]);
                }
            }
        }
        Err("deflate: código de Huffman inválido".into())
    }
}

// Tabelas de comprimento (símbolos 257..=285) — RFC 1951, seção 3.2.5.
const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
// Tabelas de distância (símbolos 0..=29).
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
// Ordem em que os comprimentos de código do alfabeto de code-length aparecem.
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Árvores de Huffman fixas (blocos tipo `01`).
fn fixed_trees() -> (Huffman, Huffman) {
    let mut lit = [0u8; 288];
    for (i, l) in lit.iter_mut().enumerate() {
        *l = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let dist = [5u8; 30];
    (
        Huffman::new(&lit).expect("árvore literal fixa"),
        Huffman::new(&dist).expect("árvore de distância fixa"),
    )
}

/// Lê as árvores dinâmicas (blocos tipo `10`) do cabeçalho do bloco.
fn dynamic_trees(br: &mut BitReader) -> Result<(Huffman, Huffman), String> {
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;

    let mut cl_lengths = [0u8; 19];
    for i in 0..hclen {
        cl_lengths[CODE_LENGTH_ORDER[i]] = br.bits(3)? as u8;
    }
    let cl_tree = Huffman::new(&cl_lengths)?;

    // Decodifica os comprimentos dos alfabetos literal+distância juntos.
    let total = hlit + hdist;
    let mut lengths = vec![0u8; total];
    let mut i = 0;
    while i < total {
        let sym = cl_tree.decode(br)?;
        match sym {
            0..=15 => {
                lengths[i] = sym as u8;
                i += 1;
            }
            16 => {
                // Repete o comprimento anterior 3..=6 vezes.
                if i == 0 {
                    return Err("deflate: repetição sem comprimento anterior".into());
                }
                let rep = br.bits(2)? + 3;
                let prev = lengths[i - 1];
                for _ in 0..rep {
                    if i >= total {
                        return Err("deflate: repetição estoura o alfabeto".into());
                    }
                    lengths[i] = prev;
                    i += 1;
                }
            }
            17 => {
                // Repete zero 3..=10 vezes.
                let rep = br.bits(3)? + 3;
                for _ in 0..rep {
                    if i >= total {
                        return Err("deflate: repetição de zero estoura o alfabeto".into());
                    }
                    lengths[i] = 0;
                    i += 1;
                }
            }
            18 => {
                // Repete zero 11..=138 vezes.
                let rep = br.bits(7)? + 11;
                for _ in 0..rep {
                    if i >= total {
                        return Err("deflate: repetição de zero estoura o alfabeto".into());
                    }
                    lengths[i] = 0;
                    i += 1;
                }
            }
            _ => return Err("deflate: símbolo de code-length inválido".into()),
        }
    }

    let lit_tree = Huffman::new(&lengths[..hlit])?;
    let dist_tree = Huffman::new(&lengths[hlit..])?;
    Ok((lit_tree, dist_tree))
}

/// Descomprime um fluxo DEFLATE cru. `size_hint` pré-aloca a saída (use o
/// tamanho descomprimido anunciado pelo ZIP; serve só de dica).
pub fn inflate(data: &[u8], size_hint: usize) -> Result<Vec<u8>, String> {
    let mut br = BitReader::new(data);
    let mut out: Vec<u8> = Vec::with_capacity(size_hint);

    loop {
        let bfinal = br.bit()?;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                // Bloco armazenado: alinha e copia LEN bytes crus.
                br.align();
                let len = br.bits(16)? as usize;
                let nlen = br.bits(16)? as usize;
                if len != (!nlen & 0xffff) {
                    return Err("deflate: LEN/NLEN não conferem".into());
                }
                for _ in 0..len {
                    let b = *br
                        .data
                        .get(br.byte_pos)
                        .ok_or("deflate: fim inesperado em bloco armazenado")?;
                    out.push(b);
                    br.byte_pos += 1;
                }
            }
            1 | 2 => {
                let (lit_tree, dist_tree) = if btype == 1 {
                    fixed_trees()
                } else {
                    dynamic_trees(&mut br)?
                };
                loop {
                    let sym = lit_tree.decode(&mut br)?;
                    match sym {
                        0..=255 => out.push(sym as u8),
                        256 => break, // fim do bloco
                        257..=285 => {
                            let li = (sym - 257) as usize;
                            let length =
                                LEN_BASE[li] as usize + br.bits(LEN_EXTRA[li] as u32)? as usize;
                            let dsym = dist_tree.decode(&mut br)? as usize;
                            if dsym >= DIST_BASE.len() {
                                return Err("deflate: símbolo de distância inválido".into());
                            }
                            let dist = DIST_BASE[dsym] as usize
                                + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
                            if dist == 0 || dist > out.len() {
                                return Err("deflate: distância fora do histórico".into());
                            }
                            // Cópia com sobreposição (LZ77): byte a byte.
                            let start = out.len() - dist;
                            for k in 0..length {
                                let b = out[start + k];
                                out.push(b);
                            }
                        }
                        _ => return Err("deflate: símbolo literal/comprimento inválido".into()),
                    }
                }
            }
            _ => return Err("deflate: tipo de bloco reservado (11)".into()),
        }
        if bfinal == 1 {
            break;
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bloco armazenado (tipo 00): "Hi" sem compressão.
    /// Byte0 = 0x01 (BFINAL=1, BTYPE=00). Depois LEN=2, NLEN=~2, dados "Hi".
    #[test]
    fn stored_block() {
        let data = [0x01, 0x02, 0x00, 0xfd, 0xff, b'H', b'i'];
        let out = inflate(&data, 2).unwrap();
        assert_eq!(out, b"Hi");
    }

    /// Fluxo deflate (Huffman fixo) de "aaaaaaaaaa" (10 'a') — gerado pelo
    /// zlib do Python com `compressobj(wbits=-15)`. Exercita back-reference.
    #[test]
    fn fixed_huffman_backref() {
        // raw deflate de b"aaaaaaaaaa"
        let data = [0x4b, 0x4c, 0x84, 0x01, 0x00];
        let out = inflate(&data, 10).unwrap();
        assert_eq!(out, b"aaaaaaaaaa");
    }
}
