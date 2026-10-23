//! Entraînement des jeunes : une feuille par groupe pour l'arbitre — les
//! feuilles de ses poules et son tableau vierge (1er P1 contre dernier P2…),
//! le tableau dans les deux sens. Données communes à l'impression du
//! logiciel et à la version web.

use serde_json::{json, Value};

use crate::contest::Contest;
use crate::player::{PlayerRef, PlayerStore};
use crate::rounds::pool::{PoolAllocatorStage, PoolSupervisorStage};
use crate::rounds::table::supervisor::{FencedPlaces, TableSupervisor};
use crate::rounds::youth::YouthTraining;
use crate::stage::{Stage, StageContext};

/// Match d'un groupe dans l'ordre où il se tire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupBout {
    /// Index de la poule dans le tour.
    pub pool: usize,
    /// Index du match dans sa poule.
    pub match_index: usize,
    /// Numéro du match dans le groupe (M1, M2…), unique pour tout le groupe.
    pub number: usize,
    pub piste: u32,
    pub referee: Option<PlayerRef>,
}

/// Matchs d'un groupe tirés ensemble : les poules sont entrelacées (1er match
/// de la poule 1, 1er de la poule 2, 2e de la poule 1…) et numérotées M1,
/// M2… pour tout le groupe. Les matchs sont répartis sur les pistes du
/// groupe (jamais deux matchs en même temps pour un tireur) ; sur une piste,
/// les arbitres alternent match après match et évitent les tireurs de leur
/// club quand un autre arbitre de la piste est libre.
pub fn group_dispatch(round: &crate::rounds::pool::round::PoolRound, group: u32, players: &PlayerStore, referees: &PlayerStore) -> Vec<GroupBout> {
    let pools: Vec<usize> = round.pools().iter().enumerate().filter(|(_, p)| p.group() == Some(group)).map(|(i, _)| i).collect();
    // Pistes et arbitres communs du groupe, dans l'ordre.
    let mut pistes: Vec<u32> = Vec::new();
    let mut refs: Vec<PlayerRef> = Vec::new();
    for i in &pools {
        let pool = &round.pools()[*i];
        for p in pool.strips() {
            if !pistes.contains(&p) {
                pistes.push(p);
            }
        }
        for r in pool.referees() {
            if !refs.contains(r) {
                refs.push(*r);
            }
        }
    }
    let lanes: Vec<(u32, Vec<PlayerRef>)> = if pistes.is_empty() {
        vec![(0, refs.clone())]
    } else {
        pistes
            .iter()
            .enumerate()
            .map(|(i, p)| (*p, refs.iter().enumerate().filter(|(k, _)| k % pistes.len() == i).map(|(_, r)| *r).collect()))
            .collect()
    };
    // Ordre entrelacé des poules
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let longest = pools.iter().map(|i| round.pools()[*i].matches().len()).max().unwrap_or(0);
    for k in 0..longest {
        for i in &pools {
            if k < round.pools()[*i].matches().len() {
                pending.push((*i, k));
            }
        }
    }
    let club = |store: &PlayerStore, r: PlayerRef| store.get(r).and_then(|p| p.str_attr("club").map(|c| c.trim().to_lowercase())).unwrap_or_default();
    let conflict = |referee: PlayerRef, fencer: PlayerRef| {
        let c = club(referees, referee);
        !c.is_empty() && c == club(players, fencer)
    };
    let fencers_of = |(pool, m): (usize, usize)| -> Vec<PlayerRef> { (0..2).filter_map(|k| round.pools()[pool].matches()[m].bout.opponent(k)).collect() };
    let mut counts = vec![0usize; lanes.len()];
    let mut refereed: std::collections::HashMap<PlayerRef, usize> = std::collections::HashMap::new();
    let mut out: Vec<GroupBout> = Vec::new();
    while !pending.is_empty() {
        let mut used: Vec<PlayerRef> = Vec::new();
        let mut order: Vec<usize> = (0..lanes.len()).collect();
        order.sort_by_key(|i| (counts[*i], *i));
        let mut placed = false;
        for lane in order {
            let Some(next) = pending.iter().position(|b| fencers_of(*b).iter().all(|f| !used.contains(f))) else { break };
            let (pool, match_index) = pending.remove(next);
            let fencers = fencers_of((pool, match_index));
            used.extend(fencers.iter().copied());
            let (piste, candidates) = &lanes[lane];
            let referee = candidates
                .iter()
                .enumerate()
                .min_by_key(|(order, r)| (fencers.iter().any(|f| conflict(**r, *f)), refereed.get(*r).copied().unwrap_or(0), *order))
                .map(|(_, r)| *r);
            if let Some(r) = referee {
                *refereed.entry(r).or_insert(0) += 1;
            }
            out.push(GroupBout { pool, match_index, number: out.len() + 1, piste: *piste, referee });
            counts[lane] += 1;
            placed = true;
        }
        if !placed {
            break;
        }
    }
    out
}

fn identity(players: &PlayerStore, r: PlayerRef) -> String {
    players
        .get(r)
        .map(|p| {
            let first = p.str_attr("first_name").unwrap_or_default();
            if first.is_empty() {
                p.name()
            } else {
                format!("{} {first}", p.name())
            }
        })
        .unwrap_or_default()
}

/// « 1er », « 2e »…
fn ordinal(n: usize) -> String {
    if n == 1 {
        "1er".into()
    } else {
        format!("{n}e")
    }
}

/// Feuille de tableau d'un groupe, dans les deux sens : le premier tour au
/// centre, les vainqueurs vers la droite jusqu'à la finale, les perdants du
/// premier tour vers la gauche (matchs de classement alignés sur les matchs
/// dont ils viennent), les autres matchs de classement en dessous.
struct BracketSheet {
    items: Vec<Value>,
    lines: Vec<Vec<(f64, f64)>>,
}

const SLOT: f64 = 128.0;
const SCORE: f64 = 22.0;
const COLW: f64 = SLOT + SCORE + 10.0;
const ROW: f64 = 30.0;

/// Profondeur d'un nœud (nombre de tours jusqu'aux entrées).
fn node_depth(set: &crate::rounds::table::TableSet, id: usize) -> usize {
    set.nodes()[id]
        .children
        .iter()
        .map(|c| 1 + node_depth(set, *c))
        .max()
        .unwrap_or(0)
}

struct TreeCtx<'a> {
    set: &'a crate::rounds::table::TableSet,
    x0: f64,
    dir: f64,
    next_leaf: f64,
    leaf_y: &'a dyn Fn(usize) -> Option<f64>,
    leaf_label: &'a dyn Fn(usize) -> String,
    /// Nom du vainqueur d'une case (tableau rempli, sinon vide).
    node_label: &'a dyn Fn(usize) -> String,
    /// Score du tireur de la case dans le match suivant.
    score_label: &'a dyn Fn(usize) -> String,
    ys: std::collections::HashMap<usize, f64>,
}

impl BracketSheet {
    fn slot(&mut self, x: f64, y: f64, text: &str) {
        self.items.push(
            json!({ "t": "slot", "x": x, "y": y - 20.0, "w": SLOT, "h": 20.0, "text": text }),
        );
    }
    fn score(&mut self, x: f64, y: f64, text: &str) {
        self.items
            .push(json!({ "t": "score", "x": x, "y": y - 20.0, "w": SCORE, "h": 20.0, "text": text }));
    }
    fn text(&mut self, kind: &str, x: f64, y: f64, text: &str) {
        self.items
            .push(json!({ "t": kind, "x": x, "y": y, "text": text }));
    }

    /// Place le nœud `id` en colonne `col` et renvoie sa hauteur.
    fn place(&mut self, t: &mut TreeCtx, id: usize, col: usize, is_root: bool) -> f64 {
        let node = &t.set.nodes()[id];
        let start = t.x0 + t.dir * col as f64 * COLW;
        let (slot_x, score_x) = if t.dir > 0.0 {
            (start, start + SLOT + 2.0)
        } else {
            (start - SLOT, start - SLOT - SCORE - 2.0)
        };
        let y = if node.children.is_empty() || col == 0 {
            let y = (t.leaf_y)(id).unwrap_or_else(|| {
                let y = t.next_leaf;
                t.next_leaf += ROW;
                y
            });
            let label = (t.leaf_label)(id);
            self.slot(slot_x, y, &label);
            y
        } else {
            let children = node.children.clone();
            let kids: Vec<f64> = children
                .iter()
                .map(|c| self.place(t, *c, col - 1, false))
                .collect();
            let y = kids.iter().sum::<f64>() / kids.len().max(1) as f64;
            let join = start - t.dir * 5.0;
            let child_end = t.x0 + t.dir * ((col - 1) as f64 * COLW + SLOT + SCORE + 2.0);
            for ky in &kids {
                self.lines.push(vec![(child_end, *ky), (join, *ky)]);
            }
            if kids.len() == 2 {
                self.lines.push(vec![(join, kids[0]), (join, kids[1])]);
            }
            self.lines.push(vec![(join, y), (start, y)]);
            if let Some(m) = t.set.nodes()[id].bout.as_ref().filter(|m| m.number != 0) {
                let label_x = if t.dir > 0.0 { join + 4.0 } else { join - 64.0 };
                self.text("match", label_x, y - 38.0, &m.name);
            }
            let label = (t.node_label)(id);
            self.slot(slot_x, y, &label);
            y
        };
        if !is_root {
            let score = (t.score_label)(id);
            self.score(score_x, y, &score);
        }
        t.ys.insert(id, y);
        y
    }

    /// Dessine la suite `set` : `dir` = 1 vers la droite, -1 vers la gauche ;
    /// `x0` est le bord intérieur de la colonne des entrées ; `leaf_y` donne la
    /// hauteur d'une entrée (sinon : à la suite, à partir de `top`). Renvoie la
    /// hauteur de chaque nœud (pour aligner d'autres suites).
    fn tree(
        &mut self,
        set: &crate::rounds::table::TableSet,
        x0: f64,
        dir: f64,
        top: f64,
        leaf_y: &dyn Fn(usize) -> Option<f64>,
        leaf_label: &dyn Fn(usize) -> String,
        node_label: &dyn Fn(usize) -> String,
        score_label: &dyn Fn(usize) -> String,
    ) -> std::collections::HashMap<usize, f64> {
        let Some(root) = set.root() else {
            return std::collections::HashMap::new();
        };
        let height = node_depth(set, root);
        let mut t = TreeCtx {
            set,
            x0,
            dir,
            next_leaf: top,
            leaf_y,
            leaf_label,
            node_label,
            score_label,
            ys: std::collections::HashMap::new(),
        };
        self.place(&mut t, root, height, true);
        t.ys
    }
}

/// Tireur qui occupe une case : l'entrant (case de départ) ou le vainqueur.
fn occupant(set: &crate::rounds::table::TableSet, id: usize) -> Option<PlayerRef> {
    let n = set.nodes().get(id)?;
    let m = n.bout.as_ref()?;
    if n.children.is_empty() {
        m.winner().or_else(|| m.opponent(0))
    } else {
        m.winner()
    }
}

/// Score du tireur d'une case dans le match où il va ensuite.
fn score_of(set: &crate::rounds::table::TableSet, id: usize) -> Option<String> {
    let parent = set.nodes().iter().position(|n| n.children.contains(&id))?;
    let fencer = occupant(set, id)?;
    let m = set.nodes()[parent].bout.as_ref()?;
    let k = (0..2).find(|k| m.opponent(*k) == Some(fencer))?;
    Some(m.score_at(k).image(m.max_score)).filter(|s| !s.is_empty())
}

/// Tableau réel d'un groupe (suites dans l'ordre) et ses tireurs.
pub(crate) struct RealTables<'a> {
    sets: Vec<&'a crate::rounds::table::TableSet>,
    players: &'a PlayerStore,
}

fn blank_brackets(c: &Contest, pool_sizes: &[usize], fenced: FencedPlaces, real: Option<RealTables>) -> Option<Value> {
    use crate::contest::players::new_player;
    use crate::player::{AttrId, PlayerClass};
    let largest = pool_sizes.iter().copied().max().unwrap_or(0);
    let mut labels = Vec::new();
    for rank in 1..=largest {
        for (k, size) in pool_sizes.iter().enumerate() {
            if rank <= *size {
                labels.push(format!("{} P{}", ordinal(rank), k + 1));
            }
        }
    }
    if labels.len() < 2 {
        return None;
    }
    let mut store = PlayerStore::new();
    let refs: Vec<PlayerRef> = labels
        .iter()
        .map(|l| {
            let mut p = new_player(PlayerClass::Fencer);
            p.set_str(&AttrId::global("name"), l);
            store.add(p)
        })
        .collect();
    let mut table = TableSupervisor::new(90);
    table.fenced_places = fenced.value();
    let settings = c.settings.clone();
    {
        let mut ctx = StageContext {
            players: &mut store,
            settings: &settings,
        };
        table.set_attendees(&mut ctx, refs);
        table.garnish(&mut ctx);
    }
    // Entrées du tableau principal (avant résultats) : places de poule.
    let entrants: std::collections::HashMap<usize, String> = table
        .table_sets()
        .first()?
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, n)| n.children.is_empty())
        .filter_map(|(i, n)| {
            n.bout
                .as_ref()
                .and_then(|m| m.winner().or_else(|| m.opponent(0)))
                .map(|w| (i, identity(&store, w)))
        })
        .collect();
    // Résultats fictifs : chaque match est gagné par son premier tireur, pour
    // que le cœur crée les tableaux de classement et leurs entrées.
    for _ in 0..64 {
        let todo: Vec<(usize, usize, PlayerRef, PlayerRef)> = table
            .table_sets()
            .iter()
            .enumerate()
            .flat_map(|(si, set)| {
                set.nodes().iter().enumerate().filter_map(move |(ni, n)| {
                    let m = n.bout.as_ref()?;
                    if m.winner().is_some() {
                        return None;
                    }
                    Some((si, ni, m.opponent(0)?, m.opponent(1)?))
                })
            })
            .collect();
        if todo.is_empty() {
            break;
        }
        let mut ctx = StageContext {
            players: &mut store,
            settings: &settings,
        };
        for (si, ni, a, b) in todo {
            table.set_score(&mut ctx, si, ni, a, "V");
            table.set_score(&mut ctx, si, ni, b, "0");
        }
    }
    let sets = table.table_sets();
    let main = sets.first()?;
    // Tableau réel du groupe, s'il a la même forme : noms et scores reportés.
    // Suite par suite, tant que la forme est la même.
    let real_set = |k: usize| real.as_ref().and_then(|r| r.sets.get(k).copied()).filter(|s| sets.get(k).is_some_and(|b| b.nodes().len() == s.nodes().len()));
    let name = |k: usize, id: usize| -> Option<String> {
        let r = real.as_ref()?;
        occupant(real_set(k)?, id).map(|p| identity(r.players, p))
    };
    let score = |k: usize, id: usize| -> String { real_set(k).and_then(|s| score_of(s, id)).unwrap_or_default() };
    // Entrée d'une suite de classement : « Perdant <match de la suite parente> ».
    let lost_in = |parent: &crate::rounds::table::TableSet,
                   set: &crate::rounds::table::TableSet,
                   leaf: usize|
     -> (String, Option<usize>) {
        let bout = set.nodes()[leaf].bout.as_ref();
        let Some(p) = bout.and_then(|m| m.winner().or_else(|| m.opponent(0))) else {
            return (String::new(), None);
        };
        for (i, n) in parent.nodes().iter().enumerate() {
            if let Some(m) = n.bout.as_ref() {
                if m.looser() == Some(p) && m.number != 0 {
                    return (format!("Perdant {}", m.name), Some(i));
                }
            }
        }
        (String::new(), None)
    };

    let mut sheet = BracketSheet {
        items: Vec::new(),
        lines: Vec::new(),
    };
    let top = 50.0;
    sheet.text("title", 0.0, 4.0, "Tableau");
    let main_ys = sheet.tree(
        main,
        0.0,
        1.0,
        top,
        &|_| None,
        &|leaf| name(0, leaf).unwrap_or_else(|| entrants.get(&leaf).cloned().unwrap_or_default()),
        &|id| name(0, id).unwrap_or_default(),
        &|id| score(0, id),
    );
    let mut bottom = main_ys.values().copied().fold(top, f64::max);

    // Perdants du premier tour : vers la gauche, alignés sur leurs matchs.
    let first_round = main.root().map(|r| node_depth(main, r)).unwrap_or(0);
    let first_losers = sets.iter().enumerate().skip(1).find(|(_, s)| {
        s.parent() == Some(0)
            && s.is_visible()
            && s.root()
                .map(|r| node_depth(s, r) + 1 == first_round)
                .unwrap_or(false)
    });
    if let Some((fi, set)) = first_losers {
        let entries = set.attendees().iter().flatten().count() as u32;
        let first = set.first_place();
        sheet.text(
            "title",
            -30.0 - SLOT,
            4.0,
            &format!("Places {first} à {}", first + entries.max(1) - 1),
        );
        let ys = sheet.tree(
            set,
            -30.0,
            -1.0,
            top,
            &|leaf| {
                lost_in(main, set, leaf)
                    .1
                    .and_then(|n| main_ys.get(&n).copied())
            },
            &|leaf| name(fi, leaf).unwrap_or_else(|| lost_in(main, set, leaf).0),
            &|id| name(fi, id).unwrap_or_default(),
            &|id| score(fi, id),
        );
        bottom = bottom.max(ys.values().copied().fold(top, f64::max));
    }
    // Autres matchs de classement : en dessous, vers la droite.
    let mut y = bottom + 80.0;
    for (i, set) in sets
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, s)| s.is_visible())
    {
        if first_losers.map(|(f, _)| f == i).unwrap_or(false) {
            continue;
        }
        let Some(parent) = set.parent().map(|p| &sets[p]) else {
            continue;
        };
        let entries = set.attendees().iter().flatten().count() as u32;
        let first = set.first_place();
        let title = if entries <= 2 {
            if first == 3 {
                "3e place".to_string()
            } else {
                format!("Places {first} et {}", first + 1)
            }
        } else {
            format!("Places {first} à {}", first + entries - 1)
        };
        sheet.text("title", 0.0, y - 30.0, &title);
        let ys = sheet.tree(
            set,
            0.0,
            1.0,
            y + 10.0,
            &|_| None,
            &|leaf| name(i, leaf).unwrap_or_else(|| lost_in(parent, set, leaf).0),
            &|id| name(i, id).unwrap_or_default(),
            &|id| score(i, id),
        );
        y = ys.values().copied().fold(y, f64::max) + 80.0;
    }

    // Recadrage : coordonnées positives.
    let (mut min_x, mut max_x, mut max_y) = (f64::MAX, f64::MIN, 0.0_f64);
    for it in &sheet.items {
        let x = it["x"].as_f64().unwrap_or(0.0);
        let w = it.get("w").and_then(Value::as_f64).unwrap_or(70.0);
        min_x = min_x.min(x);
        max_x = max_x.max(x + w);
        max_y = max_y.max(it["y"].as_f64().unwrap_or(0.0) + 24.0);
    }
    let dx = -min_x + 4.0;
    let items: Vec<Value> = sheet
        .items
        .into_iter()
        .map(|mut it| {
            let x = it["x"].as_f64().unwrap_or(0.0) + dx;
            it["x"] = json!(x);
            it
        })
        .collect();
    let lines: Vec<Value> = sheet
        .lines
        .iter()
        .map(|l| Value::Array(l.iter().map(|(x, y)| json!([x + dx, y])).collect()))
        .collect();
    Some(
        json!({ "width": max_x - min_x + 8.0, "height": max_y + 8.0, "items": items, "lines": lines }),
    )
}

impl Contest {
    /// Suites du vrai tableau d'un groupe (phase de tableaux de l'entraînement
    /// des jeunes), dans leur ordre.
    pub(crate) fn youth_real_tables(&self, group: u32) -> Option<RealTables<'_>> {
        for stage in self.schedule.stages() {
            let Some(sup) = stage.as_any().downcast_ref::<TableSupervisor>() else { continue };
            let sets: Vec<_> = sup.table_sets().iter().filter(|s| s.group() == Some(group)).collect();
            if !sets.is_empty() {
                return Some(RealTables { sets, players: &self.players });
            }
        }
        None
    }

    /// Index de la phase de poules d'un entraînement des jeunes (pour
    /// imprimer les feuilles des groupes avec le classement final).
    pub fn youth_pool_stage(&self) -> Option<usize> {
        (0..self.schedule.len()).find(|i| {
            self.schedule.stage(*i).and_then(|s| s.as_any().downcast_ref::<PoolSupervisorStage>()).is_some_and(|s| {
                s.shared_round().lock().map(|r| r.is_youth()).unwrap_or(false)
            })
        })
    }

    /// Feuilles des groupes du tour de poules `index` (composition ou
    /// poules) : `{ contest, header, date, groups: [{ group, pools, bracket }] }`.
    /// `bracket` : `{ width, height, items: [{ t, x, y, w?, h?, text? }], lines }`.
    pub fn youth_group_sheets(&self, index: usize) -> Result<Value, String> {
        // Depuis le tableau ou le classement : la phase de poules des groupes.
        let is_pools = self.schedule.stage(index).is_some_and(|s| {
            s.as_any().downcast_ref::<PoolAllocatorStage>().is_some() || s.as_any().downcast_ref::<PoolSupervisorStage>().is_some()
        });
        let index = if is_pools { index } else { self.youth_pool_stage().ok_or_else(|| "feuilles des groupes : pas d'entraînement des jeunes".to_string())? };
        let stage = self
            .schedule
            .stage(index)
            .ok_or_else(|| "phase introuvable".to_string())?;
        let round = if let Some(a) = stage.as_any().downcast_ref::<PoolAllocatorStage>() {
            a.shared_round()
        } else if let Some(s) = stage.as_any().downcast_ref::<PoolSupervisorStage>() {
            s.shared_round()
        } else {
            return Err("feuilles des groupes : phase de poules attendue".into());
        };
        let fenced = (0..self.schedule.len())
            .filter_map(|i| self.schedule.stage(i))
            .find_map(|s| {
                s.as_any()
                    .downcast_ref::<YouthTraining>()
                    .map(|y| y.settings.fenced_places)
            })
            .unwrap_or(FencedPlaces::AllPlaces);
        let round = round
            .lock()
            .map_err(|_| "tour de poules indisponible".to_string())?;
        let mut groups: Vec<(u32, Vec<usize>)> = Vec::new();
        for (i, pool) in round.pools().iter().enumerate() {
            let g = pool.group().unwrap_or(1);
            match groups.iter_mut().find(|(n, _)| *n == g) {
                Some((_, list)) => list.push(i),
                None => groups.push((g, vec![i])),
            }
        }
        let sheets: Vec<Value> = groups
        .iter()
        .map(|(g, pools)| {
            let dispatch = group_dispatch(&round, *g, &self.players, self.referees.store());
            let sizes: Vec<usize> = pools.iter().map(|i| round.pools()[*i].nb_players()).collect();
            json!({
                "group": g,
                // Index des poules du groupe dans le tour (choix « ce groupe »).
                "indices": pools,
                "pools": pools.iter().map(|i| {
                    let pool = &round.pools()[*i];
                    let sheet = pool.sheet(&self.players, self.referees.store(), false);
                    // Poule commencée : scores, victoires, indices et places imprimés.
                    let started = sheet.matches.iter().any(|m| m.opponents.iter().flatten().any(|f| !f.score.is_empty()));
                    let mine: Vec<&GroupBout> = dispatch.iter().filter(|b| b.pool == *i).collect();
                    let mut matches: Vec<_> = sheet.matches.iter().collect();
                    matches.sort_by_key(|m| mine.iter().find(|b| b.match_index == m.match_index).map(|b| b.number).unwrap_or(usize::MAX));
                    let referee_name = |r: PlayerRef| self.referees.store().get(r).map(|p| format!("{} {}", p.name(), p.str_attr("first_name").unwrap_or_default()).trim().to_string()).unwrap_or_default();
                    let referee_club = |r: PlayerRef| self.referees.store().get(r).and_then(|p| p.str_attr("club").map(str::to_string)).unwrap_or_default();
                    json!({
                        "name": sheet.title,
                        "piste": sheet.piste,
                        // Arbitres de la poule, par piste (l'ordre de la répartition).
                        "referees": pool.referees_by_piste().iter().flat_map(|(piste, refs)| refs.iter().map(move |r| (*piste, *r))).map(|(piste, r)| json!({
                            "piste": piste,
                            "name": referee_name(r),
                            "club": referee_club(r),
                        })).collect::<Vec<_>>(),
                        "rows": sheet.rows.iter().map(|r| json!({
                            "position": r.position,
                            "name": identity(&self.players, r.player),
                            "club": r.identity.club,
                            "grey": r.cells.iter().map(|cell| cell.grey).collect::<Vec<_>>(),
                            "cells": r.cells.iter().map(|cell| if started && !cell.grey { cell.text.clone() } else { String::new() }).collect::<Vec<_>>(),
                            "dashboard": r.dashboard.as_ref().filter(|_| started).map(|d| vec![d.victories.clone(), d.hits_scored.clone(), d.hits_received.clone(), d.indice.clone(), d.rank.clone()]),
                        })).collect::<Vec<_>>(),
                        "matches": matches.iter().map(|m| {
                            let bout = mine.iter().find(|b| b.match_index == m.match_index);
                            json!({
                                // Numéro du match dans le groupe, sa piste et son arbitre.
                                "piste": bout.map(|b| b.piste).filter(|p| *p != 0),
                                "referee": bout.and_then(|b| b.referee).map(referee_name),
                                "officiating": m.officiating,
                                "name": bout.map(|b| format!("M{}", b.number)).unwrap_or_else(|| m.name.clone()),
                                "boxes": m.score_boxes,
                                "opponents": m.opponents.iter().flatten().map(|f| json!({
                                    "position": f.position,
                                    "name": format!("{} {}", f.identity.name, f.identity.first_name).trim().to_string(),
                                    "score": f.score,
                                })).collect::<Vec<_>>(),
                            })
                        }).collect::<Vec<_>>(),
                    })
                }).collect::<Vec<_>>(),
                "bracket": blank_brackets(self, &sizes, fenced, self.youth_real_tables(*g)),
            })
        })
        .collect();
        Ok(
            json!({ "contest": self.name, "header": format!("{} - {} - {}", self.weapon.label(), self.gender_label(), self.category.display_image()), "date": self.date_image(), "groups": sheets }),
        )
    }
}
