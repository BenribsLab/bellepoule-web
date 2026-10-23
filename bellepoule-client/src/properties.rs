//! Propriétés d'une compétition (création et modification), avec les listes
//! de l'application : armes, sexes, catégories, niveaux.

use leptos::prelude::*;

use crate::api::{self, Meta, Properties};

/// Liste déroulante liée à un signal.
#[component]
pub fn Select(choices: Vec<api::Choice>, value: RwSignal<String>) -> impl IntoView {
    view! {
        <select on:change=move |ev| value.set(event_target_value(&ev)) prop:value=move || value.get()>
            {choices
                .into_iter()
                .map(|c| {
                    let v = c.value.clone();
                    view! { <option value=c.value selected=move || value.get() == v>{c.label}</option> }
                })
                .collect_view()}
        </select>
    }
}

/// Champs des propriétés, liés à des signaux.
#[derive(Clone, Copy)]
pub struct PropertiesForm {
    pub name: RwSignal<String>,
    pub organizer: RwSignal<String>,
    pub location: RwSignal<String>,
    pub label: RwSignal<String>,
    pub web_site: RwSignal<String>,
    pub weapon: RwSignal<String>,
    pub gender: RwSignal<String>,
    pub category: RwSignal<String>,
    pub level: RwSignal<String>,
    pub date: RwSignal<String>,
    pub color: RwSignal<String>,
    pub team_event: RwSignal<bool>,
    pub team_size: RwSignal<String>,
    pub substitutes: RwSignal<String>,
    pub points: RwSignal<Option<serde_json::Value>>,
}

impl PropertiesForm {
    pub fn new(p: &Properties) -> Self {
        PropertiesForm {
            name: RwSignal::new(p.name.clone()),
            organizer: RwSignal::new(p.organizer.clone()),
            location: RwSignal::new(p.location.clone()),
            label: RwSignal::new(p.label.clone()),
            web_site: RwSignal::new(p.web_site.clone()),
            weapon: RwSignal::new(p.weapon.clone()),
            gender: RwSignal::new(p.gender.clone()),
            category: RwSignal::new(p.category.to_string()),
            level: RwSignal::new(p.level.to_string()),
            date: RwSignal::new(iso_date(&p.date)),
            color: RwSignal::new(p.color.clone()),
            team_event: RwSignal::new(p.team_event),
            team_size: RwSignal::new(p.team_size.unwrap_or(3).to_string()),
            substitutes: RwSignal::new(p.substitutes.unwrap_or(1).to_string()),
            points: RwSignal::new(p.points.clone()),
        }
    }

    pub fn read(&self) -> Properties {
        Properties {
            name: self.name.get(),
            organizer: self.organizer.get(),
            location: self.location.get(),
            label: self.label.get(),
            web_site: self.web_site.get(),
            weapon: self.weapon.get(),
            gender: self.gender.get(),
            category: self.category.get().parse().unwrap_or(0),
            level: self.level.get().parse().unwrap_or(0),
            date: self.date.get(),
            color: self.color.get(),
            team_event: self.team_event.get(),
            team_size: self.team_size.get().parse().ok(),
            substitutes: self.substitutes.get().parse().ok(),
            points: self.points.get(),
        }
    }
}

/// `jj/mm/aaaa` → `aaaa-mm-jj` (champ date du navigateur).
fn iso_date(d: &str) -> String {
    let parts: Vec<&str> = d.split('/').collect();
    match parts[..] {
        [day, month, year] => format!("{year}-{month}-{day}"),
        _ => d.to_string(),
    }
}

/// Valeurs par défaut d'une nouvelle compétition : date du jour.
pub fn new_properties() -> Properties {
    let today = js_sys::Date::new_0();
    Properties {
        weapon: "E".into(),
        gender: "M".into(),
        date: format!("{:02}/{:02}/{}", today.get_date(), today.get_month() + 1, today.get_full_year()),
        ..Properties::default()
    }
}

fn text(label: &'static str, value: RwSignal<String>) -> impl IntoView {
    view! {
        <label>
            {label}
            <input type="text" prop:value=move || value.get() on:input=move |ev| value.set(event_target_value(&ev)) />
        </label>
    }
}

/// Champs du formulaire (sans les boutons).
#[component]
pub fn PropertiesFields(form: PropertiesForm, meta: Meta, #[prop(default = true)] team_editable: bool) -> impl IntoView {
    view! {
        <div class="fields">
            <label class="wide">
                "Nom"
                <input
                    type="text"
                    required
                    prop:value=move || form.name.get()
                    on:input=move |ev| form.name.set(event_target_value(&ev))
                />
            </label>
            <label>"Arme" <Select choices=meta.weapons.clone() value=form.weapon /></label>
            <label>"Sexe" <Select choices=meta.genders.clone() value=form.gender /></label>
            <label>"Catégorie" <Select choices=meta.categories.clone() value=form.category /></label>
            <label>"Niveau" <Select choices=meta.levels.clone() value=form.level /></label>
            <label>
                "Date"
                <input type="date" prop:value=move || form.date.get() on:input=move |ev| form.date.set(event_target_value(&ev)) />
            </label>
            {text("Lieu", form.location)}
            {text("Organisateur", form.organizer)}
            {text("Libellé", form.label)}
            {text("Site web", form.web_site)}
            <label>
                "Couleur"
                <select on:change=move |ev| form.color.set(event_target_value(&ev)) prop:value=move || form.color.get()>
                    {meta
                        .colors
                        .iter()
                        .map(|c| {
                            let c2 = c.clone();
                            let style = format!("background:{c}");
                            view! { <option value=c.clone() style=style selected=move || form.color.get() == c2>{c.clone()}</option> }
                        })
                        .collect_view()}
                </select>
            </label>
            <label class="check">
                <input
                    type="checkbox"
                    disabled=!team_editable
                    prop:checked=move || form.team_event.get()
                    on:change=move |ev| form.team_event.set(event_target_checked(&ev))
                />
                "Épreuve par équipes"
            </label>
            <Show when=move || form.team_event.get()>
                <label>
                    "Tireurs par équipe"
                    <input type="number" min="1" max="20" prop:value=move || form.team_size.get() on:input=move |ev| form.team_size.set(event_target_value(&ev)) />
                </label>
                <label>
                    "Remplaçants"
                    <input type="number" min="0" max="10" prop:value=move || form.substitutes.get() on:input=move |ev| form.substitutes.set(event_target_value(&ev)) />
                </label>
            </Show>
            {move || form.points.with(Option::is_some).then(|| view! { <PointsFields points=form.points /> })}
        </div>
    }
}

const DEFAULT_FORMULA: &str = "(N - R + 1 + B) * C";

/// Formules fournies et enregistrées par le club : choix, enregistrement
/// sous un nom, suppression, explication.
#[component]
fn FormulaLibrary(points: RwSignal<Option<serde_json::Value>>) -> impl IntoView {
    use leptos::task::spawn_local;
    use serde_json::{json, Value};
    let version = RwSignal::new(0u32);
    let library = LocalResource::new(move || {
        version.get();
        crate::api::get::<Value>("/api/points/formulas")
    });
    let name = RwSignal::new(String::new());
    let current = move || points.with(|p| p.as_ref().and_then(|p| p["formula"].as_str().map(str::to_string)).unwrap_or_else(|| DEFAULT_FORMULA.into()));
    let list = move || library.get().and_then(Result::ok).map(|l| l["formulas"].as_array().cloned().unwrap_or_default()).unwrap_or_default();
    let save = move |_| {
        let body = json!({ "name": name.get_untracked(), "formula": current() });
        spawn_local(async move {
            match crate::api::post_empty("/api/points/formulas", &body).await {
                Ok(()) => version.update(|v| *v += 1),
                Err(e) => crate::alert(&e.message),
            }
        });
    };
    let remove = move |_| {
        let n = name.get_untracked();
        if !crate::confirm(&format!("Supprimer la formule « {n} » ?")) {
            return;
        }
        spawn_local(async move {
            match crate::api::delete(&format!("/api/points/formulas?name={}", js_sys::encode_uri_component(&n))).await {
                Ok(()) => version.update(|v| *v += 1),
                Err(e) => crate::alert(&e.message),
            }
        });
    };
    view! {
        <div class="row">
            <select on:change=move |ev| {
                let i: usize = event_target_value(&ev).parse().unwrap_or(usize::MAX);
                if let Some(f) = list().get(i) {
                    let formula = f["formula"].clone();
                    name.set(f["name"].as_str().unwrap_or("").to_string());
                    points.update(|p| { if let Some(p) = p { p["formula"] = formula; } });
                }
            }>
                <option value="">"Formules enregistrées…"</option>
                {move || list().into_iter().enumerate().map(|(i, f)| {
                    let selected = f["formula"].as_str() == Some(current().as_str());
                    view! { <option value=i.to_string() selected=selected>{f["name"].as_str().unwrap_or("").to_string()}</option> }
                }).collect_view()}
            </select>
        </div>
        <div class="row">
            <input type="text" class="grow" placeholder="Nom de la formule" prop:value=move || name.get() on:input=move |ev| name.set(event_target_value(&ev)) />
            <button type="button" class="light small" disabled=move || name.get().trim().is_empty() on:click=save>"Enregistrer sous ce nom"</button>
            <button type="button" class="ghost small" disabled=move || !list().iter().any(|f| f["name"].as_str() == Some(name.get().as_str()) && f["builtin"] == false) on:click=remove>"Supprimer"</button>
        </div>
        <details class="formula-help">
            <summary>"Comment écrire une formule ?"</summary>
            <p class="muted small help-text">{move || library.get().and_then(Result::ok).map(|l| l["help"].as_str().unwrap_or("").to_string()).unwrap_or_default()}</p>
        </details>
    }
}

/// Partie « Points » : barème selon la place au classement général.
#[component]
fn PointsFields(points: RwSignal<Option<serde_json::Value>>) -> impl IntoView {
    use serde_json::{json, Value};
    let get = move |key: &str| points.with(|p| p.as_ref().map(|p| p[key].clone()).unwrap_or(Value::Null));
    let set = move |key: &'static str, value: Value| points.update(|p| { if let Some(p) = p { p[key] = value; } });
    let enabled = move || get("enabled").as_bool().unwrap_or(false);
    let list = move |key: &'static str, field: &'static str, label: &'static str| {
        view! {
            <div class="points-list">
                {move || get(key).as_array().cloned().unwrap_or_default().into_iter().enumerate().map(|(i, b)| {
                    let k = b[field].as_u64().unwrap_or(1);
                    let v = b["points"].as_f64().unwrap_or(0.0);
                    view! {
                        <div class="row">
                            {label}
                            <input type="number" min="1" prop:value=k.to_string()
                                on:change=move |ev| { let n: u64 = event_target_value(&ev).parse().unwrap_or(1); points.update(|p| { if let Some(p) = p { p[key][i][field] = json!(n.max(1)); } }); } />
                            "points :"
                            <input type="number" step="0.5" prop:value=v.to_string()
                                on:change=move |ev| { let n: f64 = event_target_value(&ev).replace(',', ".").parse().unwrap_or(0.0); points.update(|p| { if let Some(p) = p { p[key][i]["points"] = json!(n); } }); } />
                            <button type="button" class="ghost icon-only small" title="Retirer"
                                on:click=move |_| points.update(|p| { if let Some(list) = p.as_mut().and_then(|p| p[key].as_array_mut()) { list.remove(i); } })>"×"</button>
                        </div>
                    }
                }).collect_view()}
                <button type="button" class="light small" on:click=move |_| points.update(|p| {
                    if let Some(p) = p {
                        if !p[key].is_array() { p[key] = json!([]); }
                        let n = p[key].as_array().map(|l| l.len()).unwrap_or(0) as u64;
                        let next = if field == "top" { 4u64 << n } else { n + 1 };
                        if let Some(list) = p[key].as_array_mut() { list.push(json!({ field: next, "points": 1.0 })); }
                    }
                })>"+ Ajouter"</button>
            </div>
        }
    };
    view! {
        <details class="settings wide points-settings" open=enabled()>
            <summary>"Points (classement de saison)"</summary>
            <label class="check">
                <input type="checkbox" prop:checked=enabled on:change=move |ev| set("enabled", json!(event_target_checked(&ev))) />
                "Attribuer des points pour cette compétition"
            </label>
            <Show when=enabled>
                <p class="muted small">"Par défaut, sur N participants, le 1er gagne N points et le dernier 1 point."</p>
                <label class="inline">
                    "Poids de la compétition (C) "
                    <input type="number" step="0.5" min="0" prop:value=move || get("weight").as_f64().unwrap_or(1.0).to_string()
                        on:change=move |ev| set("weight", json!(event_target_value(&ev).replace(',', ".").parse::<f64>().unwrap_or(1.0))) />
                </label>
                <strong>"Bonus par place"</strong>
                {list("place_bonus", "place", "Place")}
                <strong>"Bonus pour finir dans les premiers"</strong>
                {list("top_bonus", "top", "Dans les")}
                <strong>"Formule"</strong>
                <FormulaLibrary points=points />
                <input type="text" class="grow" prop:value=move || get("formula").as_str().unwrap_or(DEFAULT_FORMULA).to_string()
                    on:change=move |ev| set("formula", json!(event_target_value(&ev))) />
            </Show>
        </details>
    }
}
