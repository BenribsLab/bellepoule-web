//! Poules : feuille de poule (grille contournée, diagonale grisée), saisie
//! des scores ligne par ligne au clavier, statuts, liste des matchs,
//! classements du tour et réglages.

use leptos::prelude::*;
use serde_json::{json, Value};

use super::{arr, b, s, u, ListTable, PrintButton, StageCtx};

/// Saisie complète : `V`, `W3`/`V3` (victoire sans le score maximum) ou un
/// nombre aussi long que le score maximum (comme l'application).
pub fn score_complete(value: &str, max: u64) -> bool {
    let v = value.trim().to_ascii_uppercase();
    if v == "V" {
        return true;
    }
    let digits = v.trim_start_matches(['V', 'W']);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let max_len = max.max(1).to_string().len();
    digits.len() >= max_len || format!("{digits}0").parse::<u64>().map(|n| n > max).unwrap_or(true)
}

#[component]
pub fn Pools(ctx: StageCtx) -> impl IntoView {
    let selected = RwSignal::new(0usize);
    let view_mode = RwSignal::new("sheet");
    view! {
        {move || {
            ctx.data
                .get()
                .map(|v| {
                    let d = v["data"].clone();
                    let club = s(&v, "club");
                    let locked = b(&d, "locked");
                    let pools = arr(&d, "pools");
                    let max = u(&d, "max_score");
                    let classification = d["classification"].clone();
                    let current = d["current_classification"].clone();
                    let sel = selected.get().min(pools.len().saturating_sub(1));
                    let youth = b(&d, "youth");
                    let pool = pools.get(sel).cloned();
                    view! {
                        <PoolSettings ctx=ctx d=d.clone() locked=locked />
                        <div class="stage-bar">
                            <select on:change=move |ev| selected.set(event_target_value(&ev).parse().unwrap_or(0))>
                                {pools
                                    .iter()
                                    .map(|p| {
                                        let i = u(p, "index") as usize;
                                        let state = if b(p, "error") { " ⚠" } else if b(p, "over") { " ✔" } else { "" };
                                        view! { <option value=i.to_string() selected=i == sel>{format!("{}{state}", s(p, "name"))}</option> }
                                    })
                                    .collect_view()}
                            </select>
                            <button class="light" disabled={sel == 0} on:click=move |_| selected.update(|x| *x = x.saturating_sub(1))>"‹"</button>
                            <button class="light" disabled={sel + 1 >= pools.len()} on:click=move |_| selected.update(|x| *x += 1)>"›"</button>
                            <span class="spacer"></span>
                            {(b(&d, "stuffing") && !locked).then(|| view! {
                                <button class="light warn" title="Scores aléatoires pour tester la formule (les scores saisis sont remplacés)"
                                    on:click=move |_| if crate::confirm("Remplir aléatoirement les scores de toutes les poules ? Les scores déjà saisis seront remplacés.") { ctx.act.run(json!({ "action": "stuff" })) }>"⚠ Remplissage automatique"</button>
                            })}
                            <button class="light" class:active=move || view_mode.get() == "sheet" on:click=move |_| view_mode.set("sheet")>"Feuille"</button>
                            <button class="light" class:active=move || view_mode.get() == "ranking" on:click=move |_| view_mode.set("ranking")>"Classement"</button>
                            <button class="light" class:active=move || view_mode.get() == "current" on:click=move |_| view_mode.set("current")>"Classement du tour"</button>
                            {b(&d, "team").then(|| view! { <super::relay_sheets::RelaySheetsButton ctx=ctx /> })}
                            // Entraînement des jeunes : on imprime toujours le groupe complet.
                            {if youth { view! { <super::groups::GroupSheetsButton ctx=ctx pool=u(pool.as_ref().unwrap_or(&Value::Null), "index") as usize /> }.into_any() } else { view! { <PrintButton /> }.into_any() }}
                        </div>
                        {move || match view_mode.get() {
                            "ranking" => view! { <ListTable list=classification.clone() /> }.into_any(),
                            "current" => view! { <ListTable list=current.clone() /> }.into_any(),
                            _ => pool.clone().map(|p| view! { <Sheet ctx=ctx pool=p max=max locked=locked club=club.clone() /> }.into_any()).unwrap_or_else(|| view! { <p class="muted">"Aucune poule."</p> }.into_any()),
                        }}
                    }
                })
        }}
    }
}

#[component]
fn PoolSettings(ctx: StageCtx, d: Value, locked: bool) -> impl IntoView {
    let name = RwSignal::new(s(&d, "round_name"));
    let max = RwSignal::new(u(&d, "max_score").to_string());
    let qualified = RwSignal::new(d.get("nb_qualified").and_then(Value::as_u64).map(|q| q.to_string()).unwrap_or_default());
    let balanced = RwSignal::new(b(&d, "balanced"));
    let save = move |_| {
        let q: Option<u64> = qualified.get().trim().parse().ok();
        ctx.act.run(json!({
            "action": "settings",
            "name": name.get(),
            "max_score": max.get().trim().parse::<u64>().unwrap_or(0),
            "nb_qualified": q,
            "balanced": balanced.get(),
        }));
    };
    view! {
        <details class="settings">
            <summary>"Réglages du tour"</summary>
            <div class="settings-row">
                <label class="inline">"Nom " <input type="text" prop:value=move || name.get() on:input=move |ev| name.set(event_target_value(&ev)) /></label>
                <label class="inline">"Score maximum " <input type="number" min="1" prop:value=move || max.get() on:input=move |ev| max.set(event_target_value(&ev)) /></label>
                <label class="inline">"Qualifiés (vide : tous) " <input type="number" min="1" disabled=locked prop:value=move || qualified.get() on:input=move |ev| qualified.set(event_target_value(&ev)) /></label>
                <label class="check">
                    <input type="checkbox" disabled=locked prop:checked=move || balanced.get() on:change=move |ev| balanced.set(event_target_checked(&ev)) />
                    "Répartition équilibrée (serpentin)"
                </label>
                <button on:click=save>"Appliquer"</button>
            </div>
        </details>
    }
}

/// Identifiant d'une case de saisie de la grille.
fn cell_id(pool: u64, m: u64, fencer: u64) -> String {
    format!("score-{pool}-{m}-{fencer}")
}

#[component]
fn Sheet(ctx: StageCtx, pool: Value, max: u64, locked: bool, club: String) -> impl IntoView {
    let p = u(&pool, "index");
    let rows = arr(&pool, "rows");
    let n = rows.len();
    let headers: Vec<String> = arr(&pool, "headers").iter().map(|h| h.as_str().unwrap_or("").to_string()).collect();
    // Ordre de saisie : ligne par ligne, de gauche à droite.
    let order: Vec<String> = rows
        .iter()
        .flat_map(|r| {
            let fencer = u(r, "reference");
            arr(r, "cells").into_iter().filter(|c| !b(c, "grey") && c.get("match").map(|m| !m.is_null()).unwrap_or(false)).map(move |c| cell_id(p, u(&c, "match"), fencer))
        })
        .collect();
    let order = StoredValue::new(order);
    let next_of = move |id: &str| order.with_value(|o| o.iter().position(|x| x == id).and_then(|i| o.get(i + 1).cloned()));
    let send = move |m: u64, fencer: u64, value: String, id: String, advance: bool| {
        let target = if advance { next_of(&id).unwrap_or(id) } else { id };
        ctx.act_then_focus(json!({ "action": "score", "pool": p, "match": m, "fencer": fencer, "value": value }), target);
    };
    view! {
        <div class="sheet-head">
            <h3>{s(&pool, "name")}</h3>
            {pool.get("piste").and_then(Value::as_str).map(|x| view! { <span class="roadmap">{format!("Piste {x}")}</span> })}
            {pool.get("referee").and_then(Value::as_str).map(|x| view! { <span class="roadmap">{format!("Arbitre : {x}")}</span> })}
            // QR code de la feuille : sans le PIN d'un arbitre de la poule, il ne donne accès à rien.
            {(!club.is_empty() && !s(&pool, "token").is_empty()).then(|| {
                let url = crate::qr::sheet_url(&club, &s(&pool, "token"));
                view! {
                    <div class="sheet-qr" title="Arbitre : scannez ce QR code avec votre téléphone, puis entrez votre code PIN">
                        <crate::qr::QrCode text=url />
                        <span>"Arbitre : scannez, puis votre code PIN"</span>
                    </div>
                }
            })}
        </div>
        <div class="sheet-wrap">
        <table class="sheet">
            <thead>
                <tr>
                    <th></th>
                    <th></th>
                    {(1..=n).map(|i| view! { <th class="num">{i}</th> }).collect_view()}
                    {headers.iter().map(|h| view! { <th class="head">{h.clone()}</th> }).collect_view()}
                </tr>
            </thead>
            <tbody>
                {rows
                    .into_iter()
                    .map(|r| {
                        let fencer = u(&r, "reference");
                        let dash = r.get("dashboard").cloned().unwrap_or(Value::Null);
                        view! {
                            <tr>
                                <td class="name">
                                    <strong>{s(&r, "name")}</strong>
                                    <span class="muted">{format!(" {}", s(&r, "club"))}</span>
                                </td>
                                <td class="pos">{u(&r, "position")}</td>
                                {arr(&r, "cells")
                                    .into_iter()
                                    .map(|c| {
                                        if b(&c, "grey") || c.get("match").map(|m| m.is_null()).unwrap_or(true) {
                                            return view! { <td class="cell grey"></td> }.into_any();
                                        }
                                        let m = u(&c, "match");
                                        let id = cell_id(p, m, fencer);
                                        let id2 = id.clone();
                                        let id3 = id.clone();
                                        view! {
                                            <td class="cell">
                                                <input
                                                    id=id
                                                    class="score"
                                                    maxlength="4"
                                                    disabled=locked
                                                    value=s(&c, "text")
                                                    on:input=move |ev| {
                                                        let value = event_target_value(&ev);
                                                        if score_complete(&value, max) {
                                                            send(m, fencer, value, id2.clone(), true);
                                                        }
                                                    }
                                                    on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                                                        if ev.key() == "Enter" || ev.key() == "Tab" {
                                                            ev.prevent_default();
                                                            send(m, fencer, event_target_value(&ev), id3.clone(), true);
                                                        }
                                                    }
                                                />
                                            </td>
                                        }
                                            .into_any()
                                    })
                                    .collect_view()}
                                {if dash.is_null() {
                                    ().into_any()
                                } else {
                                    let status = s(&dash, "status");
                                    view! {
                                        <td class="dash">
                                            <select
                                                disabled=locked
                                                on:change=move |ev| ctx.act.run(json!({ "action": "status", "pool": p, "fencer": fencer, "code": event_target_value(&ev) }))
                                            >
                                                {[("Q", "Qualifié"), ("A", "Abandon"), ("E", "Exclusion"), ("F", "Forfait")]
                                                    .into_iter()
                                                    .map(|(code, label)| view! { <option value=code selected=status == code>{label}</option> })
                                                    .collect_view()}
                                            </select>
                                        </td>
                                        <td class="dash">{s(&dash, "victories")}</td>
                                        <td class="dash">{s(&dash, "ratio")}</td>
                                        <td class="dash">{s(&dash, "hs")}</td>
                                        <td class="dash">{s(&dash, "hr")}</td>
                                        <td class="dash">{s(&dash, "indice")}</td>
                                        <td class="dash rank">{s(&dash, "rank")}</td>
                                    }
                                        .into_any()
                                }}
                            </tr>
                        }
                    })
                    .collect_view()}
            </tbody>
        </table>
        </div>
        <h4>"Matchs"</h4>
        <div class="bouts">
            {arr(&pool, "matches")
                .into_iter()
                .map(|m| {
                    let mi = u(&m, "index");
                    view! {
                        <div class="bout">
                            <strong>{s(&m, "name")}{b(&m, "rest_error").then_some(" ⏱")}</strong>
                            <span style="white-space:pre-line">{s(&m, "officiating")}</span>
                            {arr(&m, "opponents")
                                .into_iter()
                                .filter(|o| !o.is_null())
                                .map(|o| {
                                    let fencer = u(&o, "reference");
                                    let id = format!("bout-{p}-{mi}-{fencer}");
                                    let id2 = id.clone();
                                    view! {
                                        <span class="opponent">
                                            {format!("{} {}", u(&o, "position"), s(&o, "name"))}
                                            <input
                                                id=id
                                                class="score"
                                                maxlength="4"
                                                disabled=locked
                                                value=s(&o, "score")
                                                on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                                                    if ev.key() == "Enter" {
                                                        ev.prevent_default();
                                                        send(mi, fencer, event_target_value(&ev), id2.clone(), false);
                                                    }
                                                }
                                                on:change=move |ev| ctx.act.run(json!({ "action": "score", "pool": p, "match": mi, "fencer": fencer, "value": event_target_value(&ev) }))
                                            />
                                        </span>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}
