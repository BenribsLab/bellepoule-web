//! Appel : liste des tireurs, présence, ajout et modification par la fiche,
//! import d'un fichier FFF ou CSV.

use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, AttendingInput, CheckinView, FencerInput, FormField, ImportReport};
use crate::home::read_file;
use crate::{alert, confirm};

#[component]
pub fn Checkin(id: String, on_change: Callback<()>) -> impl IntoView {
    let id = StoredValue::new(id);
    let view_data = RwSignal::new(None::<CheckinView>);
    let fields = RwSignal::new(Vec::<FormField>::new());
    // Fiche ouverte : `Some(None)` pour un nouveau tireur, `Some(Some(ref))` pour une modification.
    let sheet = RwSignal::new(None::<Option<u32>>);
    let error = RwSignal::new(None::<String>);

    let load = move || {
        let id = id.get_value();
        spawn_local(async move {
            match api::get::<CheckinView>(&format!("/api/contests/{id}/checkin")).await {
                Ok(v) => view_data.set(Some(v)),
                Err(e) => error.set(Some(e.message)),
            }
            if let Ok(f) = api::get::<Vec<FormField>>(&format!("/api/contests/{id}/checkin/form")).await {
                fields.set(f);
            }
        });
    };
    load();

    // Une modification renvoie la nouvelle liste ; la formule est rafraîchie aussi.
    let apply = move |result: Result<CheckinView, api::ApiError>| match result {
        Ok(v) => {
            view_data.set(Some(v));
            on_change.run(());
        }
        Err(e) => alert(&e.message),
    };

    let set_attending = move |references: Vec<u32>, attending: bool| {
        let id = id.get_value();
        spawn_local(async move {
            let r = api::send("PUT", &format!("/api/contests/{id}/checkin/attending"), &AttendingInput { references, attending }).await;
            apply(r);
        });
    };

    let input = NodeRef::<leptos::html::Input>::new();
    let import = move |_| {
        let Some(el) = input.get() else { return };
        let id = id.get_value();
        spawn_local(async move {
            if let Some((name, bytes)) = read_file(&el).await {
                let path = format!("/api/contests/{id}/checkin/import?name={}", js_sys::encode_uri_component(&name));
                match api::upload::<ImportReport>(&path, bytes).await {
                    Ok(r) => {
                        if r.encoding_error {
                            alert("Le fichier comporte des erreurs d'encodage.");
                        }
                        view_data.set(Some(r.checkin));
                        on_change.run(());
                    }
                    Err(e) => alert(&e.message),
                }
            }
            el.set_value("");
        });
    };

    view! {
        {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        {move || {
            view_data
                .get()
                .map(|v| {
                    let locked = v.locked;
                    view! {
                        <div class="stage-bar">
                            <span class="tally">
                                <strong class="present">{v.presents}</strong>
                                " présents · "
                                <strong class="absent">{v.absents}</strong>
                                " absents"
                            </span>
                            <span class="spacer"></span>
                            <Show when=move || !locked>
                                <button class="light" on:click=move |_| set_attending(Vec::new(), true)>"Tous présents"</button>
                                <button class="light" on:click=move |_| set_attending(Vec::new(), false)>"Tous absents"</button>
                                <label class="button light">
                                    "Importer (.fff, .csv)"
                                    <input type="file" accept=".fff,.FFF,.csv,.txt" class="hidden" node_ref=input on:change=import />
                                </label>
                                <button on:click=move |_| sheet.set(Some(None))>"Ajouter un tireur"</button>
                            </Show>
                            <Show when=move || locked>
                                <span class="muted">"Appel terminé : « Tour précédent » pour le modifier."</span>
                            </Show>
                        </div>
                        <table class="list">
                            <thead>
                                <tr>
                                    <th>"Présent"</th>
                                    {v.columns.iter().map(|c| view! { <th>{c.title.clone()}</th> }).collect_view()}
                                </tr>
                            </thead>
                            <tbody>
                                {v
                                    .fencers
                                    .iter()
                                    .map(|f| {
                                        let reference = f.reference;
                                        let attending = f.attending;
                                        view! {
                                            <tr class:absent=!attending class:clickable=!locked on:click=move |_| if !locked { sheet.set(Some(Some(reference))) }>
                                                <td on:click=|ev| ev.stop_propagation()>
                                                    <input
                                                        type="checkbox"
                                                        checked=attending
                                                        disabled=locked
                                                        on:change=move |ev| set_attending(vec![reference], event_target_checked(&ev))
                                                    />
                                                </td>
                                                {f.cells.iter().map(|c| view! { <td>{c.clone()}</td> }).collect_view()}
                                            </tr>
                                        }
                                    })
                                    .collect_view()}
                            </tbody>
                        </table>
                    }
                })
        }}
        {move || sheet.get().map(|reference| view! { <FencerSheet id=id.get_value() reference=reference fields=fields.get() on_close=Callback::new(move |_| sheet.set(None)) on_saved=Callback::new(apply) /> })}
    }
}

/// Fiche d'un tireur (« Formulaire » de l'application) : la touche Entrée
/// valide ; après un ajout, la fiche reste ouverte pour le tireur suivant.
#[component]
fn FencerSheet(
    id: String,
    reference: Option<u32>,
    fields: Vec<FormField>,
    on_close: Callback<()>,
    on_saved: Callback<Result<CheckinView, api::ApiError>>,
) -> impl IntoView {
    let id = StoredValue::new(id);
    let values: BTreeMap<String, RwSignal<String>> = fields.iter().map(|f| (f.code.clone(), RwSignal::new(String::new()))).collect();
    let values = StoredValue::new(values);
    let error = RwSignal::new(None::<String>);
    let first = NodeRef::<leptos::html::Input>::new();

    // Modification : valeurs actuelles du tireur.
    if let Some(r) = reference {
        let id = id.get_value();
        spawn_local(async move {
            if let Ok(current) = api::get::<BTreeMap<String, String>>(&format!("/api/contests/{id}/checkin/fencers/{r}")).await {
                values.with_value(|v| {
                    for (code, signal) in v {
                        signal.set(current.get(code).cloned().unwrap_or_default());
                    }
                });
            }
        });
    }

    let singular: Vec<String> = fields.iter().filter(|f| f.singular).map(|f| f.code.clone()).collect();
    let singular = StoredValue::new(singular);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let input = FencerInput { values: values.with_value(|v| v.iter().map(|(k, s)| (k.clone(), s.get_untracked())).collect()) };
        let id = id.get_value();
        spawn_local(async move {
            let result = match reference {
                Some(r) => api::send::<_, CheckinView>("PUT", &format!("/api/contests/{id}/checkin/fencers/{r}"), &input).await,
                None => api::send::<_, CheckinView>("POST", &format!("/api/contests/{id}/checkin/fencers"), &input).await,
            };
            match result {
                Ok(v) => {
                    on_saved.run(Ok(v));
                    if reference.is_some() {
                        on_close.run(());
                    } else {
                        // Les champs propres au tireur se vident ; le club, le pays… restent.
                        values.with_value(|v| {
                            singular.with_value(|s| {
                                for code in s {
                                    if let Some(sig) = v.get(code) {
                                        sig.set(String::new());
                                    }
                                }
                            })
                        });
                        error.set(None);
                        if let Some(el) = first.get() {
                            let _ = el.focus();
                        }
                    }
                }
                Err(e) => error.set(Some(e.message)),
            }
        });
    };

    let remove = move |_| {
        let Some(r) = reference else { return };
        if !confirm("Retirer ce tireur de la compétition ?") {
            return;
        }
        let id = id.get_value();
        spawn_local(async move {
            let result = api::send::<_, CheckinView>("DELETE", &format!("/api/contests/{id}/checkin/fencers/{r}"), &serde_json::json!({})).await;
            on_saved.run(result);
            on_close.run(());
        });
    };

    let mut first_text = true;
    let rows = fields
        .into_iter()
        .filter(|f| f.code != "team")
        .map(|f| {
            let signal = values.with_value(|v| v[&f.code]);
            let control = match f.kind.as_str() {
                "bool" => view! {
                    <input
                        type="checkbox"
                        prop:checked=move || signal.get() == "1"
                        on:change=move |ev| signal.set(if event_target_checked(&ev) { "1".into() } else { "0".into() })
                    />
                }
                .into_any(),
                "select" => {
                    let choices = f.choices.clone();
                    view! {
                        <select on:change=move |ev| signal.set(event_target_value(&ev)) prop:value=move || signal.get()>
                            <option value="">""</option>
                            {choices
                                .into_iter()
                                .map(|c| {
                                    let v = c.value.clone();
                                    view! { <option value=c.value selected=move || signal.get() == v>{c.label}</option> }
                                })
                                .collect_view()}
                        </select>
                    }
                    .into_any()
                }
                kind => {
                    let list_id = format!("list-{}", f.code);
                    let has_list = !f.choices.is_empty();
                    let input_type = match kind {
                        "number" => "number",
                        _ => "text",
                    };
                    let is_first = first_text;
                    first_text = false;
                    let placeholder = if kind == "date" { "jj.mm.aaaa" } else { "" };
                    let list = has_list.then(|| list_id.clone());
                    let input = if is_first {
                        view! {
                            <input
                                type=input_type
                                list=list
                                placeholder=placeholder
                                node_ref=first
                                prop:value=move || signal.get()
                                on:input=move |ev| signal.set(event_target_value(&ev))
                            />
                        }
                        .into_any()
                    } else {
                        view! {
                            <input
                                type=input_type
                                list=list
                                placeholder=placeholder
                                prop:value=move || signal.get()
                                on:input=move |ev| signal.set(event_target_value(&ev))
                            />
                        }
                        .into_any()
                    };
                    view! {
                        {input}
                        {has_list
                            .then(|| {
                                view! {
                                    <datalist id=list_id>
                                        {f.choices.iter().map(|c| view! { <option value=c.label.clone()></option> }).collect_view()}
                                    </datalist>
                                }
                            })}
                    }
                    .into_any()
                }
            };
            view! {
                <label>
                    {f.title.clone()}
                    {control}
                </label>
            }
        })
        .collect_view();

    Effect::new(move |_| {
        if let Some(el) = first.get() {
            let _ = el.focus();
        }
    });

    view! {
        <div class="overlay">
            <form class="dialog" on:submit=submit>
                <h2>{if reference.is_some() { "Modifier le tireur" } else { "Nouveau tireur" }}</h2>
                <div class="fields">{rows}</div>
                {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
                <div class="buttons">
                    {reference.is_some().then(|| view! { <button type="button" class="danger" on:click=remove>"Retirer"</button> })}
                    <span class="spacer"></span>
                    <button type="button" class="light" on:click=move |_| on_close.run(())>"Fermer"</button>
                    <button type="submit">{if reference.is_some() { "Enregistrer" } else { "Ajouter (Entrée)" }}</button>
                </div>
            </form>
        </div>
    }
}
