//! Organisation (pistes, arbitres, planning) : la logique du Marshaller
//! (`marshaller::hall::Hall`) tourne dans l'API, une salle par client, comme
//! la vue intégrée du logiciel de bureau.
//!
//! Synchronisation (`Store::sync_organisation`, appelée à chaque requête) :
//! les colis des compétitions organisées (`Contest::parcels`) alimentent la
//! salle ; les feuilles de route et les arbitres qu'elle produit sont
//! appliqués aux compétitions, qui sont enregistrées.
//!
//! ```text
//! <data>/clients/<client>/organisation/hall.json          disposition des pistes
//! <data>/clients/<client>/organisation/referees.xml       arbitres
//! <data>/clients/<client>/organisation/settings.json      options et clé
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use axum::extract::State;
use axum::Json;
use bp_net::Message;
use marshaller::assign::{AssignOptions, RefereeChoice};
use marshaller::hall::Hall;
use marshaller::model::{JobStatus, NetCommand};
use marshaller::time::{self, Span, MINUTE};
use serde_json::{json, Value};

use crate::api::{bad_request, internal, ApiError, Shared};
use crate::auth::Client;
use crate::store::Store;

/// Salle d'un client et état de la synchronisation.
pub struct Organisation {
    pub hall: Hall,
    /// Colis déjà transmis à la salle : (nom, identifiant) → message.
    inputs: HashMap<(String, u32), Message>,
    pub referees: bool,
    pub pistes: bool,
    /// Compétitions ouvertes (onglets, comme le logiciel de bureau) : ce
    /// sont elles qui sont organisées.
    pub contests: Option<Vec<String>>,
    dir: PathBuf,
}

impl Organisation {
    fn load(dir: PathBuf) -> Organisation {
        let mut hall = Hall::new(None);
        if !hall.load_file(&dir.join("hall.json")) {
            hall.default_layout();
        }
        hall.load_referee_catalog(&dir.join("referees.xml"));
        let settings: Value = std::fs::read_to_string(dir.join("settings.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let key = settings
            .get("key")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| marshaller::wifi::new_key256());
        hall.referees.cryptor_key = Some(key);
        let flag = |k: &str| settings.get(k).and_then(Value::as_bool).unwrap_or(true);
        let contests = settings.get("contests").and_then(Value::as_array).map(|l| l.iter().filter_map(|v| v.as_str().map(str::to_string)).collect());
        let o = Organisation { hall, inputs: HashMap::new(), referees: flag("referees"), pistes: flag("pistes"), contests, dir };
        o.save_settings();
        o
    }

    fn save_settings(&self) {
        let _ = std::fs::create_dir_all(&self.dir);
        let v = json!({
            "referees": self.referees,
            "pistes": self.pistes,
            "key": self.hall.referees.cryptor_key,
            "contests": self.contests,
        });
        let _ = std::fs::write(self.dir.join("settings.json"), serde_json::to_string_pretty(&v).unwrap_or_default());
    }

    fn save(&mut self) {
        let _ = std::fs::create_dir_all(&self.dir);
        if self.hall.dirty {
            let _ = self.hall.save_file(&self.dir.join("hall.json"));
        }
        let _ = self.hall.save_referee_catalog(&self.dir.join("referees.xml"));
    }
}

fn key(m: &Message) -> (String, u32) {
    (m.name().unwrap_or_default().to_string(), m.net_id())
}

impl Store {
    /// Salle du client (chargée au premier accès).
    pub fn organisation(&mut self, client: &Client) -> &mut Organisation {
        let dir = self.dir.join("clients").join(&client.id).join("organisation");
        self.organisations.entry(client.id.clone()).or_insert_with(|| Organisation::load(dir))
    }

    /// Compétitions ouvertes (onglets de l'application, comme le logiciel de
    /// bureau), encore présentes dans le club.
    pub fn organised_contests(&mut self, client: &Client) -> Vec<String> {
        let open = self.organisation(client).contests.clone().unwrap_or_default();
        open.into_iter().filter(|id| self.contests.contains_key(&(client.id.clone(), id.clone()))).collect()
    }

    /// Ouvre une compétition (onglet) : elle entre dans l'organisation.
    pub fn open_contest(&mut self, client: &Client, id: &str) {
        let org = self.organisation(client);
        let list = org.contests.get_or_insert_with(Vec::new);
        if !list.iter().any(|x| x == id) {
            list.push(id.to_string());
            org.save_settings();
        }
    }

    /// Ferme une compétition (onglet) : elle sort de l'organisation.
    pub fn close_contest(&mut self, client: &Client, id: &str) {
        let org = self.organisation(client);
        if let Some(list) = org.contests.as_mut() {
            list.retain(|x| x != id);
            org.save_settings();
        }
    }

    /// Onglets des compétitions ouvertes.
    pub fn open_tabs(&mut self, client: &Client) -> Value {
        let open = self.organised_contests(client);
        json!(open
            .iter()
            .filter_map(|id| self.contests.get(&(client.id.clone(), id.clone())).map(|c| json!({
                "id": id,
                "name": c.name,
                "detail": ([c.weapon.label(), c.gender_label(), c.category.display_image()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")),
                "color": c.color,
            })))
            .collect::<Vec<_>>())
    }

    /// Identifiants réseau des compétitions de la version web : toutes, et
    /// celles qui sont organisées (filtre des signalements des arbitres).
    pub fn organised_net_ids(&mut self, client: &Client) -> (Vec<u32>, Vec<u32>) {
        let organised = self.organised_contests(client);
        let mut all = Vec::new();
        let mut chosen = Vec::new();
        for ((c, id), contest) in self.contests.iter() {
            if c != &client.id {
                continue;
            }
            all.push(contest.net_id);
            if organised.contains(id) {
                chosen.push(contest.net_id);
            }
        }
        (all, chosen)
    }

    /// Transmet à la salle les colis des compétitions organisées, puis
    /// applique aux compétitions ce que la salle a produit.
    pub fn sync_organisation(&mut self, client: &Client) {
        self.organisation(client);
        let organised = self.organised_contests(client);
        // Colis voulus (compétitions organisées)
        let mut wanted = Vec::new();
        for ((c, id), contest) in self.contests.iter() {
            if c != &client.id || contest.schedule.is_over() || !organised.contains(id) {
                continue;
            }
            for parcel in contest.parcels() {
                let mut m = Message::new(parcel.name);
                m.set_net_id(parcel.net_id);
                m.set_fitness(1);
                for (k, v) in parcel.fields {
                    m.set_str(&k, &v);
                }
                wanted.push(m);
            }
        }
        let org = self.organisations.get_mut(&client.id).unwrap();
        let keys: Vec<_> = wanted.iter().map(key).collect();
        let gone: Vec<_> = org.inputs.keys().filter(|k| !keys.contains(k)).cloned().collect();
        let mut removed: Vec<Message> = gone.into_iter().filter_map(|k| org.inputs.remove(&k)).collect();
        // Matchs, puis lots, puis compétitions.
        removed.sort_by_key(|m| if m.is("BellePoule::Job") { 0 } else if m.is("BellePoule::Batch") { 1 } else { 2 });
        for mut m in removed {
            m.set_fitness(0);
            org.hall.on_message(&m);
        }
        for m in wanted {
            let k = key(&m);
            if org.inputs.get(&k).is_none_or(|old| old.parcel() != m.parcel()) {
                org.hall.on_message(&m);
                org.inputs.insert(k, m);
            }
        }
        // Chaque arbitre a sa clé personnelle (QR code personnel, PIN).
        if let Some(org) = self.organisations.get_mut(&client.id) {
            org.hall.ensure_referee_keys();
        }
        self.apply_outbox(client);
    }

    /// Feuilles de route, arbitres et horloge produits par la salle.
    fn apply_outbox(&mut self, client: &Client) {
        let Some(org) = self.organisations.get_mut(&client.id) else { return };
        let commands = org.hall.take_outbox();
        let _ = org.hall.take_events();
        let referees_enabled = org.referees;
        let cryptor = org.hall.referees.cryptor_key.clone().unwrap_or_default();
        let mut touched: Vec<String> = Vec::new();
        for command in commands {
            let m = match command {
                NetCommand::Spread(mut m) => {
                    m.set_fitness(1);
                    m
                }
                NetCommand::Recall(mut m) => {
                    m.set_fitness(0);
                    m
                }
                _ => continue,
            };
            if m.is("BellePoule2D::ClockOffset") {
                bp_core::bout::set_clock_offset(m.get_int("offset") as i64);
                continue;
            }
            for ((c, id), contest) in self.contests.iter_mut() {
                if c != &client.id {
                    continue;
                }
                let applied = if m.is("BellePoule2D::Roadmap") {
                    if contest.net_id != m.get_uint("competition") {
                        continue;
                    }
                    let pistes = m.get_list("pistes");
                    let pistes = if pistes.is_empty() { vec![m.get_uint("piste")] } else { pistes };
                    let (date, time) = (m.get_str("start_date"), m.get_str("start_time"));
                    contest.apply_pool_roadmap(m.get_uint("source"), m.fitness(), &pistes, date.as_deref(), time.as_deref(), &m.get_list("referees"))
                        || contest.apply_table_roadmap(
                            m.get_uint("batch"),
                            m.get_uint("source"),
                            m.fitness(),
                            m.get_uint("piste"),
                            date.as_deref(),
                            time.as_deref(),
                            m.get_uint("duration_span"),
                            &m.get_list("referees"),
                        )
                } else if m.is("BellePoule2D::Referee") && referees_enabled {
                    let Some(xml) = m.get_str("xml") else { continue };
                    if m.fitness() == 0 {
                        let Ok(doc) = roxmltree::Document::parse(&xml) else { continue };
                        let reference = doc.root_element().attribute("REF").and_then(|r| r.trim().parse::<u32>().ok()).unwrap_or(0);
                        let exists = contest.referees.get(reference).is_some();
                        if exists {
                            contest.referees.remove(reference);
                        }
                        exists
                    } else {
                        let decrypt = |cypher: &str| marshaller::crypto::decrypt(cypher, &marshaller::crypto::zero_iv_b64(), &cryptor);
                        contest.manage_referee_xml(&xml, &decrypt)
                    }
                } else {
                    false
                };
                if applied && !touched.contains(id) {
                    touched.push(id.clone());
                }
            }
        }
        for id in touched {
            let _ = self.save(client, &id);
        }
        if let Some(org) = self.organisations.get_mut(&client.id) {
            org.save();
        }
    }
}

// --------------------------------------------------------------------- vue

fn hh_mm(t: time::Time) -> String {
    time::hh_mm(t)
}

/// État complet de l'organisation pour l'écran.
fn view(org: &Organisation) -> Value {
    let hall = &org.hall;
    let cursor = hall.timeline.cursor_time(false);
    let now = hall.clock.now();
    let piste_id = |k| hall.piste(k).map(|p| p.id).unwrap_or(0);

    // Pistes : ce qui s'y passe à l'heure du curseur.
    let pistes: Vec<Value> = hall
        .pistes
        .iter()
        .map(|p| {
            let b = p.bounds();
            let slot = hall.piste_slot_at(p.key, cursor).and_then(|sk| hall.slots.get(&sk).map(|s| (sk, s)));
            let current = slot.map(|(sk, s)| {
                let jobs: Vec<&marshaller::model::Job> = s.jobs.iter().filter_map(|j| hall.jobs.get(j)).collect();
                let batch = jobs.first().and_then(|j| hall.batch(j.competition, j.batch)).map(|b| b.name.clone()).unwrap_or_default();
                let elapsed = time::diff(now, s.start);
                json!({
                    "jobs": jobs.iter().map(|j| json!({ "key": j.key, "name": j.name })).collect::<Vec<_>>(),
                    "batch": batch,
                    "color": jobs.first().map(|j| j.color.to_css()),
                    "referees": s.referees.iter().filter_map(|r| hall.referees.get(*r)).map(|r| format!("{} {}", r.player.name(), r.player.first_name())).collect::<Vec<_>>(),
                    "start": hh_mm(s.start),
                    "end": hh_mm(time::add(s.start, s.duration)),
                    "progress": if elapsed > 0 && s.duration > 0 { (elapsed as f64 / s.duration as f64).min(1.0) } else { 0.0 },
                    "over": hall.slot_is_over(sk),
                })
            });
            json!({
                "key": p.key,
                "id": p.id,
                "x": b.x1,
                "y": b.y1,
                "w": b.x2 - b.x1,
                "h": b.y2 - b.y1,
                "horizontal": p.horizontal,
                "blocked": p.blocked,
                "slot": current,
            })
        })
        .collect();

    // Compétitions et lots
    let competitions: Vec<Value> = hall
        .timeline
        .competitions
        .iter()
        .filter_map(|cid| hall.competition(*cid))
        .map(|c| {
            json!({
                "id": c.id,
                "title": format!("{} · {} · {}", c.weapon_label, c.gender, c.category),
                "weapon": c.weapon,
                "color": c.color.to_css(),
                "frozen": c.frozen,
                "batches": c.batches.iter().filter(|b| b.panel.visible).map(|b| json!({
                    "id": b.id,
                    "name": b.name,
                    "expected": b.panel.expected,
                    "ready": b.panel.ready,
                    "complete": b.status == marshaller::model::BatchStatus::Complete,
                    "can_assign": b.assign_sensitive,
                    "can_cancel": b.cancel_sensitive,
                    "extension1": b.extension1,
                    "extension2": b.extension2,
                    "extension": b.extension_choice,
                    // Entraînement des jeunes : groupes des poules du lot.
                    "groups": hall.batch_groups(c.id, b.id),
                    // Arbitres libres pour chaque groupe (les autres ne sont pas proposés).
                    "group_referees": hall.batch_groups(c.id, b.id).into_iter().map(|g| json!({
                        "group": g,
                        "referees": hall.referees.referees.keys().copied().filter(|r| hall.group_referee_available(c.id, b.id, g, *r)).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                    "jobs": b.rows.iter().filter_map(|row| hall.jobs.get(&row.job).map(|j| {
                        let slot = j.slot.and_then(|s| hall.slots.get(&s));
                        json!({
                            "key": j.key,
                            "name": j.name,
                            "pool": j.is_pool,
                            "group": j.group,
                            "placed": slot.is_some(),
                            "referee_keys": slot.map(|s| s.referees.clone()).unwrap_or_default(),
                            // Arbitres libres pendant ce match.
                            "available_referees": hall.referees.referees.keys().copied().filter(|r| hall.pool_referee_available(j.key, *r)).collect::<Vec<_>>(),
                            "status": match row.status { JobStatus::None => "ok", JobStatus::Question => "pending", JobStatus::Error => "error" },
                            "warning": row.warning.map(|w| w.to_css()),
                            "pistes": slot.map(|s| s.pistes().into_iter().map(piste_id).collect::<Vec<_>>()).unwrap_or_default(),
                            "piste_keys": slot.map(|s| s.pistes()).unwrap_or_default(),
                            "start": slot.map(|s| hh_mm(s.start)),
                            "referees": slot.map(|s| s.referees.iter().filter_map(|r| hall.referees.get(*r)).map(|r| r.player.name()).collect::<Vec<_>>()).unwrap_or_default(),
                            "fencers": j.fencers.iter().map(|f| json!({
                                "name": format!("{} {}", f.player.name(), f.player.first_name()),
                                "club": f.player.str_attr("club").unwrap_or_default(),
                            })).collect::<Vec<_>>(),
                        })
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    // Arbitres par arme
    let referees: Vec<Value> = hall
        .referees
        .lists
        .iter()
        .enumerate()
        .map(|(i, list)| {
            let (present, absent) = hall.referees.tally(i);
            json!({
                "weapon": list.weapon.xml,
                "label": bp_core::i18n::tr(list.weapon.image),
                "present": present,
                "absent": absent,
                "members": list.members.iter().filter_map(|r| hall.referees.get(*r).map(|x| {
                    let p = &x.player;
                    let get = |k: &str| p.attr(k).map(|v| v.xml_image()).unwrap_or_default();
                    json!({
                        "reference": r,
                        "name": p.name(),
                        "first_name": p.first_name(),
                        "club": get("club"),
                        "league": get("league"),
                        "country": get("country"),
                        "licence": get("licence"),
                        "level": get("level"),
                        "attending": p.attending(),
                        "available": hall.referees.available_for_weapon(*r, i),
                        "workload": p.attr("workload_rate").map(|v| v.as_uint()).unwrap_or(0),
                        "busy": x.cell_color.map(|c| c.to_css()),
                    })
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    // Frise : matchs planifiés par compétition
    let origin = hall.timeline.origin;
    let timeline: Vec<Value> = hall
        .timeline
        .competitions
        .iter()
        .enumerate()
        .filter_map(|(row, cid)| hall.competition(*cid).map(|c| (row, c)))
        .flat_map(|(row, c)| {
            c.batches
                .iter()
                .flat_map(|b| b.scheduled.iter())
                .filter_map(|j| hall.jobs.get(j).and_then(|j| j.slot).and_then(|s| hall.slots.get(&s)))
                .map(move |s| json!({
                    "row": row,
                    "color": c.color.to_css(),
                    "start": time::diff(s.start, origin) / MINUTE,
                    "duration": s.duration / MINUTE,
                }))
                .collect::<Vec<_>>()
        })
        .collect();

    json!({
        "referees_enabled": org.referees,
        "pistes_enabled": org.pistes,
        "clock": hh_mm(now),
        "cursor": hh_mm(cursor),
        "cursor_minutes": hall.timeline.cursor / MINUTE,
        "now_minutes": time::diff(now, origin) / MINUTE,
        "origin": hh_mm(origin),
        "span_minutes": (marshaller::model::HOURS_MONITORED * time::HOUR + marshaller::model::START_MARGIN) / MINUTE,
        "timeline": timeline,
        "pistes": pistes,
        "competitions": competitions,
        "referees": referees,
        "warnings": {
            "club": hall.affinities.validity[0],
            "league": hall.affinities.validity[1],
            "country": hall.affinities.validity[2],
        },
        "colors": {
            "club": hall.affinities.colors[0].to_css(),
            "league": hall.affinities.colors[1].to_css(),
            "country": hall.affinities.colors[2].to_css(),
        },
        "overlaps": hall.overlaps.iter().map(|o| json!({ "message": o.message, "color": o.color.to_css() })).collect::<Vec<_>>(),
    })
}

pub async fn get(State(store): State<Shared>, client: Client) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    store.sync_organisation(&client);
    Ok(Json(view(store.organisation(&client))))
}

// ------------------------------------------------------------------ actions

fn u32_of(a: &Value, k: &str) -> u32 {
    a.get(k).and_then(Value::as_u64).unwrap_or(0) as u32
}

fn list_of(a: &Value, k: &str) -> Vec<u32> {
    a.get(k).and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_u64).map(|v| v as u32).collect()).unwrap_or_default()
}

fn str_of<'a>(a: &'a Value, k: &str) -> &'a str {
    a.get(k).and_then(Value::as_str).unwrap_or("")
}

fn weapon_list(hall: &Hall, a: &Value) -> Result<usize, ApiError> {
    hall.referees.list_index(str_of(a, "weapon")).ok_or_else(|| bad_request("arme inconnue"))
}

/// Applique `f` aux pistes désignées par `pistes` (sélection temporaire).
fn with_selection(hall: &mut Hall, pistes: &[u32], f: impl FnOnce(&mut Hall)) {
    hall.cancel_selection();
    for k in pistes {
        hall.select_piste(*k);
    }
    f(hall);
    hall.cancel_selection();
}

pub async fn action(State(store): State<Shared>, client: Client, Json(a): Json<Value>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    store.sync_organisation(&client);
    let org = store.organisation(&client);
    let hall = &mut org.hall;
    match str_of(&a, "action") {
        // Options
        "features" => {
            org.referees = a.get("referees").and_then(Value::as_bool).unwrap_or(org.referees);
            org.pistes = a.get("pistes").and_then(Value::as_bool).unwrap_or(org.pistes);
            org.save_settings();
        }
        "warnings" => {
            for t in ["club", "league", "country"] {
                if let Some(v) = a.get(t).and_then(Value::as_bool) {
                    hall.affinities.set_validity(t, v);
                }
            }
            hall.on_new_warning_policy();
        }
        // Frise : heure du curseur (HH:MM) ou décalage en minutes
        "cursor" => {
            let mut t = hall.timeline.cursor_time(true);
            if let Some(delta) = a.get("delta").and_then(Value::as_i64) {
                t = time::add(t, delta * MINUTE);
            } else if let Some((h, m)) = str_of(&a, "time").split_once(':') {
                use chrono::Timelike;
                if let (Ok(h), Ok(m)) = (h.trim().parse::<u32>(), m.trim().parse::<u32>()) {
                    if let Some(x) = t.with_hour(h).and_then(|x| x.with_minute(m)) {
                        t = x;
                    }
                }
            } else if str_of(&a, "time") == "now" {
                t = Hall::round_time(hall.clock.now());
            }
            hall.timeline.set_cursor_time(t);
            hall.on_timeline_cursor_moved();
            hall.on_timeline_cursor_changed();
        }
        // Pistes
        "add_piste" => {
            hall.add_piste(0.0, 0.0);
        }
        "remove_pistes" => with_selection(hall, &list_of(&a, "pistes"), |h| h.remove_selected()),
        "rotate_pistes" => with_selection(hall, &list_of(&a, "pistes"), |h| h.rotate_selected()),
        "block_pistes" => {
            let block = a.get("value").and_then(Value::as_bool).unwrap_or(true);
            with_selection(hall, &list_of(&a, "pistes"), |h| h.block_selected(block));
        }
        "move_piste" => {
            let k = u32_of(&a, "piste");
            let (x, y) = (a.get("x").and_then(Value::as_f64).unwrap_or(0.0), a.get("y").and_then(Value::as_f64).unwrap_or(0.0));
            if let Some(b) = hall.piste(k).map(|p| p.bounds()) {
                with_selection(hall, &[k], |h| {
                    h.translate_selected(x - b.x1, y - b.y1);
                    h.align_selected_on_grid();
                });
            }
        }
        "renumber_piste" => {
            hall.renumber_piste(u32_of(&a, "piste"), u32_of(&a, "id")).map_err(bad_request)?;
        }
        // Matchs
        "drop_job" => {
            let (job, piste) = (u32_of(&a, "job"), u32_of(&a, "piste"));
            if !hall.job_drop_allowed(job, piste) {
                return Err(bad_request("cette piste n'est pas libre à l'heure choisie"));
            }
            hall.drop_job(job, piste);
        }
        "drop_referee" => {
            let (r, piste) = (u32_of(&a, "referee"), u32_of(&a, "piste"));
            if !hall.referee_drop_allowed(r, piste) {
                return Err(bad_request("arbitre indisponible ou aucun match sur cette piste à l'heure choisie"));
            }
            hall.drop_referee(r, piste);
        }
        "take_off" => {
            let job = u32_of(&a, "job");
            if let Some(sk) = hall.jobs.get(&job).and_then(|j| j.slot) {
                hall.slot_remove_job(sk, job);
                hall.gc_slots();
                hall.redraw();
            }
        }
        "move_job" => {
            // `OnJobMove` puis sélection de la piste d'arrivée.
            let (job, piste) = (u32_of(&a, "job"), u32_of(&a, "piste"));
            hall.start_moving_job(job);
            hall.on_timeline_cursor_moved();
            hall.select_piste(piste);
            hall.stop_moving_job();
            hall.cancel_selection();
        }
        "set_time" => {
            // Heure saisie à la main (HH:MM), le même jour que le match.
            let job = u32_of(&a, "job");
            let (h, m) = str_of(&a, "time")
                .trim()
                .split_once(|c| c == ':' || c == 'h' || c == 'H')
                .and_then(|(h, m)| Some((h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().unwrap_or(0))))
                .filter(|(h, m)| *h < 24 && *m < 60)
                .ok_or_else(|| bad_request("heure attendue sous la forme HH:MM"))?;
            let base = hall
                .jobs
                .get(&job)
                .and_then(|j| j.slot)
                .and_then(|s| hall.slots.get(&s))
                .map(|s| s.start)
                .unwrap_or_else(|| hall.timeline.cursor_time(true));
            use chrono::Timelike;
            let start = base.with_hour(h).and_then(|t| t.with_minute(m)).and_then(|t| t.with_second(0)).ok_or_else(|| bad_request("heure invalide"))?;
            hall.set_job_start(job, start).map_err(bad_request)?;
        }
        "job_referees" => {
            hall.set_job_referees(u32_of(&a, "job"), &list_of(&a, "referees")).map_err(bad_request)?;
        }
        "place_group" => {
            hall.place_group(u32_of(&a, "competition"), u32_of(&a, "batch"), u32_of(&a, "group"), &list_of(&a, "pistes"), &list_of(&a, "referees"))
                .map_err(bad_request)?;
        }
        "pool_resources" => {
            hall.set_pool_resources_optional(u32_of(&a, "job"), &list_of(&a, "pistes"), &list_of(&a, "referees"), org.referees)
                .map_err(bad_request)?;
        }
        "assign" => {
            let referees = match str_of(&a, "referees") {
                "selected" => RefereeChoice::Selected(list_of(&a, "selected_referees")),
                "none" => RefereeChoice::None,
                _ if !org.referees => RefereeChoice::None,
                _ => RefereeChoice::All,
            };
            let pistes = list_of(&a, "pistes");
            let options = AssignOptions {
                all_jobs: a.get("all_jobs").and_then(Value::as_bool).unwrap_or(true),
                selected_jobs: list_of(&a, "jobs"),
                referees,
                all_pistes: pistes.is_empty(),
            };
            let (c, b) = (u32_of(&a, "competition"), u32_of(&a, "batch"));
            with_selection(hall, &pistes, |h| h.batch_assignment(c, b, &options));
        }
        "cancel" => hall.batch_assignment_cancel(u32_of(&a, "competition"), u32_of(&a, "batch")),
        "extension" => hall.batch_extension(u32_of(&a, "competition"), u32_of(&a, "batch"), u32_of(&a, "choice") as u8),
        "fix_overlap" => hall.fix_overlap_warning(u32_of(&a, "index") as usize),
        // Arbitres
        "referee_save" => {
            let list = weapon_list(hall, &a)?;
            let existing = a.get("reference").and_then(Value::as_u64).map(|r| r as u32);
            let mut player = existing
                .and_then(|r| hall.referees.get(r).map(|x| x.player.clone()))
                .unwrap_or_else(|| bp_core::Player::new(bp_core::PlayerClass::Referee));
            if let Some(fields) = a.get("fields").and_then(Value::as_object) {
                for (k, v) in fields {
                    if let Some(v) = v.as_str() {
                        player.set_str(&bp_core::player::AttrId::global(k), v.trim());
                    }
                }
            }
            if let Some(att) = a.get("attending").and_then(Value::as_bool) {
                player.set(&bp_core::player::AttrId::global("attending"), bp_core::attribute::Value::Bool(att));
            }
            if player.name().trim().is_empty() {
                return Err(bad_request("le nom de l'arbitre est obligatoire"));
            }
            hall.save_referee_form(list, player, existing);
        }
        "referee_attending" => hall.set_referee_attending(u32_of(&a, "referee"), a.get("value").and_then(Value::as_bool).unwrap_or(true)),
        "list_attending" => {
            let list = weapon_list(hall, &a)?;
            hall.set_list_attending(list, a.get("value").and_then(Value::as_bool).unwrap_or(true));
        }
        "referee_available" => {
            let list = weapon_list(hall, &a)?;
            hall.set_referee_availability(list, &list_of(&a, "referees"), a.get("value").and_then(Value::as_bool).unwrap_or(true));
        }
        "referee_remove" => {
            let list = weapon_list(hall, &a)?;
            hall.remove_referees(list, &list_of(&a, "referees"));
        }
        other => return Err(bad_request(format!("action inconnue : {other}"))),
    }
    store.apply_outbox(&client);
    store.sync_organisation(&client);
    Ok(Json(view(store.organisation(&client))))
}

/// `GET /api/organisation/referee-codes` : QR codes personnels et PIN des
/// arbitres (feuilles personnelles, liste des PIN).
pub async fn referee_codes(State(store): State<Shared>, client: Client) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    store.sync_organisation(&client);
    store.share_referee_keys(&client);
    // Arbitres venus d'un fichier de compétition (logiciel de bureau) : une
    // clé leur est donnée s'ils n'en ont pas.
    let missing: Vec<String> = store
        .list(&client)
        .filter(|(_, c)| c.referees.refs().iter().any(|r| c.referee_password(*r).is_none_or(|k| !bp_core::smartpoule::is_referee_key(&k))))
        .map(|(id, _)| id.clone())
        .collect();
    for id in missing {
        let _ = store.update(&client, &id, |c| c.ensure_referee_keys());
    }
    let admin = store.smartpoule_admin_key(&client);
    let mut contest_referees: Vec<Value> = Vec::new();
    for (_, c) in store.list(&client) {
        for r in c.referees.refs().to_vec() {
            let (Some(p), Some(key)) = (c.referees.get(r), c.referee_password(r)) else { continue };
            if p.name().trim().is_empty() && p.first_name().trim().is_empty() {
                continue;
            }
            // Même personne qu'un arbitre déjà listé (même clé) : une seule ligne.
            if contest_referees.iter().any(|x| x["reference"] == json!(r) || x["key"] == json!(key)) {
                continue;
            }
            contest_referees.push(json!({
                "reference": r,
                "name": p.name(),
                "first_name": p.first_name(),
                "club": p.str_attr("club").unwrap_or_default(),
                "weapon": p.str_attr("weapon").unwrap_or_default(),
                "pin": bp_core::smartpoule::referee_pin(&key),
                "key": key,
            }));
        }
    }
    let hall = &store.organisation(&client).hall;
    let mut referees: Vec<Value> = hall
        .referees
        .referees
        .iter()
        .filter_map(|(r, x)| {
            let key = hall.referee_key(*r)?;
            let p = &x.player;
            Some(json!({
                "reference": r,
                "name": p.name(),
                "first_name": p.first_name(),
                "club": p.str_attr("club").unwrap_or_default(),
                "weapon": p.str_attr("weapon").unwrap_or_default(),
                "key": key,
                "pin": bp_core::smartpoule::referee_pin(&key),
            }))
        })
        .collect();
    for r in contest_referees {
        if !referees.iter().any(|x| x["reference"] == r["reference"] || x["key"] == r["key"]) {
            referees.push(r);
        }
    }
    referees.sort_by_key(|r| format!("{} {}", r["name"].as_str().unwrap_or(""), r["first_name"].as_str().unwrap_or("")).to_uppercase());
    Ok(Json(json!({ "club": client.id, "club_name": client.name, "admin_key": admin, "referees": referees })))
}

/// `POST /api/organisation/referee-codes` `{ "reference" }` : nouveau code
/// d'un arbitre (QR code personnel et PIN) ; ses téléphones sont oubliés.
pub async fn renew_referee_code(State(store): State<Shared>, client: Client, Json(a): Json<Value>) -> Result<Json<Value>, ApiError> {
    let reference = u32_of(&a, "reference");
    {
        let mut store = store.lock().await;
        store.sync_organisation(&client);
        // Arbitre de l'organisation, ou seulement d'une compétition.
        let key = store.organisation(&client).hall.renew_referee_key(reference).unwrap_or_else(bp_core::smartpoule::new_referee_key);
        let ids: Vec<String> = store.list(&client).filter(|(_, c)| c.referees.get(reference).is_some()).map(|(id, _)| id.clone()).collect();
        for id in &ids {
            let _ = store.update(&client, id, |c| {
                if let Some(p) = c.referees.get_mut(reference) {
                    p.set_str(&bp_core::player::AttrId::global("password"), &key);
                }
            });
        }
        if ids.is_empty() && store.organisation(&client).hall.referee_key(reference).is_none() {
            return Err(bad_request("arbitre inconnu"));
        }
        store.share_referee_keys(&client);
        store.forget_referee_phones(&client.id, reference);
        store.apply_outbox(&client);
    }
    referee_codes(State(store), client).await
}

impl Store {
    /// Un arbitre d'une compétition qui porte le nom d'un arbitre de
    /// l'organisation est la même personne : il prend sa clé (un seul QR
    /// code personnel, un seul PIN).
    fn share_referee_keys(&mut self, client: &Client) {
        let normalize = |n: &str, f: &str| format!("{} {}", n.trim(), f.trim()).to_uppercase();
        let hall = &self.organisation(client).hall;
        let keys: HashMap<String, String> = hall
            .referees
            .referees
            .iter()
            .filter_map(|(r, x)| Some((normalize(&x.player.name(), &x.player.first_name()), hall.referee_key(*r)?)))
            .filter(|(name, _)| !name.trim().is_empty())
            .collect();
        let ids: Vec<String> = self
            .list(client)
            .filter(|(_, c)| c.referees.refs().iter().any(|r| c.referees.get(*r).is_some_and(|p| keys.get(&normalize(&p.name(), &p.first_name())).is_some_and(|k| c.referee_password(*r).as_ref() != Some(k)))))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let _ = self.update(client, &id, |c| {
                for r in c.referees.refs().to_vec() {
                    let Some(key) = c.referees.get(r).and_then(|p| keys.get(&normalize(&p.name(), &p.first_name()))).cloned() else { continue };
                    if let Some(p) = c.referees.get_mut(r) {
                        p.set_str(&bp_core::player::AttrId::global("password"), &key);
                    }
                }
            });
        }
    }
}

#[derive(serde::Deserialize)]
pub struct ImportQuery {
    weapon: String,
    name: String,
}

/// `POST /api/organisation/import?weapon=E&name=arbitres.fff` : fichier
/// d'arbitres (FFF ou CSV) envoyé tel quel.
pub async fn import(
    State(store): State<Shared>,
    client: Client,
    axum::extract::Query(q): axum::extract::Query<ImportQuery>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    let lower = q.name.to_ascii_lowercase();
    let ext = if lower.ends_with(".fff") {
        "fff"
    } else if lower.ends_with(".csv") || lower.ends_with(".txt") {
        "csv"
    } else {
        return Err(bad_request("format non reconnu : fichiers .fff ou .csv"));
    };
    let mut store = store.lock().await;
    store.sync_organisation(&client);
    {
        let hall = &mut store.organisation(&client).hall;
        let list = hall.referees.list_index(&q.weapon).ok_or_else(|| bad_request("arme inconnue"))?;
        let path = std::env::temp_dir().join(format!("bp-referees-{}.{ext}", uuid::Uuid::new_v4().simple()));
        std::fs::write(&path, &body).map_err(internal)?;
        let result = hall.import_referees(list, &path);
        let _ = std::fs::remove_file(&path);
        result.map_err(bad_request)?;
    }
    store.apply_outbox(&client);
    store.sync_organisation(&client);
    Ok(Json(view(store.organisation(&client))))
}

/// Durée en minutes (affichage).
#[allow(dead_code)]
fn minutes(span: Span) -> i64 {
    span / MINUTE
}

/// `GET /api/open-contests` : onglets des compétitions ouvertes.
pub async fn open_tabs(State(store): State<Shared>, client: Client) -> Json<Value> {
    let mut store = store.lock().await;
    Json(store.open_tabs(&client))
}

/// `POST /api/contests/{id}/open` : ouvre la compétition (onglet).
pub async fn open_contest(State(store): State<Shared>, client: Client, axum::extract::Path(id): axum::extract::Path<String>) -> Result<Json<Value>, ApiError> {
    let mut store = store.lock().await;
    if store.get(&client, &id).is_none() {
        return Err(crate::api::not_found());
    }
    store.open_contest(&client, &id);
    Ok(Json(store.open_tabs(&client)))
}

/// `POST /api/contests/{id}/close` : ferme la compétition (onglet).
pub async fn close_contest(State(store): State<Shared>, client: Client, axum::extract::Path(id): axum::extract::Path<String>) -> Json<Value> {
    let mut store = store.lock().await;
    store.close_contest(&client, &id);
    Json(store.open_tabs(&client))
}
