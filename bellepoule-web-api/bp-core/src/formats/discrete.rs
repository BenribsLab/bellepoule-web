//! Valeurs discrètes chargées depuis `resources/` (port de
//! `AttributeDesc::AddDiscreteValueSelector`, `AddLocalizedDiscreteValues`,
//! `AddDiscreteValues (dir, selector)` et `Checkin::GuessPlayerOrganization`).
//!
//! Format des fichiers `index.txt` : une ligne par valeur,
//! `code XML;nom court;nom long;codes internes FFE séparés par '-'`.
//! Les champs vides reprennent le précédent (nom court ← code XML,
//! nom long ← nom court). Le nom long est traduit si le dossier contient un
//! sous-dossier `translations/` (domaine gettext = nom du dossier, ex.
//! `countries`). L'icône éventuelle est `<dossier>/<code XML échappé>.png`.
//!
//! Écart assumé : le C++ stocke les textes avec des espaces insécables
//! (`GetUndivadableText`) ; ici les textes sont gardés tels quels et toutes
//! les recherches sont insensibles à la différence espace / espace insécable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::attribute::{self, AttributeDesc, DiscreteSource, Look, Value};
use crate::formats::encoding::{same_text, undivadable, uri_escape};
use crate::i18n::{parse_po, tr};
use crate::player::{AttrId, Player};

/// Une valeur discrète issue des ressources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscreteEntry {
    /// Code XML (stocké dans les fichiers).
    pub xml: String,
    /// Nom court (`SHORT_TEXT`).
    pub short_text: String,
    /// Nom long non traduit.
    pub long_msgid: String,
    /// Nom long traduit (`LONG_TEXT`).
    pub long_text: String,
    /// Codes internes de la fédération française (ligues, régions).
    pub codes: Vec<u32>,
    /// Icône (drapeau, logo de ligue…), si le fichier existe.
    pub icon: Option<PathBuf>,
    /// Pays de rattachement (`DISCRETE_SELECTOR_str`, ex. `fra`), vide pour
    /// les pays eux-mêmes.
    pub selector: String,
}

/// Liste de valeurs d'un attribut.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscreteList {
    pub entries: Vec<DiscreteEntry>,
    /// `_is_selector` : la liste sert de sélecteur (pays).
    pub is_selector: bool,
    /// `_has_selector` : les valeurs sont filtrées par pays.
    pub has_selector: bool,
}

impl DiscreteList {
    // Les index C++ (`TreeModelIndex`) écrasent les clés en double : la
    // dernière valeur insérée l'emporte, d'où les recherches en partant de la fin.

    pub fn from_xml(&self, xml: &str) -> Option<&DiscreteEntry> {
        self.entries.iter().rev().find(|e| same_text(&e.xml, xml))
    }

    pub fn from_code(&self, code: u32) -> Option<&DiscreteEntry> {
        self.entries.iter().rev().find(|e| e.codes.contains(&code))
    }

    pub fn from_long_text(&self, text: &str) -> Option<&DiscreteEntry> {
        self.entries.iter().rev().find(|e| same_text(&e.long_text, text))
    }

    pub fn from_short_text(&self, text: &str) -> Option<&DiscreteEntry> {
        self.entries.iter().rev().find(|e| same_text(&e.short_text, text))
    }

    /// `DiscreteFilterForCombobox` : valeurs d'un pays (comparaison ASCII
    /// insensible à la casse avec le code XML du pays sélectionné).
    pub fn for_selector<'a>(&'a self, country: &'a str) -> impl Iterator<Item = &'a DiscreteEntry> + 'a {
        self.entries.iter().filter(move |e| e.selector.eq_ignore_ascii_case(country))
    }

    /// `AttributeDesc::AddDiscreteValues (dir, selector)` : ajoute le contenu
    /// de `dir/index.txt`. Un fichier absent est ignoré silencieusement.
    pub fn add_from_dir(&mut self, dir: &Path, selector: &str, lang: &str) {
        let Ok(raw) = std::fs::read(dir.join("index.txt")) else { return };
        let text = String::from_utf8_lossy(&raw).replace('\r', "");
        let first_line = text.lines().next().unwrap_or("");
        let nb_tokens = first_line.matches(';').count() + 1;
        if nb_tokens != 4 {
            return;
        }
        let translations = load_translations(dir, lang);
        for line in text.split('\n') {
            let fields: Vec<&str> = line.split(';').collect();
            if fields.is_empty() || fields[0].is_empty() {
                break;
            }
            let get = |i: usize| fields.get(i).copied().unwrap_or("");
            let xml = fields[0].to_string();
            let short_text = if get(1).is_empty() { xml.clone() } else { get(1).to_string() };
            let long_msgid = if get(2).is_empty() { short_text.clone() } else { get(2).to_string() };
            let long_text = if get(2).is_empty() {
                short_text.clone()
            } else {
                match &translations {
                    Some(t) => t.get(get(2)).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| get(2).to_string()),
                    None => get(2).to_string(),
                }
            };
            let codes = if get(3).is_empty() {
                Vec::new()
            } else {
                get(3).split('-').map(|c| crate::formats::encoding::atoi(c).max(0) as u32).collect()
            };
            let icon_path = dir.join(format!("{}.png", uri_escape(&xml)));
            let icon = icon_path.is_file().then_some(icon_path);
            self.entries.push(DiscreteEntry { xml, short_text, long_msgid, long_text, codes, icon, selector: selector.to_string() });
        }
    }
}

/// Catalogue `.po` d'un dossier de ressources (`dgettext (domaine, …)`).
fn load_translations(dir: &Path, lang: &str) -> Option<HashMap<String, String>> {
    let tdir = dir.join("translations");
    if !tdir.is_dir() || lang.is_empty() || lang == "en" {
        return None;
    }
    let short = lang.split(['_', '.', '@']).next().unwrap_or(lang);
    for candidate in [lang, short] {
        if let Ok(text) = std::fs::read_to_string(tdir.join(format!("{candidate}.po"))) {
            return Some(parse_po(&text));
        }
    }
    None
}

/// Valeur discrète unifiée (statique du registre ou issue des ressources).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscreteItem {
    /// Code numérique (`DISCRETE_CODE_uint`) : premier caractère du code XML
    /// pour les valeurs statiques, premier code FFE pour les ressources.
    pub code: Option<u32>,
    pub xml: String,
    pub short_text: String,
    /// Libellé long traduit.
    pub long_text: String,
    pub icon: Option<PathBuf>,
    pub selector: String,
}

/// Ensemble des listes chargées depuis `resources/`.
#[derive(Debug, Clone, Default)]
pub struct DiscreteCatalog {
    resources_dir: PathBuf,
    lists: HashMap<String, DiscreteList>,
}

impl DiscreteCatalog {
    /// Charge toutes les sources déclarées dans le registre d'attributs
    /// (`country` → `resources/countries`, `region`/`league`/`club` →
    /// `resources/localized_data/<pays>/<nom>`). `lang` sélectionne les
    /// traductions des noms longs (`fr`, `de`…).
    pub fn load(resources_dir: &Path, lang: &str) -> Self {
        let mut lists = HashMap::new();
        for desc in attribute::registry().all() {
            let Some(source) = &desc.discrete_source else { continue };
            let mut list = DiscreteList::default();
            match source {
                DiscreteSource::Selector(name) => {
                    list.is_selector = true;
                    list.add_from_dir(&resources_dir.join(name), "", lang);
                }
                DiscreteSource::Localized(name) => {
                    list.has_selector = true;
                    let base = resources_dir.join("localized_data");
                    let mut countries: Vec<String> = std::fs::read_dir(&base)
                        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                        .unwrap_or_default();
                    countries.sort();
                    for country in countries {
                        list.add_from_dir(&base.join(&country).join(name), &country, lang);
                    }
                }
            }
            lists.insert(desc.code_name.to_string(), list);
        }
        DiscreteCatalog { resources_dir: resources_dir.to_path_buf(), lists }
    }

    pub fn resources_dir(&self) -> &Path {
        &self.resources_dir
    }

    pub fn list(&self, code_name: &str) -> Option<&DiscreteList> {
        self.lists.get(code_name)
    }

    /// Toutes les valeurs d'un attribut (statiques ou issues des ressources),
    /// dans l'ordre du modèle C++.
    pub fn values(&self, code_name: &str) -> Vec<DiscreteItem> {
        if let Some(list) = self.lists.get(code_name) {
            return list
                .entries
                .iter()
                .map(|e| DiscreteItem {
                    code: e.codes.first().copied(),
                    xml: e.xml.clone(),
                    short_text: e.short_text.clone(),
                    long_text: e.long_text.clone(),
                    icon: e.icon.clone(),
                    selector: e.selector.clone(),
                })
                .collect();
        }
        let Some(desc) = attribute::registry().by_code(code_name) else { return Vec::new() };
        desc.discrete
            .iter()
            .map(|d| DiscreteItem {
                code: d.xml.chars().next().map(|c| c as u32),
                xml: d.xml.clone(),
                short_text: d.xml.clone(),
                long_text: tr(&d.user),
                icon: d.icon.as_ref().map(|i| self.resources_dir.join(i)),
                selector: String::new(),
            })
            .collect()
    }

    /// Valeurs d'un attribut localisé pour un pays donné.
    pub fn values_for_country(&self, code_name: &str, country: &str) -> Vec<DiscreteItem> {
        self.values(code_name).into_iter().filter(|v| v.selector.eq_ignore_ascii_case(country)).collect()
    }

    /// `AttributeDesc::GetUserImage (xml, look)` : image affichée d'une valeur.
    /// Sans valeur discrète correspondante : la valeur elle-même.
    pub fn user_image(&self, code_name: &str, xml: &str, look: Look) -> String {
        if let Some(list) = self.lists.get(code_name) {
            if let Some(e) = list.from_xml(xml) {
                return match look {
                    Look::ShortText => e.short_text.clone(),
                    _ => e.long_text.clone(),
                };
            }
            return xml.to_string();
        }
        match attribute::registry().by_code(code_name) {
            Some(desc) => desc.user_image(xml, look),
            None => xml.to_string(),
        }
    }

    /// `AttributeDesc::GetDiscreteXmlImage (user_image)` : code XML d'un nom long.
    pub fn xml_image_from_user(&self, code_name: &str, user: &str) -> String {
        if let Some(list) = self.lists.get(code_name) {
            return list.from_long_text(user).map(|e| e.xml.clone()).unwrap_or_else(|| user.to_string());
        }
        match attribute::registry().by_code(code_name) {
            Some(desc) => desc.xml_image_from_user(user),
            None => user.to_string(),
        }
    }

    /// `AttributeDesc::GetDiscreteUserImage (code)` : nom long d'un code FFE.
    pub fn user_image_from_code(&self, code_name: &str, code: u32) -> Option<String> {
        self.lists.get(code_name)?.from_code(code).map(|e| e.long_text.clone())
    }

    /// Icône d'une valeur (`GetDiscretePixbuf`).
    pub fn icon(&self, code_name: &str, xml: &str) -> Option<PathBuf> {
        self.values(code_name).into_iter().rev().find(|v| same_text(&v.xml, xml)).and_then(|v| v.icon)
    }

    /// `Checkin::GuessPlayerOrganization` : pour un licencié français
    /// (licence de 12 caractères), la ligue ou la région est déduite des deux
    /// premiers chiffres de la licence. Comme en C++, l'attribut est effacé si
    /// le code est inconnu.
    pub fn guess_player_organization(&self, player: &mut Player, organization: &str) {
        let Some(country) = player.str_attr("country") else { return };
        if !country.eq_ignore_ascii_case("FRA") {
            return;
        }
        let Some(licence) = player.str_attr("licence") else { return };
        if licence.len() != 12 {
            return;
        }
        let prefix = crate::formats::encoding::truncate_bytes(licence, 2);
        let code = crate::formats::encoding::atoi(prefix).max(0) as u32;
        let id = AttrId::global(organization);
        match self.user_image_from_code(organization, code) {
            Some(name) => {
                player.set(&id, Value::Str(name));
            }
            None => player.remove(&id),
        }
    }
}

static CATALOG: RwLock<Option<Arc<DiscreteCatalog>>> = RwLock::new(None);

/// Charge le catalogue global (à appeler au démarrage, puis à chaque
/// changement de langue).
pub fn init(resources_dir: &Path, lang: &str) -> Arc<DiscreteCatalog> {
    let cat = Arc::new(DiscreteCatalog::load(resources_dir, lang));
    set_catalog(cat.clone());
    cat
}

pub fn set_catalog(cat: Arc<DiscreteCatalog>) {
    *CATALOG.write().unwrap() = Some(cat);
}

/// Catalogue global, s'il a été chargé.
pub fn catalog() -> Option<Arc<DiscreteCatalog>> {
    CATALOG.read().ok().and_then(|g| g.clone())
}

/// Image utilisateur longue d'un attribut d'un joueur
/// (`Attribute::GetUserImage (LONG_TEXT)`), utilisée par les exports.
pub fn attribute_user_image(desc: &AttributeDesc, value: &Value, look: Look) -> String {
    match value {
        Value::Str(s) => {
            if desc.has_discrete_value() {
                match catalog() {
                    Some(cat) => cat.user_image(desc.code_name, s, look),
                    None => desc.user_image(s, look),
                }
            } else {
                s.clone()
            }
        }
        Value::UInt(v) => v.to_string(),
        Value::Int(v) => v.to_string(),
        Value::Bool(b) => (*b as u32).to_string(),
    }
}

/// Image « insécable » (`GetUndivadableText`) d'une valeur, utile aux clés.
pub fn undivadable_image(value: &Value) -> String {
    undivadable(&value.xml_image())
}
