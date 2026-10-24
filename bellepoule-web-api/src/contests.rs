//! Compétitions : liste, création, propriétés (mêmes listes que
//! l'application de bureau), import et export.

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bp_core::category::{Category, DISPLAY_IMAGES};
use bp_core::contest::{Contest, COLORS, GENDER_IMAGES, LEVEL_IMAGES};
use bp_core::i18n::tr;
use bp_core::weapon::Weapon;
use serde::{Deserialize, Serialize};

use crate::api::{bad_request, internal, not_found, ApiError, Shared};
use crate::auth::Client;
use crate::view::Choice;

/// Listes de valeurs des propriétés d'une compétition.
#[derive(Serialize)]
pub struct Meta {
    weapons: Vec<Choice>,
    genders: Vec<Choice>,
    categories: Vec<Choice>,
    levels: Vec<Choice>,
    colors: Vec<String>,
}

pub async fn meta() -> Json<Meta> {
    let genders = ["M", "F", "X"];
    Json(Meta {
        weapons: Weapon::list().iter().map(|w| Choice { value: w.xml_image.to_string(), label: w.label() }).collect(),
        genders: GENDER_IMAGES.iter().zip(genders).map(|(g, code)| Choice { value: code.to_string(), label: tr(g) }).collect(),
        categories: DISPLAY_IMAGES.iter().enumerate().map(|(i, d)| Choice { value: i.to_string(), label: tr(d) }).collect(),
        levels: LEVEL_IMAGES.iter().enumerate().map(|(i, l)| Choice { value: i.to_string(), label: tr(l) }).collect(),
        colors: COLORS.iter().map(|c| c.to_string()).collect(),
    })
}

/// Résumé d'une compétition (liste et en-tête).
#[derive(Serialize)]
pub struct ContestSummary {
    id: String,
    name: String,
    weapon: String,
    weapon_label: String,
    gender: String,
    gender_label: String,
    category: String,
    level: String,
    label: String,
    date: String,
    location: String,
    organizer: String,
    color: String,
    team_event: bool,
    fencers: usize,
    /// Phase en cours (classe et nom).
    stage: String,
    /// Retirée de l'accueil (conservée).
    hidden: bool,
}

pub fn summary(id: &str, c: &Contest) -> ContestSummary {
    let stage = c
        .schedule
        .current()
        .map(|s| format!("{} {}", tr(s.kind().class_name()), s.name()).trim().to_string())
        .unwrap_or_default();
    ContestSummary {
        id: id.to_string(),
        name: c.name.clone(),
        weapon: c.weapon.xml_image.to_string(),
        weapon_label: c.weapon.label(),
        gender: c.gender_code().to_string(),
        gender_label: c.gender_label(),
        category: c.category.display_image(),
        level: c.level_label(),
        label: c.label.clone(),
        date: c.date_image(),
        location: c.location.clone(),
        organizer: c.organizer.clone(),
        color: c.color.clone(),
        team_event: c.settings.team_event,
        fencers: c.schedule.checkin().map(|ch| ch.fencers(&c.players).len()).unwrap_or(0),
        stage,
        hidden: false,
    }
}

/// Propriétés modifiables (fenêtre « Propriétés » de l'application).
#[derive(Serialize, Deserialize, Default)]
pub struct Properties {
    pub name: String,
    #[serde(default)]
    pub organizer: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub web_site: String,
    /// Code de l'arme.
    pub weapon: String,
    /// `M`, `F` ou `X`.
    pub gender: String,
    /// Indice dans la liste des catégories.
    pub category: usize,
    /// Indice dans la liste des niveaux.
    #[serde(default)]
    pub level: usize,
    /// `jj/mm/aaaa`.
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub team_event: bool,
    /// Compétition par équipes : tireurs et remplaçants d'une équipe.
    #[serde(default)]
    pub team_size: Option<u32>,
    #[serde(default)]
    pub substitutes: Option<u32>,
    /// Points de la compétition (préférences « Points »).
    #[serde(default)]
    pub points: Option<bp_core::points::PointsSettings>,
    /// Remplissage automatique des scores (tests de la formule).
    #[serde(default)]
    pub score_stuffing: Option<bool>,
}

fn properties_of(c: &Contest) -> Properties {
    Properties {
        name: c.name.clone(),
        organizer: c.organizer.clone(),
        location: c.location.clone(),
        label: c.label.clone(),
        web_site: c.web_site.clone(),
        weapon: c.weapon.xml_image.to_string(),
        gender: c.gender_code().to_string(),
        category: c.category.index().unwrap_or(0),
        level: c.level,
        date: format!("{:02}/{:02}/{}", c.day, c.month, c.year),
        color: c.color.clone(),
        team_event: c.settings.team_event,
        team_size: Some(c.settings.team_size),
        substitutes: Some(c.settings.substitutes),
        points: Some(c.points.clone()),
        score_stuffing: Some(c.settings.score_stuffing_allowed),
    }
}

fn apply(c: &mut Contest, p: &Properties) -> Result<(), ApiError> {
    let name = p.name.trim();
    if name.is_empty() {
        return Err(bad_request("le nom est obligatoire"));
    }
    c.name = name.to_string();
    c.organizer = p.organizer.trim().to_string();
    c.location = p.location.trim().to_string();
    c.label = p.label.trim().to_string();
    c.web_site = p.web_site.trim().to_string();
    if let Some(w) = Weapon::from_xml(&p.weapon) {
        c.weapon = w;
    }
    c.gender = match p.gender.as_str() {
        "F" => 1,
        "X" => 2,
        _ => 0,
    };
    let mut category = Category::new();
    category.set_index(p.category);
    c.category = category;
    if p.level < LEVEL_IMAGES.len() {
        c.level = p.level;
    }
    let parts: Vec<u32> = p.date.split(['/', '-', '.']).filter_map(|v| v.trim().parse().ok()).collect();
    if let [d, m, y] = parts[..] {
        // Format jj/mm/aaaa ou aaaa-mm-jj.
        let (d, m, y) = if d > 31 { (y, m, d) } else { (d, m, y) };
        if (1..=31).contains(&d) && (1..=12).contains(&m) {
            c.day = d;
            c.month = m;
            c.year = y;
        }
    }
    if p.color.starts_with('#') && p.color.len() == 7 {
        c.color = p.color.clone();
    }
    // Le type d'épreuve ne change plus une fois l'appel commencé.
    if c.schedule.current_index() == 0 && c.players.is_empty() {
        c.set_team_event(p.team_event);
    }
    if let (Some(size), Some(subs)) = (p.team_size, p.substitutes) {
        c.set_team_size(size, subs);
    }
    if let Some(points) = &p.points {
        if let Some(e) = points.formula_error() {
            return Err(bad_request(e));
        }
        c.points = points.clone();
    }
    if let Some(stuffing) = p.score_stuffing {
        c.set_score_stuffing(stuffing);
    }
    c.apply_properties(None);
    Ok(())
}

pub async fn list(State(store): State<Shared>, client: Client) -> Json<Vec<ContestSummary>> {
    let store = store.lock().await;
    let hidden = store.hidden(&client);
    Json(
        store
            .list(&client)
            .map(|(id, c)| ContestSummary { hidden: hidden.contains(id), ..summary(id, c) })
            .collect(),
    )
}

#[derive(Deserialize)]
pub struct Hidden {
    hidden: bool,
}

/// Retire une compétition de l'accueil (sans l'effacer) ou l'y remet.
pub async fn set_hidden(State(store): State<Shared>, client: Client, Path(id): Path<String>, Json(h): Json<Hidden>) -> Result<StatusCode, ApiError> {
    let store = store.lock().await;
    store.get(&client, &id).ok_or_else(not_found)?;
    store.set_hidden(&client, &id, h.hidden).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_one(State(store): State<Shared>, client: Client, Path(id): Path<String>) -> Result<Json<ContestSummary>, ApiError> {
    let store = store.lock().await;
    let c = store.get(&client, &id).ok_or_else(not_found)?;
    Ok(Json(summary(&id, c)))
}

#[derive(Deserialize)]
pub struct NewContest {
    #[serde(flatten)]
    properties: Properties,
    /// Formule de l'entraînement des jeunes.
    #[serde(default)]
    youth_training: bool,
}

pub async fn create(State(store): State<Shared>, client: Client, Json(input): Json<NewContest>) -> Result<(StatusCode, Json<ContestSummary>), ApiError> {
    let mut c = Contest::new_with_default_stages(None);
    apply(&mut c, &input.properties)?;
    if input.youth_training {
        c.with_schedule(|s, ctx| {
            s.add_kind(ctx, bp_core::stage::StageKind::YouthTraining, Some(0));
        });
    }
    let mut store = store.lock().await;
    let id = store.insert(&client, c).map_err(internal)?;
    let c = store.get(&client, &id).ok_or_else(not_found)?;
    Ok((StatusCode::CREATED, Json(summary(&id, c))))
}

pub async fn get_properties(State(store): State<Shared>, client: Client, Path(id): Path<String>) -> Result<Json<Properties>, ApiError> {
    let store = store.lock().await;
    let c = store.get(&client, &id).ok_or_else(not_found)?;
    Ok(Json(properties_of(c)))
}

pub async fn set_properties(State(store): State<Shared>, client: Client, Path(id): Path<String>, Json(p): Json<Properties>) -> Result<Json<ContestSummary>, ApiError> {
    let mut store = store.lock().await;
    store.update(&client, &id, |c| apply(c, &p)).ok_or_else(not_found)?.map_err(internal)??;
    let c = store.get(&client, &id).ok_or_else(not_found)?;
    Ok(Json(summary(&id, c)))
}

/// Import d'un fichier `.cotcot` (corps de la requête).
#[derive(Deserialize, Default)]
pub struct ImportQuery {
    /// Nom du fichier : son extension choisit le format.
    #[serde(default)]
    name: String,
}

/// Ouverture d'un fichier comme sur le bureau : `.cotcot`/`.xml`, archive
/// `.zip` de la FFE (extranet, premier XML), AskFred `.frd` (une compétition
/// par épreuve). Renvoie les compétitions créées.
pub async fn import(
    State(store): State<Shared>,
    client: Client,
    Query(q): Query<ImportQuery>,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<Vec<ContestSummary>>), ApiError> {
    use bp_core::formats::archive::{self, FileKind};
    let xml = |bytes: &[u8]| -> Result<Contest, ApiError> {
        let text = bp_core::formats::encoding::decode_xml(bytes).map_err(|e| bad_request(e.to_string()))?;
        let mut c = Contest::new(None, false);
        c.load_xml_str(&text).map_err(|e| bad_request(e.to_string()))?;
        Ok(c)
    };
    let contests = match archive::file_kind(&q.name.to_lowercase()) {
        FileKind::Zip => {
            let entries = archive::xml_entries(&body).map_err(|e| bad_request(e.to_string()))?;
            let (_, content) = entries.into_iter().next().ok_or_else(|| bad_request("aucune compétition XML dans l'archive"))?;
            vec![xml(&content)?]
        }
        FileKind::AskFred => {
            let file = bp_core::formats::askfred::reader::AskFredFile::parse(&body).map_err(|e| bad_request(e.to_string()))?;
            if file.events.is_empty() {
                return Err(bad_request("aucune épreuve dans le fichier AskFred"));
            }
            file.events
                .iter()
                .map(|event| {
                    let mut c = Contest::new(None, false);
                    c.load_askfred(&file, event);
                    c
                })
                .collect()
        }
        FileKind::Contest => vec![xml(&body)?],
        _ => return Err(bad_request("fichier non reconnu : .cotcot, .xml, .zip ou .frd attendu")),
    };
    let mut store = store.lock().await;
    let mut created = Vec::new();
    for c in contests {
        let id = store.insert(&client, c).map_err(internal)?;
        let c = store.get(&client, &id).ok_or_else(not_found)?;
        created.push(summary(&id, c));
    }
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn remove(State(store): State<Shared>, client: Client, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let mut store = store.lock().await;
    if store.remove(&client, &id).map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

/// Modèle CSV d'import des tireurs (colonnes attendues).
pub async fn csv_template(_client: Client) -> Response {
    let disposition = "attachment; filename=\"modele-tireurs.csv\"".to_string();
    ([(header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()), (header::CONTENT_DISPOSITION, disposition)], bp_core::formats::csv::template()).into_response()
}

/// Le fichier BellePoule de la compétition (compatible avec l'application).
pub async fn download(State(store): State<Shared>, client: Client, Path(id): Path<String>) -> Result<Response, ApiError> {
    let mut store = store.lock().await;
    let (xml, name) = store
        .update(&client, &id, |c| (c.to_xml(), c.default_file_name()))
        .ok_or_else(not_found)?
        .map_err(internal)?;
    let disposition = format!("attachment; filename=\"{}\"", name.replace('"', ""));
    Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8".to_string()), (header::CONTENT_DISPOSITION, disposition)], xml).into_response())
}

/// Exports : `html` (résultats complets), `html-final` (classement final seul), `fff` (classement FFF), `frd` (AskFred).
#[derive(Deserialize, Default)]
pub struct ExportQuery {
    /// Export FFF : place de départ (décalage des places), 1 par défaut.
    #[serde(default)]
    first_place: Option<u32>,
}

pub async fn export(
    State(store): State<Shared>,
    client: Client,
    Path((id, format)): Path<(String, String)>,
    Query(q): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let mut store = store.lock().await;
    let c = store.get_mut(&client, &id).ok_or_else(not_found)?;
    let base = c.default_file_name().trim_end_matches(".cotcot").to_string();
    let (bytes, mime, ext): (Vec<u8>, &str, &str) = match format.as_str() {
        "html" | "html-final" => {
            let youth = crate::stages::youth_html(c);
            (c.dump_to_html_scope(youth, format == "html-final").into_bytes(), "text/html; charset=utf-8", "html")
        }
        "fff" => (
            c.export_final_classification_fff(q.first_place.unwrap_or(1).saturating_sub(1)).ok_or_else(|| bad_request("pas de classement général"))?,
            "text/plain; charset=utf-8",
            "fff",
        ),
        "frd" => (c.export_askfred().into_bytes(), "application/xml; charset=utf-8", "frd"),
        _ => return Err(bad_request("format inconnu : html, html-final, fff ou frd")),
    };
    let disposition = format!("attachment; filename=\"{}.{ext}\"", base.replace('"', ""));
    Ok(([(header::CONTENT_TYPE, mime.to_string()), (header::CONTENT_DISPOSITION, disposition)], bytes).into_response())
}
