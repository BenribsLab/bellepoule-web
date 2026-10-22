//! Archives ZIP de compétitions (`Tournament::OpenUriContest`) : chaque
//! fichier `.xml`/`.XML` de l'archive est ouvert comme une compétition.
//! Les fichiers de personnes (`.fff`, `.csv`, `.txt`) peuvent aussi en être
//! extraits.

use std::io::Read;

use crate::{Error, Result};

/// Fichiers d'une archive dont le nom se termine par l'un des suffixes.
pub fn entries_with_suffix(bytes: &[u8], suffixes: &[&str]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| Error::Format(format!("archive ZIP : {e}")))?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|e| Error::Format(format!("archive ZIP : {e}")))?;
        let name = f.name().map_err(|e| Error::Format(format!("archive ZIP : {e}")))?.to_string();
        if suffixes.iter().any(|s| name.ends_with(s)) {
            let mut data = Vec::new();
            f.read_to_end(&mut data)?;
            out.push((name, data));
        }
    }
    Ok(out)
}

/// Fichiers XML de compétition contenus dans l'archive.
pub fn xml_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    entries_with_suffix(bytes, &[".XML", ".xml"])
}

/// Suffixes des fichiers de compétition (`contest_suffix_table`).
pub const CONTEST_SUFFIXES: [&str; 4] = [".cotcot", ".COTCOT", ".xml", ".XML"];
/// Suffixes des fichiers de personnes (`people_suffix_table`) : `.fff` est
/// lu en FFF, le reste en CSV.
pub const PEOPLE_SUFFIXES: [&str; 6] = [".fff", ".FFF", ".csv", ".CSV", ".txt", ".TXT"];

/// Type de fichier selon son nom (`Tournament::OpenUriContest`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Zip,
    AskFred,
    Contest,
    Fff,
    Csv,
    Unknown,
}

pub fn file_kind(name: &str) -> FileKind {
    if name.ends_with(".zip") {
        FileKind::Zip
    } else if name.ends_with(".frd") {
        FileKind::AskFred
    } else if CONTEST_SUFFIXES.iter().any(|s| name.ends_with(s)) {
        FileKind::Contest
    } else if name.ends_with(".fff") || name.ends_with(".FFF") {
        FileKind::Fff
    } else if PEOPLE_SUFFIXES.iter().any(|s| name.ends_with(s)) {
        FileKind::Csv
    } else {
        FileKind::Unknown
    }
}
