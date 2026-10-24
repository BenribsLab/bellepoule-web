//! Point de scission : réglages des divisions (au classement, par critères ou
//! à la main) et division de chaque tireur, modifiable à la main.

use leptos::prelude::*;
use serde_json::{json, Value};

use super::{arr, b, s, u, PrintButton, StageCtx};

/// Texte d'un critère pour le champ de saisie.
fn criterion_text(c: &Value) -> String {
    match s(c, "kind").as_str() {
        "category" => arr(c, "values").iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "),
        "birth_years" => arr(c, "years").iter().filter_map(Value::as_i64).map(|y| y.to_string()).collect::<Vec<_>>().join(", "),
        _ => c.get("year").and_then(Value::as_i64).map(|y| y.to_string()).unwrap_or_default(),
    }
}

/// Critère lu dans le champ, selon son genre.
fn criterion_from(kind: &str, text: &str) -> Value {
    let years: Vec<i64> = text.split(|c: char| !c.is_ascii_digit()).filter_map(|s| s.parse().ok()).filter(|y| (1900..2200).contains(y)).collect();
    match kind {
        "category" => {
            let values: Vec<&str> = text.split([',', ';', ' ']).map(str::trim).filter(|s| !s.is_empty()).collect();
            if values.is_empty() { Value::Null } else { json!({ "kind": "category", "values": values }) }
        }
        "birth_years" => if years.is_empty() { Value::Null } else { json!({ "kind": "birth_years", "years": years }) },
        k => years.first().map(|y| json!({ "kind": k, "year": y })).unwrap_or(Value::Null),
    }
}

/// Taille : `{"count": n}` ou `{"percent": n}` → (nombre, unité).
fn size_parts(v: &Value) -> (u64, &'static str) {
    if let Some(n) = v.get("percent").and_then(Value::as_u64) {
        (n, "percent")
    } else {
        (v.get("count").and_then(Value::as_u64).unwrap_or(0), "count")
    }
}

#[component]
pub fn Splitting(ctx: StageCtx) -> impl IntoView {
    view! {
        {move || {
            ctx.data.get().map(|v| {
                let d = v["data"].clone();
                let locked = b(&d, "locked");
                let names: Vec<String> = arr(&d, "divisions").iter().filter_map(|n| n.as_str().map(String::from)).collect();
                let assignments = d["assignments"].clone();
                let label = s(&d, "label");
                view! {
                    <div class="stage-bar">
                        <label class="inline">
                            "Libellé (utilisé par GREG) "
                            <input type="text" list="split-labels" disabled=locked prop:value=label
                                on:change=move |ev| ctx.act.run(json!({ "action": "label", "label": event_target_value(&ev) })) />
                        </label>
                        <datalist id="split-labels">
                            <option value="Tableau A"></option>
                            <option value="Tableau B"></option>
                            <option value="N1"></option>
                            <option value="N2"></option>
                        </datalist>
                        <span class="muted small">"N1 ou N2, Tableau A ou B : les deux divisions sont préparées."</span>
                    </div>
                    <DivisionsSettings ctx=ctx config=d["config"].clone() locked=locked />
                    <div class="stage-bar">
                        <span class="muted">"Choisissez la division de chaque tireur : à la phase suivante, chaque division devient une nouvelle compétition, les autres tireurs continuent ici."</span>
                        <span class="spacer"></span>
                        <PrintButton />
                    </div>
                    <SplitTable ctx=ctx list=d["list"].clone() names=names assignments=assignments locked=locked />
                }
            })
        }}
    }
}

#[component]
fn SplitTable(ctx: StageCtx, list: Value, names: Vec<String>, assignments: Value, locked: bool) -> impl IntoView {
    let columns = arr(&list, "columns");
    view! {
        <div class="table-scroll">
            <table class="list">
                <thead>
                    <tr>
                        <th>"Division"</th>
                        {columns.iter().map(|c| view! { <th>{s(c, "title")}</th> }).collect_view()}
                    </tr>
                </thead>
                <tbody>
                    {arr(&list, "rows").into_iter().map(|r| {
                        let reference = u(&r, "reference");
                        let current = assignments.get(reference.to_string()).and_then(Value::as_u64).unwrap_or(0);
                        view! {
                            <tr class:absent=b(&r, "dimmed") class:moved={current > 0}>
                                <td>
                                    <select
                                        disabled=locked
                                        on:change=move |ev| {
                                            let division: u64 = event_target_value(&ev).parse().unwrap_or(0);
                                            ctx.act.run(json!({ "action": "division", "players": [reference], "division": division }));
                                        }
                                    >
                                        <option value="0" selected=current == 0>"Reste ici"</option>
                                        {names.iter().enumerate().map(|(i, n)| {
                                            let value = i as u64 + 1;
                                            view! { <option value=value.to_string() selected=current == value>{n.clone()}</option> }
                                        }).collect_view()}
                                    </select>
                                </td>
                                {arr(&r, "cells").iter().map(|c| view! { <td>{c.as_str().unwrap_or("").to_string()}</td> }).collect_view()}
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

/// Réglages des divisions (comme dans la formule du logiciel de bureau).
#[component]
pub(crate) fn DivisionsSettings(ctx: StageCtx, config: Value, locked: bool) -> impl IntoView {
    let config = RwSignal::new(config);
    let send = move || ctx.act.run(json!({ "action": "config", "config": config.get_untracked() }));
    let edit = move |f: Box<dyn FnOnce(&mut Value)>| {
        config.update(|c| f(c));
        send();
    };
    let mode = move || config.with(|c| s(c, "mode"));
    let count = move || config.with(|c| arr(c, "divisions").len());
    view! {
        <details class="settings" open=true>
            <summary>"Divisions"</summary>
            <div class="split-settings">
                <label class="inline">
                    "Remplissage "
                    <select disabled=locked on:change=move |ev| { let m = event_target_value(&ev); edit(Box::new(move |c| c["mode"] = json!(m))); }>
                        <option value="manual" selected=move || mode() == "manual">"à la main"</option>
                        <option value="ranking" selected=move || mode() == "ranking">"au classement"</option>
                        <option value="criteria" selected=move || mode() == "criteria">"par critères"</option>
                    </select>
                </label>
                <Show when=move || mode() == "ranking">
                    {move || {
                        let (n, unit) = config.with(|c| size_parts(&c["keep"]));
                        view! {
                            <label class="inline">
                                "Gardés dans cette compétition : les "
                                <input type="number" min="0" disabled=locked prop:value=n.to_string()
                                    on:change=move |ev| { let n: u64 = event_target_value(&ev).parse().unwrap_or(0); edit(Box::new(move |c| { let unit = size_parts(&c["keep"]).1; c["keep"] = if n == 0 { Value::Null } else { json!({ unit: n }) }; })); } />
                                <select disabled=locked on:change=move |ev| { let unit = event_target_value(&ev); edit(Box::new(move |c| { let n = size_parts(&c["keep"]).0; if n > 0 { c["keep"] = json!({ unit: n }); } })); }>
                                    <option value="count" selected=unit == "count">"places"</option>
                                    <option value="percent" selected=unit == "percent">"%"</option>
                                </select>
                                " premiers"
                            </label>
                        }
                    }}
                </Show>
                {move || (0..count()).map(|i| {
                    let division = config.with(|c| c["divisions"][i].clone());
                    let last = i + 1 == count();
                    let (n, unit) = size_parts(&division["size"]);
                    let kind = division["criterion"].get("kind").and_then(Value::as_str).unwrap_or("category").to_string();
                    let text = criterion_text(&division["criterion"]);
                    let kind_now = StoredValue::new(kind.clone());
                    view! {
                        <div class="split-row">
                            <input type="text" placeholder="Nom de la division" disabled=locked prop:value=s(&division, "name")
                                on:change=move |ev| { let name = event_target_value(&ev); edit(Box::new(move |c| c["divisions"][i]["name"] = json!(name))); } />
                            <Show when=move || mode() == "ranking">
                                {if last {
                                    view! { <span class="muted">"le reste"</span> }.into_any()
                                } else {
                                    view! {
                                        <input type="number" min="0" disabled=locked prop:value=n.to_string()
                                            on:change=move |ev| { let n: u64 = event_target_value(&ev).parse().unwrap_or(0); edit(Box::new(move |c| { let unit = size_parts(&c["divisions"][i]["size"]).1; c["divisions"][i]["size"] = json!({ unit: n }); })); } />
                                        <select disabled=locked on:change=move |ev| { let unit = event_target_value(&ev); edit(Box::new(move |c| { let n = size_parts(&c["divisions"][i]["size"]).0; c["divisions"][i]["size"] = json!({ unit: n }); })); }>
                                            <option value="count" selected=unit == "count">"places"</option>
                                            <option value="percent" selected=unit == "percent">"%"</option>
                                        </select>
                                    }.into_any()
                                }}
                            </Show>
                            <Show when=move || mode() == "criteria">
                                <select disabled=locked on:change=move |ev| {
                                    let k = event_target_value(&ev);
                                    kind_now.set_value(k.clone());
                                    edit(Box::new(move |c| { let t = criterion_text(&c["divisions"][i]["criterion"]); c["divisions"][i]["criterion"] = criterion_from(&k, &t); }));
                                }>
                                    <option value="category" selected=kind == "category">"Catégories"</option>
                                    <option value="birth_years" selected=kind == "birth_years">"Années de naissance"</option>
                                    <option value="born_before" selected=kind == "born_before">"Nés en … ou avant"</option>
                                    <option value="born_from" selected=kind == "born_from">"Nés en … ou après"</option>
                                </select>
                                <input type="text" disabled=locked prop:value=text.clone()
                                    placeholder=move || match kind_now.get_value().as_str() { "category" => "ex. : M13, M15", "birth_years" => "ex. : 2014, 2015", _ => "année" }
                                    on:change=move |ev| { let t = event_target_value(&ev); let k = kind_now.get_value(); edit(Box::new(move |c| c["divisions"][i]["criterion"] = criterion_from(&k, &t))); } />
                            </Show>
                            <button class="ghost icon-only small" title="Retirer cette division" disabled=move || locked || count() <= 1
                                on:click=move |_| edit(Box::new(move |c| { if let Some(list) = c["divisions"].as_array_mut() { list.remove(i); } }))>
                                "×"
                            </button>
                        </div>
                    }
                }).collect_view()}
                <div>
                    <button class="light small" disabled=locked on:click=move |_| {
                        let n = count() + 2;
                        edit(Box::new(move |c| {
                            if let Some(list) = c["divisions"].as_array_mut() {
                                list.push(json!({ "name": format!("Division {n}") }));
                            }
                        }))
                    }>"+ Ajouter une division"</button>
                </div>
                <p class="muted small">
                    "Au classement : les premiers restent ici, chaque division prend les suivants, la dernière le reste. "
                    "Par critères : chaque tireur va dans la première division qui lui convient, sinon il reste. "
                    "La répartition reste modifiable tireur par tireur."
                </p>
            </div>
        </details>
    }
}
