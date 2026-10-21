//! Page d'une compétition : en-tête, propriétés, phases de la formule et
//! navigation (« Tour précédent », « Tour suivant »).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, ContestSummary, Meta, Properties, ScheduleView};
use crate::checkin::Checkin;
use crate::properties::{PropertiesFields, PropertiesForm};
use crate::{alert, confirm};

#[component]
pub fn ContestPage(id: String) -> impl IntoView {
    let id = StoredValue::new(id);
    let summary = RwSignal::new(None::<ContestSummary>);
    let schedule = RwSignal::new(None::<ScheduleView>);
    let selected = RwSignal::new(None::<usize>);
    let error = RwSignal::new(None::<String>);
    let editing = RwSignal::new(None::<(Properties, Meta)>);
    let formula = RwSignal::new(false);
    // Incrémenté quand la compétition change (rafraîchit l'écran de la phase).
    let version = RwSignal::new(0u32);
    // Écran affiché : onglet, type de phase, génération (changée au passage de phase).
    let panel = Memo::new(move |_| {
        let index = selected.get()?;
        let kind = schedule.with(|s| s.as_ref().and_then(|s| s.stages.get(index).map(|st| st.kind.clone())))?;
        Some((index, kind, version.get()))
    });

    let reload = move || {
        let id = id.get_value();
        spawn_local(async move {
            match api::get::<ContestSummary>(&format!("/api/contests/{id}")).await {
                Ok(s) => summary.set(Some(s)),
                Err(e) => error.set(Some(e.message)),
            }
            match api::get::<ScheduleView>(&format!("/api/contests/{id}/schedule")).await {
                Ok(s) => {
                    if selected.get_untracked().is_none_or(|i| i >= s.stages.len()) {
                        selected.set(Some(s.current));
                    }
                    schedule.set(Some(s));
                }
                Err(e) => error.set(Some(e.message)),
            }
        });
    };
    reload();

    let step = move |forward: bool| {
        let id = id.get_value();
        if !forward && !confirm("Revenir à la phase précédente ? Ce qui a été fait dans la phase en cours sera défait.") {
            return;
        }
        spawn_local(async move {
            let path = format!("/api/contests/{id}/{}", if forward { "next" } else { "previous" });
            match api::send::<_, ScheduleView>("POST", &path, &serde_json::json!({})).await {
                Ok(s) => {
                    selected.set(Some(s.current));
                    schedule.set(Some(s));
                    // Nouvelle phase : l'écran est reconstruit.
                    version.update(|v| *v += 1);
                    reload();
                }
                Err(e) => alert(&e.message),
            }
        });
    };

    let open_properties = move |_| {
        let id = id.get_value();
        spawn_local(async move {
            let p = api::get::<Properties>(&format!("/api/contests/{id}/properties")).await;
            let m = api::get::<Meta>("/api/meta").await;
            match (p, m) {
                (Ok(p), Ok(m)) => editing.set(Some((p, m))),
                (Err(e), _) | (_, Err(e)) => alert(&e.message),
            }
        });
    };

    let download = move |_| {
        let id = id.get_value();
        let name = summary.get_untracked().map(|s| format!("{}.cotcot", s.name)).unwrap_or_else(|| "competition.cotcot".into());
        spawn_local(async move {
            if let Err(e) = api::download(&format!("/api/contests/{id}/cotcot"), &name).await {
                alert(&e.message);
            }
        });
    };

    view! {
        {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        {move || {
            summary
                .get()
                .map(|s| {
                    let style = format!("background:{}", s.color);
                    view! {
                        <section class="contest-header" style=style>
                            <div class="title">
                                <a href="#/" class="back">"‹ Compétitions"</a>
                                <h2>{s.name.clone()}</h2>
                                <span>{format!("{} - {} - {}", s.weapon_label, s.gender_label, s.category)}</span>
                            </div>
                            <div class="meta">
                                <span>{s.date.clone()}</span>
                                <span>{s.location.clone()}</span>
                            </div>
                        </section>
                    }
                })
        }}
        <section class="toolbar">
            <button class="light" on:click=open_properties>"Propriétés"</button>
            <button class="light" on:click=move |_| formula.set(true)>"Formule"</button>
            <button class="light" on:click=download>"Enregistrer (.cotcot)"</button>
            <span class="spacer"></span>
            {move || schedule.get().and_then(|s| s.error.clone()).map(|e| view! { <span class="stage-error">{e}</span> })}
            <button
                class="light"
                disabled=move || !schedule.get().map(|s| s.can_previous).unwrap_or(false)
                on:click=move |_| step(false)
            >
                "‹ Tour précédent"
            </button>
            <button disabled=move || !schedule.get().map(|s| s.can_next).unwrap_or(false) on:click=move |_| step(true)>
                "Tour suivant ›"
            </button>
        </section>
        <nav class="tabs">
            {move || {
                schedule
                    .get()
                    .map(|s| {
                        let current = s.current;
                        s.stages
                            .iter()
                            .map(|st| {
                                let index = st.index;
                                let future = index > current;
                                let label = if st.name.is_empty() { st.class_name.clone() } else { format!("{} — {}", st.class_name, st.name) };
                                let lock = if st.locked { " 🔒" } else { "" };
                                view! {
                                    <button
                                        class="tab"
                                        class:future=future
                                        class:active=move || selected.get() == Some(index)
                                        on:click=move |_| selected.set(Some(index))
                                    >
                                        {label}
                                        {lock}
                                    </button>
                                }
                            })
                            .collect_view()
                    })
            }}
        </nav>
        <section class="stage">
            {move || {
                // Seuls le changement d'onglet ou de phase reconstruisent l'écran :
                // une saisie (score, présence…) ne doit pas faire perdre le focus.
                let (index, kind, _) = panel.get()?;
                let contest = id.get_value();
                Some(match kind.as_str() {
                    "checkin" => view! { <Checkin id=contest on_change=Callback::new(move |_| reload()) /> }.into_any(),
                    kind => view! { <crate::stages::StageView id=contest index=index kind=kind.to_string() on_change=Callback::new(move |_| reload()) /> }.into_any(),
                })
            }}
        </section>
        {move || {
            editing
                .get()
                .map(|(p, m)| {
                    let form = PropertiesForm::new(&p);
                    let team_editable = summary.get_untracked().map(|s| s.fencers == 0).unwrap_or(true);
                    let err = RwSignal::new(None::<String>);
                    let save = move |ev: leptos::ev::SubmitEvent| {
                        ev.prevent_default();
                        let id = id.get_value();
                        let props = form.read();
                        spawn_local(async move {
                            match api::send::<_, ContestSummary>("PUT", &format!("/api/contests/{id}/properties"), &props).await {
                                Ok(s) => {
                                    summary.set(Some(s));
                                    editing.set(None);
                                }
                                Err(e) => err.set(Some(e.message)),
                            }
                        });
                    };
                    view! {
                        <div class="overlay">
                            <form class="dialog" on:submit=save>
                                <h2>"Propriétés de la compétition"</h2>
                                <PropertiesFields form=form meta=m team_editable=team_editable />
                                {move || err.get().map(|e| view! { <p class="error">{e}</p> })}
                                <div class="buttons">
                                    <button type="button" class="light" on:click=move |_| editing.set(None)>"Annuler"</button>
                                    <button type="submit">"Enregistrer"</button>
                                </div>
                            </form>
                        </div>
                    }
                })
        }}
        <Show when=move || formula.get()>
            <FormulaDialog id=id.get_value() schedule=schedule on_close=Callback::new(move |_| { formula.set(false); version.update(|v| *v += 1); reload(); }) />
        </Show>
    }
}

/// Formule : phases de la compétition, ajout et retrait (« Préférences » de l'application).
#[component]
fn FormulaDialog(id: String, schedule: RwSignal<Option<ScheduleView>>, on_close: Callback<()>) -> impl IntoView {
    let id = StoredValue::new(id);
    let kind = RwSignal::new("table".to_string());
    let after = RwSignal::new(String::new());
    let add = move |_| {
        let id = id.get_value();
        let body = serde_json::json!({ "kind": kind.get(), "after": after.get().parse::<usize>().ok() });
        spawn_local(async move {
            match api::send::<_, ScheduleView>("POST", &format!("/api/contests/{id}/schedule/stages"), &body).await {
                Ok(s) => schedule.set(Some(s)),
                Err(e) => alert(&e.message),
            }
        });
    };
    let remove = move |index: usize| {
        if !confirm("Retirer cette phase de la formule ?") {
            return;
        }
        let id = id.get_value();
        spawn_local(async move {
            match api::send::<_, ScheduleView>("DELETE", &format!("/api/contests/{id}/schedule/stages/{index}"), &serde_json::json!({})).await {
                Ok(s) => schedule.set(Some(s)),
                Err(e) => alert(&e.message),
            }
        });
    };
    view! {
        <div class="overlay">
            <div class="dialog">
                <h2>"Formule de la compétition"</h2>
                <table class="list">
                    <tbody>
                        {move || {
                            schedule
                                .get()
                                .map(|s| {
                                    s.stages
                                        .iter()
                                        .map(|st| {
                                            let index = st.index;
                                            let label = if st.name.is_empty() { st.class_name.clone() } else { format!("{} — {}", st.class_name, st.name) };
                                            view! {
                                                <tr>
                                                    <td class="number">{index + 1}</td>
                                                    <td>{label}{if st.locked { " 🔒" } else { "" }}{if index == s.current { " (en cours)" } else { "" }}</td>
                                                    <td class="actions">
                                                        {st.removable.then(|| view! { <button class="danger small" on:click=move |_| remove(index)>"Retirer"</button> })}
                                                    </td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()
                                })
                        }}
                    </tbody>
                </table>
                <div class="settings-row">
                    "Ajouter "
                    <select on:change=move |ev| kind.set(event_target_value(&ev))>
                        <option value="table" selected=true>"Tableau"</option>
                        <option value="pool_supervisor">"Tour de poules"</option>
                        <option value="splitting">"Point de scission"</option>
                        <option value="youth">"Entraînement des jeunes"</option>
                    </select>
                    " après "
                    <select on:change=move |ev| after.set(event_target_value(&ev))>
                        <option value="">"(à la fin)"</option>
                        {move || {
                            schedule
                                .get()
                                .map(|s| {
                                    s.stages
                                        .iter()
                                        .map(|st| view! { <option value=st.index.to_string()>{format!("{}. {}", st.index + 1, st.class_name)}</option> })
                                        .collect_view()
                                })
                        }}
                    </select>
                    <button on:click=add>"Ajouter"</button>
                </div>
                <div class="buttons">
                    <button on:click=move |_| on_close.run(())>"Fermer"</button>
                </div>
            </div>
        </div>
    }
}
