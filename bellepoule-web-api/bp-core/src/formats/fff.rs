//! Fichiers FFF de la Fédération française d'escrime
//! (`Checkin::ImportFFF`, `Classification::DumpToFFF`).
//!
//! Structure (une ligne par tireur, après deux lignes d'en-tête) :
//! ```text
//! FFF;WIN;CLASSEMENT;FFE
//! 08/11/2009;;;;
//! NOM,Prénom,jj/mm/aaaa,M,FRA;<FIE>;licence,ligue,club,classement,points;
//! ```
//! Variantes d'encodage : `FFF;WIN;` → ISO-8859-1, sinon UTF-8 (avec ou sans
//! BOM, ex. `FFF;UTF8;` d'Engarde). En plus du C++, les fichiers UTF-16
//! (avec BOM) sont acceptés.

use chrono::Datelike;

use crate::attribute::{parse_date, Value};
use crate::formats::discrete;
use crate::formats::encoding::{self, atoi, TextEncoding};
use crate::formats::header::ContestHeader;
use crate::formats::team::PlayerList;
use crate::player::{AttrId, Player, PlayerClass, PlayerRef, PlayerStore};
use crate::{Error, Result};

/// Message d'erreur du C++ (msgid) en cas d'erreur d'encodage.
pub const ENCODING_ERROR: &str = "The imported FFF file has encoding errors.";

#[derive(Debug, Clone)]
pub struct FffImport {
    pub encoding: TextEncoding,
    /// Champs de la première ligne (`FFF;WIN;CLASSEMENT;FFE`).
    pub kind: Vec<String>,
    /// Champs de la deuxième ligne (date du fichier…).
    pub info: Vec<String>,
    pub players: PlayerList,
}

/// Décode le contenu d'un fichier FFF (détection d'encodage du C++,
/// augmentée de l'UTF-16).
pub fn decode_fff(bytes: &[u8]) -> Result<(String, TextEncoding)> {
    if let Some(enc @ (TextEncoding::Utf16Le | TextEncoding::Utf16Be)) = encoding::detect_bom(bytes).or_else(|| encoding::guess_utf16_without_bom(bytes)) {
        let text = encoding::decode(bytes, enc)?;
        return Ok((encoding::strip_cr(&text), enc));
    }
    // GetFileContent : les '\r' sont retirés avant toute conversion.
    let raw: Vec<u8> = bytes.iter().copied().filter(|&b| b != b'\r').collect();
    if raw.windows(8).any(|w| w == b"FFF;WIN;") {
        return Ok((encoding::latin1_decode(&raw), TextEncoding::Latin1));
    }
    let enc = if raw.starts_with(&[0xEF, 0xBB, 0xBF]) { TextEncoding::Utf8Bom } else { TextEncoding::Utf8 };
    match encoding::decode(&raw, enc) {
        Ok(t) => Ok((t, enc)),
        Err(_) => Err(Error::Format(ENCODING_ERROR.into())),
    }
}

/// Import avec l'année courante du système (correction des années sur
/// deux chiffres).
pub fn import(bytes: &[u8], class: PlayerClass) -> Result<FffImport> {
    import_with_year(bytes, class, chrono::Local::now().year())
}

pub fn import_file(path: &std::path::Path, class: PlayerClass) -> Result<FffImport> {
    import(&std::fs::read(path)?, class)
}

pub fn import_with_year(bytes: &[u8], class: PlayerClass, current_year: i32) -> Result<FffImport> {
    let (text, enc) = decode_fff(bytes)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let mut result = FffImport { encoding: enc, kind: Vec::new(), info: Vec::new(), players: PlayerList::default() };
    if lines.len() < 2 {
        return Ok(result);
    }
    result.kind = lines[0].trim_start_matches('\u{feff}').split(';').map(str::to_string).collect();
    result.info = lines[1].split(';').map(str::to_string).collect();
    let catalog = discrete::catalog();

    for line in &lines[2..] {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        let mut player = Player::new(class);

        // Identité
        let tokens: Vec<&str> = parts[0].split(',').collect();
        let tok = |i: usize| tokens.get(i).copied();
        if let Some(name) = tok(0) {
            player.set_str(&AttrId::global("name"), name);
        }
        if let Some(first) = tok(1) {
            player.set_str(&AttrId::global("first_name"), first);
        }
        if let Some(date) = tok(2) {
            if let Some(d) = fff_birth_date(date, current_year) {
                player.set_str(&AttrId::global("birth_date"), &d);
            }
        }
        if let Some(gender) = tok(3) {
            // Contournement des fichiers mal formés : « H » pour homme.
            let g = match gender.chars().next() {
                Some('H') | Some('h') => format!("M{}", &gender[1..]),
                _ => gender.to_string(),
            };
            player.set_str(&AttrId::global("gender"), &g);
        }
        if let Some(country) = tok(4) {
            player.set_str(&AttrId::global("country"), country);
        }

        // Fédération nationale
        if let Some(national) = parts.get(2) {
            let t: Vec<&str> = national.split(',').collect();
            if let Some(licence) = t.first() {
                player.set_str(&AttrId::global("licence"), licence);
            }
            if let Some(club) = t.get(2) {
                player.set_str(&AttrId::global("club"), club);
            }
            match t.get(3) {
                Some(ranking) => player.set_str(&AttrId::global("ranking"), ranking),
                None => player.set_uint(&AttrId::global("ranking"), 0),
            };
            if let Some(cat) = &catalog {
                cat.guess_player_organization(&mut player, "league");
                cat.guess_player_organization(&mut player, "region");
            }
        }
        result.players.push(player, None);
    }
    Ok(result)
}

/// Date `jj/mm/aa[aa]` → `jj.mm.aaaa` (années sur deux chiffres ramenées
/// dans le siècle passé, comme le C++).
pub fn fff_birth_date(date: &str, current_year: i32) -> Option<String> {
    let parts: Vec<&str> = date.split('/').collect();
    if parts.len() < 3 {
        return None;
    }
    let year_txt = parts[2];
    let long_year = if year_txt.len() <= 2 {
        let short = atoi(year_txt) as i32;
        let mut y = short + current_year - current_year % 100;
        if short >= current_year % 100 {
            y -= 100;
        }
        y
    } else {
        atoi(year_txt) as i32
    };
    let day = atoi(parts[0]) as u32;
    let month = atoi(parts[1]) as u32;
    chrono::NaiveDate::from_ymd_opt(long_year, month, day)?;
    Some(format!("{day:02}.{month:02}.{long_year}"))
}

/// `Classification::DumpToFFF` : classement final au format FFF
/// (ISO-8859-1). `refs` est la liste ordonnée du classement ;
/// `place_shifting` décale les places (saisie « place de départ » − 1).
pub fn export(header: &ContestHeader, store: &PlayerStore, refs: &[PlayerRef], place_shifting: u32) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    // Écart : le C++ écrit l'organisateur sans conversion (UTF-8 brut dans un
    // fichier Latin-1) ; ici il est converti comme le reste.
    let first = format!("FFF;WIN;competition;{};individuel\n", header.organizer);
    out.extend(encoding::latin1_encode(&first).unwrap_or_else(|| first.into_bytes()));
    let second = format!(
        "{};{};{};{};{};{}\n",
        header.date_image(),
        header.weapon_image(),
        header.gender,
        header.category_image(),
        header.name,
        header.name
    );
    if let Some(b) = encoding::latin1_encode(&second) {
        out.extend(b);
    }

    for r in refs {
        let Some(p) = store.get(*r) else { continue };
        let status = p.str_attr("global_status");
        if status.map(|s| s.starts_with('E')).unwrap_or(false) {
            continue;
        }
        let mut line: Vec<u8> = Vec::new();
        for (i, a) in ["name", "first_name", "birth_date", "gender", "country"].iter().enumerate() {
            if i > 0 {
                line.push(b',');
            }
            write_field(&mut line, p, a, place_shifting);
        }
        line.extend(b",;");
        line.extend(b",,;");
        for a in ["licence", "league", "club", "ranking"] {
            write_field(&mut line, p, a, place_shifting);
            line.push(b',');
        }
        line.extend(b",;");
        write_field(&mut line, p, "final_rank", place_shifting);
        line.push(b',');
        match status {
            Some(s) if !s.starts_with('Q') => line.push(b'p'),
            _ => line.push(b't'),
        }
        line.push(b'\n');
        out.extend(line);
    }
    out
}

/// `Classification::WriteFFFString`.
fn write_field(out: &mut Vec<u8>, p: &Player, attr: &str, place_shifting: u32) {
    let Some(value) = p.attr(attr) else { return };
    let image = match attr {
        "final_rank" => (value.as_int() + place_shifting as i32).to_string(),
        "birth_date" => match value.as_str().and_then(parse_date) {
            Some(d) => d.format("%d/%m/%Y").to_string(),
            None => value.xml_image(),
        },
        _ => match value {
            Value::Bool(b) => (*b as u32).to_string(),
            v => v.xml_image(),
        },
    };
    if let Some(bytes) = encoding::latin1_encode(&image) {
        // Contournement « CLAS » : espaces insécables → espaces.
        out.extend(bytes.into_iter().map(|b| if b == 0xA0 { b' ' } else { b }));
    }
}
