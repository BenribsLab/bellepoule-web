//! ManageComp — client web (navigateur).
//!
//! Navigation par l'adresse : `#/` (compétitions), `#/contest/<id>`.

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
        <Show when=move || logged.get() fallback=move || view! { <Login logged=logged /> }>
            <div class="shell">
                <aside class="sidebar">
                    <a class="brand" href="#/">
                        <icons::Logo />
                        <span class="brand-text"><strong>"ManageComp"</strong><small>"Compétitions d'escrime"</small></span>
                    </a>
                    <nav class="side-nav">
                        <span class="side-title">"Club"</span>
                        <a href="#/" class:active=move || matches!(route.get(), Route::Home | Route::Contest(_))>
                            <icons::Icon name="trophy" />"Compétitions"
                        </a>
                        <a href="#/organisation" class:active=move || route.get() == Route::Organisation>
                            <icons::Icon name="grid" />"Organisation"
                        </a>
                    </nav>
                    <span class="spacer"></span>
                    <div class="side-user">
                        <span class="avatar">{move || me.get().and_then(|m| m.name.chars().next()).map(|c| c.to_uppercase().to_string()).unwrap_or_default()}</span>
                        <span class="side-user-name">{move || me.get().map(|m| m.name).unwrap_or_default()}</span>
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
        <div class="login-screen">
            <div class="login-glow"></div>
            <section class="login-card">
                <icons::Logo />
                <h1>"ManageComp"</h1>
                <p class="muted">"Saisissez la clé d'API de votre club, fournie par l'administrateur du serveur."</p>
                <form on:submit=submit>
                    <label class="input-icon">
                        <icons::Icon name="key" />
                        <input
                            type="password"
                            placeholder="Clé d'API"
                            autocomplete="off"
                            prop:value=move || key.get()
                            on:input=move |ev| key.set(event_target_value(&ev))
                        />
                    </label>
                    <button type="submit" class="big">"Se connecter"</button>
                </form>
                {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
            </section>
            <Footer />
        </div>
    }
}
