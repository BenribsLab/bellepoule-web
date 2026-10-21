//! Écriture XML (port de `common/util/xml_scheme.cpp`, basé sur
//! `xmlTextWriter` en C++) et petits utilitaires de lecture `roxmltree`.
//!
//! La sortie reproduit la mise en forme de libxml2 (indentation d'un espace,
//! éléments vides fermés par `/>`), afin que les fichiers `.cotcot` produits
//! restent lisibles par la version C++.

pub struct XmlWriter {
    out: String,
    stack: Vec<String>,
    /// Vrai si la balise ouvrante courante n'est pas encore fermée (`>`).
    open_tag: bool,
    /// Vrai si l'élément courant contient du texte (pas d'indentation à la fermeture).
    has_text: Vec<bool>,
    indent: bool,
}

impl Default for XmlWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl XmlWriter {
    pub fn new() -> Self {
        XmlWriter { out: String::new(), stack: Vec::new(), open_tag: false, has_text: Vec::new(), indent: true }
    }

    /// Document complet avec en-tête `<?xml …?>`.
    pub fn document() -> Self {
        let mut w = Self::new();
        w.out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        w
    }

    pub fn without_indent(mut self) -> Self {
        self.indent = false;
        self
    }

    fn close_open_tag(&mut self) {
        if self.open_tag {
            self.out.push('>');
            self.open_tag = false;
        }
    }

    fn newline_indent(&mut self, depth: usize) {
        if self.indent {
            if !self.out.is_empty() && !self.out.ends_with('\n') {
                self.out.push('\n');
            }
            for _ in 0..depth {
                self.out.push(' ');
            }
        }
    }

    pub fn dtd(&mut self, name: &str) {
        self.out.push_str(&format!("<!DOCTYPE {name}>\n"));
    }

    pub fn comment(&mut self, text: &str) {
        self.close_open_tag();
        let depth = self.stack.len();
        self.newline_indent(depth);
        self.out.push_str("<!--");
        self.out.push_str(text);
        self.out.push_str("-->");
    }

    pub fn start(&mut self, name: &str) {
        self.close_open_tag();
        let depth = self.stack.len();
        self.newline_indent(depth);
        self.out.push('<');
        self.out.push_str(name);
        self.stack.push(name.to_string());
        self.has_text.push(false);
        self.open_tag = true;
    }

    pub fn attr(&mut self, name: &str, value: &str) {
        debug_assert!(self.open_tag, "attribut hors balise ouvrante : {name}");
        self.out.push(' ');
        self.out.push_str(name);
        self.out.push_str("=\"");
        escape_into(&mut self.out, value, true);
        self.out.push('"');
    }

    pub fn attr_fmt(&mut self, name: &str, value: impl std::fmt::Display) {
        self.attr(name, &value.to_string());
    }

    pub fn text(&mut self, text: &str) {
        self.close_open_tag();
        if let Some(t) = self.has_text.last_mut() {
            *t = true;
        }
        escape_into(&mut self.out, text, false);
    }

    /// Insère du XML brut (`WriteCustom`).
    pub fn raw(&mut self, xml: &str) {
        self.close_open_tag();
        self.out.push_str(xml);
    }

    /// Insère un élément XML déjà formé sur sa propre ligne, à l'indentation courante.
    pub fn raw_indented(&mut self, xml: &str) {
        self.close_open_tag();
        let depth = self.stack.len();
        self.newline_indent(depth);
        self.out.push_str(xml);
    }

    pub fn end(&mut self) {
        let name = self.stack.pop().expect("end() sans start()");
        let had_text = self.has_text.pop().unwrap_or(false);
        if self.open_tag {
            self.out.push_str("/>");
            self.open_tag = false;
        } else {
            if !had_text {
                let depth = self.stack.len();
                self.newline_indent(depth);
            }
            self.out.push_str("</");
            self.out.push_str(&name);
            self.out.push('>');
        }
    }

    /// Élément simple sans enfant.
    pub fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.start(name);
        for (k, v) in attrs {
            self.attr(k, v);
        }
        self.end();
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn finish(mut self) -> String {
        while !self.stack.is_empty() {
            self.end();
        }
        if self.indent && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out
    }
}

fn escape_into(out: &mut String, s: &str, attr: bool) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            '\n' if attr => out.push_str("&#10;"),
            '\r' if attr => out.push_str("&#13;"),
            '\t' if attr => out.push_str("&#9;"),
            _ => out.push(c),
        }
    }
}

/// Lecture d'un attribut entier (`g_ascii_strtoull`), valeur par défaut sinon.
pub fn attr_u32(node: roxmltree::Node, name: &str, default: u32) -> u32 {
    node.attribute(name)
        .map(|s| crate::attribute::parse_leading_int(s).max(0) as u32)
        .unwrap_or(default)
}

pub fn attr_i32(node: roxmltree::Node, name: &str, default: i32) -> i32 {
    node.attribute(name).map(|s| crate::attribute::parse_leading_int(s) as i32).unwrap_or(default)
}

pub fn attr_bool(node: roxmltree::Node, name: &str, default: bool) -> bool {
    node.attribute(name).map(|s| crate::attribute::parse_leading_int(s) != 0).unwrap_or(default)
}

/// Enfants éléments portant un nom donné.
pub fn children<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> {
    node.children().filter(move |n| n.is_element() && n.tag_name().name() == name)
}

pub fn child<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &'a str) -> Option<roxmltree::Node<'a, 'i>> {
    children(node, name).next()
}

/// Options de parsing tolérantes (les fichiers `.cotcot` déclarent une DTD).
pub fn parse(text: &str) -> Result<roxmltree::Document<'_>, roxmltree::Error> {
    roxmltree::Document::parse_with_options(text, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_layout_matches_libxml() {
        let mut w = XmlWriter::document();
        w.dtd("CompetitionIndividuelle");
        w.start("CompetitionIndividuelle");
        w.attr("Arme", "S");
        w.start("Tireurs");
        w.empty("Tireur", &[("ID", "1"), ("Nom", "A&B")]);
        w.end();
        w.end();
        let s = w.finish();
        assert_eq!(
            s,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE CompetitionIndividuelle>\n<CompetitionIndividuelle Arme=\"S\">\n <Tireurs>\n  <Tireur ID=\"1\" Nom=\"A&amp;B\"/>\n </Tireurs>\n</CompetitionIndividuelle>\n"
        );
    }
}
