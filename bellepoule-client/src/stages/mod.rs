//! Écrans des phases. Chaque écran lit la vue de sa phase
//! (`/api/contests/{id}/stages/{index}`) et envoie ses actions au serveur,
//! qui renvoie la vue à jour.

mod allocator;
mod groups;
mod lists;
mod pools;
mod sheets;
mod table;
mod youth;

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;

use crate::api;
use crate::alert;

/// Champ texte d'une valeur JSON.
pub fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

pub fn u(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}

pub fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

pub fn arr(v: &Value, k: &str) -> Vec<Value> {
    v.get(k).and_then(Value::as_array).cloned().unwrap_or_default()
}

/// Donne le focus à l'élément `id` (après le rafraîchissement de l'écran).
pub fn focus_later(id: String) {
    set_timeout(
        move || {
            if let Some(el) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(&id)) {
                if let Some(input) = wasm_bindgen::JsCast::dyn_ref::<web_sys::HtmlInputElement>(&el) {
                    let _ = input.focus();
                    input.select();
                }
            }
        },
        std::time::Duration::from_millis(30),
    );
}

/// Accès à une phase : sa vue, l'envoi d'actions et la case à activer une
/// fois la réponse du serveur affichée.
#[derive(Clone, Copy)]
pub struct StageCtx {
    pub data: RwSignal<Option<Value>>,
    pub act: Callback<Value>,
    pub focus: RwSignal<Option<String>>,
    /// Adresse de la phase (`/api/contests/{id}/stages/{index}`).
    pub path: StoredValue<String>,
}

impl StageCtx {
    /// Action, puis focus sur `next` quand l'écran est à jour.
    pub fn act_then_focus(&self, body: Value, next: String) {
        self.focus.set(Some(next));
        self.act.run(body);
    }
}

#[component]
pub fn StageView(id: String, index: usize, kind: String, on_change: Callback<()>) -> impl IntoView {
    let data = RwSignal::new(None::<Value>);
    let focus = RwSignal::new(None::<String>);
    let path = StoredValue::new(format!("/api/contests/{id}/stages/{index}"));
    {
        let path = path.get_value();
        spawn_local(async move {
            match api::get::<Value>(&path).await {
                Ok(v) => data.set(Some(v)),
                Err(e) => alert(&e.message),
            }
        });
    }
    let act = Callback::new(move |body: Value| {
        let path = path.get_value();
        spawn_local(async move {
            match api::send::<_, Value>("POST", &path, &body).await {
                Ok(v) => {
                    data.set(Some(v));
                    if let Some(id) = focus.get_untracked() {
                        focus.set(None);
                        focus_later(id);
                    }
                    on_change.run(());
                }
                Err(e) => {
                    alert(&e.message);
                    // Revenir à l'état du serveur.
                    if let Ok(v) = api::get::<Value>(&path).await {
                        data.set(Some(v));
                    }
                }
            }
        });
    });
    let ctx = StageCtx { data, act, focus, path };
    let contest = StoredValue::new(id);
    view! {
        <BoutTimingSettings ctx=ctx />
        {move || match kind.as_str() {
            "youth" => view! { <youth::Youth ctx=ctx /> }.into_any(),
            "pool_allocator" => view! { <allocator::Allocator ctx=ctx /> }.into_any(),
            "pool_supervisor" => view! { <pools::Pools ctx=ctx /> }.into_any(),
            "table" => view! { <table::Table ctx=ctx /> }.into_any(),
            "barrage" => view! { <lists::Barrage ctx=ctx /> }.into_any(),
            "splitting" => view! { <lists::Splitting ctx=ctx /> }.into_any(),
            "classification" => view! { <lists::Classification ctx=ctx contest=contest.get_value() /> }.into_any(),
            _ => view! { <p class="muted">"Rien à afficher pour cette phase."</p> }.into_any(),
        }}
    }
}

/// Bouton « Imprimer » (impression du navigateur, mise en page `@media print`).
#[component]
pub fn PrintButton() -> impl IntoView {
    view! {
        <button class="light" on:click=move |_| {
            if let Some(w) = web_sys::window() {
                let _ = w.print();
            }
        }>"Imprimer"</button>
    }
}

/// Liste générique renvoyée par l'API (`columns`, `rows`).
#[component]
pub fn ListTable(list: Value, #[prop(optional)] on_toggle: Option<Callback<(u64, bool)>>, #[prop(optional)] locked: bool) -> impl IntoView {
    let columns = arr(&list, "columns");
    let rows = arr(&list, "rows");
    let has_check = rows.iter().any(|r| r.get("checked").map(|c| !c.is_null()).unwrap_or(false));
    view! {
        <table class="list">
            <thead>
                <tr>
                    {has_check.then(|| view! { <th></th> })}
                    {columns.iter().map(|c| view! { <th>{s(c, "title")}</th> }).collect_view()}
                </tr>
            </thead>
            <tbody>
                {rows
                    .into_iter()
                    .map(|r| {
                        let reference = u(&r, "reference");
                        let checked = r.get("checked").and_then(Value::as_bool);
                        view! {
                            <tr class:absent=b(&r, "dimmed")>
                                {checked
                                    .map(|c| {
                                        view! {
                                            <td>
                                                <input
                                                    type="checkbox"
                                                    checked=c
                                                    disabled=locked
                                                    on:change=move |ev| {
                                                        if let Some(cb) = on_toggle {
                                                            cb.run((reference, event_target_checked(&ev)));
                                                        }
                                                    }
                                                />
                                            </td>
                                        }
                                    })}
                                {arr(&r, "cells").iter().map(|c| view! { <td>{c.as_str().unwrap_or("").to_string()}</td> }).collect_view()}
                            </tr>
                        }
                    })
                    .collect_view()}
            </tbody>
        </table>
    }
}


#[component]
fn BoutTimingSettings(ctx: StageCtx) -> impl IntoView {
    view! { {move || ctx.data.get().and_then(|view| {
        let kind = s(&view, "kind");
        if !["pool_supervisor", "pool_allocator", "table", "barrage"].contains(&kind.as_str()) { return None; }
        let timing = &view["data"]["timing"];
        let seconds = RwSignal::new(timing.get("period_seconds").and_then(Value::as_u64).unwrap_or(180).to_string());
        let periods = RwSignal::new(timing.get("periods").and_then(Value::as_u64).unwrap_or(1).to_string());
        let locked = b(&view, "locked");
        Some(view! {
            <details class="settings">
                <summary>"Chronomètre — formule des matchs"</summary>
                <div class="settings-row">
                    <label class="inline">"Durée d'une manche (secondes) "<input type="number" min="1" max="3600" disabled=locked prop:value=move || seconds.get() on:input=move |ev| seconds.set(event_target_value(&ev)) /></label>
                    <label class="inline">"Nombre de manches "<input type="number" min="1" max="20" disabled=locked prop:value=move || periods.get() on:input=move |ev| periods.set(event_target_value(&ev)) /></label>
                    <button disabled=locked on:click=move |_| ctx.act.run(serde_json::json!({"action":"timing", "timing":{"period_seconds":seconds.get().parse::<u32>().unwrap_or(0),"periods":periods.get().parse::<u32>().unwrap_or(0)}}))>"Appliquer"</button>
                </div>
            </details>
        })
    })} }
}
