//! Administration du serveur : demandes d'accès, comptes, clubs, chiffres et
//! compétitions de tous les clubs.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;

use crate::api::{self, ClubInfo, UserInfo};
use crate::icons::Icon;
use crate::{alert, confirm};

#[component]
pub fn AdminPage() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let reload = move || version.update(|v| *v += 1);
    let stats = LocalResource::new(move || {
        version.get();
        api::get::<Value>("/api/admin/stats")
    });
    let users = LocalResource::new(move || {
        version.get();
        api::get::<Vec<UserInfo>>("/api/admin/users")
    });
    let clubs = LocalResource::new(move || {
        version.get();
        api::get::<Vec<ClubInfo>>("/api/admin/clients")
    });

    view! {
        <header class="page-head">
            <div>
                <h1>"Administration"</h1>
                <p class="muted">"Accès, comptes et activité du serveur"</p>
            </div>
            <span class="spacer"></span>
            <button class="light" on:click=move |_| reload()><Icon name="activity" />"Actualiser"</button>
        </header>
        {move || stats.get().map(|s| match s {
            Ok(s) => {
                let n = |v: &Value| v.as_u64().unwrap_or(0);
                let tiles = [
                    ("user", "Comptes actifs", n(&s["users"]["active"]), ""),
                    ("clock", "Demandes en attente", n(&s["users"]["pending"]), if n(&s["users"]["pending"]) > 0 { "warn" } else { "" }),
                    ("building", "Clubs", n(&s["clubs"]), ""),
                    ("trophy", "Compétitions", n(&s["contests"]), ""),
                    ("users", "Tireurs inscrits", n(&s["fencers"]), ""),
                    ("activity", "Écrans de piste actifs", n(&s["live"]), ""),
                ];
                view! {
                    <div class="stat-grid">
                        {tiles.into_iter().enumerate().map(|(i, (icon, label, value, class))| view! {
                            <div class=format!("stat {class}") style=format!("--i:{i}")>
                                <Icon name=icon />
                                <strong>{value}</strong>
                                <span>{label}</span>
                            </div>
                        }).collect_view()}
                    </div>
                }.into_any()
            }
            Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
        })}
        {move || match (users.get(), clubs.get()) {
            (Some(Ok(users)), Some(Ok(clubs))) => {
                let pending: Vec<UserInfo> = users.iter().filter(|u| u.status == "pending").cloned().collect();
                let others: Vec<UserInfo> = users.iter().filter(|u| u.status != "pending").cloned().collect();
                view! {
                    <Pending list=pending clubs=clubs.clone() on_change=reload />
                    <Users list=others on_change=reload />
                    <Clubs list=clubs />
                }.into_any()
            }
            (Some(Err(e)), _) | (_, Some(Err(e))) => view! { <p class="error">{e.message}</p> }.into_any(),
            _ => view! { <div class="card skeleton-block"></div> }.into_any(),
        }}
        {move || stats.get().and_then(Result::ok).map(|s| view! { <Contests list=s["list"].as_array().cloned().unwrap_or_default() /> })}
    }
}

async fn act(method: &str, path: String, body: Value) {
    if let Err(e) = api::send::<_, Value>(method, &path, &body).await {
        alert(&e.message);
    }
}

#[component]
fn Pending(list: Vec<UserInfo>, clubs: Vec<ClubInfo>, on_change: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    if list.is_empty() {
        return view! {
            <section class="card">
                <div class="card-head"><Icon name="clock" /><h2>"Demandes d'accès"</h2></div>
                <p class="muted">"Aucune demande en attente."</p>
            </section>
        }
        .into_any();
    }
    view! {
        <section class="card highlight">
            <div class="card-head">
                <Icon name="clock" />
                <h2>"Demandes d'accès"</h2>
                <span class="pill pending">{list.len()}</span>
            </div>
            <div class="requests">
                {list.into_iter().map(|u| {
                    let target = RwSignal::new(String::new());
                    let (approve_id, refuse_id, who) = (u.id.clone(), u.id.clone(), u.email.clone());
                    let approve = move |_| {
                        let id = approve_id.clone();
                        let club = target.get_untracked();
                        spawn_local(async move {
                            act("POST", format!("/api/admin/users/{id}/approve"), serde_json::json!({ "club": (!club.is_empty()).then_some(club) })).await;
                            on_change();
                        });
                    };
                    let refuse = move |_| {
                        if !confirm(&format!("Refuser la demande de {who} ? Le compte sera supprimé.")) {
                            return;
                        }
                        let id = refuse_id.clone();
                        spawn_local(async move {
                            if let Err(e) = api::delete(&format!("/api/admin/users/{id}")).await {
                                alert(&e.message);
                            }
                            on_change();
                        });
                    };
                    view! {
                        <div class="request">
                            <span class="avatar">{u.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()}</span>
                            <div class="request-who">
                                <strong>{u.name.clone()}</strong>
                                <span class="muted">{u.email.clone()}</span>
                                <span class="muted small">{format!("Club : {} · demandé le {}", u.club_name, api::date_label(&u.created))}</span>
                            </div>
                            <select on:change=move |ev| target.set(event_target_value(&ev))>
                                <option value="">{format!("Nouveau club « {} »", u.club_name)}</option>
                                {clubs.iter().map(|c| view! { <option value=c.id.clone()>{format!("Rejoindre {}", c.name)}</option> }).collect_view()}
                            </select>
                            <div class="row">
                                <button on:click=approve><Icon name="check" />"Valider"</button>
                                <button class="danger" on:click=refuse><Icon name="x" />"Refuser"</button>
                            </div>
                        </div>
                    }
                }).collect_view()}
            </div>
        </section>
    }
    .into_any()
}

#[component]
fn Users(list: Vec<UserInfo>, on_change: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    view! {
        <section class="card">
            <div class="card-head"><Icon name="user" /><h2>"Comptes"</h2><span class="chip">{list.len()}</span></div>
            <div class="table-scroll">
                <table class="list">
                    <thead><tr><th>"Nom"</th><th>"E-mail"</th><th>"Club"</th><th>"État"</th><th>"Dernière connexion"</th><th></th></tr></thead>
                    <tbody>
                        {list.into_iter().map(|u| {
                            let active = u.status == "active";
                            let (id1, id2, id3) = (u.id.clone(), u.id.clone(), u.id.clone());
                            let admin = u.admin;
                            let who = u.email.clone();
                            let toggle = move |_| {
                                let id = id1.clone();
                                spawn_local(async move {
                                    act("PUT", format!("/api/admin/users/{id}"), serde_json::json!({ "status": if active { "disabled" } else { "active" } })).await;
                                    on_change();
                                });
                            };
                            let role = move |_| {
                                let id = id2.clone();
                                spawn_local(async move {
                                    act("PUT", format!("/api/admin/users/{id}"), serde_json::json!({ "admin": !admin })).await;
                                    on_change();
                                });
                            };
                            let remove = move |_| {
                                if !confirm(&format!("Supprimer le compte {who} ? Son club et ses compétitions restent.")) {
                                    return;
                                }
                                let id = id3.clone();
                                spawn_local(async move {
                                    if let Err(e) = api::delete(&format!("/api/admin/users/{id}")).await {
                                        alert(&e.message);
                                    }
                                    on_change();
                                });
                            };
                            view! {
                                <tr>
                                    <td class="name">{u.name.clone()}{u.admin.then(|| view! { <span class="chip accent tiny">"admin"</span> })}</td>
                                    <td>{u.email.clone()}</td>
                                    <td>{u.client_name.clone().unwrap_or_else(|| u.club_name.clone())}</td>
                                    <td>{if active { view! { <span class="pill placed">"Actif"</span> } } else { view! { <span class="pill off">"Désactivé"</span> } }}</td>
                                    <td class="muted">{u.last_login.as_deref().map(api::date_label).unwrap_or_else(|| "jamais".into())}</td>
                                    <td class="actions">
                                        <button class="light small" on:click=toggle>{if active { "Désactiver" } else { "Réactiver" }}</button>
                                        <button class="light small" on:click=role>{if admin { "Retirer admin" } else { "Nommer admin" }}</button>
                                        <button class="ghost danger icon-only" title="Supprimer" on:click=remove><Icon name="trash" /></button>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

#[component]
fn Clubs(list: Vec<ClubInfo>) -> impl IntoView {
    view! {
        <section class="card">
            <div class="card-head"><Icon name="building" /><h2>"Clubs"</h2><span class="chip">{list.len()}</span></div>
            <div class="table-scroll">
                <table class="list">
                    <thead><tr><th>"Club"</th><th class="number">"Comptes"</th><th class="number">"Compétitions"</th><th>"Créé le"</th></tr></thead>
                    <tbody>
                        {list.into_iter().map(|c| view! {
                            <tr>
                                <td class="name">{c.name.clone()}</td>
                                <td class="number">{c.users}</td>
                                <td class="number">{c.contests}</td>
                                <td class="muted">{api::date_label(&c.created)}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

#[component]
fn Contests(list: Vec<Value>) -> impl IntoView {
    let s = |v: &Value, k: &str| v[k].as_str().unwrap_or_default().to_string();
    view! {
        <section class="card">
            <div class="card-head"><Icon name="trophy" /><h2>"Compétitions de tous les clubs"</h2><span class="chip">{list.len()}</span></div>
            <div class="table-scroll">
                <table class="list">
                    <thead><tr><th></th><th>"Compétition"</th><th>"Club"</th><th>"Arme · catégorie"</th><th>"Date"</th><th class="number">"Tireurs"</th><th>"Phase"</th></tr></thead>
                    <tbody>
                        {list.into_iter().map(|c| view! {
                            <tr>
                                <td><span class="swatch" style=format!("background:{}", s(&c, "color"))></span></td>
                                <td class="name">{s(&c, "name")}</td>
                                <td>{s(&c, "club")}</td>
                                <td>{format!("{} · {} · {}", s(&c, "weapon_label"), s(&c, "gender_label"), s(&c, "category"))}</td>
                                <td>{s(&c, "date")}</td>
                                <td class="number">{c["fencers"].as_u64().unwrap_or(0)}</td>
                                <td>{s(&c, "stage")}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </section>
    }
}
