//! Phase de barrage (port de `rounds/barrage/barrage.cpp`).
//!
//! Quand des ex-aequo font dépasser le quota de qualifiés de la phase
//! précédente, la compétition insère un barrage : les tireurs ex-aequo en fin
//! de liste (`ties_count`, le dépassement de quota de la phase précédente)
//! sont départagés à la main en cochant « promu » (attribut local
//! `promoted`). Les autres participants gardent leur rang précédent.
//!
//! Extension : les ex-aequo se départagent par des matchs (un seul match
//! pour deux tireurs, tous contre tous sinon) disputés en autant de touches
//! que les matchs de la phase d'où vient le barrage. Une fois les matchs
//! terminés, les meilleurs prennent les places restantes (`promoted`).

use std::any::Any;

use crate::bout::Match;
use crate::player::{AttrId, Owner, PlayerRef, PlayerStore};
use crate::rounds::table::common::{self, codes, compare_players};
use crate::stage::{load_attendee, save_attendees, PreviousStage, RankEntry, Stage, StageBase, StageContext, StageKind};
use crate::xml::XmlWriter;

pub struct Barrage {
    base: StageBase,
    previous: Option<PreviousStage>,
    ties_count: u32,
    result: Option<Vec<PlayerRef>>,
    output_short_list: Option<Vec<PlayerRef>>,
    /// Touches des matchs de barrage (celles de la phase précédente).
    max_score: u32,
    /// Quota de qualifiés de la phase précédente.
    previous_quota: Option<u32>,
    matches: Vec<Match>,
}

impl Barrage {
    pub fn new(id: u32) -> Self {
        let mut base = StageBase::new(StageKind::Barrage, id);
        base.filter = codes(&["promoted", "name", "first_name", "birth_date", "gender", "club", "country"]);
        base.classification_filter = codes(&["rank", "status", "name", "first_name", "club"]);
        Barrage {
            base,
            previous: None,
            ties_count: 0,
            result: None,
            output_short_list: None,
            max_score: 5,
            previous_quota: None,
            matches: Vec::new(),
        }
    }

    fn owner(&self) -> Owner {
        self.base.owner()
    }

    /// Nombre d'ex-aequo à départager (dépassement de quota précédent).
    pub fn ties_count(&self) -> u32 {
        self.ties_count
    }

    pub fn set_ties_count(&mut self, ties: u32) {
        self.ties_count = ties;
    }

    fn first_barrage_index(&self) -> usize {
        self.base.attendees.len().saturating_sub(self.ties_count as usize)
    }

    /// Tireurs du barrage (fin de la liste des participants).
    pub fn barrage_players(&self) -> &[PlayerRef] {
        &self.base.attendees[self.first_barrage_index()..]
    }

    pub fn is_promoted(&self, players: &PlayerStore, r: PlayerRef) -> bool {
        players
            .get(r)
            .and_then(|p| p.get(&AttrId::local("promoted", self.owner())))
            .map(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn promoted_count(&self, players: &PlayerStore) -> u32 {
        self.barrage_players().iter().filter(|r| self.is_promoted(players, **r)).count() as u32
    }

    /// Promeut ou non un tireur du barrage (statut local `Q`/`N`).
    pub fn set_promoted(&mut self, ctx: &mut StageContext, r: PlayerRef, promoted: bool) {
        if self.base.locked {
            return;
        }
        let owner = self.owner();
        if let Some(p) = ctx.players.get_mut(r) {
            p.set_uint(&AttrId::local("promoted", owner), promoted as u32);
            p.set_str(&AttrId::local("status", owner), if promoted { "Q" } else { "N" });
        }
    }

    /// Nombre de qualifiés affiché (`promoted_label`).
    pub fn qualified_count(&self, players: &PlayerStore) -> u32 {
        let len = self.base.attendees.len() as u32;
        len.saturating_sub(self.ties_count.saturating_sub(self.promoted_count(players)))
    }

    /// Nombre d'éliminés affiché (`dropped_label`).
    pub fn dropped_count(&self, players: &PlayerStore) -> u32 {
        self.ties_count.saturating_sub(self.promoted_count(players))
    }

    /// `GetCurrentClassification` : ordre et rang, sans modifier les joueurs.
    pub fn compute_classification(&self, players: &PlayerStore) -> Vec<(PlayerRef, u32)> {
        let owner = self.owner();
        let prev = common::previous_rank_id(self.previous, owner);
        let prev_rank = |r: PlayerRef| players.get(r).and_then(|p| p.get(&prev)).map(|v| v.as_uint()).unwrap_or(0);
        let first = self.first_barrage_index();
        let mut out: Vec<(PlayerRef, u32)> = self.base.attendees[..first].iter().map(|r| (*r, prev_rank(*r))).collect();

        let mut list = self.barrage_players().to_vec();
        let promoted_id = AttrId::local("promoted", owner);
        crate::sort::glib_sort_by(&mut list, |a, b| compare_players(players, *a, *b, &promoted_id, self.base.rand_seed));
        list.reverse();
        let len = self.base.attendees.len() as u32;
        let eliminated_rank = len - (self.ties_count - self.promoted_count(players).min(self.ties_count)) + 1;
        for r in list {
            let rank = if self.is_promoted(players, r) { prev_rank(r) } else { eliminated_rank };
            out.push((r, rank));
        }
        out
    }

    pub fn max_score(&self) -> u32 {
        self.max_score
    }

    /// Places à attribuer parmi les ex-aequo.
    pub fn places(&self) -> u32 {
        let len = self.base.attendees.len() as u32;
        let ties = self.ties_count.min(len);
        match self.previous_quota {
            Some(q) if q > 0 => q.saturating_sub(len - ties).min(ties),
            _ => ties,
        }
    }

    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    /// Crée les matchs entre ex-aequo (tous contre tous).
    fn create_matches(&mut self) {
        self.matches.clear();
        let places = self.places();
        let players = self.barrage_players().to_vec();
        if places == 0 || places as usize >= players.len() {
            return;
        }
        for i in 0..players.len() {
            for j in i + 1..players.len() {
                let mut m = Match::with_opponents(players[i], players[j], self.max_score);
                m.number = self.matches.len() as u32 + 1;
                m.name = format!("{} {}", crate::i18n::tr("Barrage"), m.number);
                self.matches.push(m);
            }
        }
    }

    pub fn matches_over(&self) -> bool {
        self.matches.iter().all(|m| m.is_over())
    }

    /// Saisie d'un score (`V`, `4`…) ; renvoie vrai si la saisie est complète.
    pub fn set_score(&mut self, ctx: &mut StageContext, index: usize, fencer: PlayerRef, value: &str) -> bool {
        if self.base.locked {
            return false;
        }
        let Some(m) = self.matches.get_mut(index) else { return false };
        let complete = m.set_score_str(fencer, value.trim());
        self.update_promotions(ctx);
        complete
    }

    /// Victoires, indice et touches données de chaque ex-aequo, du meilleur
    /// au moins bon (à égalité : l'ordre de la phase précédente).
    pub fn standings(&self) -> Vec<(PlayerRef, u32, i32, u32)> {
        let mut out: Vec<(PlayerRef, u32, i32, u32)> = Vec::new();
        for r in self.barrage_players() {
            let (mut v, mut hs, mut hr) = (0u32, 0u32, 0u32);
            for m in self.matches.iter().filter(|m| m.has_fencer(*r) && m.is_over()) {
                let pos = m.position_of(*r).unwrap_or(0);
                hs += m.score_at(pos).get();
                hr += m.score_at(1 - pos).get();
                if m.winner() == Some(*r) {
                    v += 1;
                }
            }
            out.push((*r, v, hs as i32 - hr as i32, hs));
        }
        out.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(b.3.cmp(&a.3)));
        out
    }

    /// Matchs terminés : les meilleurs prennent les places, les autres sont
    /// éliminés ; avant, personne n'est promu.
    fn update_promotions(&mut self, ctx: &mut StageContext) {
        if self.matches.is_empty() {
            return;
        }
        let over = self.matches_over();
        let places = self.places() as usize;
        let owner = self.owner();
        for (i, (r, ..)) in self.standings().iter().enumerate() {
            let promoted = over && i < places;
            if let Some(p) = ctx.players.get_mut(*r) {
                p.set_uint(&AttrId::local("promoted", owner), promoted as u32);
                p.set_str(&AttrId::local("status", owner), if promoted { "Q" } else { "N" });
            }
        }
    }

    /// `<Match ID=…><Tireur REF Score Statut/>…</Match>`.
    fn load_match(&mut self, node: roxmltree::Node) {
        let fencers: Vec<(roxmltree::Node, PlayerRef)> = node
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() != "Arbitre")
            .filter_map(|n| n.attribute("REF").map(|r| (n, crate::attribute::parse_leading_int(r) as PlayerRef)))
            .collect();
        let [(na, a), (nb, b)] = fencers.as_slice() else { return };
        let mut m = Match::with_opponents(*a, *b, self.max_score);
        m.load_header(node);
        m.name = format!("{} {}", crate::i18n::tr("Barrage"), m.number);
        m.load_fencer(*na, *a);
        m.load_fencer(*nb, *b);
        m.after_load();
        match self.matches.iter_mut().find(|x| x.number == m.number) {
            Some(existing) => *existing = m,
            None => self.matches.push(m),
        }
    }

    fn rank_of(list: &[(PlayerRef, u32)]) -> impl Fn(PlayerRef) -> u32 + '_ {
        move |r| list.iter().find(|(p, _)| *p == r).map(|(_, x)| *x).unwrap_or(0)
    }
}

impl Stage for Barrage {
    fn base(&self) -> &StageBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut StageBase {
        &mut self.base
    }

    fn set_previous(&mut self, previous: Option<PreviousStage>) {
        self.previous = previous;
        if let Some(p) = previous {
            self.ties_count = p.quota_exceedance;
            self.previous_quota = p.nb_qualified;
            if let Some(max) = p.max_score.filter(|m| *m > 0) {
                self.max_score = max;
                for m in &mut self.matches {
                    m.max_score = max;
                }
            }
        }
        // Barrage créé avant l'ajout des matchs (ou rechargé) : on les crée.
        if !self.base.locked && self.matches.is_empty() && self.ties_count > 0 {
            self.create_matches();
        }
    }

    fn set_attendees(&mut self, ctx: &mut StageContext, attendees: Vec<PlayerRef>) {
        common::retrieve_attendees(&mut self.base, ctx, attendees);
    }

    fn garnish(&mut self, ctx: &mut StageContext) {
        let owner = self.owner();
        for r in self.barrage_players().to_vec() {
            if let Some(p) = ctx.players.get_mut(r) {
                p.set_uint(&AttrId::local("promoted", owner), 1);
            }
        }
        if self.matches.is_empty() {
            self.create_matches();
        }
        self.update_promotions(ctx);
    }

    fn reset(&mut self, ctx: &mut StageContext) {
        common::reset_statuses(&self.base, ctx);
        self.result = None;
        self.output_short_list = None;
        self.matches.clear();
    }

    fn load(&mut self, ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), crate::Error> {
        common::load_configuration(&mut self.base, node);
        let tag = if ctx.settings.team_event { "Equipe" } else { "Tireur" };
        let fill = self.base.attendees.is_empty();
        let owner = self.owner();
        for n in node.children().filter(|n| n.is_element()) {
            if n.tag_name().name() == "Match" {
                self.load_match(n);
                continue;
            }
            if n.tag_name().name() != tag {
                break;
            }
            if let Some(r) = load_attendee(&self.base, ctx, n, owner) {
                if fill {
                    self.base.attendees.push(r);
                }
                if let Some(p) = ctx.players.get_mut(r) {
                    let q = p.get(&AttrId::local("status", owner)).map(|v| v.xml_image().starts_with('Q')).unwrap_or(false);
                    p.set_uint(&AttrId::local("promoted", owner), q as u32);
                }
            }
        }
        Ok(())
    }

    fn save(&self, ctx: &StageContext, w: &mut XmlWriter) {
        w.start(StageKind::Barrage.xml_name());
        self.save_configuration(w);
        save_attendees(&self.base, ctx, w, self.owner());
        for m in &self.matches {
            m.save(ctx.players, w);
        }
        w.end();
    }

    fn save_configuration(&self, w: &mut XmlWriter) {
        common::save_configuration(&self.base, w, true);
    }

    fn is_over(&self, _ctx: &StageContext) -> bool {
        self.matches_over()
    }

    fn classification(&self, ctx: &StageContext) -> Vec<RankEntry> {
        let list = self.compute_classification(ctx.players);
        let result: Vec<PlayerRef> = match &self.result {
            Some(r) => r.clone(),
            None => list.iter().map(|(p, _)| *p).collect(),
        };
        let rank_of = Self::rank_of(&list);
        let entries = common::rank_entries(ctx.players, self.owner(), &result, &rank_of);
        entries
    }

    fn output(&self, ctx: &StageContext) -> Vec<PlayerRef> {
        if let Some(o) = &self.output_short_list {
            return o.clone();
        }
        let list = self.compute_classification(ctx.players);
        let result: Vec<PlayerRef> = list.iter().map(|(p, _)| *p).collect();
        let rank_of = Self::rank_of(&list);
        let output = common::compute_output(&self.base, ctx.players, &result, &rank_of, true).output;
        output
    }

    fn lock(&mut self, ctx: &mut StageContext) {
        self.base.locked = true;
        let previous_output = self.output_short_list.take().unwrap_or_default();
        common::restore_not_qualified(self.owner(), ctx, &previous_output);
        let list = self.compute_classification(ctx.players);
        common::write_ranks(self.owner(), ctx, &list);
        let result: Vec<PlayerRef> = list.iter().map(|(p, _)| *p).collect();
        let c = common::compute_output(&self.base, ctx.players, &result, &Self::rank_of(&list), true);
        common::apply_output(&mut self.base, ctx, &c);
        self.result = Some(result);
        self.output_short_list = Some(c.output);
    }

    fn unlock(&mut self, _ctx: &mut StageContext) {
        self.result = None;
        self.output_short_list = None;
        self.base.quota_exceedance = 0;
        self.base.locked = false;
    }

    fn remap_players(&mut self, map: &std::collections::HashMap<PlayerRef, PlayerRef>) {
        let tr = |r: &mut PlayerRef| {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        };
        self.base.attendees.iter_mut().for_each(tr);
        self.result.iter_mut().flatten().for_each(tr);
        self.output_short_list.iter_mut().flatten().for_each(tr);
        for m in &mut self.matches {
            for o in &mut m.opponents {
                if let Some(f) = o.fencer.as_mut() {
                    tr(f);
                }
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Fabrique des phases de ce module (signature fixe, appelée par `rounds::create_stage`).
pub fn new_stage(kind: StageKind, id: u32) -> Box<dyn Stage> {
    match kind {
        StageKind::Barrage => Box::new(Barrage::new(id)),
        _ => Box::new(crate::stage::RawStage::new(kind, id)),
    }
}
