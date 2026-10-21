//! Données du serveur : les clients (clés d'API) et leurs compétitions.
//!
//! ```text
//! <data>/clients.json                     clients et empreintes de leurs clés
//! <data>/clients/<client>/<id>.cotcot     compétitions d'un client (format BellePoule)
//! ```
//! Un client ne voit que les compétitions de son dossier.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bp_core::contest::Contest;

use crate::auth::{Client, ClientRecord};

pub struct Store {
    pub(crate) dir: PathBuf,
    clients: Vec<ClientRecord>,
    /// (client, compétition) → compétition.
    pub(crate) contests: BTreeMap<(String, String), Contest>,
    /// Organisation (pistes, arbitres) de chaque client.
    pub(crate) organisations: std::collections::HashMap<String, crate::organisation::Organisation>,
    /// Relais SmartPoule de chaque client.
    pub(crate) relays: std::collections::HashMap<String, crate::smartpoule::Relay>,
}

impl Store {
    /// Ouvre le dossier de données (créé au besoin) et relit tout.
    pub fn open(dir: PathBuf) -> std::io::Result<Store> {
        std::fs::create_dir_all(dir.join("clients"))?;
        let clients: Vec<ClientRecord> = match std::fs::read_to_string(dir.join("clients.json")) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        let mut contests = BTreeMap::new();
        for client in &clients {
            let cdir = dir.join("clients").join(&client.id);
            let Ok(entries) = std::fs::read_dir(&cdir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("cotcot") {
                    continue;
                }
                let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
                let mut contest = Contest::new(None, false);
                match contest.load_file(&path) {
                    Ok(()) => {
                        contests.insert((client.id.clone(), id), contest);
                    }
                    Err(e) => tracing::warn!("{} : {e}", path.display()),
                }
            }
        }
        tracing::info!("{} client(s), {} compétition(s) chargés depuis {}", clients.len(), contests.len(), dir.display());
        Ok(Store { dir, clients, contests, organisations: Default::default(), relays: Default::default() })
    }

    // ------------------------------------------------------------- clients

    pub fn clients(&self) -> &[ClientRecord] {
        &self.clients
    }

    pub fn client_by_hash(&self, hash: &str) -> Option<Client> {
        self.clients.iter().find(|c| c.key_hash == hash).map(|c| Client { id: c.id.clone(), name: c.name.clone() })
    }

    pub fn count_for(&self, client: &str) -> usize {
        self.contests.keys().filter(|(c, _)| c == client).count()
    }

    pub fn add_client(&mut self, name: &str, key_hash: &str) -> std::io::Result<String> {
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.clients.push(ClientRecord {
            id: id.clone(),
            name: name.to_string(),
            key_hash: key_hash.to_string(),
            created: chrono_now(),
        });
        std::fs::create_dir_all(self.dir.join("clients").join(&id))?;
        self.save_clients()?;
        Ok(id)
    }

    pub fn set_client_key(&mut self, id: &str, key_hash: &str) -> std::io::Result<Option<String>> {
        let Some(c) = self.clients.iter_mut().find(|c| c.id == id) else { return Ok(None) };
        c.key_hash = key_hash.to_string();
        let name = c.name.clone();
        self.save_clients()?;
        Ok(Some(name))
    }

    pub fn remove_client(&mut self, id: &str) -> std::io::Result<bool> {
        let before = self.clients.len();
        self.clients.retain(|c| c.id != id);
        if self.clients.len() == before {
            return Ok(false);
        }
        self.contests.retain(|(c, _), _| c != id);
        self.save_clients()?;
        Ok(true)
    }

    fn save_clients(&self) -> std::io::Result<()> {
        let path = self.dir.join("clients.json");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&self.clients).unwrap_or_default())?;
        std::fs::rename(tmp, path)
    }

    // -------------------------------------------------------- compétitions

    pub fn list<'a>(&'a self, client: &'a Client) -> impl Iterator<Item = (&'a String, &'a Contest)> {
        self.contests.iter().filter(move |((c, _), _)| *c == client.id).map(|((_, id), contest)| (id, contest))
    }

    pub fn get(&self, client: &Client, id: &str) -> Option<&Contest> {
        self.contests.get(&(client.id.clone(), id.to_string()))
    }

    /// Accès en écriture sans enregistrement (lectures qui recalculent).
    pub fn get_mut(&mut self, client: &Client, id: &str) -> Option<&mut Contest> {
        self.contests.get_mut(&(client.id.clone(), id.to_string()))
    }

    /// Ajoute une compétition et l'enregistre ; renvoie son identifiant.
    pub fn insert(&mut self, client: &Client, contest: Contest) -> std::io::Result<String> {
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.contests.insert((client.id.clone(), id.clone()), contest);
        self.save(client, &id)?;
        Ok(id)
    }

    /// Modifie une compétition puis l'enregistre.
    pub fn update<R>(&mut self, client: &Client, id: &str, f: impl FnOnce(&mut Contest) -> R) -> Option<std::io::Result<R>> {
        let contest = self.contests.get_mut(&(client.id.clone(), id.to_string()))?;
        let result = f(contest);
        Some(self.save(client, id).map(|()| result))
    }

    pub fn remove(&mut self, client: &Client, id: &str) -> std::io::Result<bool> {
        if self.contests.remove(&(client.id.clone(), id.to_string())).is_none() {
            return Ok(false);
        }
        let path = self.path(client, id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(true)
    }

    fn path(&self, client: &Client, id: &str) -> PathBuf {
        self.dir.join("clients").join(&client.id).join(format!("{id}.cotcot"))
    }

    pub(crate) fn save(&mut self, client: &Client, id: &str) -> std::io::Result<()> {
        let path = self.path(client, id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let Some(contest) = self.contests.get_mut(&(client.id.clone(), id.to_string())) else { return Ok(()) };
        // Écriture atomique : fichier temporaire puis renommage.
        let tmp = path.with_extension("cotcot.tmp");
        std::fs::write(&tmp, contest.to_xml())?;
        std::fs::rename(tmp, path)
    }
}

/// Date et heure courantes (UTC), pour l'information.
fn chrono_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{secs}")
}
