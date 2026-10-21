//! Fichier source d'une liste de tireurs (port de `actors/source.cpp`).
//!
//! Mémorise le fichier importé et son empreinte SHA-1 pour proposer le
//! rechargement si le fichier a changé (`HasUpdates`). Sérialisé dans
//! l'attribut `Source` de `<Pointage>` sous la forme `<sha1>-<chemin>`.

use sha1::{Digest, Sha1};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Source {
    url: Option<String>,
    sha1: Option<String>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Source {
    pub fn new() -> Self {
        Self::default()
    }

    /// `ComputeSha1` : empreinte du contenu du fichier.
    pub fn compute_sha1(url: &str) -> Option<String> {
        let content = std::fs::read(url).ok()?;
        // `g_compute_checksum_for_string (…, -1)` s'arrête au premier octet nul.
        let end = content.iter().position(|b| *b == 0).unwrap_or(content.len());
        Some(hex(&Sha1::digest(&content[..end])))
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// `SetUrl` : l'empreinte n'est mémorisée que pour le premier fichier ou
    /// pour le même fichier.
    pub fn set_url(&mut self, url: &str) {
        self.sha1 = None;
        if self.url.is_none() || self.url.as_deref() == Some(url) {
            self.sha1 = Self::compute_sha1(url);
            if self.sha1.is_some() {
                self.url = Some(url.to_string());
            }
        }
    }

    /// `HasUpdates` : le fichier a-t-il changé depuis l'import ?
    pub fn has_updates(&self) -> bool {
        match (&self.sha1, &self.url) {
            (Some(old), Some(url)) => Self::compute_sha1(url).map(|n| &n != old).unwrap_or(false),
            _ => false,
        }
    }

    /// `Serialize` : `<sha1>-<url>`.
    pub fn serialize(&self) -> Option<String> {
        match (&self.sha1, &self.url) {
            (Some(s), Some(u)) => Some(format!("{s}-{u}")),
            _ => None,
        }
    }

    /// `Load`.
    pub fn load(&mut self, data: Option<&str>) {
        if let Some(d) = data {
            if let Some((s, u)) = d.split_once('-') {
                self.sha1 = Some(s.to_string());
                self.url = Some(u.to_string());
            }
        }
    }
}
