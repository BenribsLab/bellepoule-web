//! Catégories générales du club : modèles enregistrés (le modèle FFE est
//! fourni), modèle actif, bascule de la saison
//! (`clients/<club>/categories.json`).

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use bp_core::categories::{CategoryModel, CategoryStore};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{bad_request, internal, ApiError, Shared};
use crate::auth::Client;

/// `{ active, models, season, birth_years }` : années de naissance de chaque
/// catégorie du modèle actif pour la saison en cours.
pub async fn get(State(store): State<Shared>, client: Client) -> Json<Value> {
    let store = store.lock().await;
    let s = CategoryStore::load(&store.categories_path(&client));
    let current = s.current();
    let season = current.season(bp_core::categories::today());
    Json(json!({ "active": s.active, "models": s.models, "season": season }))
}

#[derive(Deserialize)]
pub struct SaveModel {
    model: CategoryModel,
}

/// Enregistre un modèle (et le rend actif). Le modèle FFE fourni ne peut
/// être remplacé que par lui-même.
pub async fn save(State(store): State<Shared>, client: Client, Json(input): Json<SaveModel>) -> Result<StatusCode, ApiError> {
    let store = store.lock().await;
    let model = input.model;
    if model.name.trim().is_empty() {
        return Err(bad_request("donnez un nom au modèle"));
    }
    if model.name == "FFE" && model != CategoryModel::ffe() {
        return Err(bad_request("le modèle FFE est fourni : enregistrez vos modifications sous un autre nom"));
    }
    let path = store.categories_path(&client);
    let mut s = CategoryStore::load(&path);
    s.put(model);
    s.save(&path).map_err(internal)?;
    store.categories_changed(&client);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Name {
    name: String,
}

/// Choisit le modèle actif.
pub async fn activate(State(store): State<Shared>, client: Client, Json(input): Json<Name>) -> Result<StatusCode, ApiError> {
    let store = store.lock().await;
    let path = store.categories_path(&client);
    let mut s = CategoryStore::load(&path);
    if !s.models.iter().any(|m| m.name == input.name) {
        return Err(bad_request("modèle inconnu"));
    }
    s.active = input.name;
    s.save(&path).map_err(internal)?;
    store.categories_changed(&client);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove(State(store): State<Shared>, client: Client, Query(q): Query<Name>) -> Result<StatusCode, ApiError> {
    if q.name == "FFE" {
        return Err(bad_request("le modèle FFE est fourni et ne peut pas être supprimé"));
    }
    let store = store.lock().await;
    let path = store.categories_path(&client);
    let mut s = CategoryStore::load(&path);
    s.models.retain(|m| m.name != q.name);
    if s.active == q.name {
        s.active = "FFE".into();
    }
    s.save(&path).map_err(internal)?;
    store.categories_changed(&client);
    Ok(StatusCode::NO_CONTENT)
}
