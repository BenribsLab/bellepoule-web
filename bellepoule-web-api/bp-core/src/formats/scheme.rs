//! Écrivain XML « traducteur » (port de `common/util/xml_scheme.cpp`).
//!
//! En C++, `XmlScheme` est la classe de base de toutes les sauvegardes ;
//! ses sous-classes (ex. AskFred) traduisent noms et valeurs à la volée.
//! Ici, la sauvegarde ordinaire passe par `xml::XmlWriter` ; un schéma
//! traducteur est alimenté en « rejouant » un document déjà produit
//! ([`replay`]), ce qui donne exactement la même suite d'appels.

use crate::xml::XmlWriter;

pub trait XmlScheme {
    fn start_element(&mut self, name: &str);
    fn end_element(&mut self);
    /// `WriteAttribute`.
    fn write_attribute(&mut self, name: &str, value: &str);
    /// `WriteFormatAttribute` (valeur déjà formatée).
    fn write_format_attribute(&mut self, name: &str, value: &str) {
        self.write_attribute(name, value);
    }
    /// `WriteCustom` : contenu propre au schéma (ex. base des clubs AskFred).
    fn write_custom(&mut self, _what: &str) {}
    /// `SaveFencersAndTeamsSeparatly`.
    fn save_fencers_and_teams_separately(&self) -> bool {
        false
    }
}

/// Schéma identité : écrit tel quel dans un `XmlWriter`.
pub struct PlainScheme {
    pub writer: XmlWriter,
}

impl PlainScheme {
    pub fn new(writer: XmlWriter) -> Self {
        PlainScheme { writer }
    }

    pub fn finish(self) -> String {
        self.writer.finish()
    }
}

impl XmlScheme for PlainScheme {
    fn start_element(&mut self, name: &str) {
        self.writer.start(name);
    }

    fn end_element(&mut self) {
        self.writer.end();
    }

    fn write_attribute(&mut self, name: &str, value: &str) {
        self.writer.attr(name, value);
    }
}

/// Rejoue un élément (attributs dans l'ordre du document, puis enfants
/// éléments) dans un schéma. Le texte et les commentaires sont ignorés.
pub fn replay(node: roxmltree::Node, scheme: &mut dyn XmlScheme) {
    if !node.is_element() {
        return;
    }
    scheme.start_element(node.tag_name().name());
    for a in node.attributes() {
        scheme.write_attribute(a.name(), a.value());
    }
    for c in node.children() {
        replay(c, scheme);
    }
    scheme.end_element();
}

/// Rejoue seulement les enfants d'un élément.
pub fn replay_children(node: roxmltree::Node, scheme: &mut dyn XmlScheme) {
    for c in node.children() {
        replay(c, scheme);
    }
}
