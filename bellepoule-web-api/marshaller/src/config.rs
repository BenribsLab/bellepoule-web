//! Configuration utilisateur et emplacement des ressources
//! (port de `util/user_config.cpp` et `Global::_share_dir`).
//!
//! Le fichier `config.ini` (format GKeyFile) est rangé dans
//! `<config utilisateur>/BellePoule2D/`, à côté de `hall.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Dossier de configuration du Marshaller.
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("BELLEPOULE2D_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(feature = "gui")]
    {
        glib::user_config_dir().join("BellePoule2D")
    }
    #[cfg(not(feature = "gui"))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .or_else(|| std::env::var_os("LOCALAPPDATA"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(std::env::temp_dir);
        base.join("BellePoule2D")
    }
}

/// Dossier `resources` (variable `BELLEPOULE_RESOURCES`, à côté de
/// l'exécutable, dans un parent de l'exécutable, ou dans les sources).
pub fn resources_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("BELLEPOULE_RESOURCES") {
        return PathBuf::from(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        for _ in 0..5 {
            match dir {
                Some(d) => {
                    let candidate = d.join("resources");
                    if candidate.join("glade").is_dir() || candidate.join("translations").is_dir() {
                        return candidate;
                    }
                    dir = d.parent().map(Path::to_path_buf);
                }
                None => break,
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("resources")
}

/// Image de `resources/glade/images`.
pub fn image_path(name: &str) -> PathBuf {
    resources_dir().join("glade").join("images").join(name)
}

/// Fichier INI minimal compatible GKeyFile (groupes, clés, commentaires
/// conservés en tête de groupe ignorés).
#[derive(Debug, Clone, Default)]
pub struct UserConfig {
    path: Option<PathBuf>,
    groups: BTreeMap<String, BTreeMap<String, String>>,
}

impl UserConfig {
    /// `UserConfig::UserConfig`.
    pub fn load(path: &Path) -> Self {
        let mut cfg = UserConfig { path: Some(path.to_path_buf()), groups: BTreeMap::new() };
        if let Ok(text) = std::fs::read_to_string(path) {
            cfg.parse(&text);
        }
        cfg
    }

    pub fn parse(&mut self, text: &str) {
        let mut group = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(g) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                group = g.to_string();
            } else if let Some((k, v)) = line.split_once('=') {
                self.groups.entry(group.clone()).or_default().insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (g, entries) in &self.groups {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{g}]\n"));
            for (k, v) in entries {
                out.push_str(&format!("{k}={v}\n"));
            }
        }
        out
    }

    pub fn get(&self, group: &str, key: &str) -> Option<&str> {
        self.groups.get(group)?.get(key).map(String::as_str)
    }

    pub fn set(&mut self, group: &str, key: &str, value: &str) {
        self.groups.entry(group.to_string()).or_default().insert(key.to_string(), value.to_string());
    }

    /// `UserConfig::Save`.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(p) = &self.path {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(p, self.to_text())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_roundtrip() {
        let mut c = UserConfig::default();
        c.set("Tournament", "interface_language", "fr");
        c.set("Checkin", "default_import_dir_name", "C:\\x");
        let mut d = UserConfig::default();
        d.parse(&c.to_text());
        assert_eq!(d.get("Tournament", "interface_language"), Some("fr"));
        assert_eq!(d.get("Checkin", "default_import_dir_name"), Some("C:\\x"));
    }
}
