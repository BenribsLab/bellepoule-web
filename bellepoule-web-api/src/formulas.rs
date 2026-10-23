//! Formules de points du club : les formules fournies et celles enregistrées
//! (`clients/<club>/formules.json`).

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use bp_core::points::{builtin_formulas, FormulaStore, HELP};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{bad_request, internal, ApiError, Shared};
use crate::auth::Client;

fn path(store: &crate::store::Store, client: &Client) -> std::path::PathBuf {
    store.dir.join("clients").join(&client.id).join("formules.json")
}

/// `{ formulas: [{ name, formula, builtin }], help }`.
pub async fn list(State(store): State<Shared>, client: Client) -> Json<Value> {
    let store = store.lock().await;
    let builtin: Vec<String> = builtin_formulas().into_iter().map(|f| f.name).collect();
    let formulas: Vec<Value> = FormulaStore::load(&path(&store, &client))
        .all()
        .into_iter()
        .map(|f| json!({ "builtin": builtin.contains(&f.name), "name": f.name, "formula": f.formula }))
        .collect();
    Json(json!({ "formulas": formulas, "help": HELP }))
}

#[derive(Deserialize)]
pub struct NewFormula {
    name: String,
    formula: String,
}

pub async fn save(State(store): State<Shared>, client: Client, Json(f): Json<NewFormula>) -> Result<StatusCode, ApiError> {
    let store = store.lock().await;
    let file = path(&store, &client);
    let mut formulas = FormulaStore::load(&file);
    formulas.put(&f.name, &f.formula).map_err(bad_request)?;
    formulas.save(&file).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Name {
    name: String,
}

pub async fn remove(State(store): State<Shared>, client: Client, Query(q): Query<Name>) -> Result<StatusCode, ApiError> {
    let store = store.lock().await;
    let file = path(&store, &client);
    let mut formulas = FormulaStore::load(&file);
    if !formulas.remove(&q.name) {
        return Err(bad_request("seules les formules enregistrées par le club peuvent être supprimées"));
    }
    formulas.save(&file).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
