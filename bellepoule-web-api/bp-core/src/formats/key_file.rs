//! Fichiers « .ini » au format `GKeyFile` (configuration utilisateur).
//!
//! Groupes `[Nom]`, paires `clé=valeur`, commentaires `#` conservés,
//! échappements `\s \n \t \r \\` et listes séparées par `;`.

#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    /// Commentaire ou ligne vide (conservés tels quels).
    Comment(String),
    Entry(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Group {
    name: String,
    lines: Vec<Line>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyFile {
    /// Commentaires avant le premier groupe.
    header: Vec<String>,
    groups: Vec<Group>,
}

/// `g_key_file_get_string` : déséchappement.
pub fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
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
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `g_key_file_set_string` : échappement (espaces de tête en `\s`).
pub fn escape(s: &str) -> String {
    let mut out = String::new();
    let mut leading = true;
    for c in s.chars() {
        match c {
            ' ' if leading => out.push_str("\\s"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
        if c != ' ' {
            leading = false;
        }
    }
    out
}

impl KeyFile {
    pub fn new() -> Self {
        Self::default()
    }

    /// `g_key_file_load_from_data` (avec `G_KEY_FILE_KEEP_COMMENTS`).
    pub fn parse(text: &str) -> KeyFile {
        let mut kf = KeyFile::default();
        for raw in text.lines() {
            let line = raw.trim_start();
            if line.starts_with('[') {
                if let Some(end) = line.find(']') {
                    kf.groups.push(Group { name: line[1..end].to_string(), lines: Vec::new() });
                    continue;
                }
            }
            let target = kf.groups.last_mut();
            if line.is_empty() || line.starts_with('#') {
                match target {
                    Some(g) => g.lines.push(Line::Comment(raw.to_string())),
                    None => kf.header.push(raw.to_string()),
                }
                continue;
            }
            if let (Some(g), Some(eq)) = (target, line.find('=')) {
                let key = line[..eq].trim().to_string();
                let value = line[eq + 1..].trim_start().to_string();
                if let Some(Line::Entry(_, v)) = g.lines.iter_mut().find(|l| matches!(l, Line::Entry(k, _) if *k == key)) {
                    *v = value;
                } else {
                    g.lines.push(Line::Entry(key, value));
                }
            }
        }
        kf
    }

    pub fn load_from_file(path: &std::path::Path) -> std::io::Result<KeyFile> {
        Ok(KeyFile::parse(&std::fs::read_to_string(path)?))
    }

    /// `g_key_file_to_data`.
    pub fn to_data(&self) -> String {
        let mut out = String::new();
        for h in &self.header {
            out.push_str(h);
            out.push('\n');
        }
        for g in &self.groups {
            out.push('[');
            out.push_str(&g.name);
            out.push_str("]\n");
            for l in &g.lines {
                match l {
                    Line::Comment(c) => out.push_str(c),
                    Line::Entry(k, v) => {
                        out.push_str(k);
                        out.push('=');
                        out.push_str(v);
                    }
                }
                out.push('\n');
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub fn groups(&self) -> Vec<&str> {
        self.groups.iter().map(|g| g.name.as_str()).collect()
    }

    pub fn has_group(&self, group: &str) -> bool {
        self.groups.iter().any(|g| g.name == group)
    }

    pub fn keys(&self, group: &str) -> Vec<&str> {
        self.groups
            .iter()
            .find(|g| g.name == group)
            .map(|g| g.lines.iter().filter_map(|l| if let Line::Entry(k, _) = l { Some(k.as_str()) } else { None }).collect())
            .unwrap_or_default()
    }

    /// Valeur brute (non déséchappée).
    pub fn get_value(&self, group: &str, key: &str) -> Option<&str> {
        self.groups.iter().find(|g| g.name == group)?.lines.iter().find_map(|l| match l {
            Line::Entry(k, v) if k == key => Some(v.as_str()),
            _ => None,
        })
    }

    pub fn set_value(&mut self, group: &str, key: &str, value: &str) {
        let g = match self.groups.iter().position(|g| g.name == group) {
            Some(i) => &mut self.groups[i],
            None => {
                self.groups.push(Group { name: group.to_string(), lines: Vec::new() });
                self.groups.last_mut().unwrap()
            }
        };
        match g.lines.iter_mut().find(|l| matches!(l, Line::Entry(k, _) if k == key)) {
            Some(Line::Entry(_, v)) => *v = value.to_string(),
            _ => g.lines.push(Line::Entry(key.to_string(), value.to_string())),
        }
    }

    pub fn get_string(&self, group: &str, key: &str) -> Option<String> {
        self.get_value(group, key).map(unescape)
    }

    pub fn set_string(&mut self, group: &str, key: &str, value: &str) {
        self.set_value(group, key, &escape(value));
    }

    pub fn get_integer(&self, group: &str, key: &str) -> Option<i64> {
        self.get_value(group, key)?.trim().parse().ok()
    }

    pub fn set_integer(&mut self, group: &str, key: &str, value: i64) {
        self.set_value(group, key, &value.to_string());
    }

    pub fn get_double(&self, group: &str, key: &str) -> Option<f64> {
        self.get_value(group, key)?.trim().parse().ok()
    }

    pub fn set_double(&mut self, group: &str, key: &str, value: f64) {
        self.set_value(group, key, &value.to_string());
    }

    pub fn get_boolean(&self, group: &str, key: &str) -> Option<bool> {
        match self.get_value(group, key)?.trim() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        }
    }

    pub fn set_boolean(&mut self, group: &str, key: &str, value: bool) {
        self.set_value(group, key, if value { "true" } else { "false" });
    }

    /// `g_key_file_get_string_list` (séparateur `;`, `\;` échappé).
    pub fn get_string_list(&self, group: &str, key: &str) -> Option<Vec<String>> {
        let raw = self.get_value(group, key)?;
        let mut items = Vec::new();
        let mut cur = String::new();
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' && chars.peek() == Some(&';') {
                cur.push(';');
                chars.next();
            } else if c == ';' {
                items.push(unescape(&cur));
                cur.clear();
            } else {
                cur.push(c);
            }
        }
        if !cur.is_empty() {
            items.push(unescape(&cur));
        }
        Some(items)
    }

    pub fn set_string_list(&mut self, group: &str, key: &str, list: &[&str]) {
        let mut v = String::new();
        for item in list {
            v.push_str(&escape(item).replace(';', "\\;"));
            v.push(';');
        }
        self.set_value(group, key, &v);
    }

    pub fn remove_key(&mut self, group: &str, key: &str) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.name == group) {
            g.lines.retain(|l| !matches!(l, Line::Entry(k, _) if k == key));
        }
    }

    pub fn remove_group(&mut self, group: &str) {
        self.groups.retain(|g| g.name != group);
    }
}
