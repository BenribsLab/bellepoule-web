//! Configuration utilisateur (port de `common/util/user_config.cpp`) :
//! `<dossier de configuration>/<application>/config.ini`, sauvegardé à la
//! destruction (sauf en lecture seule).
//!
//! Groupes/clés utilisés par le C++ pour les imports et exports :
//! `[Checkin] default_import_dir_name`, `[CheckinSupervisor]
//! default_import_dir_name` (classements), `[Competiton] default_dir_name`
//! et `export_dir` (orthographe d'origine conservée).

use std::path::{Path, PathBuf};

use crate::formats::key_file::KeyFile;

/// `g_get_user_config_dir` : `%LOCALAPPDATA%` sous Windows,
/// `$XDG_CONFIG_HOME` ou `~/.config` ailleurs.
pub fn user_config_dir() -> PathBuf {
    if cfg!(windows) {
        if let Some(d) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(d);
        }
    }
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default();
    PathBuf::from(home).join(".config")
}

#[derive(Debug)]
pub struct UserConfig {
    pub key_file: KeyFile,
    file_path: PathBuf,
    read_only: bool,
}

impl UserConfig {
    /// `UserConfig (app_name, read_only)`.
    pub fn new(app_name: &str, read_only: bool) -> Self {
        Self::with_dir(&user_config_dir().join(app_name), read_only)
    }

    /// Variante avec dossier explicite (tests, configuration portable).
    pub fn with_dir(dir: &Path, read_only: bool) -> Self {
        let file_path = dir.join("config.ini");
        let key_file = match KeyFile::load_from_file(&file_path) {
            Ok(k) => k,
            Err(_) => {
                let _ = std::fs::create_dir_all(dir);
                KeyFile::new()
            }
        };
        UserConfig { key_file, file_path, read_only }
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }

    pub fn is_empty(&self) -> bool {
        self.key_file.is_empty()
    }

    pub fn save(&self) -> std::io::Result<()> {
        if self.read_only {
            return Ok(());
        }
        std::fs::write(&self.file_path, self.key_file.to_data())
    }
}

impl Drop for UserConfig {
    fn drop(&mut self) {
        if let Err(e) = self.save() {
            log::error!("sauvegarde de {} impossible : {e}", self.file_path.display());
        }
    }
}
