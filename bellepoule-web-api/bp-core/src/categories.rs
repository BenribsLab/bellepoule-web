//! Catégories d'âge (« catégorie générale ») : calculées à partir de l'année
//! de naissance, jamais saisies.
//!
//! Un modèle décrit chaque catégorie par l'âge atteint pendant l'année où
//! commence la saison (âge = année de début de saison − année de naissance),
//! ce qui le garde juste d'une saison à l'autre. La saison bascule à une date
//! réglable (le 1er septembre en escrime).
//!
//! Modèle fourni : catégories de la FFE (règlement sportif 2025-2026) —
//! pour la saison 2025-2026, M11 = nés en 2015 et 2016, seniors = 2005 et
//! avant, V1 = 1977 à 1986…
//!
//! Garde-fou (jamais bloquant) : par rapport à la catégorie de la compétition,
//! un tireur peut être surclassé (une catégorie plus jeune), surclassé deux
//! fois, ou paraître en erreur (trop âgé pour une compétition de jeunes, trop
//! jeune de plus de deux catégories, date de naissance inconnue).

use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// Une catégorie : âges en début de saison, du plus jeune au plus âgé.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgeCategory {
    pub name: String,
    pub min_age: u32,
    /// Sans limite haute : « et avant » (seniors, dernier vétéran).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u32>,
}

impl AgeCategory {
    fn new(name: &str, min_age: u32, max_age: Option<u32>) -> Self {
        AgeCategory { name: name.into(), min_age, max_age }
    }

    /// Années de naissance pour une saison qui commence en `season` :
    /// (la plus ancienne, sans limite si `None` ; la plus récente).
    pub fn birth_years(&self, season: i32) -> (Option<i32>, i32) {
        (self.max_age.map(|a| season - a as i32), season - self.min_age as i32)
    }

    /// Âges correspondant à des années de naissance (la plus ancienne en
    /// premier, `None` : « et avant »).
    pub fn from_birth_years(name: &str, season: i32, oldest: Option<i32>, newest: i32) -> Self {
        AgeCategory {
            name: name.into(),
            min_age: (season - newest).max(0) as u32,
            max_age: oldest.map(|y| (season - y).max(0) as u32),
        }
    }

    fn contains(&self, age: i32) -> bool {
        age >= self.min_age as i32 && self.max_age.is_none_or(|m| age <= m as i32)
    }
}

/// Modèle de catégories, avec la date de bascule de la saison.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryModel {
    pub name: String,
    /// Bascule de saison : mois (1-12) et jour.
    pub switch_month: u32,
    pub switch_day: u32,
    /// Du plus jeune au plus âgé ; les catégories adultes (sans limite haute
    /// ou à partir de 20 ans) acceptent les plus âgés.
    pub categories: Vec<AgeCategory>,
}

impl CategoryModel {
    /// Catégories de la FFE (règlement sportif 2025-2026).
    pub fn ffe() -> Self {
        CategoryModel {
            name: "FFE".into(),
            switch_month: 9,
            switch_day: 1,
            categories: vec![
                AgeCategory::new("M5", 3, Some(4)),
                AgeCategory::new("M7", 5, Some(6)),
                AgeCategory::new("M9", 7, Some(8)),
                AgeCategory::new("M11", 9, Some(10)),
                AgeCategory::new("M13", 11, Some(12)),
                AgeCategory::new("M15", 13, Some(14)),
                AgeCategory::new("M17", 15, Some(16)),
                AgeCategory::new("M20", 17, Some(19)),
                AgeCategory::new("Seniors", 20, Some(38)),
                AgeCategory::new("V1", 39, Some(48)),
                AgeCategory::new("V2", 49, Some(58)),
                AgeCategory::new("V3", 59, Some(68)),
                AgeCategory::new("V4", 69, None),
            ],
        }
    }

    /// Année de début de la saison en cours à la date donnée.
    pub fn season(&self, (year, month, day): (i32, u32, u32)) -> i32 {
        if (month, day) >= (self.switch_month, self.switch_day) { year } else { year - 1 }
    }

    /// Catégorie (indice) d'une année de naissance pour une saison.
    pub fn index_of(&self, birth_year: i32, season: i32) -> Option<usize> {
        let age = season - birth_year;
        self.categories.iter().position(|c| c.contains(age))
    }

    /// Indice d'une catégorie par son nom ou son code : casse et accents
    /// ignorés, « U15 » = « M15 », « S » = « Seniors », « VET » ou
    /// « Vétérans » = premier vétéran, « Vétérans 1 » = « V1 ».
    pub fn find(&self, name: &str) -> Option<usize> {
        let wanted = category_key(name);
        if wanted.is_empty() {
            return None;
        }
        self.categories.iter().position(|c| category_key(&c.name) == wanted)
    }

    /// Une catégorie adulte accepte les tireurs plus âgés (vétérans en
    /// seniors…).
    fn is_adult(&self, index: usize) -> bool {
        self.categories.get(index).is_some_and(|c| c.min_age >= 20 || c.max_age.is_none())
    }
}

impl Default for CategoryModel {
    fn default() -> Self {
        CategoryModel::ffe()
    }
}

/// Place d'un tireur par rapport à la catégorie de la compétition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Dans la catégorie (ou plus âgé dans une catégorie adulte).
    Normal,
    /// Une catégorie plus jeune.
    Upgraded,
    /// Deux catégories plus jeunes.
    DoubleUpgraded,
    /// À vérifier : trop jeune, trop âgé pour une compétition de jeunes, ou
    /// catégorie inconnue.
    Check,
}

impl CategoryModel {
    /// Garde-fou : `fencer` et `contest` sont des indices de catégories.
    pub fn placement(&self, fencer: Option<usize>, contest: usize) -> Placement {
        let Some(fencer) = fencer else { return Placement::Check };
        match contest as i64 - fencer as i64 {
            0 => Placement::Normal,
            1 => Placement::Upgraded,
            2 => Placement::DoubleUpgraded,
            d if d < 0 && self.is_adult(contest) && self.is_adult(fencer) => Placement::Normal,
            _ => Placement::Check,
        }
    }
}

/// Forme comparable d'un nom de catégorie (voir [`CategoryModel::find`]).
fn category_key(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            c => c,
        })
        .filter(|c| c.is_alphanumeric())
        .collect();
    let s = s.replace("veterans", "v").replace("veteran", "v").replace("vet", "v");
    if s == "v" {
        return "v1".into();
    }
    if s == "s" || s == "senior" || s == "seniors" {
        return "senior".into();
    }
    if let Some(rest) = s.strip_prefix('u').filter(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit())) {
        return format!("m{rest}");
    }
    s
}

/// Code de catégorie d'une compétition (« M13 », « S », « V1 »…) pour un nom
/// de catégorie générale ; un nom inconnu reste tel quel (texte libre).
pub fn contest_code(name: &str) -> String {
    let key = category_key(name);
    match key.as_str() {
        "senior" => "S".into(),
        k if k.starts_with('m') || k.starts_with('v') => k.to_uppercase(),
        _ => name.trim().to_string(),
    }
}

/// Année d'une date de naissance (premier nombre de quatre chiffres :
/// « 12.03.2014 », « 2014-03-12 »…).
pub fn birth_year(date: Option<&str>) -> Option<i32> {
    let date = date?;
    let digits: Vec<char> = date.chars().collect();
    let mut i = 0;
    while i < digits.len() {
        if digits[i].is_ascii_digit() {
            let start = i;
            while i < digits.len() && digits[i].is_ascii_digit() {
                i += 1;
            }
            if i - start == 4 {
                return digits[start..i].iter().collect::<String>().parse().ok();
            }
        } else {
            i += 1;
        }
    }
    None
}

// ------------------------------------------------------------ modèle actif

static ACTIVE: RwLock<Option<CategoryModel>> = RwLock::new(None);

/// Modèle utilisé pour calculer les catégories (FFE par défaut).
pub fn active() -> CategoryModel {
    ACTIVE.read().ok().and_then(|m| m.clone()).unwrap_or_default()
}

/// Change le modèle utilisé (préférences).
pub fn set_active(model: CategoryModel) {
    if let Ok(mut m) = ACTIVE.write() {
        *m = Some(model);
    }
}

/// Date du jour (année, mois, jour), heure locale.
pub fn today() -> (i32, u32, u32) {
    use chrono::Datelike;
    let d = chrono::Local::now().date_naive();
    (d.year(), d.month(), d.day())
}

/// Catégorie générale d'une date de naissance, avec le modèle actif et la
/// saison en cours.
pub fn general_category(birth_date: Option<&str>) -> Option<String> {
    let model = active();
    let season = model.season(today());
    let year = birth_year(birth_date)?;
    model.index_of(year, season).map(|i| model.categories[i].name.clone())
}

/// Texte de la colonne « Catégorie générale » avec le garde-fou par rapport
/// à la catégorie de la compétition : « M13 ▲ » (surclassé), « M11 ▲▲ »
/// (double surclassement), « ⚠ M17 » ou « ⚠ ? » (à vérifier).
pub fn general_category_label(birth_date: Option<&str>, contest_category: &str) -> String {
    let model = active();
    let season = model.season(today());
    let fencer = birth_year(birth_date).and_then(|y| model.index_of(y, season));
    let name = fencer.map(|i| model.categories[i].name.clone()).unwrap_or_else(|| "?".into());
    match model.find(contest_category) {
        None => fencer.map(|_| name).unwrap_or_default(),
        Some(contest) => match model.placement(fencer, contest) {
            Placement::Normal => name,
            Placement::Upgraded => format!("{name} ▲"),
            Placement::DoubleUpgraded => format!("{name} ▲▲"),
            Placement::Check => format!("⚠ {name}"),
        },
    }
}

// ----------------------------------------------------- modèles enregistrés

/// Fichier des modèles enregistrés : `{ "active": nom, "models": [...] }`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryStore {
    #[serde(default)]
    pub active: String,
    #[serde(default)]
    pub models: Vec<CategoryModel>,
}

impl CategoryStore {
    /// Relit le fichier (le modèle FFE est toujours proposé).
    pub fn load(path: &std::path::Path) -> Self {
        let mut store: CategoryStore = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if !store.models.iter().any(|m| m.name == "FFE") {
            store.models.insert(0, CategoryModel::ffe());
        }
        if store.active.is_empty() {
            store.active = "FFE".into();
        }
        store
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }

    /// Le modèle actif (FFE s'il a disparu).
    pub fn current(&self) -> CategoryModel {
        self.models.iter().find(|m| m.name == self.active).cloned().unwrap_or_default()
    }

    /// Enregistre (ou remplace) un modèle et le rend actif.
    pub fn put(&mut self, model: CategoryModel) {
        self.active = model.name.clone();
        match self.models.iter_mut().find(|m| m.name == model.name) {
            Some(m) => *m = model,
            None => self.models.push(model),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffe_model_matches_the_2025_2026_rules() {
        let m = CategoryModel::ffe();
        let season = m.season((2025, 10, 23));
        assert_eq!(season, 2025);
        let name = |y| m.index_of(y, season).map(|i| m.categories[i].name.clone()).unwrap();
        assert_eq!(name(2016), "M11");
        assert_eq!(name(2015), "M11");
        assert_eq!(name(2014), "M13");
        assert_eq!(name(2012), "M15");
        assert_eq!(name(2010), "M17");
        assert_eq!(name(2006), "M20");
        assert_eq!(name(2005), "Seniors");
        assert_eq!(name(1987), "Seniors");
        assert_eq!(name(1986), "V1");
        assert_eq!(name(1977), "V1");
        assert_eq!(name(1967), "V2");
        assert_eq!(name(1957), "V3");
        assert_eq!(name(1956), "V4");
        assert_eq!(m.categories[3].birth_years(2025), (Some(2015), 2016));
    }

    #[test]
    fn season_switches_on_the_first_of_september() {
        let m = CategoryModel::ffe();
        assert_eq!(m.season((2026, 8, 31)), 2025);
        assert_eq!(m.season((2026, 9, 1)), 2026);
        // En 2026-2027, les nés en 2016 passent M13 ? Non : M11 = 2016-2017.
        let i = m.index_of(2017, 2026).unwrap();
        assert_eq!(m.categories[i].name, "M11");
    }

    #[test]
    fn placement_guard() {
        let m = CategoryModel::ffe();
        let m15 = m.find("M15").unwrap();
        assert_eq!(m.placement(m.find("M15"), m15), Placement::Normal);
        assert_eq!(m.placement(m.find("M13"), m15), Placement::Upgraded);
        assert_eq!(m.placement(m.find("M11"), m15), Placement::DoubleUpgraded);
        assert_eq!(m.placement(m.find("M9"), m15), Placement::Check);
        assert_eq!(m.placement(m.find("M17"), m15), Placement::Check, "trop âgé pour M15");
        assert_eq!(m.placement(None, m15), Placement::Check);
        assert_eq!(m.find("U15"), Some(m15));
        assert_eq!(m.find("S"), m.find("Seniors"));
        assert_eq!(m.find("VET"), m.find("V1"));
        let seniors = m.find("Senior").unwrap();
        assert_eq!(m.placement(m.find("Vétérans 2"), seniors), Placement::Normal, "vétéran en seniors");
        assert_eq!(m.placement(m.find("M20"), seniors), Placement::Upgraded);
    }

    #[test]
    fn birth_years_round_trip() {
        let c = AgeCategory::from_birth_years("M11", 2025, Some(2015), 2016);
        assert_eq!((c.min_age, c.max_age), (9, Some(10)));
        let c = AgeCategory::from_birth_years("Seniors", 2025, None, 2005);
        assert_eq!((c.min_age, c.max_age), (20, None));
    }
}
