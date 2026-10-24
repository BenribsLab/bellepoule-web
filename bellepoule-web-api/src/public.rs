//! Publication des compétitions (compete.managecomp.org) : copie publique
//! d'une épreuve, gardée en archive même si le club supprime ou retire sa
//! compétition.
//!
//! ```text
//! <data>/public/index.json        épreuves publiées (club, adresse, options)
//! <data>/public/<id>.cotcot        copie de l'épreuve, mise à jour à chaque
//!                                 enregistrement (version web) ou envoi
//!                                 du logiciel de bureau
//! ```
//!
//! Adresses : `/<club>/<épreuve>` (lisible) et `/e/<id>` (QR code, court).
//! La compétition garde l'identifiant de sa publication
//! (`Contest::public_id`), dans la version web comme dans le logiciel.

use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use bp_core::contest::Contest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{bad_request, internal, not_found, ApiError, Shared};
use crate::auth::Client;
use crate::store::Store;

/// Une épreuve publiée.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Publication {
    /// Identifiant court (lien du QR code).
    pub id: String,
    /// Club propriétaire (seul à pouvoir la mettre à jour ou la retirer).
    pub club: String,
    pub club_name: String,
    pub club_slug: String,
    pub slug: String,
    pub title: String,
    /// « Fleuret · Dames · M15 ».
    pub subtitle: String,
    /// `aaaa-mm-jj`.
    pub date: String,
    pub location: String,
    /// `full` : noms complets ; `short` : prénom et initiale du nom.
    pub names: String,
    /// Page cachée aux moteurs de recherche.
    pub noindex: bool,
    pub created: u64,
    pub updated: u64,
}

/// Ce que la publication reprend de la compétition.
pub struct PublicMeta {
    pub public_id: Option<String>,
    title: String,
    subtitle: String,
    date: String,
    location: String,
}

impl PublicMeta {
    pub fn of(c: &Contest) -> PublicMeta {
        let subtitle = [c.weapon.label(), c.gender_label(), c.category.display_image(), c.label.clone()]
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        PublicMeta {
            public_id: c.public_id.clone(),
            title: c.name.trim().to_string(),
            subtitle,
            date: format!("{:04}-{:02}-{:02}", c.year, c.month, c.day),
            location: c.location.trim().to_string(),
        }
    }
}

/// Options choisies à la publication.
#[derive(Clone, Default, Deserialize)]
pub struct PublishOptions {
    #[serde(default)]
    pub names: Option<String>,
    #[serde(default)]
    pub noindex: Option<bool>,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Adresse lisible : minuscules sans accents, mots séparés par des tirets.
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let mapped: &str = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => "a",
            'ç' | 'Ç' => "c",
            'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => "e",
            'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => "i",
            'ñ' | 'Ñ' => "n",
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' => "o",
            'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => "u",
            'ý' | 'ÿ' | 'Ý' => "y",
            'œ' | 'Œ' => "oe",
            'æ' | 'Æ' => "ae",
            _ => "",
        };
        if !mapped.is_empty() {
            out.push_str(mapped);
        } else if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "competition".into() } else { out.chars().take(80).collect::<String>().trim_matches('-').to_string() }
}

/// Identifiant court (6 caractères, lettres et chiffres).
fn short_id() -> String {
    const ALPHABET: &[u8] = b"23456789abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";
    uuid::Uuid::new_v4().as_bytes().iter().take(6).map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
}

/// Racine des pages publiques vue par le navigateur : `compete.<domaine>`
/// derrière `app.`, `api.` ou `arbitre.<domaine>`, sinon `/compete/` sur le
/// même site (serveur local).
pub fn public_base(headers: &HeaderMap) -> String {
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("localhost");
    let proto = headers.get("x-forwarded-proto").and_then(|h| h.to_str().ok()).filter(|p| !p.is_empty()).unwrap_or("http");
    if host.starts_with("compete.") {
        return format!("{proto}://{host}/");
    }
    for prefix in ["app.", "api.", "arbitre.", "live."] {
        if let Some(domain) = host.strip_prefix(prefix) {
            return format!("{proto}://compete.{domain}/");
        }
    }
    format!("{proto}://{host}/compete/")
}

impl Store {
    fn public_dir(&self) -> PathBuf {
        self.dir.join("public")
    }

    /// Épreuves publiées (lues au premier accès).
    pub fn publications(&mut self) -> &mut Vec<Publication> {
        if self.publications.is_none() {
            let list = std::fs::read(self.public_dir().join("index.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            self.publications = Some(list);
        }
        self.publications.as_mut().unwrap()
    }

    fn save_publications(&mut self) -> std::io::Result<()> {
        let dir = self.public_dir();
        std::fs::create_dir_all(&dir)?;
        let data = serde_json::to_vec_pretty(self.publications())?;
        let tmp = dir.join("index.json.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(tmp, dir.join("index.json"))
    }

    /// Copie publique d'une épreuve.
    pub fn publication_file(&self, id: &str) -> PathBuf {
        self.public_dir().join(format!("{id}.cotcot"))
    }

    /// Publie (ou met à jour) l'épreuve : copie, adresse, options. Une
    /// publication d'un autre club n'est jamais touchée.
    pub fn publish(&mut self, client: &Client, meta: &PublicMeta, xml: &str, options: Option<&PublishOptions>) -> std::io::Result<Publication> {
        let existing = meta.public_id.as_deref().and_then(|id| self.publications().iter().position(|p| p.id == id && p.club == client.id));
        let index = match existing {
            Some(i) => i,
            None => {
                let mut id = short_id();
                while self.publications().iter().any(|p| p.id == id) {
                    id = short_id();
                }
                let others = self.publications().iter().filter(|p| p.club != client.id).map(|p| p.club_slug.clone()).collect::<Vec<_>>();
                let mut club_slug = slugify(&client.name);
                if others.contains(&club_slug) {
                    club_slug = format!("{club_slug}-{}", client.id.chars().take(4).collect::<String>());
                }
                let created = now();
                self.publications().push(Publication { id, club: client.id.clone(), club_slug, names: "full".into(), created, ..Default::default() });
                self.publications().len() - 1
            }
        };
        let mut p = self.publications()[index].clone();
        p.club_name = client.name.clone();
        p.title = meta.title.clone();
        p.subtitle = meta.subtitle.clone();
        p.date = meta.date.clone();
        p.location = meta.location.clone();
        if p.slug.is_empty() {
            let base = slugify(&format!("{} {} {}", p.date, p.title, p.subtitle));
            let taken = self.publications().iter().any(|o| o.id != p.id && o.club == p.club && o.slug == base);
            p.slug = if taken { format!("{base}-{}", p.id.to_lowercase()) } else { base };
        }
        if let Some(o) = options {
            if let Some(names) = o.names.as_deref().filter(|n| *n == "full" || *n == "short") {
                p.names = names.to_string();
            }
            if let Some(noindex) = o.noindex {
                p.noindex = noindex;
            }
        }
        p.updated = now();
        std::fs::create_dir_all(self.public_dir())?;
        let file = self.publication_file(&p.id);
        let tmp = file.with_extension("cotcot.tmp");
        std::fs::write(&tmp, xml)?;
        std::fs::rename(tmp, file)?;
        self.publications()[index] = p.clone();
        self.save_publications()?;
        Ok(p)
    }

    /// Après chaque enregistrement d'une compétition publiée : la copie
    /// publique suit.
    pub fn refresh_publication(&mut self, client: &Client, meta: &PublicMeta, xml: &str) {
        let Some(id) = meta.public_id.as_deref() else { return };
        if self.publications().iter().any(|p| p.id == id && p.club == client.id) {
            let _ = self.publish(client, meta, xml, None);
        }
    }

    /// Retire une épreuve publiée (copie effacée).
    pub fn unpublish(&mut self, client: &Client, id: &str) -> std::io::Result<bool> {
        let before = self.publications().len();
        self.publications().retain(|p| !(p.id == id && p.club == client.id));
        if self.publications().len() == before {
            return Ok(false);
        }
        let _ = std::fs::remove_file(self.publication_file(id));
        self.save_publications()?;
        Ok(true)
    }
}

/// État d'une publication pour les applications.
fn status(headers: &HeaderMap, p: Option<&Publication>) -> Value {
    let base = public_base(headers);
    match p {
        None => json!({ "published": false, "base": base }),
        Some(p) => json!({
            "published": true,
            "id": p.id,
            "url": format!("{base}{}/{}", p.club_slug, p.slug),
            "short_url": format!("{base}e/{}", p.id),
            "club_url": format!("{base}{}", p.club_slug),
            "names": p.names,
            "noindex": p.noindex,
            "updated": p.updated,
        }),
    }
}

// ------------------------------------------------- version web

/// `GET /api/contests/{id}/publication`.
pub async fn get_contest(State(store): State<Shared>, client: Client, headers: HeaderMap, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    let public_id = store.get(&client, &id).ok_or_else(not_found)?.public_id.clone();
    let p = public_id.and_then(|pid| store.publications().iter().find(|p| p.id == pid && p.club == client.id).cloned());
    Ok(Json(status(&headers, p.as_ref())))
}

/// `POST /api/contests/{id}/publication` `{ names, noindex }` : publie
/// l'épreuve, ou change ses options.
pub async fn publish_contest(State(store): State<Shared>, client: Client, headers: HeaderMap, Path(id): Path<String>, Json(o): Json<PublishOptions>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    let (meta, xml) = {
        let c = store.get_mut(&client, &id).ok_or_else(not_found)?;
        (PublicMeta::of(c), c.to_xml())
    };
    let p = store.publish(&client, &meta, &xml, Some(&o)).map_err(internal)?;
    let pid = p.id.clone();
    store.update(&client, &id, |c| c.public_id = Some(pid)).ok_or_else(not_found)?.map_err(internal)?;
    Ok(Json(status(&headers, Some(&p))))
}

/// `DELETE /api/contests/{id}/publication` : l'épreuve n'est plus publiée.
pub async fn unpublish_contest(State(store): State<Shared>, client: Client, headers: HeaderMap, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    let public_id = store.get(&client, &id).ok_or_else(not_found)?.public_id.clone();
    if let Some(pid) = public_id {
        store.unpublish(&client, &pid).map_err(internal)?;
    }
    store.update(&client, &id, |c| c.public_id = None).ok_or_else(not_found)?.map_err(internal)?;
    Ok(Json(status(&headers, None)))
}

// ------------------------------------------------- logiciel de bureau

#[derive(Deserialize)]
pub struct DesktopPublish {
    /// Fichier de la compétition (`.cotcot`).
    xml: String,
    #[serde(flatten)]
    options: PublishOptions,
}

/// `PUT /api/publication` (clé d'API du club) : le logiciel publie ou met à
/// jour une épreuve ; l'identifiant renvoyé est gardé dans la compétition.
pub async fn publish_desktop(State(store): State<Shared>, client: Client, headers: HeaderMap, Json(d): Json<DesktopPublish>) -> Result<Json<Value>, ApiError> {
    let mut contest = Contest::new(None, false);
    contest.load_xml_str(&d.xml).map_err(|e| bad_request(e.to_string()))?;
    let meta = PublicMeta::of(&contest);
    let mut store = store.lock().await;
    let p = store.publish(&client, &meta, &d.xml, Some(&d.options)).map_err(internal)?;
    Ok(Json(status(&headers, Some(&p))))
}

/// `DELETE /api/publication/{id}` (clé d'API du club).
pub async fn unpublish_desktop(State(store): State<Shared>, client: Client, headers: HeaderMap, Path(id): Path<String>) -> Result<(StatusCode, Json<Value>), ApiError> {
    let mut store = store.lock().await;
    if !store.unpublish(&client, &id).map_err(internal)? {
        return Err(not_found());
    }
    Ok((StatusCode::OK, Json(status(&headers, None))))
}
