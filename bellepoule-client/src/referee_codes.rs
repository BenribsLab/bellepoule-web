//! Arbitres sur téléphone (Organisation) : QR codes personnels et PIN,
//! feuilles personnelles, téléphones reconnus et feuilles bloquées.
//!
//! Le QR code d'une feuille de poule ou de match ne donne accès à rien sans
//! le PIN d'un arbitre affecté à cette feuille. Après 20 PIN faux, la feuille
//! se bloque 5 minutes ; après 20 de plus, jusqu'à ce que l'organisation la
//! débloque (alerte ci-dessous).

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{json, Value};

use crate::qr::{personal_url, QrCode};
use crate::{alert, api, confirm};

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}
fn u(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}
fn arr(v: &Value, k: &str) -> Vec<Value> {
    v.get(k).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn set_print_mode(on: bool) {
    if let Some(body) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.body()) {
        let _ = if on { body.class_list().add_1("print-groups") } else { body.class_list().remove_1("print-groups") };
    }
}

fn full_name(r: &Value) -> String {
    format!("{} {}", s(r, "name"), s(r, "first_name")).trim().to_string()
}

/// Alertes de sécurité : feuilles bloquées (PIN), à débloquer.
#[component]
pub fn SecurityBanner() -> impl IntoView {
    let data = RwSignal::new(None::<Value>);
    let fetch = move || {
        spawn_local(async move {
            if let Ok(v) = api::get::<Value>("/api/smartpoule/security").await {
                data.set(Some(v));
            }
        });
    };
    fetch();
    let handle = set_interval_with_handle(fetch, std::time::Duration::from_secs(20)).ok();
    on_cleanup(move || {
        if let Some(h) = handle {
            h.clear();
        }
    });
    let unlock = move |token: String| {
        spawn_local(async move {
            match api::send::<_, Value>("POST", "/api/smartpoule/security", &json!({ "action": "unlock", "token": token })).await {
                Ok(v) => data.set(Some(v)),
                Err(e) => alert(&e.message),
            }
        });
    };
    view! {
        {move || {
            let d = data.get()?;
            let alerts = arr(&d, "alerts");
            let paused = arr(&d, "paused");
            (!alerts.is_empty() || !paused.is_empty()).then(|| view! {
                <div class="security-alert">
                    {alerts.into_iter().map(|a| {
                        let token = s(&a, "token");
                        view! {
                            <div class="row">
                                <strong>"⚠ Feuille bloquée"</strong>
                                <span>{format!("{} : trop de codes PIN faux (depuis {}).", s(&a, "name"), s(&a, "since"))}</span>
                                <span class="spacer"></span>
                                <button class="small" on:click=move |_| unlock(token.clone())>"Débloquer"</button>
                            </div>
                        }
                    }).collect_view()}
                    {paused.into_iter().map(|a| view! {
                        <div class="row muted">{format!("⏸ {} : 20 codes PIN faux, nouvel essai possible dans {} min.", s(&a, "name"), u(&a, "minutes"))}</div>
                    }).collect_view()}
                </div>
            })
        }}
    }
}

/// Bouton « Arbitres sur téléphone » et sa fenêtre.
#[component]
pub fn RefereeCodesButton() -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <button class="light small" title="QR codes personnels, codes PIN et téléphones des arbitres" on:click=move |_| open.set(true)>"📱 Arbitres sur téléphone"</button>
        <Show when=move || open.get()>
            <RefereeCodes on_close=move || open.set(false) />
        </Show>
    }
}

/// Ce qu'on imprime : feuilles personnelles (toutes, une, administrateur)
/// ou liste des PIN.
#[derive(Clone, PartialEq)]
enum Printing {
    Personal(Option<u64>),
    Admin,
    Pins,
}

#[component]
fn RefereeCodes(on_close: impl Fn() + Copy + 'static) -> impl IntoView {
    let codes = RwSignal::new(None::<Value>);
    let security = RwSignal::new(None::<Value>);
    let printing = RwSignal::new(None::<Printing>);
    let error = RwSignal::new(None::<String>);
    let load = move || {
        spawn_local(async move {
            match api::get::<Value>("/api/organisation/referee-codes").await {
                Ok(v) => codes.set(Some(v)),
                Err(e) => error.set(Some(e.message)),
            }
            if let Ok(v) = api::get::<Value>("/api/smartpoule/security").await {
                security.set(Some(v));
            }
        });
    };
    load();
    let renew = move |r: u64, name: String| {
        if !confirm(&format!("Nouveau code pour {name} ?\n\nSon QR code personnel et son PIN actuels ne marcheront plus, et ses téléphones devront se reconnecter. Pensez à lui remettre sa nouvelle feuille.")) {
            return;
        }
        spawn_local(async move {
            match api::send::<_, Value>("POST", "/api/organisation/referee-codes", &json!({ "reference": r })).await {
                Ok(v) => codes.set(Some(v)),
                Err(e) => alert(&e.message),
            }
            if let Ok(v) = api::get::<Value>("/api/smartpoule/security").await {
                security.set(Some(v));
            }
        });
    };
    let logout = move |r: u64| {
        spawn_local(async move {
            match api::send::<_, Value>("POST", "/api/smartpoule/security", &json!({ "action": "logout", "referee": r })).await {
                Ok(v) => security.set(Some(v)),
                Err(e) => alert(&e.message),
            }
        });
    };
    let phones = move |r: u64| security.with(|s| s.as_ref().map(|s| arr(s, "phones").iter().find(|p| u(p, "referee") == r).map(|p| u(p, "count")).unwrap_or(0)).unwrap_or(0));
    let print = move |what: Printing| {
        set_print_mode(true);
        printing.set(Some(what));
    };
    view! {
        <div class="overlay" on:click=move |_| on_close()>
            <div class="dialog referee-codes" on:click=|ev| ev.stop_propagation()>
                <h2>"Arbitres sur téléphone"</h2>
                <p class="muted small">
                    "Chaque arbitre a un QR code personnel (sans mot de passe, à lui remettre en main propre) et un code PIN. "
                    "Les feuilles de poule et de match portent aussi un QR code : il faut le PIN d'un arbitre de la feuille, à entrer une seule fois pour la compétition."
                </p>
                {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
                {move || codes.get().map(|d| {
                    let referees = arr(&d, "referees");
                    if referees.is_empty() {
                        return view! { <p class="muted">"Aucun arbitre : ajoutez-les dans la colonne Arbitres de l'organisation."</p> }.into_any();
                    }
                    view! {
                        <div class="row">
                            <button on:click=move |_| print(Printing::Personal(None))>"Imprimer les feuilles personnelles"</button>
                            <button class="light" on:click=move |_| print(Printing::Pins)>"Imprimer la liste des PIN"</button>
                            <button class="light" title="Accès à tous les matchs, sans PIN : à garder pour la table d'arbitrage" on:click=move |_| print(Printing::Admin)>"Feuille administrateur"</button>
                        </div>
                        <div class="table-scroll">
                            <table class="list">
                                <thead><tr><th>"Arbitre"</th><th>"Club"</th><th>"PIN"</th><th>"Téléphones"</th><th></th></tr></thead>
                                <tbody>
                                    {referees.into_iter().map(|r| {
                                        let reference = u(&r, "reference");
                                        let name = full_name(&r);
                                        let renew_name = name.clone();
                                        view! {
                                            <tr>
                                                <td>{name}</td>
                                                <td class="muted">{s(&r, "club")}</td>
                                                <td><code class="pin">{s(&r, "pin")}</code></td>
                                                <td>{move || {
                                                    let n = phones(reference);
                                                    if n == 0 { view! { <span class="muted">"—"</span> }.into_any() } else {
                                                        view! { <span>{format!("{n} ")}<button class="ghost small" title="Ces téléphones devront entrer à nouveau le PIN" on:click=move |_| logout(reference)>"Déconnecter"</button></span> }.into_any()
                                                    }
                                                }}</td>
                                                <td class="actions">
                                                    <button class="ghost small" on:click=move |_| print(Printing::Personal(Some(reference)))>"Feuille"</button>
                                                    <button class="ghost small" title="Nouveau QR code personnel et nouveau PIN (si un PIN a circulé)" on:click=move |_| renew(reference, renew_name.clone())>"Nouveau code"</button>
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                })}
                <div class="buttons">
                    <button type="button" class="light" on:click=move |_| on_close()>"Fermer"</button>
                </div>
            </div>
        </div>
        {move || printing.get().and_then(|what| codes.get().map(|d| view! {
            <PrintCodes data=d what=what on_close=Callback::new(move |_| { set_print_mode(false); printing.set(None); }) />
        }))}
    }
}

#[component]
fn PrintCodes(data: Value, what: Printing, on_close: Callback<()>) -> impl IntoView {
    let (data, what) = (StoredValue::new(data), StoredValue::new(what));
    view! {
        <leptos::portal::Portal>
            <div class="print-sheets">
                <div class="print-bar">
                    <strong>"Arbitres sur téléphone"</strong>
                    <span class="spacer"></span>
                    <button on:click=move |_| { if let Some(w) = web_sys::window() { let _ = w.print(); } }>"Imprimer"</button>
                    <button class="light" on:click=move |_| on_close.run(())>"Fermer"</button>
                </div>
                {move || print_pages(&data.get_value(), &what.get_value())}
            </div>
        </leptos::portal::Portal>
    }
}

/// Pages imprimées : feuilles personnelles, feuille administrateur ou liste des PIN.
fn print_pages(data: &Value, what: &Printing) -> AnyView {
    let data = data.clone();
    let club = s(&data, "club");
    let club_name = s(&data, "club_name");
    let referees: Vec<Value> = match &what {
        Printing::Personal(Some(r)) => arr(&data, "referees").into_iter().filter(|x| u(x, "reference") == *r).collect(),
        _ => arr(&data, "referees"),
    };
    match what {
        Printing::Pins => view! {
            <section class="print-page">
                <h2>{format!("Codes PIN des arbitres — {club_name}")}</h2>
                <p class="small">"Confidentiel : pour l'organisation. Un PIN confirme l'arbitre qui scanne le QR code d'une feuille de poule ou de match."</p>
                <table class="list pin-list">
                    <thead><tr><th>"Arbitre"</th><th>"Club"</th><th>"PIN"</th></tr></thead>
                    <tbody>
                        {referees.iter().map(|r| view! { <tr><td>{full_name(r)}</td><td>{s(r, "club")}</td><td><strong>{s(r, "pin")}</strong></td></tr> }).collect_view()}
                    </tbody>
                </table>
            </section>
        }.into_any(),
        Printing::Admin => {
            let url = personal_url(&club, 0, &s(&data, "admin_key"));
            view! {
                <section class="print-page referee-page">
                    <img class="rp-logo" src="/header.png" alt="ManageComp" />
                    <h1>"Administrateur"</h1>
                    <p class="rp-club">{club_name.clone()}</p>
                    <div class="rp-qr"><QrCode text=url /></div>
                    <p class="rp-help">"Ce QR code donne accès à tous les matchs, sans code PIN : gardez-le à la table d'arbitrage, ne le laissez pas traîner."</p>
                </section>
            }.into_any()
        }
        Printing::Personal(_) => referees.iter().map(|r| {
            let url = personal_url(&club, u(r, "reference"), &s(r, "key"));
            view! {
                <section class="print-page referee-page">
                    <img class="rp-logo" src="/header.png" alt="ManageComp" />
                    <h1>{full_name(r)}</h1>
                    <p class="rp-club">{[s(r, "club"), club_name.clone()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ")}</p>
                    <div class="rp-qr"><QrCode text=url /></div>
                    <p class="rp-help">"Scannez ce QR code avec l'appareil photo de votre téléphone : vos poules et vos matchs s'affichent, sans mot de passe. Cette feuille est personnelle, gardez-la pour vous."</p>
                    <div class="rp-pin">"Votre code PIN : "<strong>{s(r, "pin")}</strong></div>
                    <p class="rp-help">"Les feuilles de poule et de match portent aussi un QR code : scannez-le, puis entrez ce code PIN (une seule fois pour la compétition)."</p>
                </section>
            }
        }).collect_view().into_any(),
    }
}
