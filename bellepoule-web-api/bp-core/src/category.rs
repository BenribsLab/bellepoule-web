//! Catégories d'âge (port de `supervisor/category.cpp`).
//!
//! Une catégorie est soit une des 18 catégories connues (`id`), soit un
//! texte libre (`fallback`) saisi par l'utilisateur ou lu dans un fichier.

const NB_CATEGORY: usize = 18;

/// Libellés (msgid gettext).
pub const DISPLAY_IMAGES: [&str; NB_CATEGORY] = [
    "U5", "U7", "U9", "U11", "U13", "U15", "U17", "U20", "U23", "Senior", "Veteran", "Veteran 1", "Veteran 2",
    "Veteran 3", "Veteran 4", "Veteran 5", "Veteran 3+4", "Veteran 4+5",
];

/// Codes XML écrits dans les fichiers.
pub const XML_IMAGES: [&str; NB_CATEGORY] =
    ["M5", "M7", "M9", "M11", "M13", "M15", "M17", "M20", "M23", "S", "VET", "V1", "V2", "V3", "V4", "V5", "V34", "V45"];

/// Anciens codes (FFE) acceptés à la lecture.
pub const XML_ALIASES: [&str; NB_CATEGORY] =
    ["M5", "M7", "PO", "PUP", "B", "M", "C", "J", "M23", "S", "VET", "V1", "V2", "V3", "V4", "V5", "V34", "V45"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    /// Index de la catégorie connue, `None` si texte libre (`_id == -1`).
    id: Option<usize>,
    /// Texte libre (`_fallback`).
    fallback: Option<String>,
}

impl Default for Category {
    fn default() -> Self {
        Category { id: Some(0), fallback: None }
    }
}

impl Category {
    pub fn new() -> Self {
        Self::default()
    }

    /// Nombre de catégories connues.
    pub fn count() -> usize {
        NB_CATEGORY
    }

    /// Index courant (`None` pour une saisie libre).
    pub fn index(&self) -> Option<usize> {
        self.id
    }

    pub fn set_index(&mut self, index: usize) {
        if index < NB_CATEGORY {
            self.id = Some(index);
            self.fallback = None;
        }
    }

    /// `ReadUserChoice` : `index` est la ligne choisie dans la liste
    /// déroulante (`None` si aucune), `text` le contenu de la zone de saisie.
    pub fn read_user_choice(&mut self, index: Option<usize>, text: &str) {
        self.fallback = None;
        self.id = index.filter(|i| *i < NB_CATEGORY);
        if self.id.is_none() {
            self.id = DISPLAY_IMAGES.iter().position(|d| crate::i18n::tr(d) == text);
            if self.id.is_none() {
                self.fallback = Some(text.to_string());
            }
        }
    }

    /// `Replicate`.
    pub fn replicate(&mut self, what: &Category) {
        *self = what.clone();
    }

    /// `ParseXml` : code exact, puis alias, sinon texte libre.
    pub fn parse_xml(&mut self, xml: &str) {
        self.fallback = None;
        self.id = XML_IMAGES.iter().position(|x| *x == xml).or_else(|| XML_ALIASES.iter().position(|x| *x == xml));
        if self.id.is_none() {
            self.fallback = Some(xml.to_string());
        }
    }

    /// Libellé affiché (traduit) ou texte libre.
    pub fn display_image(&self) -> String {
        match self.id {
            Some(i) => crate::i18n::tr(DISPLAY_IMAGES[i]),
            None => self.fallback.clone().unwrap_or_default(),
        }
    }

    /// Code XML ou texte libre.
    pub fn xml_image(&self) -> String {
        match self.id {
            Some(i) => XML_IMAGES[i].to_string(),
            None => self.fallback.clone().unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_alias_and_fallback() {
        let mut c = Category::new();
        c.parse_xml("J");
        assert_eq!(c.xml_image(), "M20");
        c.parse_xml("XYZ");
        assert_eq!(c.index(), None);
        assert_eq!(c.xml_image(), "XYZ");
        c.read_user_choice(None, "Senior");
        assert_eq!(c.xml_image(), "S");
    }
}
