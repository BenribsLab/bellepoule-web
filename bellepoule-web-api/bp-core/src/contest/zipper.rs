//! Archive des étapes d'une compétition (port de `supervisor/zipper.cpp`).
//!
//! À chaque passage à la phase suivante, une copie du fichier `.cotcot` est
//! ajoutée à l'archive `.zip` portant le même nom :
//! `NN-[jj_mm_aaaa]-[hh_mm_ss]-titre.cotcot`.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Nom de l'archive associée à un fichier `.cotcot`.
pub fn archive_path(cotcot: &Path) -> Option<PathBuf> {
    let name = cotcot.to_string_lossy();
    name.strip_suffix(".cotcot").map(|base| PathBuf::from(format!("{base}.zip")))
}

/// Ajoute `what` à l'archive `zip_path` (créée au besoin) sous le titre donné.
pub fn archive(zip_path: &Path, what: &Path, title: &str) -> std::io::Result<()> {
    let mut data = Vec::new();
    File::open(what)?.read_to_end(&mut data)?;

    let temp = PathBuf::from(format!("{}__", zip_path.display()));
    let result = write_archive(zip_path, &temp, &data, title);
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn write_archive(zip_path: &Path, temp: &Path, data: &[u8], title: &str) -> std::io::Result<()> {
    let mut writer = ZipWriter::new(File::create(temp)?);
    let mut count = 0usize;
    if zip_path.exists() {
        let mut previous = ZipArchive::new(File::open(zip_path)?).map_err(to_io)?;
        count = previous.len();
        for i in 0..previous.len() {
            let entry = previous.by_index_raw(i).map_err(to_io)?;
            writer.raw_copy_file(entry).map_err(to_io)?;
        }
    }
    let now = chrono::Local::now();
    let name = format!("{:02}-{}-{}.cotcot", count, now.format("[%d_%m_%Y]-[%H_%M_%S]"), title);
    writer.start_file(name, SimpleFileOptions::default()).map_err(to_io)?;
    writer.write_all(data)?;
    writer.finish().map_err(to_io)?;

    if zip_path.exists() {
        std::fs::remove_file(zip_path)?;
    }
    std::fs::rename(temp, zip_path)
}

fn to_io(e: zip::result::ZipError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_accumulates_entries() {
        let dir = std::env::temp_dir().join(format!("bp_zip_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.cotcot");
        std::fs::write(&f, "<x/>").unwrap();
        let z = archive_path(&f).unwrap();
        archive(&z, &f, "Round #1").unwrap();
        archive(&z, &f, "Table").unwrap();
        let a = ZipArchive::new(File::open(&z).unwrap()).unwrap();
        assert_eq!(a.len(), 2);
        assert!(a.file_names().any(|n| n.is_ok_and(|n| n.starts_with("01-") && n.ends_with("-Table.cotcot"))));
        std::fs::remove_dir_all(&dir).ok();
    }
}
