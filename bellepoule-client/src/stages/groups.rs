//! Entraînement des jeunes : une feuille par groupe pour l'arbitre — les
//! feuilles de ses poules (grilles, liste des matchs avec cases de touches)
//! et son tableau à remplir (1er P1 contre dernier P2…).

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;

use super::{arr, s, u, StageCtx};
use crate::{alert, api};

fn set_body_class(on: bool) {
    if let Some(body) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.body()) {
        let _ = if on { body.class_list().add_1("print-groups") } else { body.class_list().remove_1("print-groups") };
    }
}

#[component]
pub fn GroupSheetsButton(ctx: StageCtx, #[prop(optional)] pool: Option<usize>) -> impl IntoView {
    let sheets = RwSignal::new(None::<Value>);
    // Groupe à imprimer (numéro), ou tous (`None`).
    let only = RwSignal::new(None::<u64>);
    let open = move |_| {
        let path = format!("{}/group_sheets", ctx.path.get_value());
        spawn_local(async move {
            match api::get::<Value>(&path).await {
                Ok(v) => {
                    set_body_class(true);
                    // Par défaut : le groupe de la poule affichée.
                    let group = pool.and_then(|p| {
                        arr(&v, "groups")
                            .into_iter()
                            .find(|g| arr(g, "indices").iter().any(|i| i.as_u64() == Some(p as u64)))
                            .map(|g| u(&g, "group"))
                    });
                    only.set(group);
                    sheets.set(Some(v));
                }
                Err(e) => alert(&e.message),
            }
        });
    };
    let close = move |_| {
        set_body_class(false);
        sheets.set(None);
    };
    view! {
        <button class="light" title="Une page par groupe : ses poules et son tableau" on:click=open>"Imprimer"</button>
        {move || {
            sheets
                .get()
                .map(|v| {
                    // Hors de la barre d'outils (masquée à l'impression) : attachée au document.
                    view! {
                        <leptos::portal::Portal>
                        <div class="print-sheets">
                            <div class="print-bar">
                                <strong>"Une feuille par groupe : ses poules et son tableau"</strong>
                                <span class="spacer"></span>
                                <select on:change=move |ev| only.set(event_target_value(&ev).parse().ok())>
                                    <option value="" selected=move || only.get().is_none()>"Tous les groupes"</option>
                                    {arr(&v, "groups")
                                        .into_iter()
                                        .map(|g| {
                                            let n = u(&g, "group");
                                            view! { <option value=n.to_string() selected=move || only.get() == Some(n)>{format!("Seulement le groupe {n}")}</option> }
                                        })
                                        .collect_view()}
                                </select>
                                <button on:click=move |_| {
                                    if let Some(w) = web_sys::window() {
                                        let _ = w.print();
                                    }
                                }>"Imprimer"</button>
                                <button class="light" on:click=close>"Fermer"</button>
                            </div>
                            {let v = v.clone(); move || arr(&v, "groups")
                                .into_iter()
                                .filter(|g| only.get().is_none_or(|n| u(g, "group") == n))
                                .map(|g| {
                                    view! {
                                        <section class="print-page">
                                            <header class="page-header">
                                                <div>
                                                    <strong>{s(&v, "contest")}</strong>
                                                    <br />
                                                    {s(&v, "header")}
                                                </div>
                                                <div class="group-title">
                                                    {format!("Groupe {}", u(&g, "group"))}
                                                    {
                                                        let mut all: Vec<String> = Vec::new();
                                                        for p in arr(&g, "pools") {
                                                            for name in referee_names(&p) {
                                                                if !all.contains(&name) {
                                                                    all.push(name);
                                                                }
                                                            }
                                                        }
                                                        (!all.is_empty()).then(|| view! { <div class="group-referees">{format!("Arbitres : {}", all.join(" · "))}</div> })
                                                    }
                                                </div>
                                                <div>{s(&v, "date")}</div>
                                            </header>
                                            <div class="page-pools">
                                                {arr(&g, "pools").into_iter().map(|p| view! { <PrintPool pool=p /> }).collect_view()}
                                            </div>
                                            {g.get("bracket").filter(|b| !b.is_null()).cloned().map(|b| view! { <PrintBracket bracket=b /> })}
                                        </section>
                                    }
                                })
                                .collect_view()}
                        </div>
                        </leptos::portal::Portal>
                    }
                })
        }}
    }
}

/// « DUPONT Hugo (CLUB B) » pour chaque arbitre de la poule.
fn referee_names(pool: &Value) -> Vec<String> {
    arr(pool, "referees")
        .iter()
        .map(|r| {
            let club = s(r, "club");
            if club.is_empty() { s(r, "name") } else { format!("{} ({club})", s(r, "name")) }
        })
        .collect()
}

#[component]
fn PrintPool(pool: Value) -> impl IntoView {
    let rows = arr(&pool, "rows");
    let n = rows.len();
    let referees = referee_names(&pool);
    let dispatch = arr(&pool, "matches").iter().any(|m| !s(m, "referee").is_empty() || m.get("piste").is_some_and(|p| !p.is_null()));
    view! {
        <div class="print-pool">
            <div class="pp-head">
                <strong>{s(&pool, "name")}</strong>
                <span class="box-label">{if referees.len() > 1 { "Arbitres" } else { "Arbitre" }}</span>
                {if referees.is_empty() {
                    view! { <span class="box-line"></span> }.into_any()
                } else {
                    view! { <span class="pp-referees">{referees.join(" · ")}</span> }.into_any()
                }}
                <span class="box-label">"Piste"</span>
                <span class="box-line short">{pool.get("piste").and_then(Value::as_str).unwrap_or("").to_string()}</span>
            </div>
            <table class="sheet print">
                <thead>
                    <tr>
                        <th></th>
                        <th></th>
                        {(1..=n).map(|i| view! { <th class="num">{i}</th> }).collect_view()}
                        <th class="head">"V"</th>
                        <th class="head">"TD"</th>
                        <th class="head">"TR"</th>
                        <th class="head">"Ind."</th>
                        <th class="head">"Pl."</th>
                    </tr>
                </thead>
                <tbody>
                    {rows
                        .into_iter()
                        .map(|r| {
                            view! {
                                <tr>
                                    <td class="name">{s(&r, "name")}</td>
                                    <td class="pos">{u(&r, "position")}</td>
                                    {arr(&r, "grey").into_iter().map(|g| {
                                        let grey = g.as_bool().unwrap_or(false);
                                        view! { <td class="cell" class:grey=grey></td> }
                                    }).collect_view()}
                                    <td class="cell small"></td>
                                    <td class="cell small"></td>
                                    <td class="cell small"></td>
                                    <td class="cell small"></td>
                                    <td class="cell small"></td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
            <div class="pp-bouts">
                {arr(&pool, "matches")
                    .into_iter()
                    .map(|m| {
                        let boxes = u(&m, "boxes");
                        view! {
                            <div class="pp-bout">
                                <strong>{s(&m, "name")}</strong>
                                {dispatch.then(|| view! {
                                    <span class="pp-res">
                                        {m.get("piste").and_then(Value::as_u64).map(|p| format!("Piste {p:02}")).unwrap_or_default()}
                                        <br />
                                        <b>{s(&m, "referee")}</b>
                                    </span>
                                })}
                                <div style="white-space:pre-line;font-size:10px">{s(&m, "officiating")}</div>
                                <div>
                                    {arr(&m, "opponents")
                                        .into_iter()
                                        .map(|o| {
                                            view! {
                                                <div class="pp-opp">
                                                    <span>{format!("{} {}", u(&o, "position"), s(&o, "name"))}</span>
                                                    <span class="touches">{(1..=boxes).map(|i| view! { <span class="touch">{i}</span> }).collect_view()}</span>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}

#[component]
fn PrintBracket(bracket: Value) -> impl IntoView {
    let width = bracket.get("width").and_then(Value::as_f64).unwrap_or(0.0);
    let height = bracket.get("height").and_then(Value::as_f64).unwrap_or(0.0);
    // Réduit pour tenir dans la largeur d'une page A4 (≈ 720 px utiles).
    let scale = (720.0 / width.max(1.0)).min(1.0);
    let lines: Vec<String> = arr(&bracket, "lines")
        .iter()
        .map(|points| {
            points
                .as_array()
                .map(|p| p.iter().filter_map(|xy| Some(format!("{},{}", xy.get(0)?.as_f64()?, xy.get(1)?.as_f64()?))).collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
        })
        .collect();
    let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    view! {
        <div class="print-bracket" style=format!("width:{}px;height:{}px", width * scale, height * scale)>
            <div class="bracket" style=format!("width:{width}px;height:{height}px;transform:scale({scale});transform-origin:0 0")>
                <svg class="connectors" width=width.to_string() height=height.to_string()>
                    {lines.into_iter().map(|p| view! { <polyline points=p fill="none" stroke="#333" stroke-width="1.5" /> }).collect_view()}
                </svg>
                {arr(&bracket, "items")
                    .into_iter()
                    .map(|it| {
                        let pos = format!("left:{}px;top:{}px", num(&it, "x"), num(&it, "y"));
                        match s(&it, "t").as_str() {
                            "slot" => view! { <div class="bs-slot" style=format!("{pos};width:{}px;height:{}px", num(&it, "w"), num(&it, "h"))>{s(&it, "text")}</div> }.into_any(),
                            "score" => view! { <div class="bs-score" style=format!("{pos};width:{}px;height:{}px", num(&it, "w"), num(&it, "h"))></div> }.into_any(),
                            "match" => view! { <div class="bs-match" style=pos>{s(&it, "text")}</div> }.into_any(),
                            _ => view! { <div class="bs-title" style=pos>{s(&it, "text")}</div> }.into_any(),
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}
