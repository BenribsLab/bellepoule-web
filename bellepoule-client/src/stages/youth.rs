//! Entraînement des jeunes : réglages, création des groupes (glisser-déposer
//! d'un groupe à l'autre), résultats de chaque groupe.

use leptos::prelude::*;
use serde_json::{json, Value};

use super::{arr, b, s, u, StageCtx};

#[component]
pub fn Youth(ctx: StageCtx) -> impl IntoView {
    view! {
        {move || {
            ctx.data
                .get()
                .map(|v| {
                    let locked = b(&v, "locked");
                    let d = v["data"].clone();
                    let settings = d["settings"].clone();
                    view! {
                        <Settings ctx=ctx settings=settings choices=arr(&d, "swapping_choices") locked=locked />
                        <div class="stage-bar">
                            <strong>{format!("{} tireurs → {} groupes", u(&d, "count"), arr(&d, "groups").len())}</strong>
                            {b(&d, "customized").then(|| view! { <span class="muted">"(modifiée)"</span> })}
                            <span class="spacer"></span>
                            <label class="inline">
                                "Nombre de groupes "
                                <input
                                    type="number"
                                    min="1"
                                    disabled=locked
                                    prop:value=arr(&d, "groups").len().to_string()
                                    on:change=move |ev| {
                                        let count: u64 = event_target_value(&ev).parse().unwrap_or(1);
                                        ctx.act.run(json!({ "action": "group_count", "count": count }));
                                    }
                                />
                            </label>
                            <button class="light" disabled=locked on:click=move |_| ctx.act.run(json!({ "action": "reset" }))>
                                "Répartition par défaut"
                            </button>
                        </div>
                        <p class="muted">
                            {if locked {
                                "Groupes faits. « Tour précédent » pour les modifier."
                            } else {
                                "Glissez un tireur sur un autre groupe pour le déplacer. Chaque groupe a ensuite ses poules et son tableau."
                            }}
                        </p>
                        <div class="groups">
                            {arr(&d, "groups").into_iter().map(|g| view! { <Group ctx=ctx group=g locked=locked /> }).collect_view()}
                        </div>
                    }
                })
        }}
    }
}

#[component]
fn Group(ctx: StageCtx, group: Value, locked: bool) -> impl IntoView {
    let number = u(&group, "number");
    let index = number.saturating_sub(1);
    let best = u(&group, "best_pools");
    let current = u(&group, "nb_pools");
    let size = u(&group, "size");
    let over = RwSignal::new(false);
    let drop = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        over.set(false);
        if let Some(player) = ev.data_transfer().and_then(|t| t.get_data("text/plain").ok()).and_then(|t| t.parse::<u64>().ok()) {
            ctx.act.run(json!({ "action": "move", "player": player, "to": index }));
        }
    };
    view! {
        <div
            class="group card"
            class:drop-over=move || over.get()
            on:dragover=move |ev| {
                if !locked {
                    ev.prevent_default();
                    over.set(true);
                }
            }
            on:dragleave=move |_| over.set(false)
            on:drop=drop
        >
            <div class="group-head">
                <strong>{format!("Groupe {number}")}</strong>
                <label class="inline">
                    "Tireurs "
                    <input
                        type="number"
                        min="3"
                        disabled=locked
                        prop:value=size.to_string()
                        on:change=move |ev| {
                            let size: u64 = event_target_value(&ev).parse().unwrap_or(size);
                            ctx.act.run(json!({ "action": "group_size", "group": index, "size": size }));
                        }
                    />
                </label>
                <label class="inline">
                    "Poules "
                    <select
                        disabled=locked
                        on:change=move |ev| {
                            let n: u64 = event_target_value(&ev).parse().unwrap_or(1);
                            ctx.act.run(json!({ "action": "group_pools", "group": index, "nb_pools": n }));
                        }
                    >
                        {arr(&group, "pool_choices")
                            .into_iter()
                            .map(|c| {
                                let n = u(&c, "nb_pools");
                                let (small, large) = (u(&c, "smallest"), u(&c, "largest"));
                                let of = if n > 1 { "poules de" } else { "poule de" };
                                let sizes = if small == large { small.to_string() } else { format!("{small} et {large}") };
                                let star = if n == best { "★ " } else { "" };
                                view! { <option value=n.to_string() selected=n == current>{format!("{star}{n} {of} {sizes}")}</option> }
                            })
                            .collect_view()}
                    </select>
                </label>
                <span class="muted">{format!("{} matchs par tireur", u(&group, "bouts"))}</span>
            </div>
            <ul class="fencers">
                {arr(&group, "fencers")
                    .into_iter()
                    .map(|f| {
                        let reference = u(&f, "reference");
                        let rank = f.get("ranking").and_then(Value::as_u64).map(|r| format!("{r}. ")).unwrap_or_default();
                        view! {
                            <li
                                draggable=if locked { "false" } else { "true" }
                                on:dragstart=move |ev: leptos::ev::DragEvent| {
                                    if let Some(t) = ev.data_transfer() {
                                        let _ = t.set_data("text/plain", &reference.to_string());
                                    }
                                }
                            >
                                <span>{format!("{rank}{}", s(&f, "name"))}</span>
                                <span class="muted">{s(&f, "club")}</span>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
        </div>
    }
}

/// Réglages de la phase (les mêmes que dans les préférences de l'application).
#[component]
pub(crate) fn Settings(ctx: StageCtx, settings: Value, choices: Vec<Value>, locked: bool) -> impl IntoView {
    let open = RwSignal::new(false);
    let st = settings.clone();
    let send = move |changes: Value| {
        let mut all = settings.clone();
        if let (Some(a), Some(c)) = (all.as_object_mut(), changes.as_object()) {
            for (k, v) in c {
                a.insert(k.clone(), v.clone());
            }
        }
        ctx.act.run(json!({ "action": "settings", "settings": all }));
    };
    let summary = format!(
        "Groupes de {} · {} poules par groupe · {} · places tirées : {} · points : {}",
        u(&st, "group_size"),
        u(&st, "pools_per_group"),
        match s(&st, "group_mode").as_str() {
            "ranking" => "au classement (serpentin)",
            "random" => "au hasard",
            _ => "par division",
        },
        match s(&st, "fenced_places").as_str() {
            "third" => "3e place",
            "none" => "aucune",
            _ => "toutes",
        },
        match s(&st, "points_mode").as_str() {
            "victory" => format!("{} par victoire", u(&st, "points_value")),
            "place" => "selon la place".to_string(),
            _ => format!("{} pour tous", u(&st, "points_value")),
        }
    );
    let swapping: Vec<String> = arr(&st, "swapping").iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    let points_list = arr(&st, "points_list").iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
    let send = StoredValue::new(send);
    let radio = move |name: &'static str, key: &'static str, value: &'static str, label: &'static str, current: String| {
        view! {
            <label class="check">
                <input
                    type="radio"
                    name=name
                    checked=current == value
                    disabled=locked
                    on:change=move |_| send.with_value(|f| f(json!({ key: value })))
                />
                {label}
            </label>
        }
    };
    view! {
        <details class="settings" prop:open=move || open.get()>
            <summary>{format!("Réglages — {summary}")}</summary>
            <div class="settings-grid">
                <div>
                    <label class="inline">
                        "Tireurs par groupe "
                        <input type="number" min="4" disabled=locked prop:value=u(&st, "group_size").to_string()
                            on:change=move |ev| send.with_value(|f| f(json!({ "group_size": event_target_value(&ev).parse::<u64>().unwrap_or(8) }))) />
                    </label>
                    <label class="inline">
                        "Poules par groupe "
                        <input type="number" min="1" disabled=locked prop:value=u(&st, "pools_per_group").to_string()
                            on:change=move |ev| send.with_value(|f| f(json!({ "pools_per_group": event_target_value(&ev).parse::<u64>().unwrap_or(2) }))) />
                    </label>
                </div>
                <div>
                    <strong>"Groupes"</strong>
                    {radio("group_mode", "group_mode", "division", "Par division (les meilleurs dans le groupe 1)", s(&st, "group_mode"))}
                    {radio("group_mode", "group_mode", "ranking", "Au classement (serpentin, même niveau)", s(&st, "group_mode"))}
                    {radio("group_mode", "group_mode", "random", "Au hasard", s(&st, "group_mode"))}
                    <div class="inline-list">
                        "Décalage : "
                        {choices
                            .into_iter()
                            .map(|c| {
                                let code = s(&c, "code");
                                let on = swapping.contains(&code);
                                let all = swapping.clone();
                                view! {
                                    <label class="check">
                                        <input type="checkbox" checked=on disabled=locked
                                            on:change=move |ev| {
                                                let mut list = all.clone();
                                                list.retain(|x| x != &code);
                                                if event_target_checked(&ev) {
                                                    list.push(code.clone());
                                                }
                                                send.with_value(|f| f(json!({ "swapping": list })));
                                            } />
                                        {s(&c, "label")}
                                    </label>
                                }
                            })
                            .collect_view()}
                    </div>
                </div>
                <div>
                    <strong>"Poules d'un groupe"</strong>
                    {radio("pool_mode", "pool_mode", "snake", "Serpentin (1-4-5-8 / 2-3-6-7)", s(&st, "pool_mode"))}
                    {radio("pool_mode", "pool_mode", "level", "Les meilleurs dans la poule 1", s(&st, "pool_mode"))}
                </div>
                <div>
                    <strong>"Tableau (1er P1 contre dernier P2…)"</strong>
                    {radio("fenced", "fenced_places", "all", "Toutes les places tirées", s(&st, "fenced_places"))}
                    {radio("fenced", "fenced_places", "third", "3e place tirée", s(&st, "fenced_places"))}
                    {radio("fenced", "fenced_places", "none", "Ni l'une ni l'autre", s(&st, "fenced_places"))}
                </div>
                <div>
                    <strong>"Points"</strong>
                    {radio("points", "points_mode", "same", "Le même nombre pour tous", s(&st, "points_mode"))}
                    {radio("points", "points_mode", "victory", "Par victoire (poules et tableau)", s(&st, "points_mode"))}
                    {radio("points", "points_mode", "place", "Selon la place dans le groupe", s(&st, "points_mode"))}
                    <label class="inline">
                        "Points (pour tous / par victoire) "
                        <input type="number" min="0" prop:value=u(&st, "points_value").to_string()
                            on:change=move |ev| send.with_value(|f| f(json!({ "points_value": event_target_value(&ev).parse::<u64>().unwrap_or(10) }))) />
                    </label>
                    <label class="inline">
                        "Points par place "
                        <input type="text" placeholder="10, 8, 6, 5, 4, 3, 2, 1" prop:value=points_list
                            on:change=move |ev| {
                                let list: Vec<u64> = event_target_value(&ev).split([',', ';', ' ']).filter_map(|x| x.trim().parse().ok()).collect();
                                send.with_value(|f| f(json!({ "points_list": list, "points_mode": "place" })));
                            } />
                    </label>
                </div>
            </div>
        </details>
    }
}
