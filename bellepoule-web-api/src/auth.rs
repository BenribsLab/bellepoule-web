//! Sécurité : une clé d'API par client (club, comité…).
//!
//! - Chaque requête porte sa clé dans l'en-tête `X-Api-Key` (ou
//!   `Authorization: Bearer <clé>`). Le client n'accède qu'à ses compétitions.
//! - Un club peut aussi avoir des clés nommées (logiciel de bureau,
//!   applications tierces), créées par ses comptes ([`crate::accounts`]).
//! - Une session (connexion par e-mail et mot de passe) donne le même accès.
//! - La clé d'administration (`BP_ADMIN_KEY`) ou un compte administrateur
//!   crée et retire les clients.
//! - Les clés ne sont jamais enregistrées en clair : seulement leur empreinte
//!   SHA-256 (`clients.json` dans le dossier de données).

use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::api::{bad_request, internal, ApiError, Shared};

/// Client authentifié.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Client {
    pub id: String,
    pub name: String,
}

/// Client enregistré (empreinte de sa clé).
#[derive(Clone, Serialize, Deserialize)]
pub struct ClientRecord {
    pub id: String,
    pub name: String,
    pub key_hash: String,
    pub created: String,
    #[serde(default)]
    pub keys: Vec<KeyRecord>,
}

/// Clé nommée d'un club (empreinte seulement).
#[derive(Clone, Serialize, Deserialize)]
pub struct KeyRecord {
    pub id: String,
    pub name: String,
    pub hash: String,
    pub created: String,
}

pub fn hash_key(key: &str) -> String {
    Sha256::digest(key.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Nouvelle clé : 64 caractères hexadécimaux aléatoires.
pub fn new_key() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Comparaison en temps constant (pas d'indice sur la clé attendue).
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub(crate) fn presented_key(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(v) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        return Some(v.trim().to_string());
    }
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim().to_string())
}

pub(crate) fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "clé d'API absente ou invalide".into())
}

impl FromRequestParts<Shared> for Client {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        let key = presented_key(&parts.headers).filter(|k| !k.is_empty()).ok_or_else(unauthorized)?;
        let hash = hash_key(&key);
        let mut store = state.lock().await;
        if let Some(client) = store.client_by_hash(&hash) {
            return Ok(client);
        }
        let user = crate::accounts::session_user(&mut store, &hash).ok_or_else(unauthorized)?;
        user.client.as_deref().and_then(|id| store.client(id)).ok_or_else(unauthorized)
    }
}

/// Requête faite avec la clé d'administration.
pub struct Admin;

impl FromRequestParts<Shared> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        // Compte administrateur connecté.
        if let Some(key) = presented_key(&parts.headers) {
            let mut store = state.lock().await;
            if crate::accounts::session_user(&mut store, &hash_key(&key)).is_some_and(|u| u.admin) {
                return Ok(Admin);
            }
        }
        let expected = std::env::var("BP_ADMIN_KEY").ok().filter(|k| k.len() >= 32);
        let Some(expected) = expected else {
            return Err(ApiError(StatusCode::FORBIDDEN, "administration désactivée : définir BP_ADMIN_KEY (32 caractères au moins)".into()));
        };
        let key = presented_key(&parts.headers).ok_or_else(unauthorized)?;
        if same(&key, &expected) {
            Ok(Admin)
        } else {
            Err(unauthorized())
        }
    }
}

/// Le client de la clé présentée.
pub async fn me(client: Client) -> Json<Client> {
    Json(client)
}

#[derive(Deserialize)]
pub struct NewClient {
    name: String,
}

/// Client créé : la clé n'est montrée qu'une seule fois.
#[derive(Serialize)]
pub struct CreatedClient {
    id: String,
    name: String,
    key: String,
}

pub async fn create_client(_: Admin, State(store): State<Shared>, Json(input): Json<NewClient>) -> Result<(StatusCode, Json<CreatedClient>), ApiError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(bad_request("le nom du client est obligatoire"));
    }
    let key = new_key();
    let mut store = store.lock().await;
    let id = store.add_client(name, &hash_key(&key)).map_err(internal)?;
    Ok((StatusCode::CREATED, Json(CreatedClient { id, name: name.to_string(), key })))
}

#[derive(Serialize)]
pub struct ClientInfo {
    id: String,
    name: String,
    created: String,
    contests: usize,
    users: usize,
}

pub async fn list_clients(_: Admin, State(store): State<Shared>) -> Json<Vec<ClientInfo>> {
    let store = store.lock().await;
    Json(
        store
            .clients()
            .iter()
            .map(|c| ClientInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                created: c.created.clone(),
                contests: store.count_for(&c.id),
                users: store.users.iter().filter(|u| u.client.as_deref() == Some(c.id.as_str())).count(),
            })
            .collect(),
    )
}

/// Nouvelle clé pour un client (l'ancienne cesse de fonctionner).
pub async fn renew_key(_: Admin, State(store): State<Shared>, Path(id): Path<String>) -> Result<Json<CreatedClient>, ApiError> {
    let key = new_key();
    let mut store = store.lock().await;
    let name = store
        .set_client_key(&id, &hash_key(&key))
        .map_err(internal)?
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "client introuvable".into()))?;
    Ok(Json(CreatedClient { id, name, key }))
}

/// Retire l'accès d'un client (ses compétitions restent sur le disque).
pub async fn remove_client(_: Admin, State(store): State<Shared>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let mut store = store.lock().await;
    if store.remove_client(&id).map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(StatusCode::NOT_FOUND, "client introuvable".into()))
    }
}
