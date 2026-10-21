//! Traductions (remplace gettext). Les catalogues `.po` de
//! `resources/translations/` sont chargés à l'exécution.
//!
//! `tr("msgid")` renvoie la traduction de la langue active, ou le msgid.

use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;

static CATALOG: RwLock<Option<HashMap<String, String>>> = RwLock::new(None);

pub fn tr(msgid: &str) -> String {
    if let Ok(guard) = CATALOG.read() {
        if let Some(cat) = guard.as_ref() {
            if let Some(t) = cat.get(msgid) {
                if !t.is_empty() {
                    return t.clone();
                }
            }
        }
    }
    msgid.to_string()
}

/// Active une langue (`fr`, `de`…) à partir du dossier `resources/translations`.
/// Une langue vide ou `en` désactive la traduction.
pub fn set_language(translations_dir: &Path, lang: &str) -> std::io::Result<()> {
    let catalog = if lang.is_empty() || lang == "en" {
        None
    } else {
        let text = std::fs::read_to_string(translations_dir.join(format!("{lang}.po")))?;
        Some(parse_po(&text))
    };
    *CATALOG.write().unwrap() = catalog;
    Ok(())
}

/// Parseur `.po` minimal : msgid/msgstr sur une ou plusieurs lignes, contextes
/// et pluriels ignorés (seule la forme singulière est retenue).
pub fn parse_po(text: &str) -> HashMap<String, String> {
    #[derive(PartialEq)]
    enum Field {
        None,
        Id,
        Str,
        Other,
    }
    let mut map = HashMap::new();
    let mut id = String::new();
    let mut s = String::new();
    let mut field = Field::None;
    let mut fuzzy = false;

    let mut flush = |id: &mut String, s: &mut String, fuzzy: &mut bool| {
        if !id.is_empty() && !*fuzzy {
            map.insert(std::mem::take(id), std::mem::take(s));
        }
        id.clear();
        s.clear();
        *fuzzy = false;
    };

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("#,") && line.contains("fuzzy") {
            flush(&mut id, &mut s, &mut fuzzy);
            field = Field::None;
            fuzzy = true;
        } else if line.starts_with('#') || line.is_empty() {
            continue;
        } else if let Some(rest) = line.strip_prefix("msgid ") {
            if field == Field::Str || field == Field::Other {
                flush(&mut id, &mut s, &mut fuzzy);
            }
            id = unquote(rest);
            field = Field::Id;
        } else if let Some(rest) = line.strip_prefix("msgstr[0] ").or_else(|| line.strip_prefix("msgstr ")) {
            s = unquote(rest);
            field = Field::Str;
        } else if line.starts_with("msgstr[") || line.starts_with("msgid_plural") || line.starts_with("msgctxt") {
            field = Field::Other;
        } else if line.starts_with('"') {
            match field {
                Field::Id => id.push_str(&unquote(line)),
                Field::Str => s.push_str(&unquote(line)),
                _ => {}
            }
        }
    }
    flush(&mut id, &mut s, &mut fuzzy);
    map
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix('"').unwrap_or(s);
    let s = s.strip_suffix('"').unwrap_or(s);
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o)
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_simple_po() {
        let po = "msgid \"\"\nmsgstr \"header\"\n\nmsgid \"Male\"\nmsgstr \"Homme\"\n\n#, fuzzy\nmsgid \"X\"\nmsgstr \"Y\"\n\nmsgid \"a\"\n\"b\"\nmsgstr \"c\"\n\"d\"\n";
        let m = super::parse_po(po);
        assert_eq!(m.get("Male").unwrap(), "Homme");
        assert!(!m.contains_key("X"));
        assert_eq!(m.get("ab").unwrap(), "cd");
    }
}
