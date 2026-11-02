//! Cartridge reading: the file directly, or the file(s) inside a .zip/.7z; system files
//! (BIOS) found by CRC32.

use std::io::Read;

use crate::i18n::trf;
use std::path::{Path, PathBuf};

/// The game's ROM. With `multi` (`SystemInfo::multi_file_roms`) a .zip holding several
/// files is one game on several chips: they are joined, largest first.
pub fn read(path: &Path, multi: bool) -> Result<Vec<u8>, String> {
    let ext = |x: &str| path.extension().is_some_and(|e| e.eq_ignore_ascii_case(x));
    if ext("7z") {
        return read_7z(path, multi);
    }
    if !ext("zip") {
        return std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    }
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| trf("{file}: invalid zip ({error})", &[("file", &path.display()), ("error", &e)]))?;
    let mut parts = Vec::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| e.to_string())?;
        if e.is_file() {
            let mut v = Vec::new();
            e.read_to_end(&mut v).map_err(|e| e.to_string())?;
            if !multi {
                return Ok(v);
            }
            parts.push(v);
        }
    }
    if parts.is_empty() {
        return Err(trf("{file}: empty zip", &[("file", &path.display())]));
    }
    parts.sort_by_key(|p| std::cmp::Reverse(p.len()));
    Ok(parts.concat())
}

/// Writes the game in `src` (a file, or a .zip/.7z with its parts) as a single file at `dest`: the
/// parts joined, largest first. Returns what was written.
pub fn export_joined(src: &Path, dest: &Path) -> Result<Vec<u8>, String> {
    let rom = read(src, true)?;
    std::fs::write(dest, &rom).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(rom)
}

/// Looks for the system files of `list` by CRC32 in `dirs`: loose files and the entries of
/// .zip files (their CRC is read from the zip directory, nothing is unpacked until it
/// matches). Files larger than `max_size` are skipped. Returns one slot per entry of `list`.
pub fn find_bios(list: &[emu_api::BiosFile], dirs: &[(PathBuf, u64)]) -> Vec<Option<Vec<u8>>> {
    let mut found: Vec<Option<Vec<u8>>> = vec![None; list.len()];
    let slot = |crc: u32, found: &[Option<Vec<u8>>]| list.iter().position(|b| b.crc32 == crc).filter(|&i| found[i].is_none());
    for (dir, max_size) in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() || entry.metadata().map_or(true, |m| m.len() > *max_size) {
                continue;
            }
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
                let Ok(mut z) = std::fs::File::open(&p).map_err(|_| ()).and_then(|f| zip::ZipArchive::new(f).map_err(|_| ())) else { continue };
                for i in 0..z.len() {
                    let Ok(mut e) = z.by_index(i) else { continue };
                    if let Some(k) = slot(e.crc32(), &found) {
                        let mut v = Vec::new();
                        if e.read_to_end(&mut v).is_ok() && emu_api::crc32(&v) == list[k].crc32 {
                            found[k] = Some(v);
                        }
                    }
                }
            } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("7z")) {
                // the CRC32 of each file is in the archive's directory: nothing is unpacked unless
                // one of them is a system file that is still missing
                let Ok(mut r) = sevenz_rust::SevenZReader::open(&p, sevenz_rust::Password::empty()) else { continue };
                let wanted = |found: &[Option<Vec<u8>>], crc: u64| list.iter().zip(found).any(|(b, s)| s.is_none() && u64::from(b.crc32) == crc);
                if !r.archive().files.iter().any(|f| f.has_stream() && !f.is_directory() && f.has_crc && wanted(&found, f.crc)) {
                    continue;
                }
                let biggest = list.iter().map(|b| b.size as u64).max().unwrap_or(0);
                let _ = r.for_each_entries(|e, reader| {
                    if e.has_stream() && !e.is_directory() && e.size() <= biggest {
                        let mut v = Vec::new();
                        reader.read_to_end(&mut v)?;
                        if let Some(k) = slot(emu_api::crc32(&v), &found) {
                            found[k] = Some(v);
                        }
                    } else {
                        std::io::copy(reader, &mut std::io::sink())?; // skip ahead in the solid block
                    }
                    Ok(true)
                });
            } else if let Ok(v) = std::fs::read(&p)
                && let Some(k) = slot(emu_api::crc32(&v), &found)
            {
                found[k] = Some(v);
            }
        }
    }
    found
}

/// Largest file inside the .7z; with `multi`, all the files (the chips of one game), joined largest
/// first, as in a .zip.
fn read_7z(path: &Path, multi: bool) -> Result<Vec<u8>, String> {
    let bad = |e: &dyn std::fmt::Display| trf("{file}: invalid 7z ({error})", &[("file", &path.display()), ("error", e)]);
    let mut r = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty()).map_err(|e| bad(&e))?;
    let largest = r
        .archive()
        .files
        .iter()
        .filter(|e| !e.is_directory() && e.has_stream())
        .max_by_key(|e| e.size())
        .map(|e| e.name().to_string())
        .ok_or_else(|| trf("{file}: empty 7z", &[("file", &path.display())]))?;
    let mut parts: Vec<Vec<u8>> = Vec::new();
    r.for_each_entries(|entry, reader| {
        if !entry.is_directory() && entry.has_stream() && (multi || entry.name() == largest) {
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf)?;
            parts.push(buf);
            return Ok(multi); // one file is enough unless the game is on several chips
        }
        std::io::copy(reader, &mut std::io::sink())?; // skip ahead in the solid block
        Ok(true)
    })
    .map_err(|e| bad(&e))?;
    parts.sort_by_key(|p| std::cmp::Reverse(p.len()));
    let data = (!parts.is_empty()).then(|| parts.concat());
    data.ok_or_else(|| trf("{file}: empty 7z", &[("file", &path.display())]))
}

/// ROM file: compressed (.zip/.7z, from the frontend) or a system extension.
pub fn is_rom(p: &Path, exts: &[&str]) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["zip", "7z"].iter().chain(exts).any(|x| e.eq_ignore_ascii_case(x)))
}

/// Games in a folder, in alphabetical order.
pub fn list(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_rom(p, exts)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// A system file inside a .7z is found by its CRC32, with any file name.
    #[test]
    fn finds_a_system_file_inside_a_7z() {
        let dir = std::env::temp_dir().join(format!("emu-bios-7z-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let data: Vec<u8> = (0..4096u32).map(|i| (i * 7 + 3) as u8).collect();
        std::fs::write(dir.join("src/whatever.bin"), &data).unwrap();
        sevenz_rust::compress_to_path(dir.join("src"), dir.join("bios.7z")).unwrap();
        let list = [emu_api::BiosFile { name: "x.bin", crc32: emu_api::crc32(&data), size: 4096, required: true }];
        let found = super::find_bios(&list, &[(dir.clone(), 1 << 20)]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found[0].as_deref(), Some(&data[..]));
    }

    /// A game on several chips in a .7z: with `multi` all the files, joined; without, the largest.
    /// Skipped unless EMU_TEST_7Z_MULTI points to such a .7z (ROMs are never in the repository).
    #[test]
    fn a_7z_with_several_chips_is_joined() {
        let Some(p) = std::env::var_os("EMU_TEST_7Z_MULTI") else {
            eprintln!("EMU_TEST_7Z_MULTI not set: test skipped");
            return;
        };
        let joined = super::read(Path::new(&p), true).unwrap();
        let largest = super::read(Path::new(&p), false).unwrap();
        assert!(joined.len() > largest.len(), "{} against {}", joined.len(), largest.len());
        assert!(joined.starts_with(&largest), "the largest file goes first");
        // and exporting writes exactly that
        let dest = std::env::temp_dir().join(format!("emu-export-{}.bin", std::process::id()));
        let written = super::export_joined(Path::new(&p), &dest).unwrap();
        let on_disk = std::fs::read(&dest).unwrap();
        let _ = std::fs::remove_file(&dest);
        assert_eq!(on_disk, joined);
        assert_eq!(written, joined);
    }
}
