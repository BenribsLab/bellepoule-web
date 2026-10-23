//! Poules de division (extension propre au portage Rust) : échelle de
//! divisions avec montées et descentes.
//!
//! La phase répartit les tireurs en divisions (D1 en haut, D2…) ; le tour
//! de poules qui la suit compose ses poules division par division (comme
//! les groupes de l'entraînement des jeunes : attribut `youth_group`).
//! Une phase de division placée après un tour de division propose les
//! montées et les descentes : les `up` premiers de chaque division montent,
//! les `down` derniers descendent (ni montée en D1, ni descente de la
//! dernière). La répartition proposée reste modifiable à la main.
//! Au classement général, la D1 vient d'abord, puis la D2…

use std::any::Any;
use std::collections::BTreeMap;

use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

use crate::player::{AttrId, PlayerRef};
use crate::rounds::table::common::{self, codes};
use crate::stage::{PreviousStage, RankEntry, Stage, StageBase, StageContext, StageKind};
use crate::xml::XmlWriter;

/// Répartition de départ (premier tour).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DivisionStart {
    /// Au classement : les meilleurs en D1.
    #[default]
    Ranking,
    /// Au hasard.
    Random,
}

/// Réglages (préférences de la phase).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DivisionSettings {
    /// Nombre de divisions.
    pub divisions: u32,
    /// Poules par division.
    pub pools: u32,
    /// Montées et descentes à chaque tour.
    pub up: u32,
    pub down: u32,
    #[serde(default)]
    pub start: DivisionStart,
    /// Décalage des poules (club par défaut).
    #[serde(default = "default_swapping")]
    pub swapping: Vec<String>,
}

fn default_swapping() -> Vec<String> {
    vec!["club".into()]
}

impl Default for DivisionSettings {
    fn default() -> Self {
        DivisionSettings { divisions: 2, pools: 1, up: 2, down: 2, start: DivisionStart::Ranking, swapping: default_swapping() }
    }
}

/// Mouvement d'un tireur par rapport au tour précédent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Stays,
    Up,
    Down,
    /// Premier tour, ou tireur sans division au tour précédent.
    New,
}

pub struct DivisionRound {
    base: StageBase,
    previous: Option<PreviousStage>,
    pub settings: DivisionSettings,
    /// Division de chaque tireur (1 = D1).
    assignment: BTreeMap<PlayerRef, u32>,
    /// Division au tour précédent (vide au premier tour).
    origin: BTreeMap<PlayerRef, u32>,
}

impl DivisionRound {
    pub fn new(id: u32) -> Self {
        let mut base = StageBase::new(StageKind::DivisionRound, id);
        base.filter = codes(&["stage_start_rank", "name", "first_name", "club", "category"]);
        DivisionRound { base, previous: None, settings: DivisionSettings::default(), assignment: BTreeMap::new(), origin: BTreeMap::new() }
    }

    /// Division d'un tireur (D1 si inconnue).
    pub fn division_of(&self, r: PlayerRef) -> u32 {
        self.assignment.get(&r).copied().unwrap_or(1)
    }

    /// Division au tour précédent.
    pub fn origin_of(&self, r: PlayerRef) -> Option<u32> {
        self.origin.get(&r).copied()
    }

    pub fn movement(&self, r: PlayerRef) -> Move {
        match self.origin_of(r) {
            None => Move::New,
            Some(o) => match self.division_of(r).cmp(&o) {
                std::cmp::Ordering::Less => Move::Up,
                std::cmp::Ordering::Greater => Move::Down,
                std::cmp::Ordering::Equal => Move::Stays,
            },
        }
    }

    /// Répartition actuelle (pour la phase de division suivante).
    pub fn assignment(&self) -> &BTreeMap<PlayerRef, u32> {
        &self.assignment
    }

    /// Change la division d'un tireur à la main.
    pub fn set_division(&mut self, r: PlayerRef, division: u32) {
        if self.base.locked || !self.base.attendees.contains(&r) {
            return;
        }
        self.assignment.insert(r, division.clamp(1, self.settings.divisions.max(1)));
    }

    /// Nouveaux réglages : la répartition est refaite.
    pub fn set_settings(&mut self, settings: DivisionSettings) {
        if self.base.locked {
            return;
        }
        let mut settings = settings;
        settings.divisions = settings.divisions.clamp(1, 26);
        settings.pools = settings.pools.clamp(1, 20);
        self.settings = settings;
        self.assignment.clear();
        self.propose();
    }

    /// À l'entrée dans la phase : la répartition du tour de division
    /// précédent (s'il y en a un), puis la proposition.
    pub fn prepare(&mut self, previous: Option<&BTreeMap<PlayerRef, u32>>) {
        self.origin = previous.cloned().unwrap_or_default();
        if self.assignment.is_empty() || self.base.attendees.iter().any(|r| !self.assignment.contains_key(r)) {
            self.assignment.clear();
            self.propose();
        }
    }

    /// Propose la répartition : montées et descentes après un tour, sinon
    /// la répartition de départ.
    fn propose(&mut self) {
        let n = self.settings.divisions.max(1);
        let attendees = self.base.attendees.clone();
        if !self.origin.is_empty() {
            // Les participants arrivent dans l'ordre du tour précédent :
            // division par division, du premier au dernier.
            for d in 1..=n {
                let members: Vec<PlayerRef> = attendees.iter().copied().filter(|r| self.origin.get(r) == Some(&d)).collect();
                let len = members.len();
                let up = if d > 1 { (self.settings.up as usize).min(len) } else { 0 };
                let down = if d < n { (self.settings.down as usize).min(len - up) } else { 0 };
                for (i, r) in members.iter().enumerate() {
                    let target = if i < up {
                        d - 1
                    } else if i >= len - down {
                        d + 1
                    } else {
                        d
                    };
                    self.assignment.insert(*r, target);
                }
            }
            // Nouveaux venus : dernière division.
            for r in &attendees {
                self.assignment.entry(*r).or_insert(n);
            }
            return;
        }
        let mut order = attendees;
        if self.settings.start == DivisionStart::Random {
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(self.base.rand_seed as u64);
            order.shuffle(&mut rng);
        }
        let total = order.len();
        let mut at = 0usize;
        for d in 0..n as usize {
            let size = total / n as usize + usize::from(d < total % n as usize);
            for r in &order[at..at + size] {
                self.assignment.insert(*r, d as u32 + 1);
            }
            at += size;
        }
    }

    /// Participants d'une division, dans l'ordre d'entrée.
    pub fn members(&self, division: u32) -> Vec<PlayerRef> {
        self.base.attendees.iter().copied().filter(|r| self.division_of(*r) == division).collect()
    }

    fn ordered(&self) -> Vec<PlayerRef> {
        let mut list = self.base.attendees.clone();
        list.sort_by_key(|r| (self.division_of(*r), self.base.attendees.iter().position(|x| x == r)));
        list
    }
}

fn parse_map(v: Option<&str>) -> BTreeMap<PlayerRef, u32> {
    v.unwrap_or("")
        .split(',')
        .filter_map(|pair| {
            let (r, d) = pair.split_once(':')?;
            Some((r.trim().parse().ok()?, d.trim().parse().ok()?))
        })
        .collect()
}

fn map_image(m: &BTreeMap<PlayerRef, u32>) -> String {
    m.iter().map(|(r, d)| format!("{r}:{d}")).collect::<Vec<_>>().join(",")
}

impl Stage for DivisionRound {
    fn base(&self) -> &StageBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut StageBase {
        &mut self.base
    }

    fn set_previous(&mut self, previous: Option<PreviousStage>) {
        self.previous = previous;
    }

    fn set_attendees(&mut self, ctx: &mut StageContext, attendees: Vec<PlayerRef>) {
        common::retrieve_attendees(&mut self.base, ctx, attendees);
    }

    fn reset(&mut self, _ctx: &mut StageContext) {
        self.assignment.clear();
    }

    fn load(&mut self, ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), crate::Error> {
        common::load_configuration(&mut self.base, node);
        if let Some(s) = node.attribute("Divisions").and_then(|v| serde_json::from_str::<DivisionSettings>(v).ok()) {
            self.settings = s;
        }
        self.assignment = parse_map(node.attribute("Repartition"));
        self.origin = parse_map(node.attribute("Origine"));
        let attendees = self.base.attendees.clone();
        common::retrieve_attendees(&mut self.base, ctx, attendees);
        Ok(())
    }

    fn save(&self, _ctx: &StageContext, w: &mut XmlWriter) {
        w.start(StageKind::DivisionRound.xml_name());
        self.save_configuration(w);
        if !self.assignment.is_empty() {
            w.attr("Repartition", &map_image(&self.assignment));
        }
        if !self.origin.is_empty() {
            w.attr("Origine", &map_image(&self.origin));
        }
        w.end();
    }

    fn save_configuration(&self, w: &mut XmlWriter) {
        common::save_configuration(&self.base, w, false);
        w.attr("Divisions", &serde_json::to_string(&self.settings).unwrap_or_default());
    }

    fn is_over(&self, _ctx: &StageContext) -> bool {
        !self.base.attendees.is_empty()
    }

    fn classification(&self, ctx: &StageContext) -> Vec<RankEntry> {
        let list = self.ordered();
        let rank = |r: PlayerRef| list.iter().position(|x| *x == r).map(|i| i as u32 + 1).unwrap_or(0);
        common::rank_entries(ctx.players, self.base.owner(), &list, &rank)
    }

    fn output(&self, _ctx: &StageContext) -> Vec<PlayerRef> {
        self.ordered()
    }

    /// Les divisions sont figées : chaque tireur reçoit son numéro de
    /// division (attribut `youth_group`), qui compose les poules du tour.
    fn lock(&mut self, ctx: &mut StageContext) {
        self.base.locked = true;
        for r in self.base.attendees.clone() {
            let d = self.division_of(r);
            if let Some(p) = ctx.players.get_mut(r) {
                p.set_uint(&AttrId::global("youth_group"), d);
            }
        }
    }

    fn unlock(&mut self, _ctx: &mut StageContext) {
        self.base.locked = false;
    }

    fn remap_players(&mut self, map: &std::collections::HashMap<PlayerRef, PlayerRef>) {
        let tr = |r: &mut PlayerRef| {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        };
        self.base.attendees.iter_mut().for_each(tr);
        for m in [&mut self.assignment, &mut self.origin] {
            *m = std::mem::take(m).into_iter().map(|(r, d)| (map.get(&r).copied().unwrap_or(r), d)).collect();
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Fabrique (appelée par `rounds::create_stage`).
pub fn new_stage(_kind: StageKind, id: u32) -> Box<dyn Stage> {
    Box::new(DivisionRound::new(id))
}

/// Nombre de participants de chaque division (pour l'affichage).
pub fn sizes(round: &DivisionRound) -> Vec<usize> {
    (1..=round.settings.divisions).map(|d| round.members(d).len()).collect()
}
