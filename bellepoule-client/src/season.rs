//! Classement de saison du club : cumul des points des compétitions dont
//! les points sont activés, par catégorie, avec les corrections à la main.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{json, Value};

use crate::alert;
use crate::api;
use crate::icons::Icon;

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn number(v: &Value) -> String {
    match v.as_f64() {
        Some(n) if (n - n.round()).abs() < 1e-9 => format!("{}", n.round() as i64),
        Some(n) => format!("{n:.2}").trim_end_matches('0').replace('.', ","),
        None => String::new(),
    }
}

/// Téléchargement d'un texte (CSV) produit dans le navigateur.
fn save_text(name: &str, text: &str) {
    let array = js_sys::Array::new();
    array.push(&wasm_bindgen::JsValue::from_str(&format!("\u{feff}{text}")));
    let Ok(blob) = web_sys::Blob::new_with_str_sequence(&array) else { return };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else { return };
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Ok(a) = doc.create_element("a") {
            let _ = a.set_attribute("href", &url);
            let _ = a.set_attribute("download", name);
            if let Some(a) = wasm_bindgen::JsCast::dyn_ref::<web_sys::HtmlElement>(&a) {
                a.click();
            }
        }
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

#[component]
pub fn SeasonPage() -> impl IntoView {
    let category = RwSignal::new(String::new());
    let version = RwSignal::new(0u32);
    let data = LocalResource::new(move || {
        version.get();
        let c = category.get();
        async move { api::get::<Value>(&format!("/api/season?category={}", js_sys::encode_uri_component(&c))).await }
    });
    let selected = RwSignal::new(Vec::<String>::new());
    let correct = move |body: Value| {
        spawn_local(async move {
            if let Err(e) = api::post_empty("/api/season/corrections", &body).await {
                alert(&e.message);
            }
            selected.set(Vec::new());
            version.update(|v| *v += 1);
        });
    };
    view! {
        <header class="page-head">
            <div>
                <h1>"Classement de saison"</h1>
                <p class="muted">"Cumul des points des compétitions du club dont les points sont activés (préférences de la compétition)."</p>
            </div>
            <span class="spacer"></span>
            <label class="inline">
                "Catégorie "
                <select on:change=move |ev| { category.set(event_target_value(&ev)); selected.set(Vec::new()); }>
                    <option value="">"Toutes"</option>
                    {move || data.get().and_then(Result::ok).map(|d| d["categories"].as_array().cloned().unwrap_or_default().into_iter().map(|c| {
                        let c = c.as_str().unwrap_or("").to_string();
                        let on = category.get_untracked() == c;
                        view! { <option value=c.clone() selected=on>{c.clone()}</option> }
                    }).collect_view())}
                </select>
            </label>
        </header>
        {move || match data.get() {
            None => view! { <div class="card skeleton-block"></div> }.into_any(),
            Some(Err(e)) => view! { <p class="error">{e.message}</p> }.into_any(),
            Some(Ok(d)) => {
                let contests = d["contests"].as_array().cloned().unwrap_or_default();
                let rows = d["rows"].as_array().cloned().unwrap_or_default();
                if contests.is_empty() {
                    return view! {
                        <div class="empty">
                            <Icon name="trophy" />
                            <h2>"Aucune compétition avec des points"</h2>
                            <p class="muted">"Activez les points dans les propriétés d'une compétition (partie « Points ») ; elle compte dès qu'elle est classée."</p>
                        </div>
                    }.into_any();
                }
                let titles: Vec<String> = contests.iter().map(|c| format!("{} {}", s(c, "name"), s(c, "date")).trim().to_string()).collect();
                let csv = {
                    let mut out = format!("Rang;Prénom;Nom;Catégorie;Compétitions;Total;{}\n", titles.join(";"));
                    for r in &rows {
                        let pts: Vec<String> = r["points"].as_array().cloned().unwrap_or_default().iter().map(number).collect();
                        out.push_str(&format!("{};{};{};{};{};{};{}\n", r["rank"], s(r, "first_name"), s(r, "last_name"), s(r, "category"), r["competitions"], number(&r["total"]), pts.join(";")));
                    }
                    out
                };
                let corrections = d["corrections"].as_u64().unwrap_or(0);
                view! {
                    <section class="card">
                        <div class="card-head">
                            <span class="chip">{format!("{} compétition(s)", contests.len())}</span>
                            <span class="chip">{format!("{} tireur(s)", rows.len())}</span>
                            {(corrections > 0).then(|| view! { <span class="chip accent">{format!("{corrections} correction(s)")}</span> })}
                            <span class="spacer"></span>
                            <button class="light small" disabled=move || selected.get().len() < 2 title="Les lignes cochées sont un seul et même tireur"
                                on:click=move |_| correct(json!({ "action": "merge", "keys": selected.get_untracked() }))>"Regrouper"</button>
                            <button class="light small" disabled=move || selected.get().len() != 1 title="Distinguer les tireurs de ce prénom par leur nom et leur catégorie"
                                on:click={
                                    let rows = rows.clone();
                                    move |_| {
                                        let key = selected.get_untracked().first().cloned().unwrap_or_default();
                                        let first = rows.iter().find(|r| s(r, "key") == key).map(|r| s(r, "first_name")).unwrap_or_default();
                                        correct(json!({ "action": "split", "first_name": first }));
                                    }
                                }>"Séparer ce prénom"</button>
                            {(corrections > 0).then(|| view! {
                                <button class="ghost small" on:click=move |_| { if crate::confirm("Annuler toutes les corrections faites à la main ?") { correct(json!({ "action": "reset" })); } }>"Annuler les corrections"</button>
                            })}
                            <button class="light small" on:click=move |_| save_text("classement-saison.csv", &csv)><Icon name="download" />"CSV"</button>
                            <button class="light small" on:click=move |_| { if let Some(w) = web_sys::window() { let _ = w.print(); } }>"Imprimer"</button>
                        </div>
                        <div class="table-scroll">
                            <table class="list season">
                                <thead>
                                    <tr>
                                        <th></th>
                                        <th class="number">"Rang"</th>
                                        <th>"Prénom"</th>
                                        <th>"Nom"</th>
                                        <th>"Catégorie"</th>
                                        <th class="number">"Compét."</th>
                                        <th class="number">"Total"</th>
                                        {titles.iter().map(|t| view! { <th class="number">{t.clone()}</th> }).collect_view()}
                                    </tr>
                                </thead>
                                <tbody>
                                    {rows.into_iter().map(|r| {
                                        let key = s(&r, "key");
                                        let k2 = key.clone();
                                        view! {
                                            <tr class:selected=move || selected.get().contains(&k2)>
                                                <td>
                                                    <input type="checkbox" prop:checked={ let k = key.clone(); move || selected.get().contains(&k) }
                                                        on:change={ let k = key.clone(); move |ev| { let on = event_target_checked(&ev); selected.update(|l| { l.retain(|x| *x != k); if on { l.push(k.clone()); } }); } } />
                                                </td>
                                                <td class="number">{r["rank"].as_u64().unwrap_or(0)}</td>
                                                <td class="name">{s(&r, "first_name")}</td>
                                                <td>{s(&r, "last_name")}</td>
                                                <td>{s(&r, "category")}</td>
                                                <td class="number">{r["competitions"].as_u64().unwrap_or(0)}</td>
                                                <td class="number"><strong>{number(&r["total"])}</strong></td>
                                                {r["points"].as_array().cloned().unwrap_or_default().iter().map(|p| view! { <td class="number">{number(p)}</td> }).collect_view()}
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                        <p class="muted small">"Reconnaissance des tireurs : prénom seul s'il est unique, sinon prénom et catégorie, sinon prénom, nom et catégorie. Cochez des lignes pour les regrouper ou séparer un prénom."</p>
                    </section>
                }.into_any()
            }
        }}
    }
}
