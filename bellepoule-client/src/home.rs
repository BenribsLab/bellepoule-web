//! Accueil : les compétitions du client, création, import d'un fichier.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, ContestSummary, Meta, NewContest};
use crate::icons::Icon;
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

    let search = RwSignal::new(String::new());

    view! {
        <header class="page-head">
            <div>
                <h1>"Compétitions"</h1>
                <p class="muted">
                    {move || match contests.get() {
                        Some(Ok(list)) => match list.len() {
                            0 => "Aucune compétition pour l'instant".to_string(),
                            1 => "1 compétition".to_string(),
                            n => format!("{n} compétitions"),
                        },
                        _ => String::new(),
                    }}
                </p>
            </div>
            <span class="spacer"></span>
            <label class="input-icon search">
                <Icon name="search" />
                <input type="search" placeholder="Rechercher" prop:value=move || search.get() on:input=move |ev| search.set(event_target_value(&ev)) />
            </label>
            <ImportButton on_done=reload />
            <button on:click=move |_| creating.set(true)><Icon name="plus" />"Nouvelle compétition"</button>
        </header>
        {move || match contests.get() {
            None => view! { <div class="contest-grid">{(0..3).map(|_| view! { <div class="contest-card skeleton"></div> }).collect_view()}</div> }.into_any(),
            Some(Err(e)) => view! { <p class="error">{e.message}</p> }.into_any(),
            Some(Ok(list)) if list.is_empty() => view! {
                <div class="empty">
                    <Icon name="trophy" />
                    <h2>"Aucune compétition"</h2>
                    <p class="muted">"Créez une compétition ou ouvrez un fichier .cotcot du logiciel de bureau."</p>
                    <button on:click=move |_| creating.set(true)><Icon name="plus" />"Nouvelle compétition"</button>
                </div>
            }.into_any(),
            Some(Ok(list)) => {
                let needle = search.get().to_lowercase();
                let list: Vec<_> = list
                    .into_iter()
                    .filter(|c| needle.is_empty() || format!("{} {} {} {}", c.name, c.label, c.location, c.category).to_lowercase().contains(&needle))
                    .collect();
                view! { <ContestCards list=list on_change=reload /> }.into_any()
            }
        }}
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
fn ContestCards(list: Vec<ContestSummary>, on_change: impl Fn() + Copy + 'static) -> impl IntoView {
    view! {
        <div class="contest-grid">
            {list
                .into_iter()
                .enumerate()
                .map(|(i, c)| {
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
                    let style = format!("--c:{};--i:{}", c.color, i.min(12));
                    let detail = [c.weapon_label.clone(), c.gender_label.clone(), c.category.clone()]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" · ");
                    view! {
                        <article class="contest-card" style=style tabindex="0" on:click=move |_| navigate(&Route::Contest(open_id.clone()))>
                            <div class="cc-band"></div>
                            <div class="cc-body">
                                <div class="cc-top">
                                    <span class="cc-weapon">{detail}</span>
                                    <button class="ghost icon-only danger" title="Supprimer" on:click=delete><Icon name="trash" /></button>
                                </div>
                                <h3>{c.name.clone()}</h3>
                                {(!c.label.is_empty()).then(|| view! { <p class="muted cc-label">{c.label.clone()}</p> })}
                                <div class="cc-meta">
                                    {(!c.date.is_empty()).then(|| view! { <span><Icon name="calendar" />{c.date.clone()}</span> })}
                                    {(!c.location.is_empty()).then(|| view! { <span><Icon name="pin" />{c.location.clone()}</span> })}
                                </div>
                                <div class="cc-foot">
                                    <span class="chip"><Icon name="users" />{c.fencers}</span>
                                    {(!c.stage.is_empty()).then(|| view! { <span class="chip accent">{c.stage.clone()}</span> })}
                                    <span class="spacer"></span>
                                    <span class="cc-open"><Icon name="chevron-right" /></span>
                                </div>
                            </div>
                        </article>
                    }
                })
                .collect_view()}
        </div>
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
            <Icon name="upload" />
            "Ouvrir un fichier"
            <input type="file" accept=".cotcot,.xml" class="hidden" node_ref=input on:change=changed />
        </label>
    }
}
