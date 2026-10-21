//! Feuilles de match imprimables (barrage et tableaux), comme celles du
//! logiciel : nom du match, cadres arbitre et piste, puis pour chaque tireur
//! une case par touche à cocher, la case du score et la signature.

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

/// Bouton « Feuilles de match » : aperçu (4 feuilles par page A4) puis
/// impression par le navigateur.
#[component]
pub fn MatchSheetsButton(ctx: StageCtx) -> impl IntoView {
    let sheets = RwSignal::new(None::<Value>);
    let open = move |_| {
        let path = format!("{}/match_sheets", ctx.path.get_value());
        spawn_local(async move {
            match api::get::<Value>(&path).await {
                Ok(v) => {
                    if arr(&v, "sheets").is_empty() {
                        alert("Aucun match prêt à imprimer.");
                        return;
                    }
                    set_body_class(true);
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
        <button class="light" on:click=open>"Feuilles de match"</button>
        {move || {
            sheets
                .get()
                .map(|v| {
                    let list = arr(&v, "sheets");
                    let pages: Vec<Vec<Value>> = list.chunks(4).map(|c| c.to_vec()).collect();
                    view! {
                        <leptos::portal::Portal>
                            <div class="print-sheets">
                                <div class="print-bar">
                                    <strong>{format!("{} feuille(s) de match", list.len())}</strong>
                                    <span class="spacer"></span>
                                    <button on:click=move |_| {
                                        if let Some(w) = web_sys::window() {
                                            let _ = w.print();
                                        }
                                    }>"Imprimer"</button>
                                    <button class="light" on:click=close>"Fermer"</button>
                                </div>
                                {pages
                                    .clone()
                                    .into_iter()
                                    .map(|page| {
                                        let header = v.clone();
                                        view! {
                                            <section class="print-page">
                                                <header class="page-header">
                                                    <div>
                                                        <strong>{s(&header, "contest")}</strong>
                                                        <br />
                                                        {s(&header, "header")}
                                                    </div>
                                                    <div class="group-title">"Feuilles de match"</div>
                                                    <div>{s(&header, "date")}</div>
                                                </header>
                                                {page.into_iter().map(|sheet| view! { <MatchSheet sheet=sheet /> }).collect_view()}
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

#[component]
fn MatchSheet(sheet: Value) -> impl IntoView {
    let boxes = u(&sheet, "boxes") as usize;
    let referee = sheet.get("referee").and_then(Value::as_str).unwrap_or("").to_string();
    let piste = sheet.get("piste").and_then(Value::as_str).unwrap_or("").to_string();
    view! {
        <div class="match-sheet">
            <div class="ms-head">
                <span class="ms-name">{s(&sheet, "name")}</span>
                <span class="ms-box"><span class="ms-label">"Arbitre"</span>{referee}</span>
                <span class="ms-box"><span class="ms-label">"Piste"</span>{piste}</span>
            </div>
            {arr(&sheet, "fencers")
                .into_iter()
                .map(|f| {
                    view! {
                        <div class="ms-fencer">
                            <div class="ms-identity">
                                <strong>{s(&f, "name")}</strong>
                                <span class="muted">{s(&f, "details")}</span>
                            </div>
                            <div class="ms-touches">
                                {(1..=boxes).map(|i| view! { <span class="ms-touch">{i}</span> }).collect_view()}
                            </div>
                            <span class="ms-score"></span>
                            <span class="ms-signature">"Signature"</span>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}
