//! Feuille de match d'une rencontre par équipes en relais (modèle « par
//! équipes 45T ») : en-tête, composition des deux équipes, une ligne par
//! relais (numéros, nom, cartons, score cumulé, touches), échelle des
//! touches au centre et temps de chaque relais.
//!
//! La mise en page est calculée ici, en éléments simples (rectangles et
//! textes) dans un repère de 918 × 1300 (page A4 portrait), pour être
//! dessinée à l'identique par le logiciel (Cairo) et par la version web (SVG).

use serde::Serialize;

use crate::relay::RelayFormat;

/// Largeur et hauteur du repère de la feuille.
pub const WIDTH: f64 = 918.0;
pub const HEIGHT: f64 = 1300.0;

/// Élément de la feuille.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Shape {
    Rect { x: f64, y: f64, w: f64, h: f64, stroke: Option<String>, fill: Option<String>, line: f64 },
    Text { x: f64, y: f64, size: f64, bold: bool, color: String, text: String, anchor: &'static str },
    Line { x1: f64, y1: f64, x2: f64, y2: f64, color: String, line: f64 },
}

/// Ce qui est connu avant la rencontre (tout peut rester vide : feuille vierge).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RelaySheetInfo {
    pub contest: String,
    pub date: String,
    pub time: String,
    pub round: String,
    pub bout: String,
    pub piste: String,
    pub referee: String,
    pub teams: [String; 2],
    /// Exemplaire (« Équipe A », « Équipe B », « Arbitre ») ou vide.
    pub copy: String,
    /// Rencontre commencée sur SmartPoule : équipe du haut, tireurs et
    /// relais joués ; la feuille est alors imprimée remplie.
    #[serde(skip)]
    pub log: Option<crate::relay::RelayLog>,
}

/// Couleur du n-ième tireur d'une équipe (vert, bleu, saumon comme le modèle).
pub fn number_color(index: usize) -> &'static str {
    const COLORS: [&str; 8] = ["#35935f", "#3366ff", "#ff8080", "#9b59b6", "#e67e22", "#16a085", "#c0392b", "#7f8c8d"];
    COLORS[index % COLORS.len()]
}

struct Sheet {
    shapes: Vec<Shape>,
}

impl Sheet {
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, stroke: Option<&str>, fill: Option<&str>, line: f64) {
        self.shapes.push(Shape::Rect { x, y, w, h, stroke: stroke.map(str::to_string), fill: fill.map(str::to_string), line });
    }
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: &str, line: f64) {
        self.shapes.push(Shape::Line { x1, y1, x2, y2, color: color.to_string(), line });
    }
    fn text(&mut self, x: f64, y: f64, size: f64, bold: bool, color: &str, text: &str, anchor: &'static str) {
        if !text.is_empty() {
            self.shapes.push(Shape::Text { x, y, size, bold, color: color.to_string(), text: text.to_string(), anchor });
        }
    }
}

/// Mise en page d'une feuille.
pub fn layout(format: &RelayFormat, info: &RelaySheetInfo) -> Vec<Shape> {
    let mut s = Sheet { shapes: Vec::new() };
    let ink = "#1a1a1a";
    let grey = "#6b6b6b";
    let light = "#d9d9d9";
    let m = 24.0;
    let right = WIDTH - m;

    // ---------------------------------------------------------- en-tête
    s.text(m, 40.0, 20.0, true, ink, "FEUILLE DE MATCH PAR ÉQUIPES", "start");
    if !info.copy.is_empty() {
        s.rect(right - 150.0, 20.0, 150.0, 28.0, Some(ink), Some("#f2f2f2"), 1.2);
        s.text(right - 75.0, 40.0, 14.0, true, ink, &info.copy, "middle");
    }
    s.line(m, 54.0, right, 54.0, ink, 2.0);
    // Champs : libellé, valeur connue, ligne pour écrire.
    let field = |s: &mut Sheet, x: f64, y: f64, w: f64, label: &str, value: &str| {
        s.text(x, y, 12.0, true, grey, label, "start");
        let lx = x + 8.0 + label.chars().count() as f64 * 7.0;
        s.line(lx, y + 3.0, x + w, y + 3.0, light, 1.0);
        s.text(lx + 4.0, y - 1.0, 13.0, false, ink, value, "start");
    };
    let round = [info.round.as_str(), info.bout.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" · ");
    field(&mut s, m, 80.0, 420.0, "Épreuve", &info.contest);
    field(&mut s, 470.0, 80.0, 200.0, "Date", &info.date);
    field(&mut s, 690.0, 80.0, right - 690.0, "Heure", &info.time);
    field(&mut s, m, 108.0, 420.0, "Tour", &round);
    field(&mut s, 470.0, 108.0, 200.0, "Piste", &info.piste);
    field(&mut s, 690.0, 108.0, right - 690.0, "Arbitre", &info.referee);
    if !info.teams[0].is_empty() || !info.teams[1].is_empty() {
        s.text(WIDTH / 2.0, 138.0, 15.0, true, ink, &format!("{}   contre   {}", info.teams[0], info.teams[1]), "middle");
    }

    // ---------------------------------------------------- composition
    // Les équipes choisissent leur place (haut ou bas) au début de la
    // rencontre : rien n'est prérempli, sauf si l'arbitre l'a déjà saisi.
    let numbers = format.numbers();
    let log = info.log.as_ref().filter(|l| l.format == *format);
    let placed_team = |side: usize| -> Option<String> {
        let l = log?;
        let team = if side == 0 { l.top as usize } else { 1 - l.top as usize };
        Some(info.teams[team].clone()).filter(|t| !t.is_empty())
    };
    let player = |n: u32| -> String { log.and_then(|l| l.lineup.get(n as usize - 1).cloned()).unwrap_or_default() };
    // Touches de chaque relais joué : [haut, bas].
    let played: Vec<[u32; 2]> = log
        .map(|l| {
            let mut prev = [0u32, 0u32];
            l.relays.iter().map(|r| {
                let d = [r[0].saturating_sub(prev[0]), r[1].saturating_sub(prev[1])];
                prev = *r;
                d
            }).collect()
        })
        .unwrap_or_default();
    // Bilan d'un tireur : touches données moins touches reçues sur ses relais.
    let bilan = |n: u32| -> Option<i64> {
        let mut total = None;
        for ((a, b), d) in format.bouts().iter().zip(played.iter()) {
            if *a == n {
                *total.get_or_insert(0) += d[0] as i64 - d[1] as i64;
            } else if *b == n {
                *total.get_or_insert(0) += d[1] as i64 - d[0] as i64;
            }
        }
        total
    };
    let rows = numbers[0].len().max(1);
    let lines = rows + format.substitutes as usize;
    let row_h = (150.0 / lines.max(1) as f64).clamp(16.0, 26.0);
    let comp_top = 152.0;
    let panel_w = (right - m - 24.0) / 2.0;
    for (k, x) in [(0usize, m), (1usize, m + panel_w + 24.0)] {
        let title = format!("{} ({})", if k == 0 { "HAUT" } else { "BAS" }, numbers[k].iter().map(u32::to_string).collect::<Vec<_>>().join("-"));
        s.text(x, comp_top + 14.0, 13.0, true, ink, &title, "start");
        s.text(x + 128.0, comp_top + 14.0, 12.0, true, grey, "Équipe", "start");
        s.line(x + 180.0, comp_top + 17.0, x + panel_w, comp_top + 17.0, ink, 1.0);
        if let Some(team) = placed_team(k) {
            s.text(x + 184.0, comp_top + 14.0, 13.0, true, ink, &team, "start");
        }
        let top = comp_top + 26.0;
        s.text(x + panel_w - 24.0, top - 3.0, 9.0, false, grey, "bilan", "middle");
        let all: Vec<(String, &str)> = numbers[k]
            .iter()
            .enumerate()
            .map(|(i, n)| (n.to_string(), number_color(i)))
            .chain((0..format.substitutes).map(|i| (if format.substitutes > 1 { format!("R{}", i + 1) } else { "R".to_string() }, grey)))
            .collect();
        for (i, (n, color)) in all.iter().enumerate() {
            let y = top + i as f64 * row_h;
            s.rect(x, y, 30.0, row_h, Some(ink), Some(color), 1.0);
            s.text(x + 15.0, y + row_h * 0.72, (row_h * 0.6).min(14.0), true, "#ffffff", n, "middle");
            s.rect(x + 30.0, y, panel_w - 78.0, row_h, Some(ink), None, 1.0);
            s.rect(x + panel_w - 48.0, y, 48.0, row_h, Some(ink), None, 1.0);
            if let Ok(number) = n.parse::<u32>() {
                s.text(x + 36.0, y + row_h * 0.7, (row_h * 0.55).min(13.0), false, ink, &player(number), "start");
                if let Some(b) = bilan(number) {
                    s.text(x + panel_w - 24.0, y + row_h * 0.7, (row_h * 0.55).min(13.0), true, ink, &format!("{b:+}"), "middle");
                }
            }
        }
    }

    // ------------------------------------------------------------ relais
    // Colonnes : relais | tireur (numéro, nom) | cartons | score | touches |
    // échelle | touches | score | cartons | tireur | temps.
    let bouts = format.bouts();
    let heads = comp_top + 26.0 + lines as f64 * row_h + 26.0;
    let top = heads + 10.0;
    let bottom = HEIGHT - 44.0;
    let pitch = ((bottom - top) / bouts.len().max(1) as f64).min(96.0);
    let box_h = pitch - (pitch * 0.18).clamp(4.0, 14.0);
    let (lx_name, lw_name) = (56.0, 226.0);
    let lx_cards = 288.0;
    let (lx_score, lx_touch, bw) = (306.0, 358.0, 46.0);
    let (ladder_x, ladder_w) = (414.0, 90.0);
    let (rx_touch, rx_score) = (514.0, 566.0);
    let rx_cards = 618.0;
    let (rx_name, rw_name) = (636.0, 206.0);
    let time_x = 848.0;
    for (x, t) in [
        (m + 14.0, "Relais"),
        (lx_name + lw_name / 2.0, "Tireur"),
        (lx_score + bw / 2.0, "Score"),
        (lx_touch + bw / 2.0, "Touches"),
        (ladder_x + ladder_w / 2.0, "Touches"),
        (rx_touch + bw / 2.0, "Touches"),
        (rx_score + bw / 2.0, "Score"),
        (rx_name + rw_name / 2.0, "Tireur"),
        (time_x + 23.0, "Temps"),
    ] {
        s.text(x, heads, 11.0, true, grey, t, "middle");
    }
    let index_of = |n: u32, k: usize| numbers[k].iter().position(|x| *x == n).unwrap_or(0);
    let card = ((box_h - 6.0) / 3.0).clamp(5.0, 13.0);
    for (r, (a, b)) in bouts.iter().enumerate() {
        let y = top + r as f64 * pitch;
        let mid = y + box_h / 2.0;
        s.text(m + 14.0, mid + 5.0, 13.0, true, ink, &(r + 1).to_string(), "middle");
        for (k, n, x_name, w_name, x_cards, x_score, x_touch) in [
            (0usize, *a, lx_name, lw_name, lx_cards, lx_score, lx_touch),
            (1usize, *b, rx_name, rw_name, rx_cards, rx_score, rx_touch),
        ] {
            let color = number_color(index_of(n, k));
            // Numéro en couleur, nom à écrire sur fond blanc.
            s.rect(x_name, y, 30.0, box_h, Some(color), Some(color), 1.5);
            s.text(x_name + 15.0, mid + 6.0, (box_h * 0.42).min(18.0), true, "#ffffff", &n.to_string(), "middle");
            s.rect(x_name + 30.0, y, w_name - 30.0, box_h, Some(color), None, 1.5);
            s.text(x_name + 38.0, mid + 5.0, (box_h * 0.3).clamp(9.0, 14.0), false, ink, &player(n), "start");
            // Cartons jaune, rouge, noir
            for (j, c) in ["#f5d90a", "#e02424", "#111111"].iter().enumerate() {
                s.rect(x_cards, y + 1.0 + j as f64 * (box_h - card - 2.0) / 2.0, 12.0, card, Some(ink), Some(c), 0.6);
            }
            // Score cumulé (objectif du relais en petit) et touches du relais
            s.rect(x_score, y, bw, box_h, Some(ink), None, 1.2);
            s.text(x_score + 4.0, y + 11.0, 9.0, true, grey, &format!("→{}", format.target(r)), "start");
            s.rect(x_touch, y, bw, box_h, Some(ink), None, 1.2);
            if let (Some(l), Some(d)) = (log, played.get(r)) {
                let size = (box_h * 0.4).clamp(9.0, 18.0);
                s.text(x_score + bw / 2.0, mid + size * 0.5, size, true, ink, &l.relays[r][k].to_string(), "middle");
                s.text(x_touch + bw / 2.0, mid + size * 0.5, size, false, ink, &d[k].to_string(), "middle");
            }
        }
        // Temps du relais
        s.rect(time_x, y, 46.0, box_h, Some(ink), Some("#ececec"), 1.2);
    }
    // Échelle des touches : chaque ligne de relais porte les touches de
    // son relais (relais 2 en relais de 5 points : 6 à 10), à sa hauteur ;
    // une case par équipe de chaque côté du numéro.
    let tick = 22.0;
    for r in 0..bouts.len() {
        let y0 = top + r as f64 * pitch;
        let points = format.points.max(1);
        let step = box_h / points as f64;
        s.rect(ladder_x, y0, ladder_w, box_h, Some(ink), None, 1.2);
        for j in 0..points {
            let y = y0 + j as f64 * step;
            let number = format.target(r) - points + j + 1;
            s.rect(ladder_x, y, tick, step, Some(ink), None, 0.6);
            s.rect(ladder_x + ladder_w - tick, y, tick, step, Some(ink), None, 0.6);
            let size = (step * 0.7).clamp(5.0, 12.0);
            s.text(ladder_x + ladder_w / 2.0, y + step / 2.0 + size * 0.36, size, number % 5 == 0, ink, &number.to_string(), "middle");
        }
    }

    // ---------------------------------------------------------- pied
    s.line(m, HEIGHT - 30.0, right, HEIGHT - 30.0, light, 1.0);
    let order = bouts.iter().map(|(a, b)| format!("{a}-{b}")).collect::<Vec<_>>().join(" / ");
    s.text(m, HEIGHT - 14.0, 10.0, false, grey, &format.label(), "start");
    s.text(right, HEIGHT - 14.0, 10.0, false, grey, &format!("Ordre : {order}"), "end");
    s.shapes
}

/// Feuilles d'un lot : une par rencontre, ou trois (une par équipe et une
/// pour l'arbitre).
pub fn copies(info: &RelaySheetInfo, three: bool) -> Vec<RelaySheetInfo> {
    if !three {
        return vec![info.clone()];
    }
    ["Équipe A", "Équipe B", "Arbitre"]
        .iter()
        .map(|c| RelaySheetInfo { copy: c.to_string(), ..info.clone() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hauteur d'un texte ; les nombres sont cherchés dans l'échelle des touches.
    fn filled_y(shapes: &[Shape], label: &str) -> f64 {
        let ladder = label.parse::<u32>().is_ok();
        shapes
            .iter()
            .find_map(|s| match s {
                Shape::Text { text, x, y, .. } if text == label && (!ladder || (*x - 459.0).abs() < 1.0) => Some(*y),
                _ => None,
            })
            .unwrap_or(f64::NAN)
    }
    use crate::relay::RelayKind;

    #[test]
    fn sheet_has_a_row_per_relay_and_a_ladder_to_the_total() {
        let format = RelayFormat::default();
        let info = RelaySheetInfo { contest: "Coupe".into(), teams: ["YERRES".into(), "MELUN".into()], ..Default::default() };
        let shapes = layout(&format, &info);
        let texts: Vec<&str> = shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.as_str()) } else { None }).collect();
        assert!(texts.contains(&"45") && !texts.contains(&"46"), "échelle jusqu'au score de la rencontre");
        // Les touches du relais 2 (6 à 10) sont sur la ligne du relais 2.
        let y_of = |label: &str| filled_y(&shapes, label);
        assert!(y_of("6") > y_of("→5") && y_of("10") < y_of("→15"));
        assert!(texts.contains(&"→45"));
        // Les équipes ne sont pas placées en haut ou en bas : seulement nommées.
        assert!(texts.iter().any(|t| t.contains("YERRES") && t.contains("MELUN")));
        // Tout tient dans la page.
        for s in &shapes {
            if let Shape::Rect { x, y, w, h, .. } = s {
                assert!(*x >= 0.0 && *y >= 0.0 && x + w <= WIDTH && y + h <= HEIGHT);
            }
        }
        let single = RelayFormat { kind: RelayKind::Single, team_size: 6, points: 4, substitutes: 1 };
        let shapes = layout(&single, &RelaySheetInfo::default());
        assert!(shapes.iter().any(|s| matches!(s, Shape::Text { text, .. } if text == "→24")));
        // 4 contre 4, relais de 1 point : 16 touches.
        let four = RelayFormat { team_size: 4, points: 1, ..RelayFormat::default() };
        let shapes = layout(&four, &RelaySheetInfo::default());
        let numbers: Vec<u32> = shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { text.parse().ok() } else { None }).collect();
        assert_eq!(numbers.iter().copied().max(), Some(16));
        assert_eq!(copies(&info, true).len(), 3);

        // Rencontre commencée : équipes placées, tireurs, scores et bilans.
        let log = crate::relay::RelayLog {
            revision: 1,
            format,
            top: 1,
            lineup: ["A1", "A2", "A3", "B1", "B2", "B3"].iter().map(|x| x.to_string()).collect(),
            relays: vec![[5, 3], [8, 10]],
            winner: None,
        };
        let filled = layout(&format, &RelaySheetInfo { log: Some(log), ..info.clone() });
        let texts: Vec<String> = filled.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect();
        assert!(texts.contains(&"MELUN".to_string()), "l'équipe du haut est la seconde de la feuille");
        assert!(texts.contains(&"A3".to_string()) && texts.contains(&"B3".to_string()));
        // Relais 2 (1-5) : 3 touches à 7 ; bilan de 1 = -4, de 3 = +2.
        assert!(texts.contains(&"-4".to_string()) && texts.contains(&"+2".to_string()));
    }
}

impl crate::contest::Contest {
    /// Format des relais et feuilles des rencontres d'une phase : poules
    /// (toutes, ou la poule `pool`) ou tableaux (rencontres prêtes, pas
    /// encore jouées). Les numéros des tireurs ne sont jamais préremplis :
    /// les équipes donnent leur ordre à l'arbitre au début de la rencontre.
    pub fn relay_sheets(&self, index: usize, pool: Option<usize>) -> Option<(RelayFormat, Vec<RelaySheetInfo>)> {
        use crate::rounds::pool::{PoolAllocatorStage, PoolSupervisorStage};
        use crate::rounds::table::supervisor::TableSupervisor;
        let stage = self.schedule.stage(index)?;
        let format = self.relay_format(index);
        let name = |r: crate::player::PlayerRef| self.players.get(r).map(|p| p.name()).unwrap_or_default();
        let base = RelaySheetInfo { contest: self.name.clone(), date: self.date_image(), ..Default::default() };
        let mut sheets = Vec::new();
        let round = if let Some(a) = stage.as_any().downcast_ref::<PoolAllocatorStage>() {
            Some(a.shared_round())
        } else {
            stage.as_any().downcast_ref::<PoolSupervisorStage>().map(|s| s.shared_round())
        };
        if let Some(round) = round {
            let round = round.lock().ok()?;
            for (i, p) in round.pools().iter().enumerate() {
                if pool.is_some_and(|x| x != i) {
                    continue;
                }
                for m in p.matches() {
                    let (Some(a), Some(b)) = (m.bout.opponent(0), m.bout.opponent(1)) else { continue };
                    sheets.push(RelaySheetInfo {
                        log: m.bout.relay.clone(),
                        round: p.name().to_string(),
                        bout: m.name(),
                        piste: p.strips().first().map(|x| format!("{x:02}")).unwrap_or_default(),
                        time: p.start_time_image().unwrap_or_default().trim().to_string(),
                        teams: [name(a), name(b)],
                        ..base.clone()
                    });
                }
            }
        } else if let Some(sup) = stage.as_any().downcast_ref::<TableSupervisor>() {
            for set in sup.table_sets().iter().filter(|s| s.is_visible()) {
                for node in set.nodes() {
                    let Some(m) = node.bout.as_ref() else { continue };
                    let (Some(a), Some(b)) = (m.opponent(0), m.opponent(1)) else { continue };
                    // Rencontre jouée : imprimée seulement si son journal existe (feuille remplie).
                    if m.is_over() && m.relay.is_none() {
                        continue;
                    }
                    sheets.push(RelaySheetInfo {
                        log: m.relay.clone(),
                        round: set.name(),
                        bout: m.name.clone(),
                        piste: if m.piste != 0 { format!("{:02}", m.piste) } else { String::new() },
                        time: m.start_time.map(|t| t.0.format("%H:%M").to_string()).unwrap_or_default(),
                        teams: [name(a), name(b)],
                        ..base.clone()
                    });
                }
            }
        }
        Some((format, sheets))
    }

    /// Feuille vierge d'une phase (épreuve et date seulement).
    pub fn blank_relay_sheet(&self) -> RelaySheetInfo {
        RelaySheetInfo { contest: self.name.clone(), date: self.date_image(), ..Default::default() }
    }
}
