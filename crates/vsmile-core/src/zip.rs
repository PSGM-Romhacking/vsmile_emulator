//! Leitor de arquivos **ZIP** mínimo, em Rust puro, para abrir cartuchos
//! empacotados (`.zip`) sem dependências externas.
//!
//! Suporta os dois métodos de armazenamento usados por praticamente todos os
//! dumps de ROM (ex.: conjuntos No-Intro): `0` (*stored*, sem compressão) e `8`
//! (*deflate*, descomprimido por [`crate::inflate`]). Lê o *End Of Central
//! Directory* e o diretório central; não trata ZIP64 nem criptografia (não
//! aparecem em dumps de cartucho).

use crate::inflate::inflate;

/// Uma entrada (arquivo) dentro do ZIP.
#[derive(Debug, Clone)]
pub struct ZipEntry {
    pub name: String,
    method: u16,
    comp_size: usize,
    uncomp_size: usize,
    local_header_off: usize,
}

fn rd_u16(d: &[u8], off: usize) -> Result<u16, String> {
    let b = d
        .get(off..off + 2)
        .ok_or("zip: leitura u16 fora dos limites")?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn rd_u32(d: &[u8], off: usize) -> Result<u32, String> {
    let b = d
        .get(off..off + 4)
        .ok_or("zip: leitura u32 fora dos limites")?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Lista as entradas de um arquivo ZIP (via diretório central).
pub fn entries(data: &[u8]) -> Result<Vec<ZipEntry>, String> {
    // Acha o End Of Central Directory (assinatura 0x06054b50), varrendo de trás
    // para a frente (o comentário final tem até 64 KiB).
    const EOCD_SIG: u32 = 0x0605_4b50;
    if data.len() < 22 {
        return Err("zip: arquivo pequeno demais".into());
    }
    let max_back = data.len().saturating_sub(22).min(0xffff + 22);
    let mut eocd = None;
    let start = data.len() - 22;
    for off in (start.saturating_sub(max_back)..=start).rev() {
        if rd_u32(data, off)? == EOCD_SIG {
            eocd = Some(off);
            break;
        }
    }
    let eocd = eocd.ok_or("zip: End Of Central Directory não encontrado")?;

    let total = rd_u16(data, eocd + 10)? as usize; // entradas neste disco
    let cd_off = rd_u32(data, eocd + 16)? as usize; // offset do diretório central

    const CEN_SIG: u32 = 0x0201_4b50;
    let mut out = Vec::with_capacity(total);
    let mut p = cd_off;
    for _ in 0..total {
        if rd_u32(data, p)? != CEN_SIG {
            return Err("zip: assinatura de diretório central inválida".into());
        }
        let method = rd_u16(data, p + 10)?;
        let comp_size = rd_u32(data, p + 20)? as usize;
        let uncomp_size = rd_u32(data, p + 24)? as usize;
        let name_len = rd_u16(data, p + 28)? as usize;
        let extra_len = rd_u16(data, p + 30)? as usize;
        let comment_len = rd_u16(data, p + 32)? as usize;
        let local_header_off = rd_u32(data, p + 42)? as usize;

        let name_bytes = data
            .get(p + 46..p + 46 + name_len)
            .ok_or("zip: nome de entrada fora dos limites")?;
        let name = String::from_utf8_lossy(name_bytes).into_owned();

        out.push(ZipEntry {
            name,
            method,
            comp_size,
            uncomp_size,
            local_header_off,
        });
        p += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

/// Extrai (descomprime) o conteúdo de uma entrada.
pub fn read_entry(data: &[u8], e: &ZipEntry) -> Result<Vec<u8>, String> {
    const LOC_SIG: u32 = 0x0403_4b50;
    let p = e.local_header_off;
    if rd_u32(data, p)? != LOC_SIG {
        return Err("zip: assinatura de cabeçalho local inválida".into());
    }
    // O cabeçalho local tem seus próprios campos de tamanho de nome/extra.
    let name_len = rd_u16(data, p + 26)? as usize;
    let extra_len = rd_u16(data, p + 28)? as usize;
    let data_off = p + 30 + name_len + extra_len;

    let comp = data
        .get(data_off..data_off + e.comp_size)
        .ok_or("zip: dados comprimidos fora dos limites")?;

    match e.method {
        0 => Ok(comp.to_vec()), // stored
        8 => inflate(comp, e.uncomp_size),
        m => Err(format!("zip: método de compressão {m} não suportado")),
    }
}

/// Escolhe a entrada que é a ROM do cartucho e devolve seus bytes já
/// descomprimidos.
///
/// Heurística: prefere a maior entrada com extensão `.bin`; se não houver
/// `.bin`, usa a maior entrada que não seja diretório. Dumps de V.Smile têm um
/// único `.bin` por `.zip`, então isso acerta na prática.
pub fn extract_rom(data: &[u8]) -> Result<Vec<u8>, String> {
    let list = entries(data)?;
    let files: Vec<&ZipEntry> = list
        .iter()
        .filter(|e| !e.name.ends_with('/') && e.uncomp_size > 0)
        .collect();
    if files.is_empty() {
        return Err("zip: nenhum arquivo dentro do .zip".into());
    }
    let pick = files
        .iter()
        .filter(|e| e.name.to_ascii_lowercase().ends_with(".bin"))
        .max_by_key(|e| e.uncomp_size)
        .or_else(|| files.iter().max_by_key(|e| e.uncomp_size))
        .copied()
        .unwrap();
    read_entry(data, pick)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ZIP contendo um único "rom.bin" = b"V.SMILE!" comprimido (deflate),
    // gerado pelo módulo zipfile do Python (ver fixture no teste).
    // Montado em build-time pela função abaixo para não depender de arquivo.
    fn sample_zip() -> Vec<u8> {
        // Bytes de um .zip real com uma entrada "rom.bin" de 8 bytes.
        // (gerado com: zipfile.ZipFile(..., ZIP_DEFLATED).writestr("rom.bin", b"V.SMILE!"))
        // embedded: *.zip files are kept out of git
        vec![
            0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x21, 0x5c, 0x45, 0x49,
            0x6b, 0x1d, 0x0a, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x72, 0x6f,
            0x6d, 0x2e, 0x62, 0x69, 0x6e, 0x0b, 0xd3, 0x0b, 0xf6, 0xf5, 0xf4, 0x71, 0x55, 0x04, 0x00, 0x50,
            0x4b, 0x01, 0x02, 0x14, 0x03, 0x14, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x21, 0x5c, 0x45,
            0x49, 0x6b, 0x1d, 0x0a, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00, 0x72, 0x6f, 0x6d,
            0x2e, 0x62, 0x69, 0x6e, 0x50, 0x4b, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
            0x35, 0x00, 0x00, 0x00, 0x2f, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    }

    #[test]
    fn lista_e_extrai_bin() {
        let z = sample_zip();
        let list = entries(&z).unwrap();
        assert!(list.iter().any(|e| e.name == "rom.bin"));
        let rom = extract_rom(&z).unwrap();
        assert_eq!(rom, b"V.SMILE!");
    }
}
