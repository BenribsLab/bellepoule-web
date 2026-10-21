//! Sélection et ordre des colonnes affichées (port de `common/util/filter.cpp`).
//!
//! Un filtre part d'une liste d'attributs disponibles (`attr_list`). Le
//! « magasin » (`entries`) reproduit la `GtkListStore` du C++ : chaque ligne
//! porte la visibilité et l'aspect (`Look`) d'un attribut, et son ordre fixe
//! l'ordre des colonnes. La liste sélectionnée (`layout`) est la liste des
//! colonnes effectivement affichées.
//!
//! Persistance :
//! - dans les fichiers : attribut `Affichage<suffixe>` = noms XML séparés par
//!   des virgules (`Save`/`Load`) ;
//! - dans les préférences : groupe `<nom du filtre>`, clé `display`
//!   (`RestoreLast`/`UpdateAttrList`).

use crate::attribute::{registry, Look};
use crate::tournament::user_config::UserConfig;
use crate::xml::XmlWriter;

/// Colonne affichée (`Filter::Layout`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Nom de code de l'attribut.
    pub code_name: String,
    pub look: Look,
}

/// Ligne du magasin d'attributs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterEntry {
    pub code_name: String,
    pub visible: bool,
    pub look: Look,
}

/// Libellés des aspects (msgid).
pub const LOOK_IMAGES: [&str; 3] = ["Long text", "Short text", "Graphic"];

pub fn look_image(look: Look) -> &'static str {
    match look {
        Look::LongText => LOOK_IMAGES[0],
        Look::ShortText => LOOK_IMAGES[1],
        Look::Graphical => LOOK_IMAGES[2],
    }
}

fn favorite_look(code: &str) -> Look {
    registry().by_code(code).map(|d| d.favorite_look).unwrap_or(Look::LongText)
}

fn xml_name(code: &str) -> String {
    registry().by_code(code).map(|d| d.xml_name.to_string()).unwrap_or_else(|| code.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    name: String,
    attr_list: Vec<String>,
    entries: Vec<FilterEntry>,
    selected: Vec<Layout>,
    /// `Filter::PreventPersistence` (Marshaller) : pas d'écriture des préférences.
    no_persistence: bool,
}

impl Filter {
    /// `Filter (name, attr_list)` : tous les attributs sont invisibles.
    pub fn new(name: &str, attr_list: Vec<String>) -> Self {
        let entries = attr_list
            .iter()
            .map(|c| FilterEntry { code_name: c.clone(), visible: false, look: favorite_look(c) })
            .collect();
        Filter { name: name.to_string(), attr_list, entries, selected: Vec::new(), no_persistence: false }
    }

    /// `AttributeDesc::CreateExcludingList` : tous les attributs déclarés
    /// sauf ceux listés (ordre du registre).
    pub fn excluding(name: &str, excluded: &[&str]) -> Self {
        let list = registry()
            .all()
            .iter()
            .filter(|d| !excluded.contains(&d.code_name))
            .map(|d| d.code_name.to_string())
            .collect();
        Self::new(name, list)
    }

    /// `AttributeDesc::CreateIncludingList` : uniquement les attributs listés
    /// (ordre du registre).
    pub fn including(name: &str, included: &[&str]) -> Self {
        let list = registry()
            .all()
            .iter()
            .filter(|d| included.contains(&d.code_name))
            .map(|d| d.code_name.to_string())
            .collect();
        Self::new(name, list)
    }

    pub fn prevent_persistence(&mut self) {
        self.no_persistence = true;
    }

    /// Nom du filtre (groupe des préférences).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `AddOwner` : le nom devient `<nom>::<classe>` si besoin.
    pub fn add_owner(&mut self, klass_name: &str) {
        if !self.name.contains(klass_name) {
            self.name = format!("{}::{}", self.name, klass_name);
        }
    }

    /// Attributs disponibles (ordre initial).
    pub fn attr_list(&self) -> &[String] {
        &self.attr_list
    }

    /// `GetAttributeId` : position dans la liste initiale.
    pub fn attribute_id(&self, code: &str) -> Option<usize> {
        self.attr_list.iter().position(|c| c == code)
    }

    /// Lignes du magasin (pour le dialogue « Données à afficher »).
    pub fn entries(&self) -> &[FilterEntry] {
        &self.entries
    }

    /// Colonnes affichées (`GetLayoutList`).
    pub fn layout(&self) -> &[Layout] {
        &self.selected
    }

    /// Noms de code des colonnes affichées.
    pub fn selected_codes(&self) -> Vec<String> {
        self.selected.iter().map(|l| l.code_name.clone()).collect()
    }

    /// Ajoute un attribut connu du registre à la liste disponible (attributs
    /// inconnus d'un fichier importé).
    pub fn add_available(&mut self, code: &str) {
        if !self.attr_list.iter().any(|c| c == code) {
            self.attr_list.push(code.to_string());
            self.entries.push(FilterEntry { code_name: code.to_string(), visible: false, look: favorite_look(code) });
        }
    }

    /// `ShowAttribute` : rend visible et déplace la ligne juste avant la
    /// dernière ligne visible rencontrée avant elle (ou en fin de liste).
    pub fn show_attribute(&mut self, code: &str) {
        let mut sibling: Option<usize> = None;
        for i in 0..self.entries.len() {
            if self.entries[i].visible {
                sibling = Some(i);
            }
            if self.entries[i].code_name == code {
                let mut e = self.entries.remove(i);
                e.visible = true;
                let look = e.look_for_layout();
                match sibling {
                    Some(s) if s < i => self.entries.insert(s, e),
                    Some(s) if s == i => self.entries.insert(i, e),
                    _ => self.entries.push(e),
                }
                self.selected.push(Layout { code_name: code.to_string(), look });
                break;
            }
        }
    }

    /// `ApplyList` : tout masquer puis afficher la liste donnée.
    pub fn apply_list(&mut self, list: &[String]) {
        for e in &mut self.entries {
            e.visible = false;
        }
        self.selected.clear();
        for c in list {
            self.show_attribute(c);
        }
    }

    /// `UpdateAttrList` : reconstruit les colonnes affichées dans l'ordre du
    /// magasin et, si `config` est fourni, mémorise le choix.
    pub fn update_attr_list(&mut self, config: Option<&mut UserConfig>) {
        self.selected = self
            .entries
            .iter()
            .filter(|e| e.visible)
            .map(|e| Layout { code_name: e.code_name.clone(), look: e.look })
            .collect();
        if let Some(cfg) = config {
            if !self.no_persistence {
                let codes: Vec<&str> = self.selected.iter().map(|l| l.code_name.as_str()).collect();
                cfg.set_string_list(&self.name, "display", &codes);
            }
        }
    }

    /// `RestoreLast` : dernier choix mémorisé dans les préférences.
    pub fn restore_last(&mut self, config: &mut UserConfig) {
        if self.no_persistence {
            return;
        }
        if let Some(list) = config.get_string_list(&self.name, "display") {
            self.apply_list(&list);
            self.update_attr_list(Some(config));
        }
    }

    /// Bascule la visibilité d'une ligne du magasin (`OnVisibilityToggled`).
    pub fn set_visibility(&mut self, index: usize, visible: bool, config: Option<&mut UserConfig>) {
        if let Some(e) = self.entries.get_mut(index) {
            e.visible = visible;
        }
        self.update_attr_list(config);
    }

    /// Change l'aspect d'une ligne (`OnLookChanged`).
    pub fn set_look(&mut self, index: usize, look: Look, config: Option<&mut UserConfig>) {
        if let Some(e) = self.entries.get_mut(index) {
            e.look = look;
        }
        self.update_attr_list(config);
    }

    /// Déplace une ligne (glisser-déposer dans le dialogue, `OnAttrDeleted`).
    pub fn move_entry(&mut self, from: usize, to: usize, config: Option<&mut UserConfig>) {
        if from < self.entries.len() {
            let e = self.entries.remove(from);
            let to = to.min(self.entries.len());
            self.entries.insert(to, e);
        }
        self.update_attr_list(config);
    }

    /// `Save` : `Affichage<as>="Nom,Prenom,…"`.
    pub fn save(&self, w: &mut XmlWriter, as_: &str) {
        let list: Vec<String> = self.selected.iter().map(|l| xml_name(&l.code_name)).collect();
        w.attr(&format!("Affichage{as_}"), &list.join(","));
    }

    /// `Load` : lit `Affichage<as>` (noms XML ou noms de code).
    pub fn load(&mut self, node: roxmltree::Node, as_: &str) {
        if let Some(attr) = node.attribute(format!("Affichage{as_}").as_str()) {
            let list: Vec<String> = attr
                .split(',')
                .map(|s| {
                    registry()
                        .all()
                        .iter()
                        .find(|d| d.xml_name == s)
                        .map(|d| d.code_name.to_string())
                        .unwrap_or_else(|| s.to_string())
                })
                .collect();
            self.apply_list(&list);
            self.update_attr_list(None);
        }
    }
}

impl FilterEntry {
    fn look_for_layout(&self) -> Look {
        favorite_look(&self.code_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_and_save() {
        let mut f = Filter::excluding("Test", &["ref"]);
        f.show_attribute("name");
        f.show_attribute("first_name");
        f.show_attribute("club");
        assert_eq!(f.selected_codes(), vec!["name", "first_name", "club"]);
        f.update_attr_list(None);
        assert_eq!(f.selected_codes(), vec!["name", "first_name", "club"]);
        let mut w = XmlWriter::new();
        w.start("X");
        f.save(&mut w, "");
        w.end();
        let s = w.finish();
        assert!(s.contains(r#"Affichage="Nom,Prenom,Club""#));
        let doc = roxmltree::Document::parse(&s).unwrap();
        let mut g = Filter::excluding("Test", &["ref"]);
        g.load(doc.root_element(), "");
        assert_eq!(g.selected_codes(), f.selected_codes());
    }

    #[test]
    fn persistence() {
        let mut cfg = UserConfig::new();
        let mut f = Filter::excluding("Checkin", &[]);
        f.show_attribute("ranking");
        f.update_attr_list(Some(&mut cfg));
        let mut g = Filter::excluding("Checkin", &[]);
        g.restore_last(&mut cfg);
        assert_eq!(g.selected_codes(), vec!["ranking"]);
    }
}
