//! ManageComp — client web (navigateur).
//!
//! Navigation par l'adresse : `#/` (compétitions), `#/contest/<id>`.

mod account;
mod admin;
mod api;
mod arbitre;
mod checkin;
mod contest;
mod home;
mod icons;
mod live;
mod organisation;
mod properties;
mod stages;

use leptos::prelude::*;
use leptos::task::spawn_local;

fn main() {
    console_error_panic_hook::set_once();
    // `/live/?c=…&p=…` : tableau d'affichage d'une piste, sans connexion.
    if live::is_live_page() {
        leptos::mount::mount_to_body(live::LivePage);
    } else if arbitre::is_arbitre_page() {
        // `arbitre.<domaine>` : saisie des matchs par les arbitres, sans connexion club.
        leptos::mount::mount_to_body(arbitre::ArbitrePage);
    } else {
        leptos::mount::mount_to_body(App);
    }
}

/// Écran affiché.
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Home,
    Contest(String),
    Organisation,
    Account,
    Admin,
}

fn route_from_hash() -> Route {
    let hash = web_sys::window().and_then(|w| w.location().hash().ok()).unwrap_or_default();
    match hash.trim_start_matches('#').trim_start_matches('/').split_once('/') {
        Some(("contest", id)) if !id.is_empty() => Route::Contest(id.to_string()),
        _ => match hash.trim_start_matches('#').trim_start_matches('/') {
            "organisation" => Route::Organisation,
            "compte" => Route::Account,
            "admin" => Route::Admin,
            _ => Route::Home,
        },
    }
}

/// Change d'écran (et d'adresse).
pub fn navigate(route: &Route) {
    let hash = match route {
        Route::Home => "#/".to_string(),
        Route::Contest(id) => format!("#/contest/{id}"),
        Route::Organisation => "#/organisation".to_string(),
        Route::Account => "#/compte".to_string(),
        Route::Admin => "#/admin".to_string(),
    };
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_hash(&hash);
    }
}

/// Message passager (en bas de l'écran), à la place d'une fenêtre du navigateur.
pub fn alert(message: &str) {
    use wasm_bindgen::JsCast;
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Some(body) = document.body() else { return };
    let stack = match document.get_element_by_id("toasts") {
        Some(stack) => stack,
        None => {
            let Ok(stack) = document.create_element("div") else { return };
            stack.set_id("toasts");
            let _ = body.append_child(&stack);
            stack
        }
    };
    let Ok(toast) = document.create_element("div") else { return };
    toast.set_class_name("toast");
    toast.set_text_content(Some(message));
    let _ = stack.append_child(&toast);
    let leave = toast.clone();
    let close = wasm_bindgen::closure::Closure::once_into_js(move || {
        leave.set_class_name("toast leaving");
        let gone = leave.clone();
        let remove = wasm_bindgen::closure::Closure::once_into_js(move || gone.remove());
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(remove.unchecked_ref(), 300);
        }
    });
    let delay = (3500 + message.len() as i32 * 40).min(9000);
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(close.unchecked_ref(), delay);
}

pub fn confirm(message: &str) -> bool {
    web_sys::window().and_then(|w| w.confirm_with_message(message).ok()).unwrap_or(false)
}

#[component]
fn App() -> impl IntoView {
    let logged = RwSignal::new(api::api_key().is_some());
    let me = RwSignal::new(None::<api::Me>);
    // Compte connecté ; absent avec une simple clé d'API.
    let user = RwSignal::new(None::<api::UserInfo>);
    let route = RwSignal::new(route_from_hash());

    // Suit les changements d'adresse (liens, bouton « précédent » du navigateur).
    {
        let closure = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || route.set(route_from_hash()));
        if let Some(w) = web_sys::window() {
            w.set_onhashchange(Some(wasm_bindgen::JsCast::unchecked_ref(closure.as_ref())));
        }
        closure.forget();
    }

    // Vérifie la clé (ou la session) et récupère le club et le compte.
    Effect::new(move |_| {
        if logged.get() {
            spawn_local(async move {
                match api::get::<api::Me>("/api/me").await {
                    Ok(m) => me.set(Some(m)),
                    Err(e) if e.unauthorized => {
                        api::set_api_key(None);
                        logged.set(false);
                    }
                    Err(_) => {}
                }
                user.set(api::get::<api::UserInfo>("/api/auth/me").await.ok());
            });
        }
    });

    let logout = move |_| {
        spawn_local(async move {
            if user.get_untracked().is_some() {
                let _ = api::send::<_, serde_json::Value>("POST", "/api/auth/logout", &serde_json::json!({})).await;
            }
            api::set_api_key(None);
            me.set(None);
            user.set(None);
            logged.set(false);
            navigate(&Route::Home);
        });
    };
    let is_admin = move || user.get().is_some_and(|u| u.admin);

    view! {
        <Show when=move || logged.get() fallback=move || view! { <Login logged=logged /> }>
            <div class="shell">
                <aside class="sidebar">
                    <a class="brand" href="#/" title="ManageComp">
                        <icons::Header />
                        <icons::Logo />
                    </a>
                    <nav class="side-nav">
                        <span class="side-title">"Club"</span>
                        <a href="#/" class:active=move || matches!(route.get(), Route::Home | Route::Contest(_))>
                            <icons::Icon name="trophy" />"Compétitions"
                        </a>
                        <a href="#/organisation" class:active=move || route.get() == Route::Organisation>
                            <icons::Icon name="grid" />"Organisation"
                        </a>
                        <Show when=move || user.get().is_some()>
                            <a href="#/compte" class:active=move || route.get() == Route::Account>
                                <icons::Icon name="key" />"Mon compte"
                            </a>
                        </Show>
                        <Show when=is_admin>
                            <span class="side-title">"Serveur"</span>
                            <a href="#/admin" class:active=move || route.get() == Route::Admin>
                                <icons::Icon name="shield" />"Administration"
                            </a>
                        </Show>
                    </nav>
                    <span class="spacer"></span>
                    <div class="side-user">
                        <span class="avatar">
                            {move || {
                                let name = user.get().map(|u| u.name).or_else(|| me.get().map(|m| m.name)).unwrap_or_default();
                                name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
                            }}
                        </span>
                        <span class="side-user-name">
                            <strong>{move || user.get().map(|u| u.name).or_else(|| me.get().map(|m| m.name)).unwrap_or_default()}</strong>
                            <small>{move || me.get().map(|m| m.name).filter(|_| user.get().is_some()).unwrap_or_else(|| "Clé d'API".into())}</small>
                        </span>
                        <button class="ghost icon-only" title="Se déconnecter" on:click=logout><icons::Icon name="logout" /></button>
                    </div>
                </aside>
                <div class="content">
                    <main>
                        {move || {
                            let page = match route.get() {
                                Route::Home => view! { <home::Home /> }.into_any(),
                                Route::Contest(id) => view! { <contest::ContestPage id=id /> }.into_any(),
                                Route::Organisation => view! { <organisation::Organisation /> }.into_any(),
                                Route::Account => match user.get() {
                                    Some(u) => view! { <account::AccountPage user=u club=me.get().map(|m| m.name).unwrap_or_default() /> }.into_any(),
                                    None => view! { <p class="muted">"Écran réservé aux comptes (connexion par e-mail)."</p> }.into_any(),
                                },
                                Route::Admin => {
                                    if is_admin() {
                                        view! { <admin::AdminPage /> }.into_any()
                                    } else {
                                        view! { <p class="muted">"Écran réservé aux administrateurs."</p> }.into_any()
                                    }
                                }
                            };
                            view! { <div class="page">{page}</div> }
                        }}
                    </main>
                    <Footer />
                </div>
            </div>
        </Show>
    }
}

#[component]
fn Footer() -> impl IntoView {
    view! {
        <footer class="app-footer">
            "ManageComp, logiciel libre (GNU GPL v3 ou ultérieure) dérivé de "
            <a href="http://betton.escrime.free.fr" target="_blank" rel="noopener">"BellePoule"</a>
            " de Yannick Le Roux · "
            <a href="https://managecomp.org/docs/licence/" target="_blank" rel="noopener">"Licence et crédits"</a>
            " · "
            <a href="https://github.com/BenribsLab" target="_blank" rel="noopener">"Code source"</a>
        </footer>
    }
}

/// Façon de se connecter.
#[derive(Clone, Copy, PartialEq)]
enum LoginMode {
    Password,
    Register,
    Key,
}

#[component]
fn Login(logged: RwSignal<bool>) -> impl IntoView {
    let mode = RwSignal::new(LoginMode::Password);
    let error = RwSignal::new(None::<String>);
    let done = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let (email, password, confirm, name, club, key) = (
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
    );
    let switch = move |m: LoginMode| {
        mode.set(m);
        error.set(None);
        done.set(None);
    };

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        busy.set(true);
        spawn_local(async move {
            match mode.get_untracked() {
                LoginMode::Password => {
                    let body = serde_json::json!({ "email": email.get_untracked(), "password": password.get_untracked() });
                    match api::send::<_, api::LoggedIn>("POST", "/api/auth/login", &body).await {
                        Ok(r) => {
                            api::set_api_key(Some(&r.token));
                            logged.set(true);
                        }
                        Err(e) => error.set(Some(e.message)),
                    }
                }
                LoginMode::Register => {
                    if password.get_untracked() != confirm.get_untracked() {
                        error.set(Some("Les deux mots de passe sont différents.".into()));
                    } else {
                        let body = serde_json::json!({
                            "email": email.get_untracked(),
                            "password": password.get_untracked(),
                            "name": name.get_untracked(),
                            "club": club.get_untracked(),
                        });
                        match api::send::<_, serde_json::Value>("POST", "/api/auth/register", &body).await {
                            Ok(r) if r["status"] == "active" => {
                                mode.set(LoginMode::Password);
                                done.set(Some("Compte administrateur créé : vous pouvez vous connecter.".into()));
                            }
                            Ok(_) => {
                                mode.set(LoginMode::Password);
                                done.set(Some("Demande envoyée. Un administrateur doit valider votre compte avant la première connexion.".into()));
                            }
                            Err(e) => error.set(Some(e.message)),
                        }
                    }
                }
                LoginMode::Key => {
                    api::set_api_key(Some(key.get_untracked().trim()));
                    match api::get::<api::Me>("/api/me").await {
                        Ok(_) => logged.set(true),
                        Err(e) => {
                            api::set_api_key(None);
                            error.set(Some(if e.unauthorized { "Clé d'API inconnue.".into() } else { e.message }));
                        }
                    }
                }
            }
            busy.set(false);
        });
    };

    let field = move |icon: &'static str, kind: &'static str, placeholder: &'static str, auto: &'static str, value: RwSignal<String>| {
        view! {
            <label class="input-icon">
                <icons::Icon name=icon />
                <input
                    type=kind
                    placeholder=placeholder
                    autocomplete=auto
                    required=true
                    prop:value=move || value.get()
                    on:input=move |ev| value.set(event_target_value(&ev))
                />
            </label>
        }
    };

    view! {
        <div class="login-screen">
            <div class="login-glow"></div>
            <section class="login-card">
                <icons::Header />
                <p class="muted">"Gestion de compétitions d'escrime en ligne"</p>
                <div class="tabs login-tabs">
                    <button type="button" class="tab" class:active=move || mode.get() == LoginMode::Password on:click=move |_| switch(LoginMode::Password)>"Connexion"</button>
                    <button type="button" class="tab" class:active=move || mode.get() == LoginMode::Register on:click=move |_| switch(LoginMode::Register)>"Créer un compte"</button>
                    <button type="button" class="tab" class:active=move || mode.get() == LoginMode::Key on:click=move |_| switch(LoginMode::Key)>"Clé d'API"</button>
                </div>
                {move || done.get().map(|m| view! { <p class="banner ok"><icons::Icon name="check" />{m}</p> })}
                <form on:submit=submit>
                    {move || match mode.get() {
                        LoginMode::Password => view! {
                            {field("mail", "email", "Adresse e-mail", "username", email)}
                            {field("lock", "password", "Mot de passe", "current-password", password)}
                        }.into_any(),
                        LoginMode::Register => view! {
                            {field("user", "text", "Prénom et nom", "name", name)}
                            {field("trophy", "text", "Club ou structure", "organization", club)}
                            {field("mail", "email", "Adresse e-mail", "email", email)}
                            {field("lock", "password", "Mot de passe (10 caractères au moins)", "new-password", password)}
                            {field("lock", "password", "Confirmer le mot de passe", "new-password", confirm)}
                            <p class="muted small">"Votre demande sera validée par un administrateur."</p>
                        }.into_any(),
                        LoginMode::Key => view! {
                            {field("key", "password", "Clé d'API du club", "off", key)}
                            <p class="muted small">"Pour les clubs équipés d'une clé fournie par l'administrateur du serveur."</p>
                        }.into_any(),
                    }}
                    <button type="submit" class="big" disabled=move || busy.get()>
                        {move || match mode.get() {
                            LoginMode::Password => "Se connecter",
                            LoginMode::Register => "Envoyer la demande",
                            LoginMode::Key => "Se connecter avec la clé",
                        }}
                    </button>
                </form>
                {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
            </section>
            <Footer />
        </div>
    }
}
