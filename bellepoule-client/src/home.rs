//! Accueil : les compétitions du client, création, import d'un fichier.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, ContestSummary, Meta, NewContest};
use crate::properties::{new_properties, PropertiesFields, PropertiesForm};
use crate::{alert, confirm, navigate, Route};

#[component]
pub fn Home() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let contests = LocalResource::new(move || {
        version.get();
        api::get::<Vec<ContestSummary>>("/api/contests")
    });
    let meta = LocalResource::new(|| api::get::<Meta>("/api/meta"));
    let reload = move || version.update(|v| *v += 1);
    let creating = RwSignal::new(false);

    view! {
        <section class="card">
            <div class="card-head">
                <h2>"Compétitions"</h2>
                <span class="spacer"></span>
                <ImportButton on_done=reload />
                <button on:click=move |_| creating.set(true)>"Nouvelle compétition"</button>
            </div>
            {move || match contests.get() {
                None => view! { <p class="muted">"Chargement…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{e.message}</p> }.into_any(),
                Some(Ok(list)) if list.is_empty() => {
                    view! { <p class="muted">"Aucune compétition pour l'instant."</p> }.into_any()
                }
                Some(Ok(list)) => view! { <ContestTable list=list on_change=reload /> }.into_any(),
            }}
        </section>
        <Show when=move || creating.get()>
            {move || match meta.get() {
                Some(Ok(m)) => view! { <NewContestDialog meta=m on_close=move || creating.set(false) /> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{e.message}</p> }.into_any(),
                None => view! { <p class="muted">"Chargement…"</p> }.into_any(),
            }}
        </Show>
    }
}

#[component]
fn ContestTable(list: Vec<ContestSummary>, on_change: impl Fn() + Copy + 'static) -> impl IntoView {
    view! {
        <table class="list">
            <thead>
                <tr>
                    <th></th>
                    <th>"Nom"</th>
                    <th>"Arme"</th>
                    <th>"Sexe"</th>
                    <th>"Catégorie"</th>
                    <th>"Date"</th>
                    <th>"Lieu"</th>
                    <th class="number">"Tireurs"</th>
                    <th>"Phase en cours"</th>
                    <th></th>
                </tr>
            </thead>
            <tbody>
                {list
                    .into_iter()
                    .map(|c| {
                        let open_id = c.id.clone();
                        let del_id = c.id.clone();
                        let name = c.name.clone();
                        let delete = move |ev: leptos::ev::MouseEvent| {
                            ev.stop_propagation();
                            if confirm(&format!("Supprimer « {name} » ?")) {
                                let id = del_id.clone();
                                spawn_local(async move {
                                    if let Err(e) = api::delete(&format!("/api/contests/{id}")).await {
                                        alert(&e.message);
                                    }
                                    on_change();
                                });
                            }
                        };
                        let swatch = format!("background:{}", c.color);
                        view! {
                            <tr class="clickable" on:click=move |_| navigate(&Route::Contest(open_id.clone()))>
                                <td><span class="swatch" style=swatch></span></td>
                                <td class="name">{c.name.clone()}{(!c.label.is_empty()).then(|| format!(" — {}", c.label))}</td>
                                <td>{c.weapon_label.clone()}</td>
                                <td>{c.gender_label.clone()}</td>
                                <td>{c.category.clone()}</td>
                                <td>{c.date.clone()}</td>
                                <td>{c.location.clone()}</td>
                                <td class="number">{c.fencers}</td>
                                <td>{c.stage.clone()}</td>
                                <td class="actions">
                                    <button class="danger small" on:click=delete>"Supprimer"</button>
                                </td>
                            </tr>
                        }
                    })
                    .collect_view()}
            </tbody>
        </table>
    }
}

#[component]
fn NewContestDialog(meta: Meta, on_close: impl Fn() + Copy + 'static) -> impl IntoView {
    let form = PropertiesForm::new(&new_properties());
    let youth = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let input = NewContest { properties: form.read(), youth_training: youth.get() };
        spawn_local(async move {
            match api::send::<_, ContestSummary>("POST", "/api/contests", &input).await {
                Ok(c) => {
                    on_close();
                    navigate(&Route::Contest(c.id));
                }
                Err(e) => error.set(Some(e.message)),
            }
        });
    };
    view! {
        <div class="overlay">
            <form class="dialog" on:submit=submit>
                <h2>"Nouvelle compétition"</h2>
                <PropertiesFields form=form meta=meta />
                <label class="check">
                    <input type="checkbox" prop:checked=move || youth.get() on:change=move |ev| youth.set(event_target_checked(&ev)) />
                    "Entraînement des jeunes (groupes, poules et tableau par groupe)"
                </label>
                {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
                <div class="buttons">
                    <button type="button" class="light" on:click=move |_| on_close()>"Annuler"</button>
                    <button type="submit">"Créer"</button>
                </div>
            </form>
        </div>
    }
}

/// Lit le fichier choisi dans un champ `<input type="file">`.
pub async fn read_file(input: &web_sys::HtmlInputElement) -> Option<(String, Vec<u8>)> {
    let file = input.files()?.get(0)?;
    let name = file.name();
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await.ok()?;
    Some((name, js_sys::Uint8Array::new(&buffer).to_vec()))
}

/// Import d'un fichier BellePoule (`.cotcot`).
#[component]
fn ImportButton(on_done: impl Fn() + Copy + 'static) -> impl IntoView {
    let input = NodeRef::<leptos::html::Input>::new();
    let changed = move |_| {
        let Some(el) = input.get() else { return };
        spawn_local(async move {
            if let Some((_, bytes)) = read_file(&el).await {
                match api::upload::<ContestSummary>("/api/contests/import", bytes).await {
                    Ok(c) => navigate(&Route::Contest(c.id)),
                    Err(e) => alert(&e.message),
                }
                on_done();
            }
            el.set_value("");
        });
    };
    view! {
        <label class="button light">
            "Ouvrir un fichier .cotcot"
            <input type="file" accept=".cotcot,.xml" class="hidden" node_ref=input on:change=changed />
        </label>
    }
}
