//! Préférences utilisateur (port de `common/util/user_config.cpp`, fichier
//! `GKeyFile`). Format INI : `[Groupe]` puis `clé=valeur`, listes séparées
//! par `;`, échappements `\n`, `\t`, `\\`, `\s`.
//!
//! Groupes utilisés par le cœur :
//! - `Competiton` (sic) : `default_name`, `default_organizer`,
//!   `default_web_site`, `default_location`, `color_to_use`,
//!   `default_dir_name`, `export_dir` ;
//! - `Checkin` / `CheckinSupervisor` : `default_import_dir_name` ;
//! - `Tournament` : `backup_location` ;
//! - un groupe par filtre de colonnes (`display`).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserConfig {
    /// Groupes dans l'ordre d'apparition, avec leurs clés ordonnées.
    groups: Vec<(String, Vec<(String, String)>)>,
    /// Fichier d'origine (pour `save`).
    path: Option<PathBuf>,
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('s') => out.push(' '),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o)
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            ' ' if i == 0 => out.push_str("\\s"),
            _ => out.push(c),
        }
    }
    out
}

impl UserConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Emplacement par défaut : `<config utilisateur>/BellePoule/config.ini`
    /// (`%APPDATA%` sous Windows, `$XDG_CONFIG_HOME` ou `~/.config` ailleurs).
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("APPDATA")
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("BellePoule").join("config.ini")
    }

    /// Charge un fichier ; un fichier absent donne une configuration vide.
    pub fn load(path: &Path) -> Self {
        let mut cfg = std::fs::read_to_string(path).map(|t| Self::parse(&t)).unwrap_or_default();
        cfg.path = Some(path.to_path_buf());
        cfg
    }

    pub fn parse(text: &str) -> Self {
        let mut cfg = UserConfig::default();
        let mut current: Option<String> = None;
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                let g = trimmed[1..trimmed.len() - 1].to_string();
                cfg.group_mut(&g);
                current = Some(g);
            } else if let (Some(g), Some(eq)) = (&current, line.find('=')) {
                let key = line[..eq].trim().to_string();
                let value = unescape(line[eq + 1..].trim_start());
                cfg.set_string(&g.clone(), &key, &value);
            }
        }
        cfg
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (i, (g, keys)) in self.groups.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&format!("[{g}]\n"));
            for (k, v) in keys {
                out.push_str(&format!("{k}={}\n", escape(v)));
            }
        }
        out
    }

    /// Écrit le fichier d'origine (ou `path` si fourni).
    pub fn save(&self, path: Option<&Path>) -> std::io::Result<()> {
        let p = path.map(Path::to_path_buf).or_else(|| self.path.clone()).unwrap_or_else(Self::default_path);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(p, self.to_text())
    }

    /// `IsEmpty` : aucun groupe.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    fn group_mut(&mut self, group: &str) -> &mut Vec<(String, String)> {
        if let Some(i) = self.groups.iter().position(|(g, _)| g == group) {
            &mut self.groups[i].1
        } else {
            self.groups.push((group.to_string(), Vec::new()));
            &mut self.groups.last_mut().unwrap().1
        }
    }

    pub fn get_string(&self, group: &str, key: &str) -> Option<String> {
        self.groups
            .iter()
            .find(|(g, _)| g == group)
            .and_then(|(_, keys)| keys.iter().find(|(k, _)| k == key))
            .map(|(_, v)| v.clone())
    }

    pub fn set_string(&mut self, group: &str, key: &str, value: &str) {
        let keys = self.group_mut(group);
        match keys.iter_mut().find(|(k, _)| k == key) {
            Some(e) => e.1 = value.to_string(),
            None => keys.push((key.to_string(), value.to_string())),
        }
    }

    /// `g_key_file_get_integer` : 0 si absent ou invalide.
    pub fn get_integer(&self, group: &str, key: &str) -> i64 {
        self.get_string(group, key).map(|v| crate::attribute::parse_leading_int(&v)).unwrap_or(0)
    }

    pub fn set_integer(&mut self, group: &str, key: &str, value: i64) {
        self.set_string(group, key, &value.to_string());
    }

    /// Liste `a;b;c;` (séparateur `;`, terminateur final).
    pub fn get_string_list(&self, group: &str, key: &str) -> Option<Vec<String>> {
        self.get_string(group, key)
            .map(|v| v.split(';').filter(|s| !s.is_empty()).map(str::to_string).collect())
    }

    pub fn set_string_list(&mut self, group: &str, key: &str, values: &[&str]) {
        let mut s = String::new();
        for v in values {
            s.push_str(v);
            s.push(';');
        }
        self.set_string(group, key, &s);
    }

    pub fn remove_key(&mut self, group: &str, key: &str) {
        if let Some((_, keys)) = self.groups.iter_mut().find(|(g, _)| g == group) {
            keys.retain(|(k, _)| k != key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut c = UserConfig::new();
        c.set_string("Competiton", "default_name", " Open\nde Rennes");
        c.set_integer("Competiton", "color_to_use", 3);
        c.set_string_list("Checkin::Contest", "display", &["name", "first_name"]);
        let d = UserConfig::parse(&c.to_text());
        assert_eq!(d, UserConfig { path: None, ..c.clone() });
        assert_eq!(d.get_integer("Competiton", "color_to_use"), 3);
        assert_eq!(d.get_string_list("Checkin::Contest", "display").unwrap(), vec!["name", "first_name"]);
    }
}
