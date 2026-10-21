//! Fichier JSON sauvegardé en différé (port de `common/util/json_file.cpp`).
//!
//! Le propriétaire (« listener ») remplit l'objet racine à la sauvegarde et
//! le relit au chargement. `make_dirty` programme une sauvegarde 30 s plus
//! tard : sans boucle d'évènements dans le cœur, l'appelant interroge
//! [`JsonFile::save_due`] (ou sauvegarde directement).
//!
//! La sortie reproduit la mise en forme « pretty » de json-glib
//! (indentation de 2 espaces, `"clé" : valeur`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

/// Délai de sauvegarde après modification.
pub const SAVE_DELAY: Duration = Duration::from_secs(30);

pub trait JsonListener {
    /// `FeedJsonBuilder` : remplit l'objet racine.
    fn feed_json(&self, root: &mut Map<String, Value>);
    /// `ReadJson` : relit la racine ; renvoie vrai si la lecture a réussi.
    fn read_json(&mut self, root: &Value) -> bool;
}

#[derive(Debug, Clone)]
pub struct JsonFile {
    path: PathBuf,
    dirty_since: Option<Instant>,
}

impl JsonFile {
    pub fn new(path: impl AsRef<Path>) -> Self {
        JsonFile { path: path.as_ref().to_path_buf(), dirty_since: None }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `Load` : faux si le fichier est absent ou invalide.
    pub fn load(&self, listener: &mut dyn JsonListener) -> bool {
        let Ok(text) = std::fs::read_to_string(&self.path) else { return false };
        match serde_json::from_str::<Value>(&text) {
            Ok(v) => listener.read_json(&v),
            Err(_) => false,
        }
    }

    /// `MakeDirty` : n'arme le délai que s'il ne l'est pas déjà.
    pub fn make_dirty(&mut self) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_since.is_some()
    }

    /// Vrai si le délai de 30 s est écoulé depuis `make_dirty`.
    pub fn save_due(&self, now: Instant) -> bool {
        self.dirty_since.map(|t| now.duration_since(t) >= SAVE_DELAY).unwrap_or(false)
    }

    /// `Save`.
    pub fn save(&mut self, listener: &dyn JsonListener) -> std::io::Result<()> {
        self.dirty_since = None;
        let mut root = Map::new();
        listener.feed_json(&mut root);
        std::fs::write(&self.path, to_pretty_string(&Value::Object(root)))
    }
}

/// Mise en forme json-glib (`json_generator_set_pretty (TRUE)`).
pub fn to_pretty_string(v: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, v, 0);
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level * 2 {
        out.push(' ');
    }
}

fn write_value(out: &mut String, v: &Value, level: usize) {
    match v {
        Value::Object(map) => {
            out.push_str("{\n");
            let n = map.len();
            for (i, (k, val)) in map.iter().enumerate() {
                indent(out, level + 1);
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push_str(" : ");
                write_value(out, val, level + 1);
                if i + 1 < n {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, level);
            out.push('}');
        }
        Value::Array(items) => {
            out.push_str("[\n");
            let n = items.len();
            for (i, val) in items.iter().enumerate() {
                indent(out, level + 1);
                write_value(out, val, level + 1);
                if i + 1 < n {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, level);
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}
