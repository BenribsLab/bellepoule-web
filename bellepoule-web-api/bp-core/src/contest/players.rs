//! Outils communs sur les joueurs : création avec les valeurs par défaut du
//! C++ (`Player::Player`, `Referee::Referee`), comparaisons fidèles à
//! `Player::Compare`, données localisées (ligues/régions) et dossier des
//! ressources.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::attribute::{self, Value};
use crate::player::{AttrId, Player, PlayerClass, PlayerRef, PlayerStore};

use super::grand;

/// Crée un joueur avec les attributs par défaut du constructeur C++ :
/// `name="???"`, `attending=0`, `exported=0`, `global_status="F"` et, pour
/// un arbitre, `connection="Manual"`.
pub fn new_player(class: PlayerClass) -> Player {
    let mut p = Player::new(class);
    p.set_str(&AttrId::global("name"), "???");
    p.set(&AttrId::global("attending"), Value::Bool(false));
    p.set(&AttrId::global("exported"), Value::Bool(false));
    p.set_str(&AttrId::global("global_status"), "F");
    if class == PlayerClass::Referee {
        p.set_str(&AttrId::global("connection"), "Manual");
    }
    p
}

/// Charge un joueur depuis un nœud XML, avec les attributs inconnus
/// déclarés par la compétition (`extra` : noms XML).
pub fn load_player(class: PlayerClass, node: roxmltree::Node, extra: &[String]) -> Player {
    let mut p = new_player(class);
    p.load(node);
    for name in extra {
        if let Some(v) = node.attribute(name.as_str()) {
            p.set(&AttrId::global(name), Value::Str(v.to_string()));
        }
    }
    if class == PlayerClass::Team {
        // `Team::Load` : sans nom, l'identifiant (texte) sert de nom.
        let unnamed = p.str_attr("name").map(|n| n == "???").unwrap_or(true);
        if unnamed {
            if let Some(id) = node.attribute("ID") {
                p.set_str(&AttrId::global("name"), id);
            }
        }
    }
    p
}

/// Écrit les attributs d'un joueur (registre puis attributs inconnus).
pub fn write_player_attributes(p: &Player, w: &mut crate::xml::XmlWriter, extra: &[String], full_profile: bool) {
    p.write_attributes(w, full_profile);
    for name in extra {
        if let Some(v) = p.attr(name) {
            w.attr(name, &v.xml_image());
        }
    }
}

/// Comparaison de deux valeurs d'attribut (`Attribute::Compare`) : les
/// textes sont comparés octet par octet (`g_strcmp0`), un attribut absent
/// passe après.
pub fn compare_values(code: &str, a: Option<&Value>, b: Option<&Value>) -> Ordering {
    match attribute::registry().by_code(code) {
        Some(desc) if desc.compare != attribute::CompareKind::Default => attribute::compare_values(desc, a, b),
        _ => match (a, b) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(Value::Str(x)), Some(Value::Str(y))) => x.as_bytes().cmp(y.as_bytes()),
            (Some(x), Some(y)) => x.as_int().cmp(&y.as_int()),
        },
    }
}

/// `Player::Compare` : valeur, puis tirage aléatoire reproductible si
/// `rand_seed` est non nul et les valeurs égales.
pub fn compare_players(a: &Player, b: &Player, id: &AttrId, rand_seed: u32) -> Ordering {
    let o = compare_values(&id.name, a.get(id), b.get(id));
    if o == Ordering::Equal && rand_seed != 0 {
        grand::random_compare(a.reference, b.reference, rand_seed)
    } else {
        o
    }
}

/// `Player::MultiCompare` : critères successifs, aléa sur le premier.
pub fn multi_compare(a: &Player, b: &Player, ids: &[AttrId], rand_seed: u32) -> Ordering {
    for id in ids {
        let o = compare_values(&id.name, a.get(id), b.get(id));
        if o != Ordering::Equal {
            return o;
        }
    }
    if rand_seed != 0 {
        grand::random_compare(a.reference, b.reference, rand_seed)
    } else {
        Ordering::Equal
    }
}

/// Tri stable de références selon `compare_players`.
pub fn sort_refs(store: &PlayerStore, refs: &mut [PlayerRef], id: &AttrId, rand_seed: u32) {
    crate::sort::glib_sort_by(refs, |x, y| match (store.get(*x), store.get(*y)) {
        (Some(a), Some(b)) => compare_players(a, b, id, rand_seed),
        _ => Ordering::Equal,
    });
}

/// Retire les espaces insécables (le C++ les utilise pour les textes).
pub fn divisable(s: &str) -> String {
    s.replace('\u{a0}', " ")
}

// --------------------------------------------------------------------------------
// Ressources

static RESOURCES_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Fixe le dossier `resources/` (sinon : variable `BELLEPOULE_RESOURCES`,
/// dossier de l'exécutable, dossiers parents du répertoire courant, puis
/// celui du dépôt).
pub fn set_resources_dir(dir: PathBuf) {
    *RESOURCES_DIR.write().unwrap() = Some(dir);
}

pub fn resources_dir() -> PathBuf {
    if let Some(d) = RESOURCES_DIR.read().unwrap().clone() {
        return d;
    }
    if let Some(d) = std::env::var_os("BELLEPOULE_RESOURCES") {
        return PathBuf::from(d);
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("resources"));
        }
    }
    if let Ok(mut cwd) = std::env::current_dir() {
        loop {
            candidates.push(cwd.join("resources"));
            if !cwd.pop() {
                break;
            }
        }
    }
    candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources")));
    candidates
        .into_iter()
        .find(|c| c.join("localized_data").is_dir())
        .unwrap_or_else(|| PathBuf::from("resources"))
}

/// Valeur discrète localisée (`index.txt` : `XML;court;long;codes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedValue {
    /// Dossier du pays (`fra`, `gbr`…).
    pub selector: String,
    pub xml: String,
    pub short_text: String,
    pub long_text: String,
    /// Codes internes (fédération française) séparés par `-` dans le fichier.
    pub codes: Vec<u32>,
}

/// Valeurs localisées d'un type (`ligues`, `regions`, `clubs`), tous pays
/// confondus (`AttributeDesc::AddLocalizedDiscreteValues`).
pub fn localized_values(name: &str) -> &'static [LocalizedValue] {
    static CACHE: OnceLock<RwLock<HashMap<String, &'static [LocalizedValue]>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(v) = cache.read().unwrap().get(name) {
        return v;
    }
    let mut values = Vec::new();
    let root = resources_dir().join("localized_data");
    let mut countries: Vec<_> = std::fs::read_dir(&root)
        .map(|it| it.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect())
        .unwrap_or_default();
    countries.sort();
    for country in countries {
        let index = root.join(&country).join(name).join("index.txt");
        let Ok(bytes) = std::fs::read(&index) else { continue };
        let text = String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|b| *b as char).collect());
        let first = text.lines().next().unwrap_or("");
        if first.matches(';').count() != 3 {
            continue;
        }
        for line in text.lines() {
            let t: Vec<&str> = line.trim_end_matches('\r').split(';').collect();
            if t.len() < 4 || t[0].is_empty() {
                continue;
            }
            let xml = t[0].to_string();
            let short_text = if t[1].is_empty() { xml.clone() } else { t[1].to_string() };
            let long_text = if t[2].is_empty() { short_text.clone() } else { t[2].to_string() };
            let codes = t[3]
                .split('-')
                .filter(|s| !s.is_empty())
                .map(|s| attribute::parse_leading_int(s).max(0) as u32)
                .collect();
            values.push(LocalizedValue { selector: country.clone(), xml, short_text, long_text, codes });
        }
    }
    let leaked: &'static [LocalizedValue] = Box::leak(values.into_boxed_slice());
    cache.write().unwrap().insert(name.to_string(), leaked);
    leaked
}

/// `GetDiscreteUserImage (code)` pour une organisation (`league` → ligues,
/// `region` → regions).
pub fn organization_from_code(organization: &str, code: u32) -> Option<String> {
    let dir = match organization {
        "league" => "ligues",
        "region" => "regions",
        "club" => "clubs",
        other => other,
    };
    localized_values(dir).iter().find(|v| v.codes.contains(&code)).map(|v| v.long_text.clone())
}

/// `Checkin::GuessPlayerOrganization` : pour un tireur français, les deux
/// premiers chiffres d'une licence de 12 caractères donnent la ligue et la
/// région.
pub fn guess_player_organization(p: &mut Player, organization: &str) {
    let french = p.str_attr("country").map(|c| c.eq_ignore_ascii_case("FRA")).unwrap_or(false);
    if !french {
        return;
    }
    let Some(licence) = p.attr("licence").map(Value::xml_image) else { return };
    if licence.len() != 12 {
        return;
    }
    let code = attribute::parse_leading_int(&licence[..2]).max(0) as u32;
    if let Some(name) = organization_from_code(organization, code) {
        p.set_str(&AttrId::global(organization), &name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let p = new_player(PlayerClass::Referee);
        assert_eq!(p.str_attr("global_status"), Some("F"));
        assert_eq!(p.str_attr("connection"), Some("Manual"));
        assert!(!p.attending());
    }

    #[test]
    fn organization() {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("country"), "FRA");
        p.set_str(&AttrId::global("licence"), "270750360766");
        guess_player_organization(&mut p, "league");
        guess_player_organization(&mut p, "region");
        assert_eq!(p.str_attr("league"), Some("PARIS"));
        assert_eq!(p.str_attr("region"), Some("ILE DE FRANCE"));
    }
}
