//! « Mon compte » : profil, mot de passe et clés d'API du club (logiciel de
//! bureau, applications tierces).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, CreatedKey, KeyInfo, UserInfo};
use crate::icons::Icon;
use crate::{alert, confirm};

/// Copie un texte dans le presse-papiers.
pub fn copy(text: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().clipboard().write_text(text);
        alert("Copié dans le presse-papiers.");
    }
}

#[component]
pub fn AccountPage(user: UserInfo, club: String) -> impl IntoView {
    view! {
        <header class="page-head">
            <div>
                <h1>"Mon compte"</h1>
                <p class="muted">{format!("{} · {}", user.email, club)}</p>
            </div>
        </header>
        <div class="account-grid">
            <section class="card">
                <div class="card-head"><Icon name="user" /><h2>"Profil"</h2></div>
                <dl class="facts">
                    <dt>"Nom"</dt><dd>{user.name.clone()}</dd>
                    <dt>"E-mail"</dt><dd>{user.email.clone()}</dd>
                    <dt>"Club"</dt><dd>{club.clone()}</dd>
                    <dt>"Rôle"</dt><dd>{if user.admin { "Administrateur" } else { "Membre" }}</dd>
                    <dt>"Inscription"</dt><dd>{api::date_label(&user.created)}</dd>
                </dl>
            </section>
            <PasswordCard />
        </div>
        <KeysCard />
    }
}

#[component]
fn PasswordCard() -> impl IntoView {
    let (current, password, confirm_pw) = (RwSignal::new(String::new()), RwSignal::new(String::new()), RwSignal::new(String::new()));
    let error = RwSignal::new(None::<String>);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if password.get_untracked() != confirm_pw.get_untracked() {
            error.set(Some("Les deux mots de passe sont différents.".into()));
            return;
        }
        error.set(None);
        let body = serde_json::json!({ "current": current.get_untracked(), "password": password.get_untracked() });
        spawn_local(async move {
            match api::send::<_, serde_json::Value>("PUT", "/api/auth/password", &body).await {
                Ok(_) => {
                    for s in [current, password, confirm_pw] {
                        s.set(String::new());
                    }
                    alert("Mot de passe modifié.");
                }
                Err(e) => error.set(Some(e.message)),
            }
        });
    };
    let input = move |placeholder: &'static str, auto: &'static str, value: RwSignal<String>| {
        view! {
            <input type="password" placeholder=placeholder autocomplete=auto required=true prop:value=move || value.get() on:input=move |ev| value.set(event_target_value(&ev)) />
        }
    };
    view! {
        <section class="card">
            <div class="card-head"><Icon name="lock" /><h2>"Mot de passe"</h2></div>
            <form class="stack" on:submit=submit>
                {input("Mot de passe actuel", "current-password", current)}
                {input("Nouveau mot de passe (10 caractères au moins)", "new-password", password)}
                {input("Confirmer", "new-password", confirm_pw)}
                {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
                <div class="buttons"><button type="submit">"Changer le mot de passe"</button></div>
            </form>
        </section>
    }
}

#[component]
fn KeysCard() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let keys = LocalResource::new(move || {
        version.get();
        api::get::<Vec<KeyInfo>>("/api/account/keys")
    });
    let name = RwSignal::new(String::new());
    let created = RwSignal::new(None::<CreatedKey>);
    let create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let body = serde_json::json!({ "name": name.get_untracked() });
        spawn_local(async move {
            match api::send::<_, CreatedKey>("POST", "/api/account/keys", &body).await {
                Ok(k) => {
                    name.set(String::new());
                    created.set(Some(k));
                    version.update(|v| *v += 1);
                }
                Err(e) => alert(&e.message),
            }
        });
    };
    let revoke = move |k: KeyInfo| {
        if !confirm(&format!("Retirer la clé « {} » ? Les logiciels qui l'utilisent n'auront plus accès.", k.name)) {
            return;
        }
        spawn_local(async move {
            if let Err(e) = api::delete(&format!("/api/account/keys/{}", k.id)).await {
                alert(&e.message);
            }
            version.update(|v| *v += 1);
        });
    };
    view! {
        <section class="card">
            <div class="card-head">
                <Icon name="key" />
                <h2>"Clés d'API du club"</h2>
            </div>
            <p class="muted">
                "Une clé relie le logiciel de bureau ManageComp ou une application tierce à votre club : "
                "elle donne accès aux mêmes compétitions que votre compte. Donnez-lui un nom pour la reconnaître, "
                "et retirez-la quand elle ne sert plus."
            </p>
            {move || created.get().map(|k| {
                let key = k.key.clone();
                view! {
                    <div class="new-key">
                        <div>
                            <strong>{format!("Clé « {} » créée", k.name)}</strong>
                            <p class="muted small">"Copiez-la maintenant : elle ne sera plus affichée."</p>
                        </div>
                        <code>{k.key.clone()}</code>
                        <div class="buttons">
                            <button type="button" on:click=move |_| copy(&key)><Icon name="copy" />"Copier"</button>
                            <button type="button" class="light" on:click=move |_| created.set(None)>"J'ai noté la clé"</button>
                        </div>
                    </div>
                }
            })}
            <form class="row" on:submit=create>
                <input type="text" placeholder="Nom de la clé (ex. : ordinateur du club)" required=true maxlength="80" class="grow" prop:value=move || name.get() on:input=move |ev| name.set(event_target_value(&ev)) />
                <button type="submit"><Icon name="plus" />"Créer une clé"</button>
            </form>
            {move || match keys.get() {
                None => view! { <p class="muted">"Chargement…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="error">{e.message}</p> }.into_any(),
                Some(Ok(list)) if list.is_empty() => view! { <p class="muted">"Aucune clé pour l'instant."</p> }.into_any(),
                Some(Ok(list)) => view! {
                    <table class="list">
                        <thead><tr><th>"Nom"</th><th>"Créée le"</th><th></th></tr></thead>
                        <tbody>
                            {list.into_iter().map(|k| {
                                let label = api::date_label(&k.created);
                                let name = k.name.clone();
                                view! {
                                    <tr>
                                        <td class="name"><Icon name="key" />" "{name}</td>
                                        <td class="muted">{label}</td>
                                        <td class="actions"><button class="danger small" on:click=move |_| revoke(k.clone())>"Retirer"</button></td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any(),
            }}
        </section>
    }
}
