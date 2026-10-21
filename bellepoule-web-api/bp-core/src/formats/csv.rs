//! Fichiers CSV de tireurs/arbitres (`Checkin::ImportCSV`,
//! `Tournament::OnOpenTemplate`).
//!
//! - séparateur `;` si la ligne d'en-tête en contient un, `,` sinon ;
//! - pas de guillemets au sens CSV : les `"` sont remplacés par des espaces
//!   puis les champs sont rognés (comme le C++) ;
//! - chaque colonne d'en-tête est associée à un attribut « CSV ready » par
//!   son libellé (traduit ou non), son nom XML ou son nom de code.
//!
//! Encodage : UTF-8 s'il est valide, sinon la page de code Windows
//! (`g_locale_to_utf8` sous Windows). En plus du C++, les BOM UTF-8/UTF-16
//! sont reconnues.

use crate::attribute::{self, collate_key, AttributeDesc};
use crate::formats::encoding::{self, TextEncoding};
use crate::formats::team::PlayerList;
use crate::player::{AttrId, Player, PlayerClass, PlayerRef, PlayerStore};
use crate::Result;

/// Message affiché par le C++ quand le fichier n'est pas en UTF-8.
pub const LOCALE_WARNING: &str = "The imported CSV file is probably based on template generated from another computer.\n\nUse a template generated from this computer!";

/// `Application::IsCsvReady`.
pub fn is_csv_ready(desc: &AttributeDesc) -> bool {
    desc.is_csv_ready() && !matches!(desc.code_name, "final_rank" | "IP" | "exported")
}

/// `AttributeDesc::GuessDescFromUserName (name, "CSV ready")`.
pub fn guess_column(name: &str) -> Option<&'static AttributeDesc> {
    let key = collate_key(name.trim());
    if key.is_empty() {
        return None;
    }
    attribute::registry().all().iter().filter(|d| is_csv_ready(d)).find(|d| {
        collate_key(d.user_name) == key
            || collate_key(&crate::i18n::tr(d.user_name)) == key
            || collate_key(d.code_name) == key
            || collate_key(d.xml_name) == key
    })
}

#[derive(Debug, Clone)]
pub struct CsvImport {
    pub encoding: TextEncoding,
    pub delimiter: char,
    /// Attribut (nom de code) associé à chaque colonne.
    pub columns: Vec<Option<&'static str>>,
    /// Avertissement à afficher (fichier non UTF-8).
    pub warning: Option<&'static str>,
    pub players: PlayerList,
}

pub fn decode_csv(bytes: &[u8]) -> (String, TextEncoding, Option<&'static str>) {
    if let Some(enc) = encoding::detect_bom(bytes).or_else(|| encoding::guess_utf16_without_bom(bytes)) {
        if let Ok(t) = encoding::decode(bytes, enc) {
            return (encoding::strip_cr(&t), enc, None);
        }
    }
    let raw: Vec<u8> = bytes.iter().copied().filter(|&b| b != b'\r').collect();
    match std::str::from_utf8(&raw) {
        Ok(t) => (t.to_string(), TextEncoding::Utf8, None),
        Err(_) => (encoding::windows1252_decode(&raw), TextEncoding::Windows1252, Some(LOCALE_WARNING)),
    }
}

fn clean(field: &str) -> String {
    field.replace('"', " ").trim().to_string()
}

pub fn import_file(path: &std::path::Path, class: PlayerClass) -> Result<CsvImport> {
    Ok(import(&std::fs::read(path)?, class))
}

/// Import d'un fichier CSV ; `class` est la classe de base de la liste
/// (tireurs ou arbitres).
pub fn import(bytes: &[u8], class: PlayerClass) -> CsvImport {
    let (text, enc, warning) = decode_csv(bytes);
    let rows: Vec<&str> = text.split('\n').collect();
    let delimiter = if rows.first().map(|r| r.contains(';')).unwrap_or(false) { ';' } else { ',' };
    let mut result = CsvImport { encoding: enc, delimiter, columns: Vec::new(), warning, players: PlayerList::default() };
    let Some(header) = rows.first() else { return result };
    result.columns = header.split(delimiter).map(|h| guess_column(&clean(h)).map(|d| d.code_name)).collect();

    for row in &rows[1..] {
        if row.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = row.split(delimiter).collect();
        let mut player = Player::new(class);
        let mut has_attribute = false;
        for (c, column) in result.columns.iter().enumerate() {
            let Some(token) = tokens.get(c) else { break };
            if let Some(code) = column {
                player.set_str(&AttrId::global(code), &clean(token));
                has_attribute = true;
            }
        }
        if has_attribute {
            result.players.push(player, None);
        }
    }
    result
}

/// Modèle de fichier (`fencer_file_template.csv`) : les libellés traduits
/// des attributs « CSV ready », chacun suivi d'un `;`.
pub fn template() -> String {
    let mut s = String::new();
    for d in attribute::registry().all().iter().filter(|d| is_csv_ready(d)) {
        s.push_str(&crate::i18n::tr(d.user_name));
        s.push(';');
    }
    s
}

/// Export des joueurs avec les colonnes du modèle (relisible par
/// [`import`]). Complément au C++, qui n'exporte que le modèle.
pub fn export(store: &PlayerStore, refs: &[PlayerRef], delimiter: char) -> String {
    let descs: Vec<&AttributeDesc> = attribute::registry().all().iter().filter(|d| is_csv_ready(d)).collect();
    let mut out = String::new();
    for d in &descs {
        out.push_str(&crate::i18n::tr(d.user_name));
        out.push(delimiter);
    }
    out.push('\n');
    for r in refs {
        let Some(p) = store.get(*r) else { continue };
        for d in &descs {
            if let Some(v) = p.attr(d.code_name) {
                // Images XML : relues telles quelles par l'import.
                out.push_str(&v.xml_image().replace([delimiter, '"', '\n'], " "));
            }
            out.push(delimiter);
        }
        out.push('\n');
    }
    out
}
