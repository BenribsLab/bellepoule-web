//! Comptes utilisateurs : inscription, validation par un administrateur,
//! connexion par e-mail et mot de passe, clés nommées du club.
//!
//! - Les mots de passe sont hachés avec Argon2id (jamais gardés en clair).
//! - Une inscription reste « en attente » jusqu'à sa validation, qui la
//!   rattache à un club (nouveau ou existant). `BP_ADMIN_EMAIL` : l'adresse
//!   du premier administrateur, validé dès son inscription.
//! - Une connexion ouvre une session : un jeton aléatoire, dont seule
//!   l'empreinte est gardée, valable 30 jours sans usage.
//! - Après 5 échecs de connexion pour une adresse, elle est bloquée 15 minutes.

use std::time::{Duration, Instant};

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::{bad_request, internal, ApiError, Shared};
use crate::auth::{hash_key, new_key, presented_key, unauthorized, Admin};
use crate::store::{chrono_now, Store};

const SESSION_IDLE: Duration = Duration::from_secs(30 * 24 * 3600);
const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Active,
    Disabled,
}

/// Compte enregistré.
#[derive(Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: String,
    pub email: String,
    pub name: String,
    /// Club indiqué à l'inscription.
    pub club_name: String,
    pub password_hash: String,
    pub status: Status,
    #[serde(default)]
    pub admin: bool,
    /// Club rattaché (après validation).
    #[serde(default)]
    pub client: Option<String>,
    pub created: String,
    #[serde(default)]
    pub last_login: Option<String>,
}

pub struct Session {
    pub user: String,
    pub last_used: Instant,
}

/// Utilisateur de la session (mise à jour de sa dernière utilisation).
pub fn session_user(store: &mut Store, hash: &str) -> Option<UserRecord> {
    let session = store.sessions.get_mut(hash)?;
    if session.last_used.elapsed() > SESSION_IDLE {
        store.sessions.remove(hash);
        return None;
    }
    session.last_used = Instant::now();
    let id = session.user.clone();
    store.users.iter().find(|u| u.id == id && u.status == Status::Active).cloned()
}

/// Requête faite par un compte connecté.
pub struct Account(pub UserRecord);

impl FromRequestParts<Shared> for Account {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        let key = presented_key(&parts.headers).filter(|k| !k.is_empty()).ok_or_else(unauthorized)?;
        let mut store = state.lock().await;
        session_user(&mut store, &hash_key(&key)).map(Account).ok_or_else(unauthorized)
    }
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(password.as_bytes(), &salt).map(|h| h.to_string()).map_err(internal)
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

/// Empreinte de référence : même durée de vérification pour une adresse inconnue.
fn dummy_hash() -> &'static str {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| hash_password("mot de passe factice").unwrap_or_default())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f).await.map_err(internal)
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else { return false };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && email.len() <= 254 && !email.contains(char::is_whitespace)
}

fn check_password(password: &str) -> Result<(), ApiError> {
    let n = password.chars().count();
    if n < 10 {
        return Err(bad_request("le mot de passe doit faire 10 caractères au moins"));
    }
    if n > 200 {
        return Err(bad_request("mot de passe trop long"));
    }
    Ok(())
}

fn admin_email() -> Option<String> {
    std::env::var("BP_ADMIN_EMAIL").ok().map(|e| normalize_email(&e)).filter(|e| !e.is_empty())
}

// ------------------------------------------------------------ inscription

#[derive(Deserialize)]
pub struct Register {
    email: String,
    password: String,
    name: String,
    club: String,
}

#[derive(Serialize)]
pub struct Registered {
    status: Status,
}

pub async fn register(State(store): State<Shared>, Json(input): Json<Register>) -> Result<(StatusCode, Json<Registered>), ApiError> {
    let email = normalize_email(&input.email);
    let (name, club) = (input.name.trim().to_string(), input.club.trim().to_string());
    if !valid_email(&email) {
        return Err(bad_request("adresse e-mail invalide"));
    }
    if name.is_empty() || club.is_empty() {
        return Err(bad_request("le nom et le club sont obligatoires"));
    }
    if name.chars().count() > 100 || club.chars().count() > 100 {
        return Err(bad_request("nom ou club trop long"));
    }
    check_password(&input.password)?;
    let password = input.password;
    let hash = blocking(move || hash_password(&password)).await??;
    let mut store = store.lock().await;
    if store.users.iter().any(|u| u.email == email) {
        return Err(ApiError(StatusCode::CONFLICT, "un compte existe déjà avec cette adresse".into()));
    }
    // Premier administrateur : validé et doté de son club dès l'inscription.
    let first_admin = admin_email().as_deref() == Some(email.as_str());
    let client = if first_admin { Some(store.add_client(&club, "").map_err(internal)?) } else { None };
    let status = if first_admin { Status::Active } else { Status::Pending };
    store.users.push(UserRecord {
        id: uuid::Uuid::new_v4().simple().to_string(),
        email,
        name,
        club_name: club,
        password_hash: hash,
        status,
        admin: first_admin,
        client,
        created: chrono_now(),
        last_login: None,
    });
    store.save_users().map_err(internal)?;
    Ok((StatusCode::CREATED, Json(Registered { status })))
}

// --------------------------------------------------------------- connexion

#[derive(Deserialize)]
pub struct Login {
    email: String,
    password: String,
}

#[derive(Serialize)]
pub struct LoggedIn {
    token: String,
    user: UserInfo,
}

/// Compte vu par lui-même ou par un administrateur.
#[derive(Serialize)]
pub struct UserInfo {
    id: String,
    email: String,
    name: String,
    club_name: String,
    status: Status,
    admin: bool,
    client: Option<String>,
    client_name: Option<String>,
    created: String,
    last_login: Option<String>,
}

fn info(store: &Store, u: &UserRecord) -> UserInfo {
    UserInfo {
        id: u.id.clone(),
        email: u.email.clone(),
        name: u.name.clone(),
        club_name: u.club_name.clone(),
        status: u.status,
        admin: u.admin,
        client: u.client.clone(),
        client_name: u.client.as_deref().and_then(|c| store.client(c)).map(|c| c.name),
        created: u.created.clone(),
        last_login: u.last_login.clone(),
    }
}

pub async fn login(State(store): State<Shared>, Json(input): Json<Login>) -> Result<Json<LoggedIn>, ApiError> {
    let email = normalize_email(&input.email);
    let hash = {
        let mut store = store.lock().await;
        if let Some((count, since)) = store.failures.get(&email).copied() {
            if since.elapsed() > LOCKOUT {
                store.failures.remove(&email);
            } else if count >= MAX_FAILURES {
                return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "trop d'essais : réessayez dans 15 minutes".into()));
            }
        }
        store.users.iter().find(|u| u.email == email).map(|u| u.password_hash.clone())
    };
    let known = hash.is_some();
    let password = input.password;
    let hash = hash.unwrap_or_else(|| dummy_hash().to_string());
    let ok = blocking(move || verify_password(&password, &hash)).await? && known;
    let mut store = store.lock().await;
    if !ok {
        let entry = store.failures.entry(email).or_insert((0, Instant::now()));
        entry.0 += 1;
        return Err(ApiError(StatusCode::UNAUTHORIZED, "e-mail ou mot de passe incorrect".into()));
    }
    store.failures.remove(&email);
    let user = store.users.iter_mut().find(|u| u.email == email).ok_or_else(unauthorized)?;
    match user.status {
        Status::Pending => return Err(ApiError(StatusCode::FORBIDDEN, "compte en attente de validation par un administrateur".into())),
        Status::Disabled => return Err(ApiError(StatusCode::FORBIDDEN, "compte désactivé".into())),
        Status::Active => {}
    }
    user.last_login = Some(chrono_now());
    let user = user.clone();
    store.save_users().map_err(internal)?;
    let token = new_key();
    store.sessions.insert(hash_key(&token), Session { user: user.id.clone(), last_used: Instant::now() });
    let user = info(&store, &user);
    Ok(Json(LoggedIn { token, user }))
}

pub async fn logout(headers: axum::http::HeaderMap, State(store): State<Shared>) -> StatusCode {
    if let Some(key) = presented_key(&headers) {
        store.lock().await.sessions.remove(&hash_key(&key));
    }
    StatusCode::NO_CONTENT
}

pub async fn me(State(store): State<Shared>, Account(user): Account) -> Json<UserInfo> {
    let store = store.lock().await;
    Json(info(&store, &user))
}

#[derive(Deserialize)]
pub struct NewPassword {
    current: String,
    password: String,
}

pub async fn change_password(State(store): State<Shared>, Account(user): Account, Json(input): Json<NewPassword>) -> Result<StatusCode, ApiError> {
    check_password(&input.password)?;
    let (current, hash) = (input.current, user.password_hash.clone());
    if !blocking(move || verify_password(&current, &hash)).await? {
        return Err(bad_request("mot de passe actuel incorrect"));
    }
    let password = input.password;
    let hash = blocking(move || hash_password(&password)).await??;
    let mut store = store.lock().await;
    if let Some(u) = store.users.iter_mut().find(|u| u.id == user.id) {
        u.password_hash = hash;
    }
    store.save_users().map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------- clés du club

#[derive(Serialize)]
pub struct KeyInfo {
    id: String,
    name: String,
    created: String,
}

/// Clé créée : elle n'est montrée qu'une seule fois.
#[derive(Serialize)]
pub struct CreatedKey {
    id: String,
    name: String,
    key: String,
}

fn club_of(user: &UserRecord) -> Result<&str, ApiError> {
    user.client.as_deref().ok_or_else(|| bad_request("compte sans club"))
}

pub async fn list_keys(State(store): State<Shared>, Account(user): Account) -> Result<Json<Vec<KeyInfo>>, ApiError> {
    let store = store.lock().await;
    let keys = store.keys(club_of(&user)?);
    Ok(Json(keys.into_iter().map(|k| KeyInfo { id: k.id, name: k.name, created: k.created }).collect()))
}

#[derive(Deserialize)]
pub struct NewKey {
    name: String,
}

pub async fn create_key(State(store): State<Shared>, Account(user): Account, Json(input): Json<NewKey>) -> Result<(StatusCode, Json<CreatedKey>), ApiError> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(bad_request("donnez un nom à la clé (80 caractères au plus)"));
    }
    let key = new_key();
    let mut store = store.lock().await;
    let record = store.add_key(club_of(&user)?, name, &hash_key(&key)).map_err(internal)?.ok_or_else(|| bad_request("club introuvable"))?;
    Ok((StatusCode::CREATED, Json(CreatedKey { id: record.id, name: record.name, key })))
}

pub async fn remove_key(State(store): State<Shared>, Account(user): Account, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let mut store = store.lock().await;
    if store.remove_key(club_of(&user)?, &id).map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(StatusCode::NOT_FOUND, "clé introuvable".into()))
    }
}

// --------------------------------------------------------- administration

pub async fn list_users(_: Admin, State(store): State<Shared>) -> Json<Vec<UserInfo>> {
    let store = store.lock().await;
    Json(store.users.iter().map(|u| info(&store, u)).collect())
}

#[derive(Deserialize)]
pub struct Approve {
    /// Club existant ; sinon un club est créé au nom donné à l'inscription.
    #[serde(default)]
    club: Option<String>,
}

pub async fn approve(_: Admin, State(store): State<Shared>, Path(id): Path<String>, Json(input): Json<Approve>) -> Result<Json<UserInfo>, ApiError> {
    let mut store = store.lock().await;
    let user = store.users.iter().find(|u| u.id == id).cloned().ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "compte introuvable".into()))?;
    let client = match input.club.filter(|c| !c.is_empty()) {
        Some(c) if store.client(&c).is_some() => c,
        Some(_) => return Err(bad_request("club introuvable")),
        None => match user.client.clone() {
            Some(c) => c,
            None => store.add_client(&user.club_name, "").map_err(internal)?,
        },
    };
    let u = store.users.iter_mut().find(|u| u.id == id).ok_or_else(unauthorized)?;
    u.client = Some(client);
    u.status = Status::Active;
    let u = u.clone();
    store.save_users().map_err(internal)?;
    Ok(Json(info(&store, &u)))
}

#[derive(Deserialize)]
pub struct Change {
    #[serde(default)]
    status: Option<Status>,
    #[serde(default)]
    admin: Option<bool>,
}

/// Active ou désactive un compte, donne ou retire le rôle d'administrateur.
pub async fn update_user(_: Admin, State(store): State<Shared>, Path(id): Path<String>, Json(input): Json<Change>) -> Result<Json<UserInfo>, ApiError> {
    let mut store = store.lock().await;
    let admins = store.users.iter().filter(|u| u.admin && u.status == Status::Active).count();
    let u = store.users.iter_mut().find(|u| u.id == id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "compte introuvable".into()))?;
    let loses_admin = u.admin && u.status == Status::Active && (input.admin == Some(false) || input.status.is_some_and(|s| s != Status::Active));
    if loses_admin && admins <= 1 && std::env::var("BP_ADMIN_KEY").map(|k| k.len() < 32).unwrap_or(true) {
        return Err(bad_request("il doit rester au moins un administrateur"));
    }
    if let Some(status) = input.status {
        if status == Status::Active && u.client.is_none() {
            return Err(bad_request("validez d'abord le compte (choix du club)"));
        }
        u.status = status;
    }
    if let Some(admin) = input.admin {
        u.admin = admin;
    }
    let u = u.clone();
    if u.status != Status::Active {
        store.sessions.retain(|_, s| s.user != u.id);
    }
    store.save_users().map_err(internal)?;
    Ok(Json(info(&store, &u)))
}

/// Supprime un compte (son club et ses compétitions restent).
pub async fn remove_user(_: Admin, State(store): State<Shared>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let mut store = store.lock().await;
    let before = store.users.len();
    store.users.retain(|u| u.id != id);
    if store.users.len() == before {
        return Err(ApiError(StatusCode::NOT_FOUND, "compte introuvable".into()));
    }
    store.sessions.retain(|_, s| s.user != id);
    store.save_users().map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Chiffres du serveur et compétitions de tous les clubs.
pub async fn stats(_: Admin, State(store): State<Shared>) -> Json<serde_json::Value> {
    let store = store.lock().await;
    let count = |s: Status| store.users.iter().filter(|u| u.status == s).count();
    let clubs: std::collections::HashMap<&str, &str> = store.clients().iter().map(|c| (c.id.as_str(), c.name.as_str())).collect();
    let mut fencers = 0;
    let contests: Vec<serde_json::Value> = store
        .contests
        .iter()
        .map(|((club, id), c)| {
            let summary = crate::contests::summary(id, c);
            let mut v = serde_json::to_value(&summary).unwrap_or_default();
            fencers += v["fencers"].as_u64().unwrap_or(0);
            v["club"] = serde_json::json!(clubs.get(club.as_str()).copied().unwrap_or("?"));
            v
        })
        .collect();
    let live: usize = store.live.values().map(|pistes| pistes.len()).sum();
    Json(serde_json::json!({
        "users": { "total": store.users.len(), "pending": count(Status::Pending), "active": count(Status::Active), "disabled": count(Status::Disabled) },
        "sessions": store.sessions.len(),
        "clubs": store.clients().len(),
        "contests": contests.len(),
        "fencers": fencers,
        "live": live,
        "list": contests,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_checks() {
        assert!(valid_email("club@exemple.fr"));
        assert!(!valid_email("club@exemple"));
        assert!(!valid_email("@exemple.fr"));
        assert!(!valid_email("club @exemple.fr"));
        assert_eq!(normalize_email("  Club@Exemple.FR "), "club@exemple.fr");
    }

    #[test]
    fn password_round_trip() {
        let hash = hash_password("correct horse battery").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("wrong horse battery", &hash));
        assert!(check_password("court").is_err());
    }

    #[tokio::test]
    async fn register_approve_login() {
        let dir = std::env::temp_dir().join(format!("mc-accounts-{}", uuid::Uuid::new_v4().simple()));
        let store: Shared = std::sync::Arc::new(tokio::sync::Mutex::new(Store::open(dir.clone()).unwrap()));
        let body = |email: &str| Register { email: email.into(), password: "un mot de passe".into(), name: "Lina".into(), club: "Yerres CE".into() };
        let (code, Json(r)) = register(State(store.clone()), Json(body("lina@exemple.fr"))).await.ok().unwrap();
        assert_eq!((code, r.status), (StatusCode::CREATED, Status::Pending));
        assert!(register(State(store.clone()), Json(body("LINA@exemple.fr"))).await.is_err(), "adresse déjà prise");
        let login_body = || Login { email: "lina@exemple.fr".into(), password: "un mot de passe".into() };
        let refused = login(State(store.clone()), Json(login_body())).await.err().unwrap();
        assert_eq!(refused.0, StatusCode::FORBIDDEN, "en attente de validation");
        let id = store.lock().await.users[0].id.clone();
        approve(Admin, State(store.clone()), Path(id), Json(Approve { club: None })).await.ok().unwrap();
        let Json(ok) = login(State(store.clone()), Json(login_body())).await.ok().unwrap();
        let mut s = store.lock().await;
        let user = session_user(&mut s, &hash_key(&ok.token)).unwrap();
        assert_eq!(s.client(user.client.as_deref().unwrap()).unwrap().name, "Yerres CE");
        drop(s);
        for _ in 0..MAX_FAILURES {
            let _ = login(State(store.clone()), Json(Login { email: "lina@exemple.fr".into(), password: "faux".into() })).await;
        }
        let blocked = login(State(store.clone()), Json(login_body())).await.err().unwrap();
        assert_eq!(blocked.0, StatusCode::TOO_MANY_REQUESTS);
        let _ = std::fs::remove_dir_all(dir);
    }
}
