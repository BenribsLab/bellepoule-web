//! Organisation : pistes, arbitres et planning (le Marshaller du logiciel).
//!
//! La logique tourne dans l'API (`/api/organisation`) ; cet écran affiche
//! la salle à l'heure du curseur, la frise, les lots de matchs de chaque
//! compétition en cours et les arbitres de chaque arme. Les matchs et les
//! arbitres se déposent sur les pistes (glisser-déposer, ou clic sur le
//! match puis sur la piste).

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{json, Value};

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
fn f(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}
fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}
fn arr(v: &Value, k: &str) -> Vec<Value> {
    v.get(k).and_then(Value::as_array).cloned().unwrap_or_default()
}
fn strings(v: &Value, k: &str) -> Vec<String> {
    arr(v, k).iter().filter_map(|x| x.as_str().map(str::to_string)).collect()
}

/// Échelle de la salle : pixels par unité (comme le zoom du logiciel).
const ZOOM: f64 = 1.1;

/// État partagé de l'écran.
#[derive(Clone, Copy)]
struct Org {
    data: RwSignal<Option<Value>>,
    selected_pistes: RwSignal<Vec<u64>>,
    /// Match « en main » : le prochain clic sur une piste l'y dépose.
    held_job: RwSignal<Option<u64>>,
    held_referee: RwSignal<Option<u64>>,
    job_detail: RwSignal<Option<u64>>,
    weapon: RwSignal<String>,
    competition: RwSignal<Option<u64>>,
    batch: RwSignal<Option<u64>>,
    selected_jobs: RwSignal<Vec<u64>>,
    selected_referees: RwSignal<Vec<u64>>,
    editing: RwSignal<Option<Value>>,
    assigning: RwSignal<bool>,
}

impl Org {
    fn act(&self, body: Value) {
        let data = self.data;
        spawn_local(async move {
            match api::send::<_, Value>("POST", "/api/organisation", &body).await {
                Ok(v) => data.set(Some(v)),
                Err(e) => alert(&e.message),
            }
        });
    }

    fn reload(&self) {
        let data = self.data;
        spawn_local(async move {
            match api::get::<Value>("/api/organisation").await {
                Ok(v) => data.set(Some(v)),
                Err(e) => alert(&e.message),
            }
        });
    }

    /// Dépôt d'un objet glissé (« job:12 », « referee:345 ») sur une piste.
    fn drop_on(&self, payload: &str, piste: u64) {
        match payload.split_once(':') {
            Some(("job", k)) => self.act(json!({ "action": "drop_job", "job": k.parse::<u64>().unwrap_or(0), "piste": piste })),
            Some(("referee", r)) => self.act(json!({ "action": "drop_referee", "referee": r.parse::<u64>().unwrap_or(0), "piste": piste })),
            _ => {}
        }
    }
}

#[component]
pub fn Organisation() -> impl IntoView {
    let org = Org {
        data: RwSignal::new(None),
        selected_pistes: RwSignal::new(Vec::new()),
        held_job: RwSignal::new(None),
        held_referee: RwSignal::new(None),
        job_detail: RwSignal::new(None),
        weapon: RwSignal::new(String::new()),
        competition: RwSignal::new(None),
        batch: RwSignal::new(None),
        selected_jobs: RwSignal::new(Vec::new()),
        selected_referees: RwSignal::new(Vec::new()),
        editing: RwSignal::new(None),
        assigning: RwSignal::new(false),
    };
    org.reload();
    // Avancement des matchs et horloge
    let handle = set_interval_with_handle(move || org.reload(), std::time::Duration::from_secs(30)).ok();
    on_cleanup(move || {
        if let Some(h) = handle {
            h.clear();
        }
    });

    view! {
        <section class="card organisation">
            {move || match org.data.get() {
                None => view! { <p class="muted">"Chargement…"</p> }.into_any(),
                Some(d) => {
                    let referees_on = b(&d, "referees_enabled");
                    let pistes_on = b(&d, "pistes_enabled");
                    view! {
                        <Header org=org d=d.clone() />
                        <Timeline org=org d=d.clone() />
                        <Overlaps org=org d=d.clone() />
                        <div class="org-columns" class:no-referees=!referees_on class:no-pistes=!pistes_on>
                            {referees_on.then(|| view! { <Referees org=org d=d.clone() /> })}
                            {pistes_on.then(|| view! { <Hall org=org d=d.clone() /> })}
                            <Batches org=org d=d.clone() />
                        </div>
                        <JobDetail org=org d=d.clone() />
                        <RefereeForm org=org />
                        <AssignDialog org=org d=d.clone() />
                    }
                        .into_any()
                }
            }}
        </section>
    }
}

// ------------------------------------------------------------------ en-tête

#[component]
fn Header(org: Org, d: Value) -> impl IntoView {
    let referees_on = b(&d, "referees_enabled");
    let pistes_on = b(&d, "pistes_enabled");
    let warnings = d["warnings"].clone();
    let colors = d["colors"].clone();
    view! {
        <div class="card-head org-head">
            <h2>"Organisation"</h2>
            <label class="check">
                <input type="checkbox" prop:checked=referees_on on:change=move |ev| org.act(json!({ "action": "features", "referees": event_target_checked(&ev) })) />
                "Gérer les arbitres"
            </label>
            <label class="check">
                <input type="checkbox" prop:checked=pistes_on on:change=move |ev| org.act(json!({ "action": "features", "pistes": event_target_checked(&ev) })) />
                "Gérer les pistes"
            </label>
            <span class="spacer"></span>
            <span class="org-warnings" title="Affinités signalées entre arbitres et tireurs">
                {["club", "league", "country"]
                    .into_iter()
                    .map(|t| {
                        let label = match t {
                            "club" => "club",
                            "league" => "ligue",
                            _ => "pays",
                        };
                        view! {
                            <label class="warning-box" style=format!("background:{}", s(&colors, t))>
                                <input type="checkbox" prop:checked=b(&warnings, t) on:change=move |ev| org.act(json!({ "action": "warnings", t: event_target_checked(&ev) })) />
                                {label}
                            </label>
                        }
                    })
                    .collect_view()}
            </span>
            <span class="org-clock" title="Heure">{s(&d, "clock")}</span>
        </div>
        <div class="org-time">
            <button class="light" on:click=move |_| org.act(json!({ "action": "cursor", "delta": -15 }))>"−15 min"</button>
            <strong class="cursor-time">{s(&d, "cursor")}</strong>
            <button class="light" on:click=move |_| org.act(json!({ "action": "cursor", "delta": 15 }))>"+15 min"</button>
            <input
                type="time"
                step="900"
                prop:value=s(&d, "cursor")
                on:change=move |ev| org.act(json!({ "action": "cursor", "time": event_target_value(&ev) }))
            />
            <button class="light" on:click=move |_| org.act(json!({ "action": "cursor", "time": "now" }))>"Maintenant"</button>
            <span class="muted">"Les pistes affichent ce qui s'y passe à l'heure choisie ; les matchs y sont placés à partir de cette heure."</span>
        </div>
    }
}

// -------------------------------------------------------------------- frise

#[component]
fn Timeline(org: Org, d: Value) -> impl IntoView {
    let span = u(&d, "span_minutes").max(1) as f64;
    let rows = d.get("competitions").and_then(Value::as_array).map(|c| c.len()).unwrap_or(0).max(1) as f64;
    let cursor = u(&d, "cursor_minutes") as f64;
    let now = d.get("now_minutes").and_then(Value::as_i64).unwrap_or(0) as f64;
    let origin = s(&d, "origin");
    let origin_h: f64 = origin.split(':').next().and_then(|h| h.parse().ok()).unwrap_or(0.0);
    let origin_m: f64 = origin.split(':').nth(1).and_then(|m| m.parse().ok()).unwrap_or(0.0);
    let first_hour = 60.0 - origin_m;
    let hours: Vec<(f64, u32)> = (0..12)
        .map(|i| (first_hour + i as f64 * 60.0, ((origin_h as u32) + 1 + i) % 24))
        .filter(|(x, _)| *x <= span)
        .collect();
    let row_h = 30.0 / rows;
    view! {
        <svg
            class="org-timeline"
            viewBox=format!("0 0 {span} 40")
            preserveAspectRatio="none"
            on:click=move |ev: leptos::ev::MouseEvent| {
                let Some(el) = ev.current_target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()) else { return };
                let width = el.client_width().max(1) as f64;
                let minute = ev.offset_x() as f64 / width * span;
                let rounded = (minute / 15.0).round() * 15.0;
                org.act(json!({ "action": "cursor", "delta": (rounded - cursor) as i64 }));
            }
        >
            <rect x="0" y="0" width=now.max(0.0) height="40" fill="#0000000f" />
            {arr(&d, "timeline")
                .into_iter()
                .map(|t| {
                    view! {
                        <rect
                            x=f(&t, "start")
                            y=u(&t, "row") as f64 * row_h
                            width=(f(&t, "duration") - 0.5).max(1.0)
                            height=row_h
                            fill=s(&t, "color")
                            stroke="#000"
                            stroke-width="0.3"
                        />
                    }
                })
                .collect_view()}
            {hours
                .into_iter()
                .map(|(x, h)| {
                    view! {
                        <line x1=x x2=x y1="30" y2="40" stroke="#333" stroke-width="0.6" />
                        <text x=x y="39" font-size="7" text-anchor="middle" fill="#666">{format!("{h:02}:00")}</text>
                    }
                })
                .collect_view()}
            <line x1=now x2=now y1="0" y2="40" stroke="#00000060" stroke-width="1" />
            <rect x=cursor y="0" width="2.5" height="40" fill="#000" />
        </svg>
    }
}

#[component]
fn Overlaps(org: Org, d: Value) -> impl IntoView {
    let list = arr(&d, "overlaps");
    (!list.is_empty()).then(|| {
        view! {
            <div class="org-overlaps">
                {list
                    .into_iter()
                    .enumerate()
                    .map(|(i, o)| {
                        let message = s(&o, "message");
                        view! {
                            <button class="light warning" style=format!("color:{}", s(&o, "color")) on:click=move |_| {
                                if confirm(&format!("{message} est en retard.\nDécaler les matchs suivants ?")) {
                                    org.act(json!({ "action": "fix_overlap", "index": i }));
                                }
                            }>{format!("⚠ {}", s(&o, "message"))}</button>
                        }
                    })
                    .collect_view()}
            </div>
        }
    })
}

// -------------------------------------------------------------------- salle

#[component]
fn Hall(org: Org, d: Value) -> impl IntoView {
    let pistes = arr(&d, "pistes");
    let max_x = pistes.iter().map(|p| f(p, "x") + f(p, "w")).fold(300.0, f64::max) + 30.0;
    let max_y = pistes.iter().map(|p| f(p, "y") + f(p, "h")).fold(200.0, f64::max) + 30.0;
    // Déplacement d'une piste : (clé, départ souris, décalage courant).
    let dragging = RwSignal::new(None::<(u64, f64, f64, f64, f64)>);
    let selected = org.selected_pistes;
    let sel_list = move || selected.get();
    view! {
        <div class="org-col org-hall">
            <div class="org-toolbar">
                <strong>"Salle"</strong>
                <span class="spacer"></span>
                <button class="light" on:click=move |_| org.act(json!({ "action": "add_piste" }))>"+ Piste"</button>
                <button class="light" disabled=move || sel_list().is_empty() on:click=move |_| {
                    org.act(json!({ "action": "remove_pistes", "pistes": selected.get() }));
                    selected.set(Vec::new());
                }>"Supprimer"</button>
                <button class="light" disabled=move || sel_list().is_empty() on:click=move |_| org.act(json!({ "action": "rotate_pistes", "pistes": selected.get() }))>"Pivoter"</button>
                <button class="light" disabled=move || sel_list().is_empty() on:click=move |_| org.act(json!({ "action": "block_pistes", "pistes": selected.get(), "value": true }))>"Bloquer"</button>
                <button class="light" disabled=move || sel_list().is_empty() on:click=move |_| org.act(json!({ "action": "block_pistes", "pistes": selected.get(), "value": false }))>"Débloquer"</button>
            </div>
            {move || {
                org.held_job.get().map(|_| view! {
                    <p class="hint">"Cliquez sur une piste pour y placer le match. "
                        <button class="light" on:click=move |_| org.held_job.set(None)>"Annuler"</button>
                    </p>
                })
            }}
            {move || {
                org.held_referee.get().map(|_| view! {
                    <p class="hint">"Cliquez sur une piste pour y affecter l'arbitre. "
                        <button class="light" on:click=move |_| org.held_referee.set(None)>"Annuler"</button>
                    </p>
                })
            }}
            <div class="hall-scroll">
                <svg
                    class="hall"
                    width=max_x * ZOOM
                    height=max_y * ZOOM
                    viewBox=format!("0 0 {max_x} {max_y}")
                    on:pointermove=move |ev: leptos::ev::PointerEvent| {
                        if let Some((k, sx, sy, _, _)) = dragging.get_untracked() {
                            let dx = (ev.client_x() as f64 - sx) / ZOOM;
                            let dy = (ev.client_y() as f64 - sy) / ZOOM;
                            dragging.set(Some((k, sx, sy, dx, dy)));
                        }
                    }
                    on:pointerleave=move |_| dragging.set(None)
                >
                    {pistes
                        .into_iter()
                        .map(|p| view! { <Piste org=org p=p dragging=dragging /> })
                        .collect_view()}
                </svg>
            </div>
        </div>
    }
}

#[component]
fn Piste(org: Org, p: Value, dragging: RwSignal<Option<(u64, f64, f64, f64, f64)>>) -> impl IntoView {
    let key = u(&p, "key");
    let (x, y, w, h) = (f(&p, "x"), f(&p, "y"), f(&p, "w"), f(&p, "h"));
    let horizontal = b(&p, "horizontal");
    let slot = p.get("slot").cloned().filter(|v| !v.is_null());
    let fill = slot.as_ref().and_then(|s| s.get("color").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| "#d3d3d3".into());
    let progress = slot.as_ref().map(|s| f(s, "progress")).unwrap_or(0.0);
    let title = slot
        .as_ref()
        .map(|sl| {
            let jobs: Vec<String> = arr(sl, "jobs").iter().map(|j| s(j, "name")).collect();
            format!("{} - {}", s(sl, "batch"), jobs.join(" … "))
        })
        .unwrap_or_default();
    let referee = slot.as_ref().map(|sl| strings(sl, "referees").join(", ")).unwrap_or_default();
    let over = slot.as_ref().map(|sl| b(sl, "over")).unwrap_or(false);
    let blocked = b(&p, "blocked");
    let first_job = slot.as_ref().and_then(|sl| arr(sl, "jobs").first().map(|j| u(j, "key")));
    let id = u(&p, "id");
    let offset = move || match dragging.get() {
        Some((k, _, _, dx, dy)) if k == key => (dx, dy),
        _ => (0.0, 0.0),
    };
    view! {
        <g
            class="piste"
            class:selected=move || org.selected_pistes.get().contains(&key)
            transform=move || {
                let (dx, dy) = offset();
                format!("translate({},{})", x + dx, y + dy)
            }
            on:dragover=move |ev: leptos::ev::DragEvent| ev.prevent_default()
            on:drop=move |ev: leptos::ev::DragEvent| {
                ev.prevent_default();
                if let Some(data) = ev.data_transfer().and_then(|t| t.get_data("text/plain").ok()) {
                    org.drop_on(&data, key);
                }
            }
            on:dblclick=move |_| {
                if let Some(job) = first_job {
                    org.job_detail.set(Some(job));
                }
            }
            on:pointerdown=move |ev: leptos::ev::PointerEvent| {
                dragging.set(Some((key, ev.client_x() as f64, ev.client_y() as f64, 0.0, 0.0)));
            }
            on:pointerup=move |ev: leptos::ev::PointerEvent| {
                let moved = dragging.get_untracked().filter(|(k, ..)| *k == key).map(|(_, _, _, dx, dy)| (dx, dy));
                dragging.set(None);
                match moved {
                    Some((dx, dy)) if dx.abs() > 3.0 || dy.abs() > 3.0 => {
                        org.act(json!({ "action": "move_piste", "piste": key, "x": x + dx, "y": y + dy }));
                    }
                    _ => {
                        if let Some(job) = org.held_job.get_untracked() {
                            org.held_job.set(None);
                            org.act(json!({ "action": "drop_job", "job": job, "piste": key }));
                        } else if let Some(r) = org.held_referee.get_untracked() {
                            org.held_referee.set(None);
                            org.act(json!({ "action": "drop_referee", "referee": r, "piste": key }));
                        } else if ev.ctrl_key() || ev.shift_key() {
                            org.selected_pistes.update(|l| if l.contains(&key) { l.retain(|k| *k != key) } else { l.push(key) });
                        } else {
                            org.selected_pistes.set(vec![key]);
                        }
                    }
                }
            }
        >
            <rect class="piste-bg" width=w height=h fill=fill rx="2" />
            <rect class="piste-progress" width=progress * w height=h fill="#00000018" />
            <text x="3" y="13" class="piste-id">{id}</text>
            {horizontal.then(|| view! {
                <text x=w / 2.0 y="12" class="piste-title" text-anchor="middle">{title.clone()}</text>
                <text x="22" y=h - 6.0 class="piste-referee">{(!referee.is_empty()).then(|| format!("⚑ {referee}"))}</text>
            })}
            {(!horizontal).then(|| view! {
                <text x="3" y="28" class="piste-title small">{title.clone()}</text>
                <text x="3" y="40" class="piste-referee">{(!referee.is_empty()).then(|| format!("⚑ {referee}"))}</text>
            })}
            {blocked.then(|| view! { <text x=w / 2.0 y=h / 2.0 + 5.0 text-anchor="middle" class="piste-mark">"🚧"</text> })}
            {over.then(|| view! { <text x=w - 14.0 y=h - 6.0 class="piste-mark">"🏁"</text> })}
        </g>
    }
}

// ------------------------------------------------------------------ matchs

#[component]
fn Batches(org: Org, d: Value) -> impl IntoView {
    let competitions = arr(&d, "competitions");
    if competitions.is_empty() {
        return view! {
            <div class="org-col org-batches">
                <strong>"Matchs"</strong>
                <p class="muted">"Aucune compétition en cours : composez les poules (ou passez au tableau) dans une compétition pour retrouver ici ses matchs."</p>
            </div>
        }
        .into_any();
    }
    let current = org.competition.get_untracked().filter(|c| competitions.iter().any(|x| u(x, "id") == *c)).unwrap_or_else(|| u(&competitions[0], "id"));
    let comp = competitions.iter().find(|c| u(c, "id") == current).cloned().unwrap_or(Value::Null);
    let batches = arr(&comp, "batches");
    let batch_id = org.batch.get_untracked().filter(|b| batches.iter().any(|x| u(x, "id") == *b)).or_else(|| batches.first().map(|b| u(b, "id")));
    let batch = batches.iter().find(|x| Some(u(x, "id")) == batch_id).cloned();
    view! {
        <div class="org-col org-batches">
            <div class="org-toolbar">
                <strong>"Matchs"</strong>
                <span class="spacer"></span>
                <select on:change=move |ev| {
                    org.competition.set(event_target_value(&ev).parse().ok());
                    org.batch.set(None);
                    org.reload();
                }>
                    {competitions
                        .iter()
                        .map(|c| {
                            let id = u(c, "id");
                            view! { <option value=id.to_string() selected=id == current>{s(c, "title")}</option> }
                        })
                        .collect_view()}
                </select>
            </div>
            <div class="batch-tabs" style=format!("border-color:{}", s(&comp, "color"))>
                {batches
                    .iter()
                    .map(|bt| {
                        let id = u(bt, "id");
                        let active = Some(id) == batch_id;
                        view! {
                            <button class="light" class:active=active on:click=move |_| {
                                org.batch.set(Some(id));
                                org.reload();
                            }>
                                {format!("{}  {}/{}{}", s(bt, "name"), u(bt, "ready"), u(bt, "expected"), if b(bt, "complete") { "" } else { " ❓" })}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            {batch.map(|bt| view! { <BatchJobs org=org comp=comp.clone() bt=bt /> })}
        </div>
    }
    .into_any()
}

#[component]
fn BatchJobs(org: Org, comp: Value, bt: Value) -> impl IntoView {
    let c = u(&comp, "id");
    let id = u(&bt, "id");
    let ext1 = bt.get("extension1").and_then(Value::as_str).map(str::to_string);
    let ext2 = bt.get("extension2").and_then(Value::as_str).map(str::to_string);
    let choice = u(&bt, "extension");
    view! {
        <div class="org-toolbar">
            <button disabled=!b(&bt, "can_assign") on:click=move |_| {
                org.batch.set(Some(id));
                org.competition.set(Some(c));
                org.assigning.set(true);
            }>"Placer…"</button>
            <button class="light" disabled=!b(&bt, "can_cancel") on:click=move |_| {
                if confirm("Retirer tous les matchs de ce lot de leurs pistes ?") {
                    org.act(json!({ "action": "cancel", "competition": c, "batch": id }));
                }
            }>"Annuler le placement"</button>
            {ext1.map(|e1| view! {
                <span class="muted">"Prolongation : "</span>
                <select on:change=move |ev| org.act(json!({ "action": "extension", "competition": c, "batch": id, "choice": event_target_value(&ev).parse::<u64>().unwrap_or(0) }))>
                    <option value="0" selected=choice == 0>"Non"</option>
                    <option value="1" selected=choice == 1>{e1}</option>
                    {ext2.map(|e2| view! { <option value="2" selected=choice == 2>{e2}</option> })}
                </select>
            })}
        </div>
        <table class="list jobs">
            <tbody>
                {arr(&bt, "jobs")
                    .into_iter()
                    .map(|j| {
                        let key = u(&j, "key");
                        let status = s(&j, "status");
                        let icon = match status.as_str() {
                            "pending" => "❓",
                            "error" => "⚠",
                            _ => "✔",
                        };
                        let placed = strings_of_pistes(&j);
                        let warning = j.get("warning").and_then(Value::as_str).map(|w| format!("background:{w}")).unwrap_or_default();
                        view! {
                            <tr
                                draggable="true"
                                class:held=move || org.held_job.get() == Some(key)
                                class:selected=move || org.selected_jobs.get().contains(&key)
                                on:dragstart=move |ev: leptos::ev::DragEvent| {
                                    if let Some(t) = ev.data_transfer() {
                                        let _ = t.set_data("text/plain", &format!("job:{key}"));
                                    }
                                }
                                on:click=move |ev: leptos::ev::MouseEvent| {
                                    if ev.ctrl_key() || ev.shift_key() {
                                        org.selected_jobs.update(|l| if l.contains(&key) { l.retain(|k| *k != key) } else { l.push(key) });
                                    } else {
                                        org.selected_jobs.set(vec![key]);
                                        org.job_detail.set(Some(key));
                                    }
                                }
                            >
                                <td class="icon">{icon}</td>
                                <td style=warning><strong>{s(&j, "name")}</strong></td>
                                <td class="muted">{placed}</td>
                                <td class="muted">{s(&j, "start")}</td>
                                <td class="muted">{strings(&j, "referees").join(", ")}</td>
                                <td>
                                    {(status == "pending").then(|| view! {
                                        <button class="light small" title="Placer ce match : cliquez ensuite sur une piste" on:click=move |ev| {
                                            ev.stop_propagation();
                                            org.held_job.set(Some(key));
                                        }>"→ piste"</button>
                                    })}
                                </td>
                            </tr>
                        }
                    })
                    .collect_view()}
            </tbody>
        </table>
    }
}

fn strings_of_pistes(j: &Value) -> String {
    let pistes: Vec<String> = arr(j, "pistes").iter().filter_map(Value::as_u64).map(|p| format!("Piste {p}")).collect();
    pistes.join(", ")
}

/// Dialogue « Placer les matchs » (`job_dialog`).
#[component]
fn AssignDialog(org: Org, d: Value) -> impl IntoView {
    let all_jobs = RwSignal::new(true);
    let referees = RwSignal::new("all".to_string());
    let all_pistes = RwSignal::new(true);
    let referees_on = b(&d, "referees_enabled");
    let cursor = s(&d, "cursor");
    view! {
        <Show when=move || org.assigning.get()>
            <div class="modal-backdrop">
                <div class="modal">
                    <h3>"Placer les matchs"</h3>
                    <p class="muted">{format!("À partir de {cursor}")}</p>
                    <fieldset>
                        <legend>"Matchs"</legend>
                        <label class="check"><input type="radio" name="jobs" prop:checked=move || all_jobs.get() on:change=move |_| all_jobs.set(true) />"Tous"</label>
                        <label class="check"><input type="radio" name="jobs" prop:checked=move || !all_jobs.get() on:change=move |_| all_jobs.set(false) />"Seulement ceux sélectionnés"</label>
                    </fieldset>
                    {referees_on.then(|| view! {
                        <fieldset>
                            <legend>"Arbitres"</legend>
                            <label class="check"><input type="radio" name="refs" prop:checked=move || referees.get() == "all" on:change=move |_| referees.set("all".into()) />"Tous"</label>
                            <label class="check"><input type="radio" name="refs" prop:checked=move || referees.get() == "selected" on:change=move |_| referees.set("selected".into()) />"Seulement ceux sélectionnés"</label>
                            <label class="check"><input type="radio" name="refs" prop:checked=move || referees.get() == "none" on:change=move |_| referees.set("none".into()) />"Aucun"</label>
                        </fieldset>
                    })}
                    <fieldset>
                        <legend>"Pistes"</legend>
                        <label class="check"><input type="radio" name="pistes" prop:checked=move || all_pistes.get() on:change=move |_| all_pistes.set(true) />"Toutes"</label>
                        <label class="check"><input type="radio" name="pistes" prop:checked=move || !all_pistes.get() on:change=move |_| all_pistes.set(false) />"Seulement celles sélectionnées"</label>
                    </fieldset>
                    <div class="actions">
                        <button class="light" on:click=move |_| org.assigning.set(false)>"Annuler"</button>
                        <button on:click=move |_| {
                            org.assigning.set(false);
                            org.act(json!({
                                "action": "assign",
                                "competition": org.competition.get_untracked(),
                                "batch": org.batch.get_untracked(),
                                "all_jobs": all_jobs.get_untracked(),
                                "jobs": org.selected_jobs.get_untracked(),
                                "referees": if referees_on { referees.get_untracked() } else { "none".to_string() },
                                "selected_referees": org.selected_referees.get_untracked(),
                                "pistes": if all_pistes.get_untracked() { Vec::new() } else { org.selected_pistes.get_untracked() },
                            }));
                        }>"Placer"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

/// Fiche d'un match (`JobBoard`) : tireurs, piste, heure, arbitres.
#[component]
fn JobDetail(org: Org, d: Value) -> impl IntoView {
    let referees_on = b(&d, "referees_enabled");
    let pistes_on = b(&d, "pistes_enabled");
    let pistes: Vec<(u64, u64)> = arr(&d, "pistes").iter().map(|p| (u(p, "key"), u(p, "id"))).collect();
    let find = move |key: u64| -> Option<(Value, Value)> {
        let data = org.data.get()?;
        for c in arr(&data, "competitions") {
            for bt in arr(&c, "batches") {
                for j in arr(&bt, "jobs") {
                    if u(&j, "key") == key {
                        return Some((c.clone(), j));
                    }
                }
            }
        }
        None
    };
    let weapon_referees = move |weapon: &str| -> Vec<Value> {
        org.data
            .get()
            .map(|data| arr(&data, "referees").into_iter().find(|l| s(l, "weapon") == weapon).map(|l| arr(&l, "members")).unwrap_or_default())
            .unwrap_or_default()
    };
    view! {
        {move || {
            let key = org.job_detail.get()?;
            let (comp, job) = find(key)?;
            let placed = !arr(&job, "pistes").is_empty();
            let candidates: Vec<Value> = weapon_referees(&s(&comp, "weapon")).into_iter().filter(|r| b(r, "attending")).collect();
            let assigned = strings(&job, "referees");
            let move_to = RwSignal::new(String::new());
            let time = RwSignal::new(job.get("start").and_then(Value::as_str).map(str::to_string).unwrap_or_default());
            let pool = b(&job, "pool");
            let current_pistes: Vec<u64> = arr(&job, "piste_keys").iter().filter_map(Value::as_u64).collect();
            let choose = RwSignal::new(String::new());
            let pistes = pistes.clone();
            Some(view! {
                <div class="modal-backdrop" on:click=move |_| org.job_detail.set(None)>
                    <div class="modal job-board" on:click=|ev| ev.stop_propagation()>
                        <header class="job-board-head" style=format!("background:{}", s(&comp, "color"))>
                            <span>{s(&comp, "title")}</span>
                            <strong>{s(&job, "name")}</strong>
                        </header>
                        <p>
                            {if placed { strings_of_pistes(&job) } else { "Pas de piste".to_string() }}
                            " · "
                            {job.get("start").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| "Pas d'heure".into())}
                        </p>
                        <div class="row">
                            <label>"Heure : "</label>
                            <input
                                type="time"
                                prop:value=time.get_untracked()
                                on:input=move |ev| time.set(event_target_value(&ev))
                                on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                                    if ev.key() == "Enter" {
                                        org.act(json!({ "action": "set_time", "job": key, "time": time.get_untracked() }));
                                    }
                                }
                            />
                            <button class="light" on:click=move |_| org.act(json!({ "action": "set_time", "job": key, "time": time.get_untracked() }))>"Changer l'heure"</button>
                        </div>
                        <h4>"Tireurs"</h4>
                        <ul class="plain">
                            {arr(&job, "fencers").into_iter().map(|fe| view! { <li><strong>{s(&fe, "name")}</strong>" "<span class="muted">{s(&fe, "club")}</span></li> }).collect_view()}
                        </ul>
                        {referees_on.then(|| view! {
                            <h4>"Arbitres"</h4>
                            <p>{if assigned.is_empty() { "Aucun".to_string() } else { assigned.join(", ") }}</p>
                            <div class="row">
                                <select on:change=move |ev| choose.set(event_target_value(&ev))>
                                    <option value="">"Choisir un arbitre…"</option>
                                    {candidates.iter().map(|r| view! { <option value=u(r, "reference").to_string()>{format!("{} {} ({})", s(r, "name"), s(r, "first_name"), s(r, "club"))}</option> }).collect_view()}
                                </select>
                                <button class="light" on:click=move |_| {
                                    if let Ok(r) = choose.get_untracked().parse::<u64>() {
                                        org.act(json!({ "action": "job_referees", "job": key, "referees": [r] }));
                                    }
                                }>"Affecter"</button>
                                <button class="light" on:click=move |_| org.act(json!({ "action": "job_referees", "job": key, "referees": [] }))>"Retirer les arbitres"</button>
                            </div>
                        })}
                        {(placed && pool && pistes_on).then(|| {
                            let chosen: RwSignal<Vec<u64>> = RwSignal::new(current_pistes.clone());
                            let refs: Vec<RwSignal<String>> = (0..pistes.len()).map(|_| RwSignal::new(String::new())).collect();
                            let refs2 = refs.clone();
                            let candidates = candidates.clone();
                            view! {
                                <h4>"Pistes de la poule"</h4>
                                <p class="muted small">"Une poule peut se tirer sur plusieurs pistes en même temps, avec un arbitre par piste."</p>
                                <div class="form-grid">
                                    {pistes
                                        .iter()
                                        .zip(refs.iter())
                                        .map(|((k, id), r)| {
                                            let (k, id, r) = (*k, *id, *r);
                                            let candidates = candidates.clone();
                                            view! {
                                                <label class="check">
                                                    <input type="checkbox" prop:checked=move || chosen.get().contains(&k) on:change=move |ev| {
                                                        let on = event_target_checked(&ev);
                                                        chosen.update(|l| if on { if !l.contains(&k) { l.push(k) } } else { l.retain(|x| *x != k) });
                                                    } />
                                                    {format!("Piste {id}")}
                                                </label>
                                                {if referees_on {
                                                    view! {
                                                        <select on:change=move |ev| r.set(event_target_value(&ev))>
                                                            <option value="">"Arbitre…"</option>
                                                            {candidates.iter().map(|c| view! { <option value=u(c, "reference").to_string()>{format!("{} {}", s(c, "name"), s(c, "first_name"))}</option> }).collect_view()}
                                                        </select>
                                                    }.into_any()
                                                } else {
                                                    view! { <span></span> }.into_any()
                                                }}
                                            }
                                        })
                                        .collect_view()}
                                </div>
                                <button class="light" on:click={
                                    let pistes = pistes.clone();
                                    move |_| {
                                        let selected = chosen.get_untracked();
                                        let referees: Vec<u64> = pistes
                                            .iter()
                                            .zip(refs2.iter())
                                            .filter(|((k, _), _)| selected.contains(k))
                                            .filter_map(|(_, r)| r.get_untracked().parse::<u64>().ok())
                                            .collect();
                                        let ordered: Vec<u64> = pistes.iter().map(|(k, _)| *k).filter(|k| selected.contains(k)).collect();
                                        org.act(json!({ "action": "pool_resources", "job": key, "pistes": ordered, "referees": referees }));
                                    }
                                }>"Appliquer les pistes"</button>
                            }
                        })}
                        <div class="actions">
                            {placed.then(|| view! {
                                <select on:change=move |ev| move_to.set(event_target_value(&ev))>
                                    <option value="">"Déplacer vers…"</option>
                                    {pistes.iter().map(|(k, id)| view! { <option value=k.to_string()>{format!("Piste {id}")}</option> }).collect_view()}
                                </select>
                                <button class="light" on:click=move |_| {
                                    if let Ok(p) = move_to.get_untracked().parse::<u64>() {
                                        org.act(json!({ "action": "move_job", "job": key, "piste": p }));
                                    }
                                }>"Déplacer"</button>
                                <button class="light" on:click=move |_| {
                                    org.act(json!({ "action": "take_off", "job": key }));
                                    org.job_detail.set(None);
                                }>"Retirer de la piste"</button>
                            })}
                            <span class="spacer"></span>
                            <button on:click=move |_| org.job_detail.set(None)>"Fermer"</button>
                        </div>
                    </div>
                </div>
            })
        }}
    }
}

// ----------------------------------------------------------------- arbitres

#[component]
fn Referees(org: Org, d: Value) -> impl IntoView {
    let lists = arr(&d, "referees");
    // Armes des compétitions en cours d'abord.
    let busy: Vec<String> = arr(&d, "competitions").iter().map(|c| s(c, "weapon")).collect();
    let current = {
        let w = org.weapon.get_untracked();
        if lists.iter().any(|l| s(l, "weapon") == w) {
            w
        } else {
            busy.first().cloned().or_else(|| lists.first().map(|l| s(l, "weapon"))).unwrap_or_default()
        }
    };
    let list = lists.iter().find(|l| s(l, "weapon") == current).cloned().unwrap_or(Value::Null);
    let weapon = current.clone();
    let input = NodeRef::<leptos::html::Input>::new();
    let import = {
        let weapon = weapon.clone();
        move |_| {
            let Some(el) = input.get() else { return };
            let weapon = weapon.clone();
            let data = org.data;
            spawn_local(async move {
                if let Some((name, bytes)) = crate::home::read_file(&el).await {
                    let path = format!("/api/organisation/import?weapon={weapon}&name={}", js_sys::encode_uri_component(&name));
                    match api::upload::<Value>(&path, bytes).await {
                        Ok(v) => data.set(Some(v)),
                        Err(e) => alert(&e.message),
                    }
                }
                el.set_value("");
            });
        }
    };
    let w1 = weapon.clone();
    let w2 = weapon.clone();
    let w3 = weapon.clone();
    let w4 = weapon.clone();
    let w5 = weapon.clone();
    let w6 = weapon.clone();
    let w7 = weapon.clone();
    view! {
        <div class="org-col org-referees">
            <div class="org-toolbar">
                <strong>"Arbitres"</strong>
                <span class="spacer"></span>
                <PrintReferees org=org meal=true />
                <PrintReferees org=org meal=false />
            </div>
            <div class="weapon-tabs">
                {lists
                    .iter()
                    .map(|l| {
                        let code = s(l, "weapon");
                        let active = code == current;
                        let c2 = code.clone();
                        view! {
                            <button class="light" class:active=active on:click=move |_| {
                                org.weapon.set(c2.clone());
                                org.reload();
                            }>
                                {format!("{} {}/{}", s(l, "label"), u(l, "present"), u(l, "present") + u(l, "absent"))}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="org-toolbar wrap">
                <button class="light" on:click=move |_| org.editing.set(Some(json!({ "weapon": w1.clone() })))>"+ Ajouter"</button>
                <label class="button light">
                    "Importer…"
                    <input type="file" accept=".fff,.FFF,.csv,.txt" class="hidden" node_ref=input on:change=import />
                </label>
                <button class="light" on:click=move |_| org.act(json!({ "action": "list_attending", "weapon": w2.clone(), "value": true }))>"Tous présents"</button>
                <button class="light" on:click=move |_| org.act(json!({ "action": "list_attending", "weapon": w3.clone(), "value": false }))>"Tous absents"</button>
                <button class="light" on:click=move |_| org.act(json!({ "action": "referee_available", "weapon": w4.clone(), "referees": org.selected_referees.get_untracked(), "value": true }))>"Disponible"</button>
                <button class="light" on:click=move |_| org.act(json!({ "action": "referee_available", "weapon": w5.clone(), "referees": org.selected_referees.get_untracked(), "value": false }))>"Indisponible"</button>
                <button class="light" on:click=move |_| {
                    let sel = org.selected_referees.get_untracked();
                    if !sel.is_empty() && confirm("Retirer les arbitres sélectionnés de cette arme ?") {
                        org.act(json!({ "action": "referee_remove", "weapon": w6.clone(), "referees": sel }));
                        org.selected_referees.set(Vec::new());
                    }
                }>"Retirer"</button>
            </div>
            <table class="list referees">
                <thead>
                    <tr>
                        <th>"Présent"</th>
                        <th>"Nom"</th>
                        <th>"Club"</th>
                        <th>"Charge"</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    {arr(&list, "members")
                        .into_iter()
                        .map(|r| {
                            let reference = u(&r, "reference");
                            let edit = r.clone();
                            let weapon = w7.clone();
                            let style = r.get("busy").and_then(Value::as_str).map(|c| format!("background:{c}")).unwrap_or_default();
                            view! {
                                <tr
                                    draggable="true"
                                    style=style
                                    class:unavailable=!b(&r, "available")
                                    class:selected=move || org.selected_referees.get().contains(&reference)
                                    on:dragstart=move |ev: leptos::ev::DragEvent| {
                                        if let Some(t) = ev.data_transfer() {
                                            let _ = t.set_data("text/plain", &format!("referee:{reference}"));
                                        }
                                    }
                                    on:click=move |ev: leptos::ev::MouseEvent| {
                                        if ev.ctrl_key() || ev.shift_key() {
                                            org.selected_referees.update(|l| if l.contains(&reference) { l.retain(|k| *k != reference) } else { l.push(reference) });
                                        } else {
                                            org.selected_referees.set(vec![reference]);
                                        }
                                    }
                                    on:dblclick=move |_| {
                                        let mut e = edit.clone();
                                        e["weapon"] = json!(weapon.clone());
                                        org.editing.set(Some(e));
                                    }
                                >
                                    <td>
                                        <input
                                            type="checkbox"
                                            prop:checked=b(&r, "attending")
                                            on:click=|ev| ev.stop_propagation()
                                            on:change=move |ev| org.act(json!({ "action": "referee_attending", "referee": reference, "value": event_target_checked(&ev) }))
                                        />
                                    </td>
                                    <td><strong>{s(&r, "name")}</strong>" "{s(&r, "first_name")}</td>
                                    <td class="muted">{s(&r, "club")}</td>
                                    <td class="number">{format!("{}%", u(&r, "workload"))}</td>
                                    <td>
                                        <button class="light small" title="Affecter à une piste : cliquez ensuite sur la piste" on:click=move |ev| {
                                            ev.stop_propagation();
                                            org.held_referee.set(Some(reference));
                                        }>"→ piste"</button>
                                    </td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
            <p class="muted small">"Glissez un arbitre sur une piste (ou « → piste » puis clic sur la piste). Double-clic : fiche de l'arbitre. Couleur : occupé à l'heure choisie."</p>
        </div>
    }
}

/// Fiche d'un arbitre (création ou modification).
#[component]
fn RefereeForm(org: Org) -> impl IntoView {
    view! {
        {move || {
            let e = org.editing.get()?;
            let fields = ["name", "first_name", "club", "league", "country", "licence", "level"];
            let labels = ["Nom", "Prénom", "Club", "Ligue", "Pays", "Licence", "Niveau"];
            let values: Vec<RwSignal<String>> = fields.iter().map(|k| RwSignal::new(s(&e, k))).collect();
            let attending = RwSignal::new(e.get("attending").and_then(Value::as_bool).unwrap_or(true));
            let reference = e.get("reference").and_then(Value::as_u64);
            let weapon = s(&e, "weapon");
            let values2 = values.clone();
            Some(view! {
                <div class="modal-backdrop">
                    <div class="modal">
                        <h3>{if reference.is_some() { "Arbitre" } else { "Nouvel arbitre" }}</h3>
                        <div class="form-grid">
                            {fields
                                .iter()
                                .zip(labels)
                                .zip(values.iter())
                                .map(|((_, label), sig)| {
                                    let sig = *sig;
                                    view! {
                                        <label>{label}</label>
                                        <input type="text" prop:value=move || sig.get() on:input=move |ev| sig.set(event_target_value(&ev)) />
                                    }
                                })
                                .collect_view()}
                            <label>"Présent"</label>
                            <input type="checkbox" prop:checked=move || attending.get() on:change=move |ev| attending.set(event_target_checked(&ev)) />
                        </div>
                        <div class="actions">
                            <button class="light" on:click=move |_| org.editing.set(None)>"Annuler"</button>
                            <button on:click=move |_| {
                                let mut map = serde_json::Map::new();
                                for (k, sig) in fields.iter().zip(values2.iter()) {
                                    map.insert(k.to_string(), json!(sig.get_untracked()));
                                }
                                org.editing.set(None);
                                org.act(json!({
                                    "action": "referee_save",
                                    "weapon": weapon.clone(),
                                    "reference": reference,
                                    "fields": map,
                                    "attending": attending.get_untracked(),
                                }));
                            }>"Enregistrer"</button>
                        </div>
                    </div>
                </div>
            })
        }}
    }
}

fn set_print_mode(on: bool) {
    if let Some(body) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.body()) {
        let _ = if on { body.class_list().add_1("print-groups") } else { body.class_list().remove_1("print-groups") };
    }
}

/// Tickets repas ou carnet de paiement des arbitres (`Marshaller::DrawPage`).
#[component]
fn PrintReferees(org: Org, meal: bool) -> impl IntoView {
    let open = RwSignal::new(false);
    let title = if meal { "Tickets repas" } else { "Carnet de paiement" };
    view! {
        <button class="light small" on:click=move |_| {
            set_print_mode(true);
            open.set(true);
        }>{title}</button>
        {move || {
            if !open.get() {
                return None;
            }
            let data = org.data.get_untracked()?;
            let mut referees: Vec<Value> = Vec::new();
            for l in arr(&data, "referees") {
                for r in arr(&l, "members") {
                    if !referees.iter().any(|x| u(x, "reference") == u(&r, "reference")) {
                        referees.push(r);
                    }
                }
            }
            let per_page = if meal { 12 } else { 20 };
            let pages: Vec<Vec<Value>> = referees.chunks(per_page).map(|c| c.to_vec()).collect();
            Some(view! {
                <leptos::portal::Portal>
                    <div class="print-sheets">
                        <div class="print-bar">
                            <strong>{title}</strong>
                            <span class="spacer"></span>
                            <button on:click=move |_| {
                                if let Some(w) = web_sys::window() {
                                    let _ = w.print();
                                }
                            }>"Imprimer"</button>
                            <button class="light" on:click=move |_| {
                                set_print_mode(false);
                                open.set(false);
                            }>"Fermer"</button>
                        </div>
                        {pages
                            .clone()
                            .into_iter()
                            .enumerate()
                            .map(|(n, page)| {
                                if meal {
                                    view! {
                                        <section class="print-page tickets">
                                            {page.into_iter().map(|r| view! {
                                                <div class="ticket">
                                                    <div class="ticket-name"><strong>{s(&r, "name")}</strong><br />{s(&r, "first_name")}</div>
                                                    <div class="ticket-food">
                                                        <span>"Repas"</span><span>"Boisson"</span><span>"Dessert"</span><span>"Café"</span>
                                                    </div>
                                                </div>
                                            }).collect_view()}
                                        </section>
                                    }.into_any()
                                } else {
                                    view! {
                                        <section class="print-page">
                                            <h2 class="payment-title">"Carnet de paiement"</h2>
                                            <table class="payment">
                                                {page.into_iter().map(|r| view! {
                                                    <tr>
                                                        <td><strong>{s(&r, "name")}</strong></td>
                                                        <td>{s(&r, "first_name")}</td>
                                                        <td>{let l = s(&r, "level"); if l.is_empty() { "-".to_string() } else { l }}</td>
                                                        <td class="muted">"€"</td>
                                                        <td class="muted">"Signature"</td>
                                                    </tr>
                                                }).collect_view()}
                                            </table>
                                            <p class="page-number">{format!("Page {}", n + 1)}</p>
                                        </section>
                                    }.into_any()
                                }
                            })
                            .collect_view()}
                    </div>
                </leptos::portal::Portal>
            })
        }}
    }
}
