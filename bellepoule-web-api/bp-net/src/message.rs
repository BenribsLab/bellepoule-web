//! Messages échangés entre BellePoule, le Marshaller et les smartphones
//! (port de `common/network/message.cpp`).
//!
//! Le C++ sérialise un `GKeyFile` (format INI) avec deux groupes : `[Header]`
//! (`name`, `netid`, `fitness`…) et `[Body]`. Ce module produit et lit
//! le même texte (format éprouvé, aucune interopérabilité C++ requise).

use std::fmt::Write as _;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message {
    header: Vec<(String, String)>,
    body: Vec<(String, String)>,
    valid: bool,
    passphrase256: Option<String>,
    spread: bool,
}

impl Message {
    /// Nouveau message avec un `netid` aléatoire.
    pub fn new(name: &str) -> Self {
        let mut m = Message { valid: true, ..Default::default() };
        set(&mut m.header, "name", name.to_string());
        set(&mut m.header, "netid", rand::random::<u32>().to_string());
        m
    }

    /// Lit un message reçu (`Message (const guint8 *data)`).
    pub fn parse(data: &str) -> Self {
        let mut m = Message { valid: true, ..Default::default() };
        let mut group: Option<&str> = None;
        for line in data.lines() {
            let line = line.trim_end_matches('\r');
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with('[') {
                if let Some(end) = trimmed.find(']') {
                    group = Some(match &trimmed[1..end] {
                        "Header" => "Header",
                        "Body" => "Body",
                        _ => "",
                    });
                    continue;
                }
                m.valid = false;
                return m;
            }
            let Some(eq) = line.find('=') else {
                m.valid = false;
                return m;
            };
            let key = line[..eq].trim().to_string();
            let value = unescape(line[eq + 1..].trim_start());
            match group {
                Some("Header") => set(&mut m.header, &key, value),
                Some("Body") => set(&mut m.body, &key, value),
                Some(_) => {}
                None => {
                    m.valid = false;
                    return m;
                }
            }
        }
        m
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn name(&self) -> Option<&str> {
        get(&self.header, "name")
    }

    pub fn is(&self, name: &str) -> bool {
        self.name() == Some(name)
    }

    pub fn net_id(&self) -> u32 {
        get(&self.header, "netid").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) as u32
    }

    pub fn set_net_id(&mut self, id: u32) {
        set(&mut self.header, "netid", id.to_string());
    }

    pub fn fitness(&self) -> u32 {
        get(&self.header, "fitness").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) as u32
    }

    pub fn set_fitness(&mut self, value: u32) {
        set(&mut self.header, "fitness", value.to_string());
        self.valid = true;
    }

    /// Champ libre d'en-tête (ex. `source`, `address`, `role` ajoutés par `Ring::StampSender`).
    pub fn header(&self, key: &str) -> Option<&str> {
        get(&self.header, key)
    }

    pub fn set_header(&mut self, key: &str, value: &str) {
        set(&mut self.header, key, value.to_string());
    }

    pub fn passphrase256(&self) -> Option<&str> {
        self.passphrase256.as_deref()
    }

    pub fn set_passphrase256(&mut self, p: Option<&str>) {
        self.passphrase256 = p.map(str::to_string);
    }

    pub fn is_spread(&self) -> bool {
        self.spread
    }

    pub fn mark_spread(&mut self, spread: bool) {
        self.spread = spread;
    }

    pub fn set_str(&mut self, field: &str, value: &str) {
        set(&mut self.body, field, value.to_string());
        self.valid = true;
    }

    pub fn set_uint(&mut self, field: &str, value: u32) {
        set(&mut self.body, field, value.to_string());
        self.valid = true;
    }

    pub fn set_int(&mut self, field: &str, value: i32) {
        set(&mut self.body, field, value.to_string());
        self.valid = true;
    }

    /// `g_key_file_set_integer_list` : `1;2;3;`.
    pub fn set_list(&mut self, field: &str, list: &[u32]) {
        let mut s = String::new();
        for v in list {
            let _ = write!(s, "{};", *v as i32);
        }
        set(&mut self.body, field, s);
        self.valid = true;
    }

    pub fn remove(&mut self, field: &str) {
        self.body.retain(|(k, _)| k != field);
    }

    pub fn has_field(&self, field: &str) -> bool {
        get(&self.body, field).is_some()
    }

    pub fn get_str(&self, field: &str) -> Option<String> {
        get(&self.body, field).map(str::to_string)
    }

    pub fn get_uint(&self, field: &str) -> u32 {
        get(&self.body, field).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as u32
    }

    pub fn get_int(&self, field: &str) -> i32 {
        get(&self.body, field).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as i32
    }

    pub fn get_list(&self, field: &str) -> Vec<u32> {
        get(&self.body, field)
            .map(|v| v.split(';').filter(|s| !s.trim().is_empty()).filter_map(|s| s.trim().parse::<i64>().ok()).map(|n| n as u32).collect())
            .unwrap_or_default()
    }




    /// Texte sérialisé (`GetParcel`, `g_key_file_to_data`).
    pub fn parcel(&self) -> String {
        let mut out = String::new();
        let mut first = true;
        for (group, entries) in [("Header", &self.header), ("Body", &self.body)] {
            if entries.is_empty() {
                continue;
            }
            if !first {
                out.push('\n');
            }
            first = false;
            let _ = writeln!(out, "[{group}]");
            for (k, v) in entries {
                let _ = writeln!(out, "{k}={}", escape(v));
            }
        }
        out
    }
}

fn get<'a>(entries: &'a [(String, String)], key: &str) -> Option<&'a str> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn set(entries: &mut Vec<(String, String)>, key: &str, value: String) {
    match entries.iter_mut().find(|(k, _)| k == key) {
        Some(e) => e.1 = value,
        None => entries.push((key.to_string(), value)),
    }
}

/// Échappement `GKeyFile` des valeurs chaînes.
pub(crate) fn escape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for (i, c) in v.chars().enumerate() {
        match c {
            ' ' if i == 0 => out.push_str("\\s"),
            '\t' if i == 0 => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out
}

pub(crate) fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
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
    use super::*;

    #[test]
    fn roundtrip() {
        let mut m = Message::new("BellePoule::Score");
        m.set_net_id(42);
        m.set_str("xml", "<Match ID=\"1\">\n</Match>");
        m.set_uint("piste", 3);
        m.set_list("ids", &[1, 2, 3]);
        let text = m.parcel();
        assert!(text.starts_with("[Header]\nname=BellePoule::Score\nnetid=42\n\n[Body]\n"));
        assert!(text.contains("ids=1;2;3;\n"));
        let p = Message::parse(&text);
        assert!(p.is_valid());
        assert!(p.is("BellePoule::Score"));
        assert_eq!(p.net_id(), 42);
        assert_eq!(p.get_uint("piste"), 3);
        assert_eq!(p.get_list("ids"), vec![1, 2, 3]);
        assert_eq!(p.get_str("xml").unwrap(), "<Match ID=\"1\">\n</Match>");
    }
}
