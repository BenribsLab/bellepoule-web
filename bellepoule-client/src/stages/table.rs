//! Tableaux : suites de tableaux (tableau principal, places de classement,
//! groupes de l'entraînement des jeunes), dessin de l'arbre tel que le
//! calcule l'application, saisie des scores (`V`, `W3`, chiffres ; `A`, `E`,
//! `F` pour abandon, exclusion, forfait), classement et réglages.

use leptos::prelude::*;
use serde_json::{json, Value};

use super::pools::score_complete;
use super::{arr, b, s, u, ListTable, PrintButton, StageCtx};

fn bx(v: &Value, k: &str) -> (f64, f64, f64, f64) {
    let a = v.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    let f = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0);
    (f(0), f(1), f(2), f(3))
}

fn style((x, y, w, h): (f64, f64, f64, f64)) -> String {
    format!("left:{x}px;top:{y}px;width:{w}px;height:{h}px")
}

#[component]
pub fn Table(ctx: StageCtx) -> impl IntoView {
    let show_ranking = RwSignal::new(false);
    view! {
        {move || {
            ctx.data
                .get()
                .map(|v| {
                    let d = v["data"].clone();
                    let locked = b(&d, "locked");
                    let max = v.get("max_score").and_then(Value::as_u64).unwrap_or_else(|| u(&d, "max_score"));
                    let displayed = d.get("displayed").and_then(Value::as_u64);
                    let drawing = d.get("drawing").cloned().unwrap_or(Value::Null);
                    let classification = d["classification"].clone();
                    view! {
                        <TableSettings ctx=ctx d=d.clone() locked=locked />
                        <div class="stage-bar">
                            <select on:change=move |ev| {
                                let set: u64 = event_target_value(&ev).parse().unwrap_or(0);
                                ctx.act.run(json!({ "action": "select", "set": set }));
                            }>
                                {arr(&d, "sets")
                                    .into_iter()
                                    .map(|st| {
                                        let i = u(&st, "index");
                                        let indent = if st.get("parent").map(|p| p.is_null()).unwrap_or(true) { "" } else { "  ↳ " };
                                        view! { <option value=i.to_string() selected=Some(i) == displayed>{format!("{indent}{}", s(&st, "name"))}</option> }
                                    })
                                    .collect_view()}
                            </select>
                            {(!drawing.is_null())
                                .then(|| {
                                    let set = u(&drawing, "set");
                                    view! {
                                        <button class="light" disabled=!b(&drawing, "can_previous") on:click=move |_| ctx.act.run(json!({ "action": "previous", "set": set }))>"‹ Tours précédents"</button>
                                        <button class="light" disabled=!b(&drawing, "can_next") on:click=move |_| ctx.act.run(json!({ "action": "next", "set": set }))>"Tours suivants ›"</button>
                                    }
                                })}
                            <span class="spacer"></span>
                            {(b(&d, "stuffing") && !locked).then(|| view! {
                                <button class="light warn" title="Scores aléatoires pour tester la formule (les scores saisis sont remplacés)"
                                    on:click=move |_| if crate::confirm("Remplir aléatoirement les scores des matchs restants de ce tableau ?") { ctx.act.run(json!({ "action": "stuff" })) }>"⚠ Remplissage automatique"</button>
                            })}
                            <label class="inline" title="Numéro du match à afficher dans le tableau : tapez 3 pour aller au match 1-16.3">
                                "Aller au match n° "
                                <input type="text" class="place-input" placeholder="n°" on:change=move |ev| goto_match(&event_target_value(&ev)) />
                            </label>
                            <button class="light" class:active=move || !show_ranking.get() on:click=move |_| show_ranking.set(false)>"Tableau"</button>
                            <button class="light" class:active=move || show_ranking.get() on:click=move |_| show_ranking.set(true)>"Classement"</button>
                            {if b(&d, "team") { view! { <super::relay_sheets::RelaySheetsButton ctx=ctx /> }.into_any() } else { view! { <super::sheets::MatchSheetsButton ctx=ctx /> }.into_any() }}
                            {if b(&d, "youth") { view! { <super::groups::GroupSheetsButton ctx=ctx /> }.into_any() } else { view! { <PrintButton /> }.into_any() }}
                        </div>
                        {move || {
                            if show_ranking.get() {
                                view! { <ListTable list=classification.clone() /> }.into_any()
                            } else if drawing.is_null() {
                                view! { <p class="muted">"Aucun tableau."</p> }.into_any()
                            } else {
                                view! { <Bracket ctx=ctx drawing=drawing.clone() max=max locked=locked /> }.into_any()
                            }
                        }}
                    }
                })
        }}
    }
}

#[component]
fn TableSettings(ctx: StageCtx, d: Value, locked: bool) -> impl IntoView {
    let max = RwSignal::new(u(&d, "max_score").to_string());
    let fenced = RwSignal::new(s(&d, "fenced_places"));
    let save = move |_| {
        ctx.act.run(json!({
            "action": "settings",
            "max_score": max.get().trim().parse::<u64>().unwrap_or(0),
            "fenced_places": fenced.get(),
        }));
    };
    view! {
        <details class="settings">
            <summary>"Réglages des tableaux"</summary>
            <div class="settings-row">
                <label class="inline">"Score maximum " <input type="number" min="1" prop:value=move || max.get() on:input=move |ev| max.set(event_target_value(&ev)) /></label>
                <label class="inline">
                    "Places tirées "
                    <select disabled=locked on:change=move |ev| fenced.set(event_target_value(&ev))>
                        {[("none", "Aucune"), ("third", "3e place"), ("all", "Toutes les places")]
                            .into_iter()
                            .map(|(code, label)| {
                                let current = fenced.get_untracked() == code;
                                view! { <option value=code selected=current>{label}</option> }
                            })
                            .collect_view()}
                    </select>
                </label>
                <button on:click=save>"Appliquer"</button>
            </div>
        </details>
    }
}

/// Arbre du tableau : en-têtes, cases des tireurs, noms des matchs, traits
/// de liaison et cases de score, aux positions calculées par le serveur.
#[component]
fn Bracket(ctx: StageCtx, drawing: Value, max: u64, locked: bool) -> impl IntoView {
    let set = u(&drawing, "set");
    let width = drawing.get("width").and_then(Value::as_f64).unwrap_or(0.0);
    let height = drawing.get("height").and_then(Value::as_f64).unwrap_or(0.0);
    // Ordre de saisie : colonne par colonne, de haut en bas.
    let mut slots: Vec<(f64, f64, String)> = arr(&drawing, "cells")
        .iter()
        .filter_map(|c| {
            let sc = c.get("score").filter(|x| !x.is_null())?;
            let (x, y, _, _) = bx(sc, "box");
            Some((x, y, format!("t-{set}-{}-{}", u(sc, "node"), u(sc, "fencer"))))
        })
        .collect();
    slots.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()));
    let order: Vec<String> = slots.into_iter().map(|(_, _, id)| id).collect();
    let order = StoredValue::new(order);
    let next_of = move |id: &str| order.with_value(|o| o.iter().position(|x| x == id).and_then(|i| o.get(i + 1).cloned()));
    let send = move |node: u64, fencer: u64, value: String, id: String| {
        let v = value.trim().to_ascii_uppercase();
        let next = next_of(&id).unwrap_or(id);
        if matches!(v.as_str(), "A" | "E" | "F") {
            ctx.act_then_focus(json!({ "action": "status", "set": set, "node": node, "fencer": fencer, "code": v }), next);
        } else {
            ctx.act_then_focus(json!({ "action": "score", "set": set, "node": node, "fencer": fencer, "value": v }), next);
        }
    };
    let connectors: Vec<String> = arr(&drawing, "connectors")
        .iter()
        .map(|points| {
            points
                .as_array()
                .map(|p| p.iter().filter_map(|xy| Some(format!("{},{}", xy.get(0)?.as_f64()?, xy.get(1)?.as_f64()?))).collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
        })
        .collect();
    view! {
        <div class="bracket-scroll">
            <div class="bracket" style=format!("width:{width}px;height:{height}px")>
                <svg class="connectors" width=width.to_string() height=height.to_string()>
                    {connectors.into_iter().map(|p| view! { <polyline points=p fill="none" stroke="#555" stroke-width="1.5" /> }).collect_view()}
                </svg>
                {arr(&drawing, "headers")
                    .into_iter()
                    .map(|h| {
                        let status = match s(&h, "status").as_str() {
                            "over" => " ✔",
                            "error" => " ⚠",
                            "running" => " ⏵",
                            _ => "",
                        };
                        view! { <div class="b-header" style=style(bx(&h, "box"))>{format!("{}{status}", s(&h, "text"))}</div> }
                    })
                    .collect_view()}
                {arr(&drawing, "cells")
                    .into_iter()
                    .map(|c| {
                        let label = c.get("label").filter(|l| !l.is_null()).cloned();
                        let score = c.get("score").filter(|x| !x.is_null()).cloned();
                        let has_winner = !c.get("winner").map(|w| w.is_null()).unwrap_or(true);
                        view! {
                            {label
                                .map(|l| {
                                    view! {
                                        <div class="b-label" style=style(bx(&l, "box"))>
                                            <span>{s(&l, "name")}</span>
                                            {l.get("roadmap").and_then(Value::as_str).map(|r| view! { <em class="roadmap">{r.to_string()}</em> })}
                                        </div>
                                    }
                                })}
                            {has_winner
                                .then(|| {
                                    view! {
                                        <div class="b-cell" style=style(bx(&c, "box"))>
                                            <strong>{s(&c, "name")}</strong>
                                            {c.get("club").and_then(Value::as_str).filter(|x| !x.is_empty()).map(|x| view! { <span class="club">{x.to_string()}</span> })}
                                        </div>
                                    }
                                })}
                            {score
                                .map(|sc| {
                                    let node = u(&sc, "node");
                                    let fencer = u(&sc, "fencer");
                                    let id = format!("t-{set}-{node}-{fencer}");
                                    let id2 = id.clone();
                                    let id3 = id.clone();
                                    let status = s(&sc, "status");
                                    let text = if status.is_empty() { s(&sc, "text") } else { status };
                                    view! {
                                        <input
                                            id=id
                                            class="score b-score"
                                            class:error=b(&sc, "error")
                                            style=style(bx(&sc, "box"))
                                            maxlength="4"
                                            disabled=locked
                                            value=text
                                            on:input=move |ev| {
                                                let value = event_target_value(&ev);
                                                let upper = value.trim().to_ascii_uppercase();
                                                if score_complete(&value, max) || matches!(upper.as_str(), "A" | "E" | "F") {
                                                    send(node, fencer, value, id2.clone());
                                                }
                                            }
                                            on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                                                if ev.key() == "Enter" || ev.key() == "Tab" {
                                                    ev.prevent_default();
                                                    send(node, fencer, event_target_value(&ev), id3.clone());
                                                }
                                            }
                                        />
                                    }
                                })}
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}

/// Fait défiler le tableau jusqu'au match dont le nom finit par « .n »
/// (« 1-16.3 ») et le met en évidence.
fn goto_match(number: &str) {
    let number = number.trim();
    if number.is_empty() {
        return;
    }
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let Ok(labels) = doc.query_selector_all(".b-label") else { return };
    for i in 0..labels.length() {
        let Some(node) = labels.item(i) else { continue };
        let Ok(el) = wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(node) else { continue };
        let name = el.query_selector("span").ok().flatten().and_then(|s| s.text_content()).unwrap_or_default();
        if name.rsplit('.').next() == Some(number) {
            el.scroll_into_view_with_bool(false);
            let _ = el.class_list().add_1("b-found");
            let keep = el.clone();
            let clear = wasm_bindgen::closure::Closure::once_into_js(move || {
                let _ = keep.class_list().remove_1("b-found");
            });
            if let Some(w) = web_sys::window() {
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(wasm_bindgen::JsCast::unchecked_ref(&clear), 2500);
            }
            return;
        }
    }
    crate::alert(&format!("Aucun match n° {number} dans le tableau affiché."));
}
