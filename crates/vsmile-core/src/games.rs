//! The known games, by CRC32 of the ROM as the frontend reads it. Names are the
//! No-Intro names from libretro-database (`assets/games/vsmile.tsv`), which are also the
//! libretro-thumbnails file names.

use std::sync::LazyLock;

use emu_api::GameInfo;

const TABLE: &str = include_str!("../../../assets/games/vsmile.tsv");

static INFOS: LazyLock<Vec<(u32, GameInfo)>> = LazyLock::new(|| {
    TABLE
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .filter_map(|l| {
            let (crc, name) = l.split_once('\t')?;
            let crc = u32::from_str_radix(crc, 16).ok()?;
            // "Title (Region) (Extra)" -> "Title"
            let title = name.split(" (").next().unwrap_or(name).trim();
            Some((
                crc,
                GameInfo {
                    id: format!("{crc:08x}"),
                    title: title.into(),
                    alt_title: None,
                    year: None,
                    publisher: None,
                    developer: None,
                    serial: None,
                    players: None,
                    thumbnail: Some(name.into()),
                },
            ))
        })
        .collect()
});

/// CRC32 of the Art Studio cartridges, the only ones with battery-backed SRAM (on CSB2).
const ART_STUDIO: [u32; 5] = [
    0x3bbfb917, // Art Studio (USA)
    0xf9911730, // Estudio De Arte (Spain)
    0xfec3361e, // Studio de dessin (France)
    0x27449e15, // Tecknarstudio (Sweden)
    0xae5f4fe2, // Zeichenatelier (Germany)
];

pub fn game_info(rom: &[u8]) -> Option<&'static GameInfo> {
    let crc = emu_api::crc32(rom);
    INFOS.iter().find(|(c, _)| *c == crc).map(|(_, g)| g)
}

/// Whether the cartridge has the Art Studio NVRAM.
pub fn has_nvram(rom: &[u8]) -> bool {
    ART_STUDIO.contains(&emu_api::crc32(rom))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_parses() {
        assert!(INFOS.len() > 400);
        let art = INFOS.iter().find(|(c, _)| *c == 0x3bbfb917).unwrap();
        assert_eq!(art.1.title, "V.Smile - Art Studio");
    }
}
