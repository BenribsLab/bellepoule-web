//! Classement de saison du club : cumul des points de ses compétitions
//! (celles dont les points sont activés), avec les corrections à la main
//! (`clients/<club>/saison.json`).

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use bp_core::season::{self, Overrides, SeasonContest};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{bad_request, internal, ApiError, Shared};
use crate::auth::Client;
use crate::store::Store;

fn overrides_path(store: &Store, client: &Client) -> std::path::PathBuf {
    store.dir.join("clients").join(&client.id).join("saison.json")
}

fn load_overrides(store: &Store, client: &Client) -> Overrides {
    std::fs::read_to_string(overrides_path(store, client)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_overrides(store: &Store, client: &Client, o: &Overrides) -> Result<(), ApiError> {
    let path = overrides_path(store, client);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(internal)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(o).unwrap_or_default()).map_err(internal)
}

#[derive(Deserialize)]
pub struct SeasonQuery {
    /// Catégorie (code) : seulement les compétitions de cette catégorie.
    #[serde(default)]
    category: Option<String>,
}

/// `{ categories, contests, rows, corrections }`.
pub async fn get(State(store): State<Shared>, client: Client, Query(q): Query<SeasonQuery>) -> Json<Value> {
    let store = store.lock().await;
    let mut contests: Vec<SeasonContest> = store.list(&client).filter_map(|(id, c)| SeasonContest::from_contest(id, c)).collect();
    contests.sort_by(|a, b| date_key(&a.date).cmp(&date_key(&b.date)).then(a.name.cmp(&b.name)));
    let mut categories: Vec<String> = contests.iter().map(|c| c.category.clone()).filter(|c| !c.is_empty()).collect();
    categories.sort();
    categories.dedup();
    let filter = q.category.filter(|c| !c.is_empty());
    let contests: Vec<SeasonContest> = contests.into_iter().filter(|c| filter.as_ref().is_none_or(|f| c.category == *f)).collect();
    let overrides = load_overrides(&store, &client);
    let rows = season::ranking(&contests, &overrides);
    let list: Vec<Value> = contests.iter().map(|c| json!({ "id": c.id, "name": c.name, "date": c.date, "category": c.category, "participants": c.participants })).collect();
    Json(json!({
        "categories": categories,
        "contests": list,
        "rows": rows,
        "corrections": overrides.merges.len() + overrides.splits.len(),
    }))
}

#[derive(Deserialize)]
pub struct Correction {
    /// `merge` (`keys` : la première garde les autres), `split` (`first_name`), `reset`.
    action: String,
    #[serde(default)]
    keys: Vec<String>,
    #[serde(default)]
    first_name: String,
}

pub async fn correct(State(store): State<Shared>, client: Client, Json(c): Json<Correction>) -> Result<StatusCode, ApiError> {
    let store = store.lock().await;
    let mut o = load_overrides(&store, &client);
    match c.action.as_str() {
        "merge" if c.keys.len() >= 2 => o.merge(&c.keys[0], &c.keys[1..]),
        "split" if !c.first_name.trim().is_empty() => o.split(&c.first_name),
        "reset" => o = Overrides::default(),
        _ => return Err(bad_request("correction invalide")),
    }
    save_overrides(&store, &client, &o)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Date « jj/mm/aaaa » comparable.
fn date_key(date: &str) -> String {
    let parts: Vec<&str> = date.split(['/', '.', '-']).collect();
    match parts[..] {
        [d, m, y] if y.len() == 4 => format!("{y}{m:0>2}{d:0>2}"),
        _ => date.to_string(),
    }
}
