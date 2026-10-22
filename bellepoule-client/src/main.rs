//! ManageComp — client web (navigateur).
//!
//! Navigation par l'adresse : `#/` (compétitions), `#/contest/<id>`.

mod api;
mod checkin;
mod contest;
mod home;
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
}

fn route_from_hash() -> Route {
    let hash = web_sys::window().and_then(|w| w.location().hash().ok()).unwrap_or_default();
    match hash.trim_start_matches('#').trim_start_matches('/').split_once('/') {
        Some(("contest", id)) if !id.is_empty() => Route::Contest(id.to_string()),
        _ if hash.trim_start_matches('#').trim_start_matches('/') == "organisation" => Route::Organisation,
        _ => Route::Home,
    }
}

/// Change d'écran (et d'adresse).
pub fn navigate(route: &Route) {
    let hash = match route {
        Route::Home => "#/".to_string(),
        Route::Contest(id) => format!("#/contest/{id}"),
        Route::Organisation => "#/organisation".to_string(),
    };
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_hash(&hash);
    }
}

/// Message d'erreur dans une fenêtre du navigateur.
pub fn alert(message: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.alert_with_message(message);
    }
}

pub fn confirm(message: &str) -> bool {
    web_sys::window().and_then(|w| w.confirm_with_message(message).ok()).unwrap_or(false)
}

#[component]
fn App() -> impl IntoView {
    let logged = RwSignal::new(api::api_key().is_some());
    let me = RwSignal::new(None::<api::Me>);
    let route = RwSignal::new(route_from_hash());

    // Suit les changements d'adresse (liens, bouton « précédent » du navigateur).
    {
        let closure = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || route.set(route_from_hash()));
        if let Some(w) = web_sys::window() {
            w.set_onhashchange(Some(wasm_bindgen::JsCast::unchecked_ref(closure.as_ref())));
        }
        closure.forget();
    }

    // Vérifie la clé et récupère le nom du client.
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
            });
        }
    });

    let logout = move |_| {
        api::set_api_key(None);
        me.set(None);
        logged.set(false);
    };

    view! {
        <header class="topbar">
            <a class="brand" href="#/">"ManageComp"</a>
            <span class="subtitle">"Gestion de compétitions d'escrime"</span>
            <Show when=move || logged.get()>
                <nav class="topnav">
                    <a href="#/" class:active=move || route.get() == Route::Home>"Compétitions"</a>
                    <a href="#/organisation" class:active=move || route.get() == Route::Organisation>"Organisation"</a>
                </nav>
            </Show>
            <span class="spacer"></span>
            {move || me.get().map(|m| view! { <span class="who">{m.name}</span> })}
            <Show when=move || logged.get()>
                <button class="light" on:click=logout>"Se déconnecter"</button>
            </Show>
        </header>
        <main>
            <Show when=move || logged.get() fallback=move || view! { <Login logged=logged /> }>
                {move || match route.get() {
                    Route::Home => view! { <home::Home /> }.into_any(),
                    Route::Contest(id) => view! { <contest::ContestPage id=id /> }.into_any(),
                    Route::Organisation => view! { <organisation::Organisation /> }.into_any(),
                }}
            </Show>
        </main>
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

#[component]
fn Login(logged: RwSignal<bool>) -> impl IntoView {
    let key = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        api::set_api_key(Some(key.get().trim()));
        spawn_local(async move {
            match api::get::<api::Me>("/api/me").await {
                Ok(_) => logged.set(true),
                Err(e) => {
                    api::set_api_key(None);
                    error.set(Some(if e.unauthorized { "Clé d'API inconnue.".into() } else { e.message }));
                }
            }
        });
    };
    view! {
        <section class="card login">
            <h2>"Connexion"</h2>
            <p class="muted">"Saisissez la clé d'API de votre club, fournie par l'administrateur du serveur."</p>
            <form on:submit=submit>
                <input
                    type="password"
                    placeholder="Clé d'API"
                    autocomplete="off"
                    prop:value=move || key.get()
                    on:input=move |ev| key.set(event_target_value(&ev))
                />
                <button type="submit">"Se connecter"</button>
            </form>
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        </section>
    }
}
