//! Tableau d'affichage d'une piste (`/live/?c=<club>&p=<piste>`), sans
//! connexion : le score et le chrono envoyés par le téléphone de l'arbitre
//! (SmartPoule), comme sur l'écran LivePoule. À ouvrir sur une télé ou un
//! ordinateur au bord de la piste.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;

use crate::api;

fn s(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for k in path {
        match cur.get(k) {
            Some(x) => cur = x,
            None => return String::new(),
        }
    }
    match cur {
        Value::String(x) => x.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// Paramètre de l'adresse (`?c=…&p=…`).
fn param(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    search.trim_start_matches('?').split('&').find_map(|kv| kv.strip_prefix(&format!("{name}=")).map(str::to_string))
}

/// La page est-elle le tableau d'affichage ?
pub fn is_live_page() -> bool {
    web_sys::window().and_then(|w| w.location().pathname().ok()).is_some_and(|p| p.trim_end_matches('/').ends_with("/live"))
}

#[component]
pub fn LivePage() -> impl IntoView {
    let club = param("c").unwrap_or_default();
    let piste = RwSignal::new(param("p").and_then(|p| p.parse::<u32>().ok()).unwrap_or(1));
    let state = RwSignal::new(None::<Value>);
    // Heure de réception de l'état, pour faire décompter le chrono entre deux relevés.
    let received = RwSignal::new(0.0_f64);
    let now = RwSignal::new(js_sys::Date::now());
    let club = StoredValue::new(club);
    let fetch = move || {
        let club = club.get_value();
        let p = piste.get_untracked();
        spawn_local(async move {
            if let Ok(v) = api::get::<Value>(&format!("/api/live?c={club}&p={p}")).await {
                received.set(js_sys::Date::now());
                state.set(Some(v));
            }
        });
    };
    fetch();
    let poll = set_interval_with_handle(fetch, std::time::Duration::from_secs(1)).ok();
    let tick = set_interval_with_handle(move || now.set(js_sys::Date::now()), std::time::Duration::from_millis(200)).ok();
    on_cleanup(move || {
        if let Some(h) = poll {
            h.clear();
        }
        if let Some(h) = tick {
            h.clear();
        }
    });
    let change = move |step: i32| {
        piste.update(|p| *p = (*p as i32 + step).max(1) as u32);
        state.set(None);
        fetch();
    };
    let clock = move || {
        let v = state.get()?;
        let mut ms = v.get("remaining_ms").and_then(Value::as_i64).unwrap_or(0);
        if v.get("running").and_then(Value::as_bool).unwrap_or(false) {
            ms -= (now.get() - received.get()) as i64;
        }
        let seconds = (ms.max(0) + 999) / 1000;
        Some(format!("{:02}:{:02}", seconds / 60, seconds % 60))
    };
    view! {
        <div class="live-board">
            {move || {
                let v = state.get().unwrap_or(Value::Null);
                let active = v.get("active").and_then(Value::as_bool).unwrap_or(false);
                let color = s(&v, &["color"]);
                let title = [s(&v, &["competition"]), s(&v, &["bout"])].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
                view! {
                    <div class="live-title" style=if color.is_empty() { String::new() } else { format!("background:{color}") }>
                        {if active { title } else { s(&v, &["club"]) }}
                    </div>
                    <div class="live-scores">
                        <div class="live-side red">
                            <div class="live-name">{s(&v, &["red", "name"])}</div>
                            <div class="live-score">{s(&v, &["red", "score"])}</div>
                        </div>
                        <div class="live-side green">
                            <div class="live-name">{s(&v, &["green", "name"])}</div>
                            <div class="live-score">{s(&v, &["green", "score"])}</div>
                        </div>
                    </div>
                    {(!active).then(|| view! { <div class="live-waiting">"En attente d'un match sur cette piste…"</div> })}
                }
            }}
            <div class="live-clock">{move || clock().unwrap_or_else(|| "--:--".into())}</div>
            <div class="live-piste">
                <button on:click=move |_| change(-1)>"−"</button>
                <span>{move || format!("Piste {:02}", piste.get())}</span>
                <button on:click=move |_| change(1)>"+"</button>
            </div>
        </div>
    }
}
