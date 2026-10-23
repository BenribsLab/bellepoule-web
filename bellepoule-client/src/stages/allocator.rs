//! Composition des poules : formule, décalage, poules en deux colonnes et
//! glisser-déposer d'un tireur d'une poule à l'autre.

use leptos::prelude::*;
use serde_json::{json, Value};

use super::{arr, b, s, u, PrintButton, StageCtx};

#[component]
pub fn Allocator(ctx: StageCtx) -> impl IntoView {
    view! {
        {move || {
            ctx.data
                .get()
                .map(|v| {
                    let d = v["data"].clone();
                    let locked = b(&d, "locked");
                    let youth = b(&d, "youth");
                    let selected = d.get("selected").and_then(Value::as_u64);
                    let best = d.get("best").and_then(Value::as_u64);
                    let criteria: Vec<String> = arr(&d, "swapping").iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
                    let columns = arr(&d, "columns");
                    let regular: Vec<String> = arr(&d, "regular_choices").iter().map(|c| s(c, "code")).collect();
                    let extra: Vec<Value> = arr(&d, "extra_choices");
                    // Critères supplémentaires choisis, dans leur ordre de priorité.
                    let extras_on: Vec<String> = criteria.iter().filter(|c| !regular.contains(c)).cloned().collect();
                    let send_swapping = {
                        let regular = regular.clone();
                        move |regular_on: Vec<String>, extras: Vec<String>| {
                            let regular_on: Vec<String> = regular_on.into_iter().filter(|c| regular.contains(c)).collect();
                            ctx.act.run(json!({ "action": "swapping", "regular": regular_on, "extras": extras }));
                        }
                    };
                    let send_swapping = StoredValue::new(send_swapping);
                    let has_errors = u(&d, "swapping_errors") > 0;
                    view! {
                        <div class="stage-bar">
                            <label class="inline">
                                "Formule "
                                <select
                                    disabled=locked || youth
                                    on:change=move |ev| {
                                        let index: u64 = event_target_value(&ev).parse().unwrap_or(0);
                                        ctx.act.run(json!({ "action": "config", "index": index }));
                                    }
                                >
                                    {arr(&d, "configs")
                                        .into_iter()
                                        .map(|c| {
                                            let i = u(&c, "index");
                                            let star = if Some(i) == best { "★ " } else { "" };
                                            view! {
                                                <option value=i.to_string() selected=Some(i) == selected>
                                                    {format!("{star}{} poules de {} tireurs", u(&c, "nb_pools"), s(&c, "size"))}
                                                </option>
                                            }
                                        })
                                        .collect_view()}
                                </select>
                            </label>
                            {youth.then(|| view! { <span class="muted">"Poules fixées par les groupes de l'entraînement des jeunes."</span> })}
                            <span class="spacer"></span>
                            <span class:stage-error=has_errors>
                                {format!("Décalage ({} erreur(s)) :", u(&d, "swapping_errors"))}
                            </span>
                            {arr(&d, "regular_choices")
                                .into_iter()
                                .map(|c| {
                                    let code = s(&c, "code");
                                    let on = criteria.contains(&code);
                                    let all = criteria.clone();
                                    let extras_on = extras_on.clone();
                                    view! {
                                        <label class="check">
                                            <input
                                                type="checkbox"
                                                checked=on
                                                disabled=locked
                                                on:change=move |ev| {
                                                    let mut list = all.clone();
                                                    list.retain(|x| x != &code);
                                                    if event_target_checked(&ev) {
                                                        list.push(code.clone());
                                                    }
                                                    send_swapping.with_value(|f| f(list, extras_on.clone()));
                                                }
                                            />
                                            {s(&c, "label")}
                                        </label>
                                    }
                                })
                                .collect_view()}
                            // Un menu par critère supplémentaire (avec ×), puis un menu pour en ajouter un autre.
                            {extras_on
                                .iter()
                                .enumerate()
                                .map(|(i, current)| {
                                    let (all, extras_change, extras_remove) = (criteria.clone(), extras_on.clone(), extras_on.clone());
                                    let all2 = all.clone();
                                    view! {
                                        <span class="extra-criterion">
                                            <select
                                                disabled=locked
                                                on:change=move |ev| {
                                                    let mut list = extras_change.clone();
                                                    list[i] = event_target_value(&ev);
                                                    send_swapping.with_value(|f| f(all.clone(), list));
                                                }
                                            >
                                                {extra
                                                    .iter()
                                                    .map(|c| {
                                                        let code = s(c, "code");
                                                        let on = &code == current;
                                                        view! { <option value=code selected=on>{s(c, "label")}</option> }
                                                    })
                                                    .collect_view()}
                                            </select>
                                            <button
                                                class="ghost icon-only small"
                                                title="Retirer ce critère"
                                                disabled=locked
                                                on:click=move |_| {
                                                    let mut list = extras_remove.clone();
                                                    list.remove(i);
                                                    send_swapping.with_value(|f| f(all2.clone(), list));
                                                }
                                            >
                                                "×"
                                            </button>
                                        </span>
                                    }
                                })
                                .collect_view()}
                            <select
                                disabled=locked
                                on:change={
                                    let all = criteria.clone();
                                    let extras_on = extras_on.clone();
                                    move |ev| {
                                        let value = event_target_value(&ev);
                                        if !value.is_empty() {
                                            let mut list = extras_on.clone();
                                            list.push(value);
                                            send_swapping.with_value(|f| f(all.clone(), list));
                                        }
                                    }
                                }
                            >
                                <option value="" selected=true>{if extras_on.is_empty() { "(autre critère)" } else { "+ autre critère" }}</option>
                                {extra
                                    .iter()
                                    .filter(|c| !extras_on.contains(&s(c, "code")))
                                    .map(|c| view! { <option value=s(c, "code")>{s(c, "label")}</option> })
                                    .collect_view()}
                            </select>
                            // Entraînement des jeunes : on imprime toujours le groupe complet.
                            {if youth { view! { <super::groups::GroupSheetsButton ctx=ctx /> }.into_any() } else { view! { <PrintButton /> }.into_any() }}
                        </div>
                        <div class="pools-grid">
                            {arr(&d, "pools").into_iter().map(|p| view! { <PoolCard ctx=ctx pool=p columns=columns.clone() locked=locked /> }).collect_view()}
                        </div>
                    }
                })
        }}
    }
}

#[component]
fn PoolCard(ctx: StageCtx, pool: Value, columns: Vec<Value>, locked: bool) -> impl IntoView {
    let index = u(&pool, "index");
    let over = RwSignal::new(false);
    let drop = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        over.set(false);
        if let Some(player) = ev.data_transfer().and_then(|t| t.get_data("text/plain").ok()).and_then(|t| t.parse::<u64>().ok()) {
            ctx.act.run(json!({ "action": "move", "player": player, "pool": index }));
        }
    };
    let balanced = b(&pool, "balanced");
    view! {
        <div
            class="pool card"
            class:drop-over=move || over.get()
            on:dragover=move |ev| {
                if !locked {
                    ev.prevent_default();
                    over.set(true);
                }
            }
            on:dragleave=move |_| over.set(false)
            on:drop=drop
        >
            <div class="pool-head">
                <span class=if balanced { "ok" } else { "ko" }>{if balanced { "✔" } else { "✖" }}</span>
                <strong>{s(&pool, "name")}</strong>
                {pool.get("piste").and_then(Value::as_str).map(|p| view! { <em class="roadmap">{p.to_string()}</em> })}
            </div>
            <table class="list compact">
                <thead>
                    <tr>
                        <th></th>
                        {columns.iter().map(|c| view! { <th>{s(c, "title")}</th> }).collect_view()}
                    </tr>
                </thead>
                <tbody>
                    {arr(&pool, "fencers")
                        .into_iter()
                        .map(|f| {
                            let reference = u(&f, "reference");
                            let mark = match s(&f, "mark").as_str() {
                                "error" => "⚠",
                                "moved" => "↪",
                                _ => "",
                            };
                            view! {
                                <tr
                                    class="draggable"
                                    draggable=if locked { "false" } else { "true" }
                                    on:dragstart=move |ev: leptos::ev::DragEvent| {
                                        if let Some(t) = ev.data_transfer() {
                                            let _ = t.set_data("text/plain", &reference.to_string());
                                        }
                                    }
                                >
                                    <td class="mark">{mark}</td>
                                    {arr(&f, "cells").iter().map(|c| view! { <td>{c.as_str().unwrap_or("").to_string()}</td> }).collect_view()}
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
}
