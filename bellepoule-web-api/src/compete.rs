//! Pages publiques des épreuves (compete.managecomp.org) : accueil (toutes
//! les épreuves publiées), page d'un club, page d'une épreuve (formule,
//! tireurs, poules, tableaux, classement), avec filtres par nom et par club
//! et l'encadré « Mon tireur » (`compete/app.js`).
//!
//! Pages HTML fabriquées ici, légères (téléphones, réseau d'un gymnase) ;
//! sur le serveur local elles sont sous `/compete/`.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use bp_core::contest::Contest;
use bp_core::i18n::tr;
use bp_core::player::{Player, PlayerRef, PlayerStore};
use bp_core::rounds::classification::GeneralClassificationStage;
use bp_core::rounds::pool::PoolSupervisorStage;
use bp_core::rounds::table::supervisor::TableSupervisor;

use crate::api::Shared;
use crate::public::{public_base, Publication};

const CSS: &str = include_str!("compete/style.css");
const JS: &str = include_str!("compete/app.js");
/// Version des fichiers statiques (adresse changée à chaque modification).
const ASSETS: &str = "1";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// `2026-10-18` → `18/10/2026`.
fn date_fr(iso: &str) -> String {
    let p: Vec<&str> = iso.split('-').collect();
    match p[..] {
        [y, m, d] => format!("{d}/{m}/{y}"),
        _ => iso.to_string(),
    }
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Html(layout("", "Page introuvable", true, "<main class=\"wrap\"><h1>Page introuvable</h1><p>Cette épreuve n'existe pas ou n'est plus publiée.</p></main>".into()))).into_response()
}

/// Gabarit commun.
fn layout(base: &str, title: &str, noindex: bool, body: String) -> String {
    let robots = if noindex { "<meta name=\"robots\" content=\"noindex\">" } else { "" };
    format!(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">{robots}\
         <title>{t}</title><link rel=\"stylesheet\" href=\"{base}style.css?v={ASSETS}\"><script src=\"{base}app.js?v={ASSETS}\" defer></script></head>\
         <body><header class=\"top\"><a class=\"brand\" href=\"{base}\"><b>Manage</b>Comp <span>résultats</span></a></header>{body}\
         <footer class=\"foot\"><a href=\"https://managecomp.org\">ManageComp</a> · logiciel libre de gestion de compétitions d'escrime</footer></body></html>",
        t = esc(title),
    )
}

fn html(body: String) -> Response {
    ([(header::CACHE_CONTROL, "no-cache")], Html(body)).into_response()
}

// ------------------------------------------------------------ noms

/// Noms affichés : complets, ou prénom et initiale du nom (« Lina M. »).
struct Names {
    short: bool,
}

impl Names {
    fn of(&self, p: &Player) -> String {
        let (name, first) = (p.name().trim().to_string(), p.first_name().trim().to_string());
        if first.is_empty() {
            return name;
        }
        if self.short {
            let initial = name.chars().next().map(|c| format!(" {c}.")).unwrap_or_default();
            format!("{first}{initial}")
        } else {
            format!("{name} {first}")
        }
    }
    fn get(&self, players: &PlayerStore, r: PlayerRef) -> (String, String) {
        players.get(r).map(|p| (self.of(p), p.str_attr("club").unwrap_or_default().trim().to_string())).unwrap_or_default()
    }
}

/// Attributs de filtrage d'un tireur (nom affiché, club).
fn data(name: &str, club: &str) -> String {
    format!(" data-f=\"{}\" data-c=\"{}\"", esc(name), esc(club))
}

// ------------------------------------------------------------ fichiers

pub async fn style() -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")], CSS).into_response()
}

pub async fn script() -> Response {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")], JS).into_response()
}

// ------------------------------------------------------------ listes

fn cards(base: &str, list: &[Publication], with_club: bool) -> String {
    if list.is_empty() {
        return "<p class=\"muted\">Aucune épreuve publiée pour l'instant.</p>".into();
    }
    let mut out = String::from("<div class=\"cards\">");
    for p in list {
        let search = format!("{} {} {} {} {}", p.title, p.subtitle, p.club_name, p.location, date_fr(&p.date)).to_lowercase();
        out.push_str(&format!(
            "<a class=\"card\" href=\"{base}{}/{}\" data-s=\"{}\"><span class=\"date\">{}</span><strong>{}</strong><span>{}</span><span class=\"muted\">{}</span></a>",
            esc(&p.club_slug),
            esc(&p.slug),
            esc(&search),
            date_fr(&p.date),
            esc(&p.title),
            esc(&p.subtitle),
            esc(&[if with_club { p.club_name.clone() } else { String::new() }, p.location.clone()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")),
        ));
    }
    out.push_str("</div>");
    out
}

fn listed(mut list: Vec<Publication>) -> Vec<Publication> {
    list.retain(|p| !p.noindex);
    list.sort_by(|a, b| b.date.cmp(&a.date).then(b.updated.cmp(&a.updated)));
    list
}

/// Accueil : toutes les épreuves publiées (sauf celles cachées).
pub async fn home(State(store): State<Shared>, headers: HeaderMap) -> Response {
    let base = public_base(&headers);
    let list = listed(store.lock().await.publications().clone());
    let body = format!(
        "<main class=\"wrap\"><h1>Compétitions</h1><input class=\"search\" id=\"cards-q\" type=\"search\" placeholder=\"Rechercher une épreuve, un club, une ville…\">{}</main>",
        cards(&base, &list, true)
    );
    html(layout(&base, "Compétitions — ManageComp", false, body))
}

/// Page d'un club : ses épreuves publiées.
pub async fn club(State(store): State<Shared>, headers: HeaderMap, Path(club): Path<String>) -> Response {
    let base = public_base(&headers);
    let list = listed(store.lock().await.publications().iter().filter(|p| p.club_slug == club).cloned().collect());
    let Some(name) = list.first().map(|p| p.club_name.clone()) else { return not_found() };
    let body = format!(
        "<main class=\"wrap\"><p class=\"crumbs\"><a href=\"{base}\">Compétitions</a></p><h1>{}</h1><input class=\"search\" id=\"cards-q\" type=\"search\" placeholder=\"Rechercher une épreuve…\">{}</main>",
        esc(&name),
        cards(&base, &list, false)
    );
    html(layout(&base, &format!("{name} — ManageComp"), false, body))
}

/// Lien court du QR code.
pub async fn short(State(store): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let base = public_base(&headers);
    match store.lock().await.publications().iter().find(|p| p.id == id) {
        Some(p) => Redirect::to(&format!("{base}{}/{}", p.club_slug, p.slug)).into_response(),
        None => not_found(),
    }
}

// ------------------------------------------------------------ épreuve

pub async fn event(State(store): State<Shared>, headers: HeaderMap, Path((club, slug)): Path<(String, String)>) -> Response {
    let base = public_base(&headers);
    let (p, xml) = {
        let mut store = store.lock().await;
        let Some(p) = store.publications().iter().find(|p| p.club_slug == club && p.slug == slug).cloned() else { return not_found() };
        let Ok(xml) = std::fs::read_to_string(store.publication_file(&p.id)) else { return not_found() };
        (p, xml)
    };
    let mut c = Contest::new(None, false);
    if c.load_xml_str(&xml).is_err() {
        return not_found();
    }
    let names = Names { short: p.names == "short" };
    let body = event_body(&base, &p, &mut c, &names);
    html(layout(&base, &format!("{} — {}", p.title, p.subtitle), p.noindex, body))
}

fn event_body(base: &str, p: &Publication, c: &mut Contest, names: &Names) -> String {
    let current = c.schedule.current_index();
    let over = c.schedule.is_over();
    let sections: Vec<(&str, &str, String)> = vec![
        ("formule", "Formule", formula(c, current, over)),
        ("tireurs", "Tireurs", fencers(c, names)),
        ("poules", "Poules", pools(c, names)),
        ("tableaux", "Tableaux", tables(c, names)),
        ("classement", "Classement", ranking(c, names)),
    ];
    let sections: Vec<_> = sections.into_iter().filter(|(_, _, s)| !s.is_empty()).collect();
    // Clubs des tireurs (filtre).
    let mut clubs: Vec<String> = c
        .schedule
        .checkin()
        .map(|ch| ch.fencers(&c.players).into_iter().filter_map(|r| c.players.get(r).and_then(|x| x.str_attr("club").map(|s| s.trim().to_string()))).collect())
        .unwrap_or_default();
    clubs.retain(|s| !s.is_empty());
    clubs.sort();
    clubs.dedup();
    let tabs: String = sections.iter().map(|(id, title, _)| format!("<a href=\"#{id}\" data-tab=\"{id}\">{title}</a>")).collect();
    let content: String = sections.iter().map(|(id, title, s)| format!("<section id=\"{id}\" class=\"tab\"><h2>{title}</h2>{s}</section>")).collect();
    let meta = [date_fr(&p.date), p.location.clone()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
    format!(
        "<main class=\"wrap event\" data-live=\"{live}\" data-updated=\"{updated}\" data-event=\"{id}\">\
         <p class=\"crumbs\"><a href=\"{base}\">Compétitions</a> › <a href=\"{base}{club_slug}\">{club}</a></p>\
         <h1>{title}</h1><p class=\"sub\">{sub}</p><p class=\"muted\">{meta} <span id=\"updated\"></span></p>\
         <div class=\"filters\"><input id=\"q\" type=\"search\" placeholder=\"Rechercher un tireur…\" autocomplete=\"off\">\
         <select id=\"club\"><option value=\"\">Tous les clubs</option>{club_options}</select></div>\
         <div id=\"mine\"></div>\
         <nav class=\"tabs\">{tabs}</nav><div id=\"content\">{content}</div></main>",
        live = if over { 0 } else { 1 },
        updated = p.updated,
        id = esc(&p.id),
        club_slug = esc(&p.club_slug),
        club = esc(&p.club_name),
        title = esc(&p.title),
        sub = esc(&p.subtitle),
        meta = esc(&meta),
        club_options = clubs.iter().map(|c| format!("<option>{}</option>", esc(c))).collect::<String>(),
    )
}

/// Formule : phases, faites, en cours, à venir.
fn formula(c: &Contest, current: usize, over: bool) -> String {
    let mut out = String::from("<ol class=\"formula\">");
    for i in 0..c.schedule.len() {
        let Some(s) = c.schedule.stage(i) else { continue };
        let state = if over || i < current { "done" } else if i == current { "current" } else { "next" };
        let label = format!("{} {}", tr(s.kind().class_name()), s.name()).trim().to_string();
        out.push_str(&format!("<li class=\"{state}\">{}</li>", esc(&label)));
    }
    out.push_str("</ol>");
    out
}

/// Tireurs présents.
fn fencers(c: &Contest, names: &Names) -> String {
    let Some(ch) = c.schedule.checkin() else { return String::new() };
    let mut list: Vec<(String, String)> = ch
        .fencers(&c.players)
        .into_iter()
        .filter(|r| c.players.get(*r).is_some_and(|p| p.attending()))
        .map(|r| names.get(&c.players, r))
        .collect();
    if list.is_empty() {
        return String::new();
    }
    list.sort_by_key(|(n, _)| n.to_uppercase());
    let rows: String = list.iter().map(|(n, club)| format!("<tr{}><td class=\"name\">{}</td><td class=\"muted\">{}</td></tr>", data(n, club), esc(n), esc(club))).collect();
    format!("<p class=\"muted\">{} présents</p><table class=\"list\"><tbody>{rows}</tbody></table>", list.len())
}

/// Poules de chaque tour : grille, piste, heure, arbitre.
fn pools(c: &Contest, names: &Names) -> String {
    let mut out = String::new();
    let rounds: Vec<usize> = (0..c.schedule.len()).filter(|i| c.schedule.stage(*i).is_some_and(|s| s.as_any().downcast_ref::<PoolSupervisorStage>().is_some())).collect();
    for i in &rounds {
        let Some(stage) = c.schedule.stage(*i).and_then(|s| s.as_any().downcast_ref::<PoolSupervisorStage>()) else { continue };
        let round = stage.round();
        if round.pools().is_empty() {
            continue;
        }
        if rounds.len() > 1 {
            out.push_str(&format!("<h3>{}</h3>", esc(&round.name)));
        }
        for pool in round.pools() {
            let sheet = pool.sheet(&c.players, c.referees.store(), false);
            let n = sheet.rows.len();
            let piste = pool.piste_label().unwrap_or_default();
            let time = pool.start_time_image().unwrap_or_default();
            let referee = sheet.referee.as_ref().map(|(n, f)| format!("{n} {}", f.clone().unwrap_or_default()).trim().to_string()).unwrap_or_default();
            let info = [
                (!piste.is_empty()).then(|| format!("Piste {piste}")),
                (!time.is_empty()).then(|| time.clone()),
                (!referee.is_empty()).then(|| format!("Arbitre : {referee}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            let head: String = (1..=n).map(|j| format!("<th>{j}</th>")).collect();
            let mut rows = String::new();
            for row in &sheet.rows {
                let (name, club) = names.get(&c.players, row.player);
                let cells: String = row
                    .cells
                    .iter()
                    .map(|cell| {
                        let class = if cell.grey { " class=\"grey\"" } else if cell.text.starts_with('V') { " class=\"win\"" } else { "" };
                        format!("<td{class}>{}</td>", if cell.grey { String::new() } else { esc(&cell.text) })
                    })
                    .collect();
                let d = row.dashboard.as_ref();
                let dash = |f: fn(&bp_core::rounds::pool::pool::Dashboard) -> &String| d.map(|d| esc(f(d))).unwrap_or_default();
                rows.push_str(&format!(
                    "<tr{}><td class=\"name\">{}<small>{}</small></td><td class=\"pos\">{}</td>{cells}<td class=\"res\">{}</td><td class=\"res\">{}</td><td class=\"res\"><b>{}</b></td></tr>",
                    data(&name, &club),
                    esc(&name),
                    esc(&club),
                    row.position,
                    dash(|d| &d.victories),
                    dash(|d| &d.indice),
                    dash(|d| &d.rank),
                ));
            }
            out.push_str(&format!(
                "<div class=\"pool\" data-title=\"{t}\" data-piste=\"{p}\" data-time=\"{h}\"><h4>{t}{over}</h4><p class=\"muted\">{info}</p>\
                 <div class=\"scroll\"><table class=\"grid\"><thead><tr><th></th><th></th>{head}<th>V</th><th>Ind</th><th>Pl</th></tr></thead><tbody>{rows}</tbody></table></div></div>",
                t = esc(pool.name()),
                p = esc(&piste),
                h = esc(&time),
                over = if pool.is_over() { " <span class=\"ok\">✔</span>" } else { "" },
                info = esc(&info),
            ));
        }
    }
    out
}

/// Tableaux, tour par tour : les matchs avec piste, heure et score.
fn tables(c: &Contest, names: &Names) -> String {
    let mut out = String::new();
    for i in 0..c.schedule.len() {
        let Some(sup) = c.schedule.stage(i).and_then(|s| s.as_any().downcast_ref::<TableSupervisor>()) else { continue };
        for set in sup.table_sets().iter().filter(|s| s.is_visible()) {
            let mut set_html = String::new();
            for table in set.tables() {
                let mut bouts = String::new();
                for node in table.matches() {
                    let Some(m) = set.bout(*node) else { continue };
                    if m.opponent(0).is_none() && m.opponent(1).is_none() {
                        continue;
                    }
                    let winner = m.winner();
                    let mut lines = String::new();
                    let mut keys = Vec::new();
                    let mut clubs = Vec::new();
                    for k in 0..2 {
                        let Some(f) = m.opponent(k) else {
                            lines.push_str("<div class=\"line empty\">—</div>");
                            continue;
                        };
                        let (name, club) = names.get(&c.players, f);
                        let score = if m.is_over() || m.score(f).is_some_and(|s| !s.image(m.max_score).is_empty()) { m.score_at(k).image(m.max_score) } else { String::new() };
                        lines.push_str(&format!(
                            "<div class=\"line{}\"><span class=\"who\">{}<small>{}</small></span><span class=\"score\">{}</span></div>",
                            if winner == Some(f) { " won" } else { "" },
                            esc(&name),
                            esc(&club),
                            esc(&score)
                        ));
                        keys.push(name);
                        clubs.push(club);
                    }
                    let piste = if m.piste != 0 { format!("{:02}", m.piste) } else { String::new() };
                    let time = m.start_time.map(|t| t.0.format("%H:%M").to_string()).unwrap_or_default();
                    let info = [
                        (m.number != 0).then(|| format!("Match {}", m.number)),
                        (!piste.is_empty()).then(|| format!("Piste {piste}")),
                        (!time.is_empty()).then(|| time.clone()),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");
                    bouts.push_str(&format!(
                        "<div class=\"bout\" data-f=\"{}\" data-c=\"{}\" data-over=\"{}\" data-piste=\"{}\" data-time=\"{}\" data-table=\"{}\"><div class=\"info muted\">{}</div>{lines}</div>",
                        esc(&keys.join("|")),
                        esc(&clubs.join("|")),
                        if m.is_over() { 1 } else { 0 },
                        esc(&piste),
                        esc(&time),
                        esc(&table.image()),
                        esc(&info)
                    ));
                }
                if !bouts.is_empty() {
                    set_html.push_str(&format!("<h4>{}</h4><div class=\"bouts\">{bouts}</div>", esc(&table.image())));
                }
            }
            if !set_html.is_empty() {
                out.push_str(&format!("<h3>{}</h3>{set_html}", esc(&set.name())));
            }
        }
    }
    out
}

/// Classement final (phase « Classement général »).
fn ranking(c: &mut Contest, names: &Names) -> String {
    let index = (0..c.schedule.len()).rev().find(|i| c.schedule.stage(*i).is_some_and(|s| s.as_any().downcast_ref::<GeneralClassificationStage>().is_some()));
    let Some(index) = index else { return String::new() };
    if index > c.schedule.current_index() {
        return "<p class=\"muted\">Le classement s'affichera à la fin de la compétition.</p>".into();
    }
    let Some(view) = crate::stages::general_list(c, index) else { return String::new() };
    let rank_col = view.columns.iter().position(|col| col.code.contains("rank"));
    let club_col = view.columns.iter().position(|col| col.code == "club");
    let points_col = view.columns.iter().position(|col| col.code == "points");
    let mut rows = String::new();
    for row in &view.rows {
        let (name, club) = names.get(&c.players, row.reference);
        let rank = rank_col.and_then(|i| row.cells.get(i)).cloned().unwrap_or_default();
        let club = club_col.and_then(|i| row.cells.get(i)).cloned().unwrap_or(club);
        let points = points_col.and_then(|i| row.cells.get(i)).map(|p| format!("<td class=\"res\">{}</td>", esc(p))).unwrap_or_default();
        rows.push_str(&format!("<tr{}><td class=\"rank\">{}</td><td class=\"name\">{}</td><td class=\"muted\">{}</td>{points}</tr>", data(&name, &club), esc(&rank), esc(&name), esc(&club)));
    }
    let points_head = if points_col.is_some() { "<th>Points</th>" } else { "" };
    format!("<table class=\"list\"><thead><tr><th>Pl</th><th>Tireur</th><th>Club</th>{points_head}</tr></thead><tbody>{rows}</tbody></table>")
}
