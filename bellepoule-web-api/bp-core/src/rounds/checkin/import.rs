//! Import de listes de tireurs dans le pointage (port de
//! `Checkin::ImportFFF` et `Checkin::ImportCSV`).
//!
//! Ces fonctions ne font que lire le fichier et créer les joueurs ; leur
//! ajout (rattachement aux équipes, organisation…) est fait par
//! `CheckinStage::import`.

use chrono::Datelike;

use crate::attribute::{registry, Value};
use crate::contest::players::{guess_player_organization, new_player};
use crate::player::{AttrId, Player, PlayerClass};

/// Résultat d'un import.
#[derive(Debug, Default)]
pub struct ImportResult {
    pub players: Vec<Player>,
    /// Le fichier comportait des erreurs d'encodage (le C++ affiche alors
    /// « The imported FFF file has encoding errors. »).
    pub encoding_error: bool,
}

/// `GetFileContent` : contenu sans les `\r`.
fn file_content(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let mut raw = std::fs::read(path)?;
    raw.retain(|b| *b != b'\r');
    Ok(raw)
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|b| *b as char).collect()
}

/// Conversion d'une date FFF `jj/mm/aa[aa]` en `jj.mm.aaaa` (les années sur
/// deux chiffres sont rapportées au siècle courant ou précédent).
pub fn fff_birth_date(date: &str, current_year: i32) -> Option<String> {
    let parts: Vec<&str> = date.split('/').collect();
    if parts.len() < 3 {
        return None;
    }
    let atoi = |s: &str| crate::attribute::parse_leading_int(s) as i32;
    let long_year = if parts[2].len() <= 2 {
        let short_year = atoi(parts[2]);
        let mut y = short_year + current_year - current_year % 100;
        if short_year >= current_year % 100 {
            y -= 100;
        }
        y
    } else {
        atoi(parts[2])
    };
    Some(format!("{:02}.{:02}.{}", atoi(parts[0]), atoi(parts[1]), long_year))
}

/// Analyse le contenu d'un fichier FFF.
pub fn parse_fff(bytes: &[u8], class: PlayerClass) -> ImportResult {
    let mut result = ImportResult::default();
    let text = if bytes.windows(8).any(|w| w == b"FFF;WIN;") {
        latin1(bytes)
    } else {
        match String::from_utf8(bytes.to_vec()) {
            Ok(s) => s,
            Err(_) => {
                result.encoding_error = true;
                return result;
            }
        }
    };
    let current_year = chrono::Local::now().year();
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() < 2 {
        return result;
    }
    for line in &lines[2..] {
        // `g_strsplit_set` d'une ligne vide ne donne aucun champ : pas de tireur.
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(';').collect();
        let mut player = new_player(class);
        // Général
        {
            let tokens: Vec<&str> = fields[0].split(',').collect();
            let get = |i: usize| tokens.get(i).copied();
            if let Some(v) = get(0) {
                player.set_str(&AttrId::global("name"), v);
            }
            if let Some(v) = get(1) {
                player.set_str(&AttrId::global("first_name"), v);
            }
            if let Some(d) = get(2).and_then(|d| fff_birth_date(d, current_year)) {
                player.set_str(&AttrId::global("birth_date"), &d);
            }
            if let Some(g) = get(3) {
                let g = if g.starts_with(['H', 'h']) { format!("M{}", &g[1..]) } else { g.to_string() };
                player.set_str(&AttrId::global("gender"), &g);
            }
            if let Some(v) = get(4) {
                player.set_str(&AttrId::global("country"), v);
            }
        }
        // Fédération nationale
        if let Some(nat) = fields.get(2) {
            let tokens: Vec<&str> = nat.split(',').collect();
            if let Some(v) = tokens.first() {
                player.set_str(&AttrId::global("licence"), v);
            }
            if let Some(v) = tokens.get(2) {
                player.set_str(&AttrId::global("club"), v);
            }
            match tokens.get(3) {
                Some(v) => player.set_str(&AttrId::global("ranking"), v),
                None => player.set_uint(&AttrId::global("ranking"), 0),
            };
            guess_player_organization(&mut player, "league");
            guess_player_organization(&mut player, "region");
        }
        result.players.push(player);
    }
    result
}

/// `ImportFFF`.
pub fn import_fff(path: &std::path::Path, class: PlayerClass) -> std::io::Result<ImportResult> {
    Ok(parse_fff(&file_content(path)?, class))
}

fn clean_token(s: &str) -> String {
    s.replace('"', " ").trim().to_string()
}

/// Analyse le contenu d'un fichier CSV : la première ligne donne les
/// colonnes (libellés d'attributs « CSV ready »), séparateur `;` s'il est
/// présent dans l'en-tête, `,` sinon.
pub fn parse_csv(bytes: &[u8], class: PlayerClass) -> ImportResult {
    let mut result = ImportResult::default();
    let text = match String::from_utf8(bytes.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            // `g_locale_to_utf8` : on suppose une locale occidentale.
            result.encoding_error = false;
            latin1(bytes)
        }
    };
    let rows: Vec<&str> = text.split('\n').collect();
    let Some(header) = rows.first() else { return result };
    let delimiter = if header.contains(';') { ';' } else { ',' };
    let columns: Vec<Option<&'static str>> = header
        .split(delimiter)
        .map(|h| {
            registry()
                .guess_from_user_name(&clean_token(h))
                .filter(|d| d.is_csv_ready())
                .map(|d| d.code_name)
        })
        .collect();
    for row in &rows[1..] {
        // `g_strsplit_set` d'une ligne vide ne donne aucun champ : pas de tireur.
        if row.trim().is_empty() {
            continue;
        }
        let tokens: Vec<&str> = row.split(delimiter).collect();
        let mut player = new_player(class);
        let mut has_attribute = false;
        for (c, column) in columns.iter().enumerate() {
            let Some(token) = tokens.get(c) else { break };
            if let Some(code) = column {
                let value = clean_token(token);
                if *code == "attending" {
                    player.set(&AttrId::global(code), Value::parse(crate::attribute::AttrType::Bool, &value));
                } else {
                    player.set_str(&AttrId::global(code), &value);
                }
                has_attribute = true;
            }
        }
        if has_attribute {
            result.players.push(player);
        }
    }
    result
}

/// `ImportCSV`.
pub fn import_csv(path: &std::path::Path, class: PlayerClass) -> std::io::Result<ImportResult> {
    Ok(parse_csv(&file_content(path)?, class))
}

/// Modèle CSV (`Tournament::OnOpenTemplate`) : libellés des attributs
/// « CSV ready » séparés par `;`.
pub fn csv_template() -> String {
    let mut s = String::new();
    for d in registry().all().iter().filter(|d| d.is_csv_ready()) {
        s.push_str(d.user_name);
        s.push(';');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_years() {
        assert_eq!(fff_birth_date("01/02/95", 2026).unwrap(), "01.02.1995");
        assert_eq!(fff_birth_date("01/02/10", 2026).unwrap(), "01.02.2010");
        assert_eq!(fff_birth_date("1/2/1990", 2026).unwrap(), "01.02.1990");
    }

    #[test]
    fn csv() {
        let r = parse_csv(b"last name;first name;club\nDUPONT;Jean;RENNES\n", PlayerClass::Fencer);
        assert_eq!(r.players.len(), 1);
        assert_eq!(r.players[0].str_attr("club"), Some("RENNES"));
    }
}
