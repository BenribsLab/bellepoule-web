//! Appel : liste des tireurs, présence, ajout, modification, suppression,
//! import (FFF, CSV).

use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use bp_core::attribute::{self, Value};
use bp_core::contest::players::new_player;
use bp_core::contest::Contest;
use bp_core::player::{AttrId, PlayerClass, PlayerRef};
use bp_core::stage::Stage;
use serde::{Deserialize, Serialize};

use crate::api::{bad_request, internal, not_found, ApiError, Shared};
use crate::auth::Client;
use crate::view::{self, Choice, Column, FormField};

#[derive(Serialize)]
pub struct FencerRow {
    reference: PlayerRef,
    attending: bool,
    cells: Vec<String>,
}

#[derive(Serialize)]
pub struct CheckinView {
    columns: Vec<Column>,
    fencers: Vec<FencerRow>,
    presents: u32,
    absents: u32,
    /// Appel terminé (phase suivante commencée) : plus de modification.
    locked: bool,
}

fn checkin_view(c: &Contest) -> Option<CheckinView> {
    let checkin = c.schedule.checkin()?;
    let owner = checkin.base().owner();
    let codes: Vec<String> = checkin.filter.layout().iter().map(|l| l.code_name.clone()).filter(|c| c != "attending").collect();
    let fencers = checkin
        .fencers(&c.players)
        .into_iter()
        .filter_map(|r| {
            let p = c.players.get(r)?;
            Some(FencerRow { reference: r, attending: p.attending(), cells: codes.iter().map(|code| view::cell(p, code, owner)).collect() })
        })
        .collect();
    let tally = checkin.tally(&c.players, c.settings.team_event);
    Some(CheckinView {
        columns: view::columns(&codes),
        fencers,
        presents: tally.presents(),
        absents: tally.absents(),
        locked: checkin.base().locked,
    })
}

pub async fn get_checkin(State(store): State<Shared>, client: Client, Path(id): Path<String>) -> Result<Json<CheckinView>, ApiError> {
    let store = store.lock().await;
    let c = store.get(&client, &id).ok_or_else(not_found)?;
    Ok(Json(checkin_view(c).ok_or_else(not_found)?))
}

/// Champs du formulaire d'un tireur (listes de l'application).
pub async fn form(State(store): State<Shared>, client: Client, Path(id): Path<String>) -> Result<Json<Vec<FormField>>, ApiError> {
    let store = store.lock().await;
    let c = store.get(&client, &id).ok_or_else(not_found)?;
    let checkin = c.schedule.checkin().ok_or_else(not_found)?;
    Ok(Json(view::form_fields(checkin.filter.attr_list())))
}

#[derive(Deserialize)]
pub struct ChoicesQuery {
    /// Pays choisi : ligues, régions et clubs de ce pays.
    country: Option<String>,
}

pub async fn choices(Path((_id, code)): Path<(String, String)>, _client: Client, Query(q): Query<ChoicesQuery>) -> Json<Vec<Choice>> {
    Json(match q.country.filter(|c| !c.is_empty()) {
        Some(country) => view::country_choices(&code, &country),
        None => view::discrete_choices(&code),
    })
}

/// Valeurs saisies : code de l'attribut → texte.
#[derive(Deserialize)]
pub struct FencerInput {
    values: BTreeMap<String, String>,
}

fn check_open(c: &Contest) -> Result<(), ApiError> {
    match c.schedule.checkin() {
        Some(ch) if !ch.base().locked => Ok(()),
        Some(_) => Err(bad_request("l'appel est terminé : revenez à l'appel (« Tour précédent ») pour le modifier")),
        None => Err(not_found()),
    }
}

pub async fn add_fencer(State(store): State<Shared>, client: Client, Path(id): Path<String>, Json(input): Json<FencerInput>) -> Result<(StatusCode, Json<CheckinView>), ApiError> {
    let mut store = store.lock().await;
    let result = store
        .update(&client, &id, |c| {
            check_open(c)?;
            let mut player = new_player(PlayerClass::Fencer);
            for (code, text) in &input.values {
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                if let Some(desc) = attribute::registry().by_code(code) {
                    player.set(&AttrId::global(code), Value::parse(desc.ty, &view::stored_value(code, text)));
                }
            }
            if player.name().trim().is_empty() {
                return Err(bad_request("le nom est obligatoire"));
            }
            if let Some(team) = input.values.get("team").map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
                let settings = c.settings.clone();
                if let Some(message) = c.schedule.checkin().and_then(|ch| ch.team_full_error(&c.players, &settings, None, &team)) {
                    return Err(bad_request(message));
                }
            }

            c.with_schedule(|s, ctx| s.checkin_mut().map(|ch| ch.form_new_player(ctx, player)));
            checkin_view(c).ok_or_else(not_found)
        })
        .ok_or_else(not_found)?
        .map_err(internal)?;
    Ok((StatusCode::CREATED, Json(result?)))
}

/// Valeurs d'un tireur pour le formulaire de modification.
pub async fn get_fencer(State(store): State<Shared>, client: Client, Path((id, reference)): Path<(String, PlayerRef)>) -> Result<Json<BTreeMap<String, String>>, ApiError> {
    let store = store.lock().await;
    let c = store.get(&client, &id).ok_or_else(not_found)?;
    let checkin = c.schedule.checkin().ok_or_else(not_found)?;
    let p = c.players.get(reference).ok_or_else(not_found)?;
    Ok(Json(
        checkin.filter.attr_list().iter().filter_map(|code| view::form_value(p, code).map(|v| (code.clone(), v))).collect(),
    ))
}

pub async fn update_fencer(
    State(store): State<Shared>,
    client: Client,
    Path((id, reference)): Path<(String, PlayerRef)>,
    Json(input): Json<FencerInput>,
) -> Result<Json<CheckinView>, ApiError> {
    let mut store = store.lock().await;
    let result = store
        .update(&client, &id, |c| {
            check_open(c)?;
            if c.players.get(reference).is_none() {
                return Err(not_found());
            }
            if let Some(team) = input.values.get("team").map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
                let settings = c.settings.clone();
                if let Some(message) = c.schedule.checkin().and_then(|ch| ch.team_full_error(&c.players, &settings, Some(reference), &team)) {
                    return Err(bad_request(message));
                }
            }
            c.with_schedule(|s, ctx| {
                if let Some(ch) = s.checkin_mut() {
                    for (code, text) in &input.values {
                        let text = text.trim();
                        let value = (!text.is_empty()).then(|| view::stored_value(code, text));
                        ch.set_player_attribute(ctx, reference, code, value.as_deref());
                    }
                    ch.form_update_player(ctx, reference);
                }
            });
            checkin_view(c).ok_or_else(not_found)
        })
        .ok_or_else(not_found)?
        .map_err(internal)?;
    Ok(Json(result?))
}

pub async fn remove_fencer(State(store): State<Shared>, client: Client, Path((id, reference)): Path<(String, PlayerRef)>) -> Result<Json<CheckinView>, ApiError> {
    let mut store = store.lock().await;
    let result = store
        .update(&client, &id, |c| {
            check_open(c)?;
            c.with_schedule(|s, ctx| {
                if let Some(ch) = s.checkin_mut() {
                    ch.remove(ctx, reference);
                }
            });
            checkin_view(c).ok_or_else(not_found)
        })
        .ok_or_else(not_found)?
        .map_err(internal)?;
    Ok(Json(result?))
}

#[derive(Deserialize)]
pub struct AttendingInput {
    /// Tireurs concernés ; vide : tous.
    #[serde(default)]
    references: Vec<PlayerRef>,
    attending: bool,
}

pub async fn set_attending(State(store): State<Shared>, client: Client, Path(id): Path<String>, Json(input): Json<AttendingInput>) -> Result<Json<CheckinView>, ApiError> {
    let mut store = store.lock().await;
    let result = store
        .update(&client, &id, |c| {
            check_open(c)?;
            c.with_schedule(|s, ctx| {
                if let Some(ch) = s.checkin_mut() {
                    if input.references.is_empty() {
                        ch.set_all_attending(ctx, input.attending);
                    } else {
                        for r in &input.references {
                            ch.set_attending(ctx, *r, input.attending);
                        }
                    }
                }
            });
            checkin_view(c).ok_or_else(not_found)
        })
        .ok_or_else(not_found)?
        .map_err(internal)?;
    Ok(Json(result?))
}

#[derive(Deserialize)]
pub struct ImportQuery {
    /// Nom du fichier importé (son extension choisit le format : `.fff`, `.csv`).
    name: String,
}

#[derive(Serialize)]
pub struct ImportReport {
    imported: usize,
    encoding_error: bool,
    checkin: CheckinView,
}

/// Import d'une liste de tireurs (corps de la requête : le fichier).
pub async fn import(
    State(store): State<Shared>,
    client: Client,
    Path(id): Path<String>,
    Query(q): Query<ImportQuery>,
    body: axum::body::Bytes,
) -> Result<Json<ImportReport>, ApiError> {
    let extension = std::path::Path::new(&q.name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(extension.as_str(), "fff" | "csv" | "txt") {
        return Err(bad_request("format non reconnu : fichiers .fff ou .csv"));
    }
    // L'import du cœur lit un fichier : copie temporaire.
    let tmp = std::env::temp_dir().join(format!("bp-import-{}.{extension}", uuid::Uuid::new_v4().simple()));
    std::fs::write(&tmp, &body).map_err(internal)?;
    let mut store = store.lock().await;
    let result = store
        .update(&client, &id, |c| {
            check_open(c)?;
            let report = c
                .with_schedule(|s, ctx| s.checkin_mut().map(|ch| ch.import(ctx, &tmp)))
                .ok_or_else(not_found)?
                .map_err(|e| bad_request(e.to_string()))?;
            Ok(ImportReport {
                imported: report.players.len(),
                encoding_error: report.encoding_error,
                checkin: checkin_view(c).ok_or_else(not_found)?,
            })
        })
        .ok_or_else(not_found)?
        .map_err(internal);
    let _ = std::fs::remove_file(&tmp);
    Ok(Json(result??))
}
