//! Export du classement au format FFF (port de `Classification::DumpToFFF`).
//!
//! Fichier ISO-8859-1 : deux lignes d'en-tête puis une ligne par tireur
//! (sauf exclus) : `nom,prénom,naissance,sexe,nation,;,,;licence,ligue,club,
//! classement,,;rang,t|p` (`t` : terminé normalement, `p` : statut non `Q`).

use crate::attribute::Value;
use crate::player::{AttrId, Player, PlayerRef, PlayerStore};

/// Données de la compétition écrites dans l'en-tête.
#[derive(Debug, Clone)]
pub struct FffHeader {
    pub organizer: String,
    pub date: String,
    pub weapon: String,
    pub gender_code: String,
    pub category: String,
    pub name: String,
}

fn to_latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| if (c as u32) < 256 { c as u32 as u8 } else { b'?' }).collect()
}

fn fff_string(p: &Player, code: &str, place_shifting: u32) -> String {
    let Some(v) = p.get(&AttrId::global(code)) else { return String::new() };
    let image = match code {
        "final_rank" => (v.as_int() + place_shifting as i32).to_string(),
        "birth_date" => match crate::attribute::parse_date(&v.xml_image()) {
            Some(d) => d.format("%d/%m/%Y").to_string(),
            None => v.xml_image(),
        },
        _ => v.xml_image(),
    };
    // Contournement d'un défaut du logiciel CLAS : pas d'espace insécable.
    image.replace('\u{a0}', " ")
}

/// Produit le contenu du fichier (`DumpToFFF`).
pub fn dump_to_fff(header: &FffHeader, store: &PlayerStore, list: &[PlayerRef], place_shifting: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(to_latin1(&format!("FFF;WIN;competition;{};individuel\n", header.organizer)));
    out.extend(to_latin1(&format!(
        "{};{};{};{};{};{}\n",
        header.date, header.weapon, header.gender_code, header.category, header.name, header.name
    )));
    for r in list {
        let Some(p) = store.get(*r) else { continue };
        let status = p.attr("global_status").map(Value::xml_image);
        if status.as_deref().map(|s| s.starts_with('E')).unwrap_or(false) {
            continue;
        }
        let f = |c: &str| fff_string(p, c, place_shifting);
        let mut line = String::new();
        line.push_str(&format!("{},{},{},{},{},;", f("name"), f("first_name"), f("birth_date"), f("gender"), f("country")));
        line.push_str(",,;");
        line.push_str(&format!("{},{},{},{},,;", f("licence"), f("league"), f("club"), f("ranking")));
        line.push_str(&f("final_rank"));
        line.push(',');
        line.push(if status.as_deref().map(|s| !s.starts_with('Q')).unwrap_or(false) { 'p' } else { 't' });
        line.push('\n');
        out.extend(to_latin1(&line));
    }
    out
}
