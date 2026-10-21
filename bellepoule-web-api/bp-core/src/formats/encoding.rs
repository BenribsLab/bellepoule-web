//! Encodages des fichiers d'échange et petites aides textuelles.
//!
//! Le C++ s'appuie sur `g_convert` / `g_locale_to_utf8` ; ici on décode
//! explicitement :
//! - ISO-8859-1 (« Latin-1 ») octet par octet (chaque octet est un point de
//!   code Unicode) — et non Windows-1252, pour rester fidèle à `g_convert` ;
//! - UTF-16 (LE/BE) lorsqu'une marque d'ordre d'octets (BOM) est présente ;
//! - UTF-8, avec ou sans BOM.

use crate::{Error, Result};

/// Espace insécable (`GetUndivadableText` en C++).
pub const NBSP: char = '\u{a0}';

/// `Object::GetUndivadableText` : remplace les espaces par des espaces
/// insécables (utilisé pour les clés de comparaison et l'affichage).
pub fn undivadable(s: &str) -> String {
    s.replace(' ', "\u{a0}")
}

/// Inverse de [`undivadable`] (`RemoveNonBreakingSpaces` du writer AskFred,
/// contournement « CLAS » de l'export FFF).
pub fn divadable(s: &str) -> String {
    s.replace(NBSP, " ")
}

/// Égalité insensible aux espaces insécables.
pub fn same_text(a: &str, b: &str) -> bool {
    a.chars().map(|c| if c == NBSP { ' ' } else { c }).eq(b.chars().map(|c| if c == NBSP { ' ' } else { c }))
}

/// Encodage détecté d'un fichier texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    /// UTF-8 précédé d'une BOM `EF BB BF`.
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    /// ISO-8859-1.
    Latin1,
    /// Page de code Windows (`g_locale_to_utf8` sous Windows).
    Windows1252,
}

/// Détection par BOM.
pub fn detect_bom(bytes: &[u8]) -> Option<TextEncoding> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Some(TextEncoding::Utf8Bom)
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Some(TextEncoding::Utf16Le)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Some(TextEncoding::Utf16Be)
    } else {
        None
    }
}

/// UTF-16 sans BOM : heuristique sur les octets nuls (texte ASCII en UTF-16).
pub fn guess_utf16_without_bom(bytes: &[u8]) -> Option<TextEncoding> {
    if bytes.len() >= 4 {
        if bytes[0] != 0 && bytes[1] == 0 && bytes[2] != 0 && bytes[3] == 0 {
            return Some(TextEncoding::Utf16Le);
        }
        if bytes[0] == 0 && bytes[1] != 0 && bytes[2] == 0 && bytes[3] != 0 {
            return Some(TextEncoding::Utf16Be);
        }
    }
    None
}

pub fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units = bytes.chunks_exact(2).map(|c| if little_endian { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) });
    let s: String = char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect();
    s.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(s)
}

pub fn latin1_decode(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Conversion UTF-8 → ISO-8859-1 ; `None` si un caractère n'est pas
/// représentable (`g_convert` échoue alors).
pub fn latin1_encode(s: &str) -> Option<Vec<u8>> {
    s.chars().map(|c| if (c as u32) < 256 { Some(c as u32 as u8) } else { None }).collect()
}

pub fn windows1252_decode(bytes: &[u8]) -> String {
    encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned()
}

/// `Checkin::GetFileContent` : supprime les `\r`.
pub fn strip_cr(s: &str) -> String {
    s.chars().filter(|&c| c != '\r').collect()
}

/// Décode un texte selon l'encodage indiqué (sans contrôle d'erreur pour
/// Latin-1/Windows-1252, strict pour UTF-8).
pub fn decode(bytes: &[u8], enc: TextEncoding) -> Result<String> {
    match enc {
        TextEncoding::Utf8 => std::str::from_utf8(bytes).map(str::to_string).map_err(|e| Error::Format(format!("UTF-8 invalide : {e}"))),
        TextEncoding::Utf8Bom => decode(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes), TextEncoding::Utf8),
        TextEncoding::Utf16Le => Ok(decode_utf16(bytes.strip_prefix(&[0xFF, 0xFE][..]).unwrap_or(bytes), true)),
        TextEncoding::Utf16Be => Ok(decode_utf16(bytes.strip_prefix(&[0xFE, 0xFF][..]).unwrap_or(bytes), false)),
        TextEncoding::Latin1 => Ok(latin1_decode(bytes)),
        TextEncoding::Windows1252 => Ok(windows1252_decode(bytes)),
    }
}

/// Décode un document XML selon sa BOM ou sa déclaration `encoding="…"`
/// (libxml2 le fait seul ; `roxmltree` n'accepte que de l'UTF-8).
pub fn decode_xml(bytes: &[u8]) -> Result<String> {
    if let Some(enc) = detect_bom(bytes).or_else(|| guess_utf16_without_bom(bytes)) {
        return decode(bytes, enc);
    }
    let head_len = bytes.len().min(256);
    let head = latin1_decode(&bytes[..head_len]);
    let mut label = String::from("UTF-8");
    if head.starts_with("<?xml") {
        if let Some(end) = head.find("?>") {
            let decl = &head[..end];
            if let Some(pos) = decl.find("encoding") {
                let rest = decl[pos + "encoding".len()..].trim_start().trim_start_matches('=').trim_start();
                let quote = rest.chars().next().unwrap_or('"');
                if quote == '"' || quote == '\'' {
                    if let Some(close) = rest[1..].find(quote) {
                        label = rest[1..1 + close].to_string();
                    }
                }
            }
        }
    }
    let upper = label.to_ascii_uppercase();
    match upper.as_str() {
        "UTF-8" | "UTF8" => decode(bytes, TextEncoding::Utf8),
        "ISO-8859-1" | "ISO8859-1" | "LATIN1" | "LATIN-1" | "ISO_8859-1" => Ok(latin1_decode(bytes)),
        _ => match encoding_rs::Encoding::for_label(label.as_bytes()) {
            Some(enc) => Ok(enc.decode_without_bom_handling(bytes).0.into_owned()),
            None => Err(Error::Format(format!("encodage XML inconnu : {label}"))),
        },
    }
}

/// Analyse XML tolérante (DTD autorisée) d'un texte déjà décodé.
pub fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>> {
    Ok(crate::xml::parse(text)?)
}

/// `g_uri_escape_string (s, NULL, FALSE)` : tout sauf les caractères non
/// réservés (`A-Z a-z 0-9 - . _ ~`) est encodé en `%XX` (octets UTF-8).
pub fn uri_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `g_strndup` borné en octets, sans couper un caractère UTF-8.
pub fn truncate_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// `atoi` : entier en tête de chaîne (0 sinon).
pub fn atoi(s: &str) -> i64 {
    crate::attribute::parse_leading_int(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_like_glib() {
        assert_eq!(uri_escape("Québec"), "Qu%C3%A9bec");
        assert_eq!(uri_escape("COTE D'AZUR"), "COTE%20D%27AZUR");
    }

    #[test]
    fn xml_latin1_declaration() {
        let bytes = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<a b=\"\xe9\"/>";
        let text = decode_xml(bytes).unwrap();
        let doc = parse_xml(&text).unwrap();
        assert_eq!(doc.root_element().attribute("b"), Some("é"));
    }

    #[test]
    fn latin1_roundtrip() {
        assert_eq!(latin1_encode("é").unwrap(), vec![0xE9]);
        assert!(latin1_encode("€").is_none());
        assert_eq!(undivadable("a b"), "a\u{a0}b");
        assert!(same_text("a\u{a0}b", "a b"));
    }
}
