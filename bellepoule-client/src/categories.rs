//! Catégories générales du club : modèle actif (FFE fourni), bascule de la
//! saison, catégories et années de naissance, modèles enregistrés.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{json, Value};

use crate::api;
use crate::{alert, confirm};

const MONTHS: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];

fn n(v: &Value, k: &str) -> i64 {
    v.get(k).and_then(Value::as_i64).unwrap_or(0)
}

/// Saison (année de début) d'un modèle à la date du jour.
fn season_of(model: &Value) -> i64 {
    let d = js_sys::Date::new_0();
    let (y, m, day) = (d.get_full_year() as i64, d.get_month() as i64 + 1, d.get_date() as i64);
    if (m, day) >= (n(model, "switch_month"), n(model, "switch_day")) { y } else { y - 1 }
}

#[component]
pub fn CategoriesPage() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let data = LocalResource::new(move || {
        version.get();
        api::get::<Value>("/api/categories")
    });
    // Modèle en cours de modification.
    let model = RwSignal::new(None::<Value>);
    Effect::new(move |_| {
        if let Some(Ok(d)) = data.get() {
            let active = d["active"].as_str().unwrap_or("FFE").to_string();
            let m = d["models"].as_array().and_then(|l| l.iter().find(|m| m["name"] == active.as_str()).cloned());
            model.set(m);
        }
    });
    let reload = move || version.update(|v| *v += 1);
    let activate = move |name: String| {
        spawn_local(async move {
            if let Err(e) = api::post_empty("/api/categories/active", &json!({ "name": name })).await {
                alert(&e.message);
            }
            reload();
        });
    };
    let save = move |_| {
        let Some(m) = model.get_untracked() else { return };
        spawn_local(async move {
            match api::send_empty("PUT", "/api/categories", &json!({ "model": m })).await {
                Ok(()) => reload(),
                Err(e) => alert(&e.message),
            }
        });
    };
    let remove = move |_| {
        let Some(name) = model.get_untracked().and_then(|m| m["name"].as_str().map(str::to_string)) else { return };
        if !confirm(&format!("Supprimer le modèle « {name} » ?")) {
            return;
        }
        spawn_local(async move {
            if let Err(e) = api::delete(&format!("/api/categories?name={}", js_sys::encode_uri_component(&name))).await {
                alert(&e.message);
            }
            reload();
        });
    };
    let edit = move |f: Box<dyn FnOnce(&mut Value)>| model.update(|m| { if let Some(m) = m { f(m); } });

    view! {
        <header class="page-head">
            <div>
                <h1>"Catégories générales"</h1>
                <p class="muted">"La catégorie de chaque tireur est calculée d'après sa date de naissance (colonne « Catégorie générale » de l'appel, critères des divisions). Réglage commun au club."</p>
            </div>
        </header>
        {move || data.get().map(|d| match d {
            Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
            Ok(d) => {
                let models = d["models"].as_array().cloned().unwrap_or_default();
                let active = d["active"].as_str().unwrap_or("FFE").to_string();
                view! {
                    <section class="card">
                        <div class="row">
                            <label class="inline">
                                "Modèle actif "
                                <select on:change=move |ev| activate(event_target_value(&ev))>
                                    {models.iter().map(|m| {
                                        let name = m["name"].as_str().unwrap_or("").to_string();
                                        view! { <option value=name.clone() selected=name == active>{name.clone()}</option> }
                                    }).collect_view()}
                                </select>
                            </label>
                        </div>
                        {move || model.get().map(|m| {
                            let season = season_of(&m);
                            let categories = m["categories"].as_array().cloned().unwrap_or_default();
                            view! {
                                <div class="row">
                                    "Bascule de la saison le "
                                    <input type="number" min="1" max="31" class="place-input" prop:value=n(&m, "switch_day").to_string()
                                        on:change=move |ev| { let v: i64 = event_target_value(&ev).parse().unwrap_or(1); edit(Box::new(move |m| m["switch_day"] = json!(v.clamp(1, 31)))); } />
                                    <select on:change=move |ev| { let v: i64 = event_target_value(&ev).parse().unwrap_or(9); edit(Box::new(move |m| m["switch_month"] = json!(v))); }>
                                        {MONTHS.iter().enumerate().map(|(i, name)| view! { <option value=(i + 1).to_string() selected=(i as i64 + 1) == n(&m, "switch_month")>{*name}</option> }).collect_view()}
                                    </select>
                                    <span class="muted">{format!("Saison en cours : {season}-{}", season + 1)}</span>
                                </div>
                                <table class="list">
                                    <thead><tr><th>"Catégorie"</th><th>"Nés de"</th><th></th><th>"à"</th><th></th></tr></thead>
                                    <tbody>
                                        {categories.into_iter().enumerate().map(|(i, c)| {
                                            let open = c.get("max_age").map(|v| v.is_null()).unwrap_or(true);
                                            let oldest = season - c.get("max_age").and_then(Value::as_i64).unwrap_or(0);
                                            let newest = season - n(&c, "min_age");
                                            view! {
                                                <tr>
                                                    <td><input type="text" prop:value=c["name"].as_str().unwrap_or("").to_string()
                                                        on:change=move |ev| { let v = event_target_value(&ev); edit(Box::new(move |m| m["categories"][i]["name"] = json!(v))); } /></td>
                                                    <td><input type="number" class="place-input" disabled=open prop:value=oldest.to_string()
                                                        on:change=move |ev| { let y: i64 = event_target_value(&ev).parse().unwrap_or(oldest); edit(Box::new(move |m| m["categories"][i]["max_age"] = json!((season - y).max(0)))); } /></td>
                                                    <td><label class="check"><input type="checkbox" prop:checked=open
                                                        on:change=move |ev| { let on = event_target_checked(&ev); edit(Box::new(move |m| m["categories"][i]["max_age"] = if on { Value::Null } else { json!((season - oldest).max(0)) })); } />"et avant"</label></td>
                                                    <td><input type="number" class="place-input" prop:value=newest.to_string()
                                                        on:change=move |ev| { let y: i64 = event_target_value(&ev).parse().unwrap_or(newest); edit(Box::new(move |m| m["categories"][i]["min_age"] = json!((season - y).max(0)))); } /></td>
                                                    <td class="actions"><button class="ghost icon-only small" title="Retirer"
                                                        on:click=move |_| edit(Box::new(move |m| { if let Some(l) = m["categories"].as_array_mut() { l.remove(i); } }))>"×"</button></td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                                <div class="row">
                                    <button class="light small" on:click=move |_| edit(Box::new(move |m| {
                                        if let Some(l) = m["categories"].as_array_mut() { l.push(json!({ "name": "Nouvelle catégorie", "min_age": 9, "max_age": 10 })); }
                                    }))>"+ Ajouter une catégorie"</button>
                                </div>
                                <div class="row">
                                    <label class="inline">
                                        "Nom du modèle "
                                        <input type="text" prop:value=m["name"].as_str().unwrap_or("").to_string()
                                            on:change=move |ev| { let v = event_target_value(&ev); edit(Box::new(move |m| m["name"] = json!(v))); } />
                                    </label>
                                    <button on:click=save>"Enregistrer le modèle"</button>
                                    <button class="ghost" on:click=remove>"Supprimer ce modèle"</button>
                                </div>
                                <p class="muted small">"Le modèle FFE est fourni : pour le modifier, changez son nom avant d'enregistrer. Les années de naissance suivent la saison en cours."</p>
                            }
                        })}
                    </section>
                }.into_any()
            }
        })}
    }
}
