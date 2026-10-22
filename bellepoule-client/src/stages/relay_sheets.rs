//! Rencontres par équipes : feuilles de match en relais (celles des
//! rencontres de la phase, ou vierges), à l'unité ou par 3 (une par équipe
//! et une pour l'arbitre). Les pages arrivent mises en page par l'API et
//! sont dessinées en SVG.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;

use super::StageCtx;
use crate::{alert, api};

fn set_body_class(on: bool) {
    if let Some(body) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.body()) {
        let _ = if on { body.class_list().add_1("print-groups") } else { body.class_list().remove_1("print-groups") };
    }
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Une page en SVG.
fn svg(width: f64, height: f64, shapes: &[Value]) -> String {
    let mut out = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" class="relay-sheet">"#);
    for s in shapes {
        let n = |k: &str| s.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let t = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        match t("t").as_str() {
            "rect" => {
                let fill = s.get("fill").and_then(Value::as_str).unwrap_or("none");
                let stroke = s.get("stroke").and_then(Value::as_str).unwrap_or("none");
                out.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{fill}" stroke="{stroke}" stroke-width="{}"/>"#,
                    n("x"), n("y"), n("w"), n("h"), n("line")
                ));
            }
            "line" => {
                out.push_str(&format!(
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}"/>"#,
                    n("x1"), n("y1"), n("x2"), n("y2"), t("color"), n("line")
                ));
            }
            "text" => {
                let weight = if s.get("bold").and_then(Value::as_bool).unwrap_or(false) { "700" } else { "400" };
                out.push_str(&format!(
                    r#"<text x="{}" y="{}" font-size="{}" font-weight="{weight}" fill="{}" text-anchor="{}" font-family="sans-serif">{}</text>"#,
                    n("x"), n("y"), n("size"), t("color"), t("anchor"), esc(&t("text"))
                ));
            }
            _ => {}
        }
    }
    out.push_str("</svg>");
    out
}

#[component]
pub fn RelaySheetsButton(ctx: StageCtx, #[prop(optional)] blank_first: bool) -> impl IntoView {
    let choosing = RwSignal::new(false);
    let blank = RwSignal::new(blank_first);
    let count = RwSignal::new("1".to_string());
    let three = RwSignal::new(false);
    let pages = RwSignal::new(None::<Value>);
    let open = move |_| {
        let n = if blank.get_untracked() { count.get_untracked().parse::<u32>().unwrap_or(1).max(1) } else { 0 };
        let path = format!("{}/relay_sheets?blank={n}&three={}", ctx.path.get_value(), three.get_untracked());
        spawn_local(async move {
            match api::get::<Value>(&path).await {
                Ok(v) if v.get("pages").and_then(Value::as_array).is_some_and(|p| !p.is_empty()) => {
                    choosing.set(false);
                    set_body_class(true);
                    pages.set(Some(v));
                }
                Ok(_) => alert("Aucune rencontre à imprimer : choisissez des feuilles vierges."),
                Err(e) => alert(&e.message),
            }
        });
    };
    let close = move |_| {
        set_body_class(false);
        pages.set(None);
    };
    view! {
        <button class="light" on:click=move |_| choosing.set(true)>"Feuilles de match"</button>
        <Show when=move || choosing.get()>
            <div class="modal-backdrop" on:click=move |_| choosing.set(false)>
                <div class="modal" on:click=|ev| ev.stop_propagation()>
                    <h3>"Feuilles de match par équipes"</h3>
                    <label class="check"><input type="radio" name="relay-kind" prop:checked=move || !blank.get() on:change=move |_| blank.set(false) />"Les rencontres de la phase (remplies dès que l'arbitre a saisi la composition sur son téléphone)"</label>
                    <label class="check">
                        <input type="radio" name="relay-kind" prop:checked=move || blank.get() on:change=move |_| blank.set(true) />
                        "Des feuilles vierges : "
                        <input type="number" min="1" max="200" style="width:5em" prop:value=move || count.get() on:input=move |ev| count.set(event_target_value(&ev)) />
                    </label>
                    <label class="check"><input type="checkbox" prop:checked=move || three.get() on:change=move |ev| three.set(event_target_checked(&ev)) />"Par 3 : une feuille pour chaque équipe et une pour l'arbitre"</label>
                    <p class="muted small">"Les tireurs ne sont jamais préremplis : chaque équipe donne son ordre à l'arbitre au début de la rencontre."</p>
                    <div class="actions">
                        <button class="light" on:click=move |_| choosing.set(false)>"Annuler"</button>
                        <button on:click=open>"Afficher"</button>
                    </div>
                </div>
            </div>
        </Show>
        {move || {
            pages.get().map(|v| {
                let width = v.get("width").and_then(Value::as_f64).unwrap_or(918.0);
                let height = v.get("height").and_then(Value::as_f64).unwrap_or(1300.0);
                let list = v.get("pages").and_then(Value::as_array).cloned().unwrap_or_default();
                let count_pages = list.len();
                let label = v.get("label").and_then(Value::as_str).unwrap_or("").to_string();
                view! {
                    <leptos::portal::Portal>
                        <div class="print-sheets">
                            <div class="print-bar">
                                <strong>{format!("{} feuille(s) · {}", count_pages, label)}</strong>
                                <span class="spacer"></span>
                                <button on:click=move |_| {
                                    if let Some(w) = web_sys::window() {
                                        let _ = w.print();
                                    }
                                }>"Imprimer"</button>
                                <button class="light" on:click=close>"Fermer"</button>
                            </div>
                            {let list = list.clone(); move || list
                                .clone()
                                .into_iter()
                                .map(|page| {
                                    let shapes = page.as_array().cloned().unwrap_or_default();
                                    view! { <section class="print-page relay-page" inner_html=svg(width, height, &shapes)></section> }
                                })
                                .collect_view()}
                        </div>
                    </leptos::portal::Portal>
                }
            })
        }}
    }
}
