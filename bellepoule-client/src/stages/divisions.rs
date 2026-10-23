//! Poules de division : réglages, division de chaque tireur (menu D1, D2…),
//! montées et descentes proposées après un tour.

use leptos::prelude::*;
use serde_json::{json, Value};

use super::{arr, b, s, u, PrintButton, StageCtx};

#[component]
pub fn Divisions(ctx: StageCtx) -> impl IntoView {
    view! {
        {move || {
            ctx.data.get().map(|v| {
                let d = v["data"].clone();
                let locked = b(&d, "locked");
                let settings = d["settings"].clone();
                let count = u(&settings, "divisions").max(1);
                let sizes: Vec<String> = arr(&d, "sizes").iter().enumerate().map(|(i, n)| format!("D{} : {}", i + 1, n.as_u64().unwrap_or(0))).collect();
                view! {
                    <Settings ctx=ctx settings=settings locked=locked />
                    <div class="stage-bar">
                        <strong>{sizes.join("   ")}</strong>
                        <span class="muted">"Choisissez la division de chaque tireur ; le tour de poules suivant se joue division par division."</span>
                        <span class="spacer"></span>
                        <PrintButton />
                    </div>
                    <DivisionTable ctx=ctx list=d["list"].clone() fencers=d["fencers"].clone() count=count locked=locked />
                }
            })
        }}
    }
}

#[component]
fn DivisionTable(ctx: StageCtx, list: Value, fencers: Value, count: u64, locked: bool) -> impl IntoView {
    let columns = arr(&list, "columns");
    view! {
        <div class="table-scroll">
            <table class="list">
                <thead>
                    <tr>
                        <th>"Division"</th>
                        <th>"Mouvement"</th>
                        {columns.iter().map(|c| view! { <th>{s(c, "title")}</th> }).collect_view()}
                    </tr>
                </thead>
                <tbody>
                    {arr(&list, "rows").into_iter().map(|r| {
                        let reference = u(&r, "reference");
                        let f = fencers.get(reference.to_string()).cloned().unwrap_or(Value::Null);
                        let current = u(&f, "division").max(1);
                        let origin = f.get("origin").and_then(Value::as_u64);
                        let (movement, class) = match (s(&f, "movement").as_str(), origin) {
                            ("up", Some(o)) => (format!("↑ depuis D{o}"), "move-up"),
                            ("down", Some(o)) => (format!("↓ depuis D{o}"), "move-down"),
                            ("stays", _) => ("=".to_string(), ""),
                            _ => (String::new(), ""),
                        };
                        view! {
                            <tr class:absent=b(&r, "dimmed")>
                                <td>
                                    <select disabled=locked on:change=move |ev| {
                                        let division: u64 = event_target_value(&ev).parse().unwrap_or(1);
                                        ctx.act.run(json!({ "action": "division", "players": [reference], "division": division }));
                                    }>
                                        {(1..=count).map(|n| view! { <option value=n.to_string() selected=n == current>{format!("D{n}")}</option> }).collect_view()}
                                    </select>
                                </td>
                                <td class=class>{movement}</td>
                                {arr(&r, "cells").iter().map(|c| view! { <td>{c.as_str().unwrap_or("").to_string()}</td> }).collect_view()}
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

/// Réglages (comme dans la formule du logiciel de bureau).
#[component]
fn Settings(ctx: StageCtx, settings: Value, locked: bool) -> impl IntoView {
    let settings = RwSignal::new(settings);
    let set = move |key: &'static str, value: Value| {
        settings.update(|s| s[key] = value);
        ctx.act.run(json!({ "action": "settings", "settings": settings.get_untracked() }));
    };
    let number = move |key: &'static str, label: &'static str, min: u64| {
        let value = settings.with_untracked(|s| u(s, key));
        view! {
            <label class="inline">
                {label}
                <input type="number" min=min.to_string() disabled=locked prop:value=value.to_string()
                    on:change=move |ev| { let n: u64 = event_target_value(&ev).parse().unwrap_or(min).max(min); set(key, json!(n)); } />
            </label>
        }
    };
    let start = settings.with_untracked(|v| s(v, "start"));
    view! {
        <details class="settings">
            <summary>"Réglages des divisions"</summary>
            <div class="settings-row">
                {number("divisions", "Divisions ", 1)}
                {number("pools", "Poules par division ", 1)}
                {number("up", "Montées après chaque tour ", 0)}
                {number("down", "Descentes après chaque tour ", 0)}
                <label class="inline">
                    "Répartition de départ "
                    <select disabled=locked on:change=move |ev| set("start", json!(event_target_value(&ev)))>
                        <option value="ranking" selected=start != "random">"au classement (les meilleurs en D1)"</option>
                        <option value="random" selected=start == "random">"au hasard"</option>
                    </select>
                </label>
            </div>
            <p class="muted small">"Pour un nouveau tour, ajoutez une autre phase « Poules de division » dans la formule : les montées et descentes y sont proposées d'après le tour précédent."</p>
        </details>
    }
}
