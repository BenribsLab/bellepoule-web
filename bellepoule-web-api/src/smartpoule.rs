//! SmartPoule : relais entre BellePoule et les téléphones des arbitres.
//!
//! BellePoule (bureau) et les téléphones ne sont pas forcément sur le même
//! réseau : ils passent tous deux par ce serveur.
//!
//! - **BellePoule** (clé d'API du club) dépose un instantané
//!   (`PUT /api/smartpoule/state` : arbitres et leurs clés, matchs à
//!   arbitrer, feuilles), relève les scores saisis
//!   (`GET /api/smartpoule/pending`) et rend compte de leur application
//!   (`POST /api/smartpoule/results`).
//! - **Le téléphone** (identifiant du club, référence et clé de l'arbitre,
//!   lus dans son QR code) consulte ses matchs et leurs feuilles et envoie
//!   les scores (`/api/smartpoule/phone/…`).
//! - Les compétitions gérées directement par la version web sont servies
//!   sans relais (lecture et écriture immédiates).

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{bad_request, ApiError, Shared};
use crate::auth::Client;

/// Score saisi sur un téléphone, en attente de BellePoule.
#[derive(Clone, Serialize, Deserialize)]
struct Submission {
    id: u64,
    token: String,
    bout: u64,
    values: Option<[String; 2]>,
    officiating: Option<bp_core::officiating::Officiating>,
    referee: u32,
    /// `None` : en attente ; `Some(Ok)` / `Some(Err(message))` : traité.
    result: Option<Result<(), String>>,
}

/// État du relais d'un club.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Relay {
    /// Dernier instantané déposé par BellePoule.
    state: Value,
    submissions: Vec<Submission>,
    next_id: u64,
}

fn forbidden() -> ApiError {
    ApiError(StatusCode::FORBIDDEN, "QR code inconnu : demandez un nouveau QR code à l'organisation".into())
}

// --------------------------------------------------------------- BellePoule

/// `PUT /api/smartpoule/state`.
pub async fn put_state(State(store): State<Shared>, client: Client, Json(state): Json<Value>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    store.ensure_relay(&client.id);
    store.relays.entry(client.id.clone()).or_default().state = state;
    store.save_relay(&client.id)?;
    Ok(Json(json!({ "club": client.id })))
}

/// `GET /api/smartpoule/pending` : scores à appliquer.
pub async fn pending(State(store): State<Shared>, client: Client) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    store.ensure_relay(&client.id);
    let relay = store.relays.entry(client.id.clone()).or_default();
    let list: Vec<Value> = relay
        .submissions
        .iter()
        .filter(|s| s.result.is_none())
        .map(|s| json!({ "id": s.id, "token": s.token, "bout": s.bout, "values": s.values, "officiating": s.officiating, "referee": s.referee }))
        .collect();
    Ok(Json(json!(list)))
}

#[derive(Deserialize)]
pub struct Results {
    results: Vec<ResultEntry>,
}

#[derive(Deserialize)]
pub struct ResultEntry {
    id: u64,
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

/// `POST /api/smartpoule/results` : scores appliqués (ou refusés).
pub async fn results(State(store): State<Shared>, client: Client, Json(r): Json<Results>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    store.ensure_relay(&client.id);
    let relay = store.relays.entry(client.id.clone()).or_default();
    for e in r.results {
        if let Some(s) = relay.submissions.iter_mut().find(|s| s.id == e.id) {
            s.result = Some(if e.ok { Ok(()) } else { Err(e.error.unwrap_or_else(|| "score refusé".into())) });
        }
    }
    // Ne jamais supprimer une saisie encore en attente.
    let mut complete = relay.submissions.iter().filter(|s| s.result.is_some()).count();
    relay.submissions.retain(|s| {
        if s.result.is_some() && complete > 500 { complete -= 1; false } else { true }
    });
    store.save_relay(&client.id)?;
    Ok(Json(json!({ "ok": true })))
}

/// `GET /api/smartpoule/admin` : clé d'administration SmartPoule des
/// compétitions de la version web (QR code de l'administrateur).
pub async fn admin_code(State(store): State<Shared>, client: Client) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    let key = store.smartpoule_admin_key(&client);
    Ok(Json(json!({ "club": client.id, "name": client.name, "key": key })))
}

// --------------------------------------------------------------- téléphone

#[derive(Deserialize)]
pub struct PhoneQuery {
    /// Club.
    c: String,
    /// Référence de l'arbitre (0 : administrateur).
    r: u32,
    /// Clé de l'arbitre.
    k: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    id: Option<u64>,
}

/// Arbitre authentifié : `None` pour l'administrateur.
enum Who {
    Referee(u32, String),
    Admin,
}

impl crate::store::Store {
    fn ensure_relay(&mut self, club: &str) {
        if self.relays.contains_key(club) { return; }
        let path = self.dir.join("clients").join(club).join("smartpoule-relay.json");
        let relay = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        self.relays.insert(club.to_string(), relay);
    }
    fn save_relay(&self, club: &str) -> Result<(), ApiError> {
        let Some(relay) = self.relays.get(club) else { return Ok(()) };
        let path = self.dir.join("clients").join(club).join("smartpoule-relay.json");
        let data = serde_json::to_vec(relay).map_err(crate::api::internal)?;
        if std::fs::read(&path).ok().as_deref() == Some(data.as_slice()) { return Ok(()); }
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(crate::api::internal)?; }
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, data).map_err(crate::api::internal)?;
        std::fs::rename(&temporary, path).map_err(crate::api::internal)?;
        Ok(())
    }

    fn club(&self, id: &str) -> Option<Client> {
        self.clients().iter().find(|c| c.id == id).map(|c| Client { id: c.id.clone(), name: c.name.clone() })
    }

    /// Vérifie la clé présentée par le téléphone : clé d'un arbitre ou clé
    /// d'administration, dans l'instantané de BellePoule ou dans une
    /// compétition de la version web.
    fn authenticate(&mut self, q: &PhoneQuery) -> Result<(Client, Who), ApiError> {
        let club = self.club(&q.c).ok_or_else(forbidden)?;
        self.ensure_relay(&club.id);
        if q.k.len() < 16 {
            return Err(forbidden());
        }
        let state = self.relays.get(&club.id).map(|r| r.state.clone()).unwrap_or(Value::Null);
        if q.r == 0 {
            let admin = state.get("admin_key").and_then(Value::as_str).map(str::to_string);
            let web_admin = self.smartpoule_admin_key(&club);
            if admin.as_deref() == Some(q.k.as_str()) || web_admin == q.k {
                return Ok((club, Who::Admin));
            }
            return Err(forbidden());
        }
        if let Some(list) = state.get("referees").and_then(Value::as_array) {
            for r in list {
                if r.get("reference").and_then(Value::as_u64) == Some(q.r as u64) && r.get("key").and_then(Value::as_str) == Some(q.k.as_str()) {
                    let name = r.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                    return Ok((club, Who::Referee(q.r, name)));
                }
            }
        }
        let found = self
            .list(&club)
            .map(|(_, c)| c)
            .find(|c| c.referee_password(q.r).as_deref() == Some(q.k.as_str()))
            .map(|c| c.referees.get(q.r).map(|p| format!("{} {}", p.name(), p.first_name())).unwrap_or_default());
        match found {
            Some(name) => Ok((club, Who::Referee(q.r, name))),
            None => Err(forbidden()),
        }
    }

    /// Clé d'administration SmartPoule des compétitions de la version web.
    pub fn smartpoule_admin_key(&mut self, club: &Client) -> String {
        let path = self.dir.join("clients").join(&club.id).join("smartpoule.json");
        if let Some(k) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.get("admin_key").and_then(Value::as_str).map(str::to_string)) {
            return k;
        }
        let key = crate::auth::new_key();
        let _ = std::fs::write(&path, json!({ "admin_key": key }).to_string());
        key
    }
}

fn mine(job: &Value, who: &Who) -> bool {
    match who {
        Who::Admin => true,
        Who::Referee(r, _) => job.get("referees").and_then(Value::as_array).is_some_and(|l| l.iter().any(|x| x.as_u64() == Some(*r as u64))),
    }
}

/// `GET /api/smartpoule/phone/jobs` : matchs de l'arbitre.
pub async fn phone_jobs(State(store): State<Shared>, Query(q): Query<PhoneQuery>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    let (club, who) = store.authenticate(&q)?;
    let mut jobs: Vec<Value> = Vec::new();
    if let Some(list) = store.relays.get(&club.id).and_then(|r| r.state.get("jobs")).and_then(Value::as_array) {
        jobs.extend(list.iter().filter(|j| mine(j, &who)).cloned());
    }
    for (_, contest) in store.list(&club) {
        jobs.extend(contest.smartpoule_jobs(None).into_iter().filter(|j| mine(j, &who)));
    }
    let name = match &who {
        Who::Admin => "Administrateur".to_string(),
        Who::Referee(_, n) => n.clone(),
    };
    Ok(Json(json!({ "club": club.name, "name": name, "jobs": jobs })))
}

/// `GET /api/smartpoule/phone/sheet?token=…` : feuille d'un match.
pub async fn phone_sheet(State(store): State<Shared>, Query(q): Query<PhoneQuery>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    let (club, who) = store.authenticate(&q)?;
    let token = q.token.clone().ok_or_else(|| bad_request("jeton attendu"))?;
    let referee = match who {
        Who::Admin => None,
        Who::Referee(r, _) => Some(r),
    };
    if let Some(sheet) = store.relays.get(&club.id).and_then(|r| r.state.get("sheets")).and_then(|s| s.get(&token)) {
        return Ok(Json(filter_bouts(sheet.clone(), referee)));
    }
    for (_, contest) in store.list(&club) {
        if let Some(sheet) = contest.smartpoule_sheet(&token, referee) {
            return Ok(Json(sheet));
        }
    }
    Err(ApiError(StatusCode::NOT_FOUND, "match introuvable".into()))
}

/// Feuille d'une poule sur plusieurs pistes : l'arbitre ne voit que les
/// matchs de sa piste (champ `referee` de chaque match dans l'instantané).
fn filter_bouts(mut sheet: Value, referee: Option<u32>) -> Value {
    if let (Some(r), Some(bouts)) = (referee, sheet.get_mut("bouts").and_then(Value::as_array_mut)) {
        bouts.retain(|b| b.get("referee").and_then(Value::as_u64).is_none_or(|x| x == r as u64));
    }
    sheet
}

#[derive(Deserialize)]
pub struct PhoneScore {
    c: String,
    r: u32,
    k: String,
    token: String,
    bout: u64,
    values: Option<[String; 2]>,
    officiating: Option<bp_core::officiating::Officiating>,
}

/// `POST /api/smartpoule/phone/score` : score d'un match.
pub async fn phone_score(State(store): State<Shared>, Json(s): Json<PhoneScore>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    let q = PhoneQuery { c: s.c.clone(), r: s.r, k: s.k.clone(), token: None, id: None };
    let (club, who) = store.authenticate(&q)?;
    let referee = match who {
        Who::Admin => 0,
        Who::Referee(r, _) => r,
    };
    if s.values.is_none() && s.officiating.is_none() { return Err(bad_request("score ou arbitrage attendu")); }
    // L'authentification seule ne donne pas accès aux matchs d'un autre arbitre.
    let raw = store.list(&club).find_map(|(_, c)| c.smartpoule_sheet(&s.token, None))
        .or_else(|| store.relays.get(&club.id).and_then(|r| r.state.get("sheets")).and_then(|v| v.get(&s.token)).cloned())
        .ok_or_else(|| bad_request("match introuvable"))?;
    let assigned = referee == 0 || store.list(&club).flat_map(|(_, c)| c.smartpoule_jobs(None))
        .chain(store.relays.get(&club.id).and_then(|r| r.state.get("jobs")).and_then(Value::as_array).into_iter().flatten().cloned())
        .any(|job| job.get("token").and_then(Value::as_str) == Some(s.token.as_str()) && mine(&job, &Who::Referee(referee, String::new())));
    if !assigned { return Err(forbidden()); }
    let allowed = filter_bouts(raw, if referee == 0 { None } else { Some(referee) });
    if !allowed.get("bouts").and_then(Value::as_array).is_some_and(|bouts| bouts.iter().any(|b| b.get("index").and_then(Value::as_u64) == Some(s.bout))) { return Err(forbidden()); }
    // Compétition de la version web : appliqué tout de suite.
    let web: Option<String> = store.list(&club).find(|(_, c)| c.smartpoule_sheet(&s.token, None).is_some()).map(|(id, _)| id.clone());
    if let Some(id) = web {
        let result = store
            .update(&club, &id, |c| {
                if let Some(state) = s.officiating.clone() { c.smartpoule_apply_officiating(&s.token, s.bout as usize, state)?; }
                if let Some(values) = s.values.clone() { c.smartpoule_apply_score(&s.token, s.bout as usize, values)?; }
                Ok::<(), String>(())
            })
            .ok_or_else(|| bad_request("compétition introuvable"))?
            .map_err(crate::api::internal)?;
        return match result {
            Ok(()) => Ok(Json(json!({ "status": "ok" }))),
            Err(e) => Err(bad_request(e)),
        };
    }
    // Compétition de BellePoule : mis en attente.
    let relay = store.relays.entry(club.id.clone()).or_default();
    if let Some(previous) = relay.submissions.iter().find(|previous| previous.token == s.token && previous.bout == s.bout
        && previous.referee == referee && previous.values == s.values && previous.officiating == s.officiating
        && !matches!(&previous.result, Some(Err(_)))) {
        return Ok(Json(match &previous.result { Some(Ok(())) => json!({"status":"ok"}), _ => json!({"status":"pending","id":previous.id}) }));
    }
    relay.next_id += 1;
    let id = relay.next_id;
    relay.submissions.push(Submission { id, token: s.token, bout: s.bout, values: s.values, officiating: s.officiating, referee, result: None });
    store.save_relay(&club.id)?;
    Ok(Json(json!({ "status": "pending", "id": id })))
}

/// `GET /api/smartpoule/phone/result?id=…` : sort d'un score en attente.
pub async fn phone_result(State(store): State<Shared>, Query(q): Query<PhoneQuery>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    let (club, who) = store.authenticate(&q)?;
    let id = q.id.ok_or_else(|| bad_request("identifiant attendu"))?;
    let relay = store.relays.entry(club.id.clone()).or_default();
    let s = relay.submissions.iter().find(|s| s.id == id).ok_or_else(|| bad_request("saisie inconnue"))?;
    if let Who::Referee(reference, _) = who { if s.referee != reference { return Err(forbidden()); } }
    Ok(Json(match &s.result {
        None => json!({ "status": "pending" }),
        Some(Ok(())) => json!({ "status": "ok" }),
        Some(Err(e)) => json!({ "status": "error", "error": e }),
    }))
}

/// Nombre de saisies en attente (indicateur).
#[allow(dead_code)]
pub fn pending_count(relays: &HashMap<String, Relay>, club: &str) -> usize {
    relays.get(club).map(|r| r.submissions.iter().filter(|s| s.result.is_none()).count()).unwrap_or(0)
}
