//! Publication d'une épreuve sur compete.managecomp.org : page publique
//! (formule, tireurs, poules, tableaux, classement) que parents et tireurs
//! suivent en scannant un QR code, gardée en archive.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{json, Value};

use crate::icons::Icon;
use crate::qr::QrCode;
use crate::{alert, api, confirm};

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Bouton « Publier » de l'en-tête d'une compétition et sa fenêtre.
#[component]
pub fn PublishButton(contest: String) -> impl IntoView {
    let open = RwSignal::new(false);
    let contest = StoredValue::new(contest);
    view! {
        <button class="glass" title="Page publique des résultats (compete.managecomp.org)" on:click=move |_| open.set(true)>
            <Icon name="activity" />"Publier"
        </button>
        <Show when=move || open.get()>
            <leptos::portal::Portal>
                <PublishDialog contest=contest.get_value() on_close=Callback::new(move |_| open.set(false)) />
            </leptos::portal::Portal>
        </Show>
    }
}

#[component]
fn PublishDialog(contest: String, on_close: Callback<()>) -> impl IntoView {
    let path = StoredValue::new(format!("/api/contests/{contest}/publication"));
    let state = RwSignal::new(None::<Value>);
    let names = RwSignal::new(String::from("full"));
    let noindex = RwSignal::new(false);
    let busy = RwSignal::new(false);
    // Événement : les compétitions ouvertes (onglets) publiées ensemble.
    let grouped = RwSignal::new(false);
    let ev_name = RwSignal::new(String::new());
    let ev_cats = RwSignal::new(String::new());
    let open_count = RwSignal::new(0usize);
    let apply = move |v: Value| {
        if v.get("published").and_then(Value::as_bool).unwrap_or(false) {
            names.set(s(&v, "names"));
            noindex.set(v.get("noindex").and_then(Value::as_bool).unwrap_or(false));
        }
        if let Some(n) = v.get("open").and_then(Value::as_array).map(Vec::len) {
            open_count.set(n);
        }
        match v.get("event").filter(|e| !e.is_null()) {
            Some(e) => {
                grouped.set(true);
                ev_name.set(s(e, "name"));
                ev_cats.set(s(e, "categories"));
            }
            None => {
                if let Some(d) = v.get("event_defaults") {
                    if ev_name.get_untracked().is_empty() {
                        ev_name.set(s(d, "name"));
                        ev_cats.set(s(d, "categories"));
                    }
                    // Plusieurs compétitions ouvertes : l'événement est proposé.
                    if !v.get("published").and_then(Value::as_bool).unwrap_or(false) {
                        grouped.set(open_count.get_untracked() >= 2);
                    }
                }
            }
        }
        state.set(Some(v));
    };
    spawn_local(async move {
        match api::get::<Value>(&path.get_value()).await {
            Ok(v) => apply(v),
            Err(e) => alert(&e.message),
        }
    });
    let publish = move |_| {
        busy.set(true);
        let mut body = json!({ "names": names.get_untracked(), "noindex": noindex.get_untracked() });
        if grouped.get_untracked() {
            body["event"] = json!({ "name": ev_name.get_untracked(), "categories": ev_cats.get_untracked() });
        }
        spawn_local(async move {
            match api::send::<_, Value>("POST", &path.get_value(), &body).await {
                Ok(v) => apply(v),
                Err(e) => alert(&e.message),
            }
            busy.set(false);
        });
    };
    let unpublish = move |_| {
        if !confirm("Dépublier cette épreuve ?\n\nSa page publique et son QR code ne marcheront plus. Vous pourrez la publier à nouveau (nouvelle adresse).") {
            return;
        }
        spawn_local(async move {
            match api::send::<_, Value>("DELETE", &path.get_value(), &json!({})).await {
                Ok(v) => apply(v),
                Err(e) => alert(&e.message),
            }
        });
    };
    let copy = move |text: String| {
        if let Some(w) = web_sys::window() {
            let _ = w.navigator().clipboard().write_text(&text);
        }
    };
    view! {
        <div class="overlay" on:click=move |_| on_close.run(())>
            <div class="dialog publish-dialog" on:click=|ev| ev.stop_propagation()>
                <h2>"Publier l'épreuve"</h2>
                <p class="muted small">
                    "Une page publique suit l'épreuve en direct (formule, tireurs, poules, tableaux, classement), avec des filtres par nom et par club. "
                    "Parents et tireurs y accèdent en scannant le QR code ; elle reste consultable après la compétition."
                </p>
                <fieldset class="scores-policy">
                    <legend>"Noms des tireurs"</legend>
                    <label class="check">
                        <input type="radio" name="publish-names" prop:checked=move || names.get() == "full" on:change=move |_| names.set("full".into()) />
                        "Noms complets"
                    </label>
                    <label class="check" title="Conseillé pour les jeunes : « Lina M. »">
                        <input type="radio" name="publish-names" prop:checked=move || names.get() == "short" on:change=move |_| names.set("short".into()) />
                        "Prénom et initiale du nom"
                    </label>
                </fieldset>
                {move || (open_count.get() >= 2 || grouped.get()).then(|| view! {
                    <fieldset class="scores-policy publish-event">
                        <legend>"Événement"</legend>
                        <label class="check">
                            <input type="checkbox" prop:checked=move || grouped.get() on:change=move |ev| grouped.set(event_target_checked(&ev)) />
                            {move || format!("Regrouper les {} compétitions ouvertes dans un événement", open_count.get().max(1))}
                        </label>
                        <Show when=move || grouped.get()>
                            <label>
                                "Nom de l'événement"
                                <input type="text" prop:value=move || ev_name.get() on:input=move |ev| ev_name.set(event_target_value(&ev)) />
                            </label>
                            <label>
                                "Catégories"
                                <input type="text" prop:value=move || ev_cats.get() on:input=move |ev| ev_cats.set(event_target_value(&ev)) />
                            </label>
                            <p class="muted small">"Sur la page publique, on voit d'abord l'événement, puis chacune de ses épreuves. Toutes les compétitions ouvertes sont publiées."</p>
                        </Show>
                    </fieldset>
                })}
                <label class="check" title="La page reste accessible par son QR code et son lien, mais n'apparaît pas sur Google">
                    <input type="checkbox" prop:checked=move || noindex.get() on:change=move |ev| noindex.set(event_target_checked(&ev)) />
                    "Cacher aux moteurs de recherche"
                </label>
                {move || state.get().map(|v| {
                    if v.get("published").and_then(Value::as_bool).unwrap_or(false) {
                        // Événement : son QR code et son lien d'abord.
                        let event = v.get("event").filter(|e| !e.is_null()).cloned();
                        let (url, short) = match &event {
                            Some(e) => (s(e, "url"), s(e, "short_url")),
                            None => (s(&v, "url"), s(&v, "short_url")),
                        };
                        let contest_url = s(&v, "url");
                        let (url2, short2) = (url.clone(), short.clone());
                        view! {
                            <div class="publish-done">
                                <div class="publish-qr"><QrCode text=short.clone() /></div>
                                <div class="publish-links">
                                    <strong>{if event.is_some() { "Événement publié ✔" } else { "Publiée ✔" }}</strong>
                                    <a href=url.clone() target="_blank" rel="noopener">{url.clone()}</a>
                                    <div class="row">
                                        <button class="light small" on:click=move |_| copy(url2.clone())>"Copier le lien"</button>
                                        <button class="light small" title="Lien court (QR code)" on:click=move |_| copy(short2.clone())>"Copier le lien court"</button>
                                    </div>
                                    <span class="muted small">{format!("QR code : {short}")}</span>
                                    {event.is_some().then(|| view! { <span class="small">"Cette épreuve : "<a href=contest_url.clone() target="_blank" rel="noopener">{contest_url.clone()}</a></span> })}
                                </div>
                            </div>
                            <div class="buttons">
                                <button type="button" class="ghost danger" on:click=unpublish>"Dépublier"</button>
                                <span class="spacer"></span>
                                <button type="button" class="light" on:click=move |_| on_close.run(())>"Fermer"</button>
                                <button type="button" disabled=move || busy.get() on:click=publish>"Enregistrer les options"</button>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="buttons">
                                <button type="button" class="light" on:click=move |_| on_close.run(())>"Annuler"</button>
                                <button type="button" disabled=move || busy.get() on:click=publish>"Publier"</button>
                            </div>
                        }.into_any()
                    }
                })}
            </div>
        </div>
    }
}
