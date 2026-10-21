//! Point de scission (port de `rounds/splitting/splitting.cpp`).
//!
//! L'utilisateur coche « exporté » (attribut global `exported`) pour les
//! tireurs à extraire dans une nouvelle compétition (`Split`). Au
//! verrouillage, l'application duplique la compétition avec ces tireurs
//! (`exported_players`) ; les autres continuent avec leur rang précédent.

use std::any::Any;

use crate::i18n::tr;
use crate::player::{AttrId, Owner, PlayerRef, PlayerStore};
use crate::rounds::table::common::{self, codes, compare_players};
use crate::stage::{PreviousStage, RankEntry, Stage, StageBase, StageContext, StageKind};

use crate::xml::XmlWriter;

pub struct Splitting {
    base: StageBase,
    previous: Option<PreviousStage>,
    result: Option<Vec<PlayerRef>>,
}

impl Splitting {
    pub fn new(id: u32) -> Self {
        let mut base = StageBase::new(StageKind::Splitting, id);
        base.filter = codes(&[
            "exported",
            "stage_start_rank",
            "name",
            "first_name",
            "birth_date",
            "gender",
            "club",
            "country",
            "licence",
        ]);
        Splitting { base, previous: None, result: None }
    }

    fn owner(&self) -> Owner {
        self.base.owner()
    }

    /// Coche ou décoche un tireur à extraire.
    pub fn set_exported(&mut self, ctx: &mut StageContext, r: PlayerRef, value: bool) {
        if self.base.locked {
            return;
        }
        if let Some(p) = ctx.players.get_mut(r) {
            p.set_uint(&AttrId::global("exported"), value as u32);
        }
    }

    /// Tireurs à reprendre dans la nouvelle compétition, avec leur rang
    /// d'entrée (position dans la liste + 1), comme `OnLocked`. L'application
    /// les duplique (attribut `exported` remis à faux, membres d'équipe compris).
    pub fn exported_players(&self, players: &PlayerStore) -> Vec<(PlayerRef, u32)> {
        self.base
            .attendees
            .iter()
            .enumerate()
            .filter(|(_, r)| exported(players, **r))
            .map(|(i, r)| (*r, i as u32 + 1))
            .collect()
    }

    /// Tireurs restants (`GetRemainingList`), par rang de départ.
    pub fn remaining(&self, players: &PlayerStore) -> Vec<PlayerRef> {
        let mut list: Vec<PlayerRef> = self.base.attendees.iter().copied().filter(|r| !exported(players, *r)).collect();
        let id = AttrId::local("stage_start_rank", self.owner());
        crate::sort::glib_sort_by(&mut list, |a, b| compare_players(players, *a, *b, &id, self.base.rand_seed));
        list
    }

    fn previous_rank(&self, players: &PlayerStore, r: PlayerRef) -> u32 {
        let id = common::previous_rank_id(self.previous, self.owner());
        players.get(r).and_then(|p| p.get(&id)).map(|v| v.as_uint()).unwrap_or(0)
    }

    /// Nom à imprimer (`GetPrintName`).
    pub fn print_name(&self) -> String {
        format!("{} - {}", tr("Split"), self.base.name)
    }

    fn clear_exported(&self, ctx: &mut StageContext) {
        for r in &self.base.attendees {
            if let Some(p) = ctx.players.get_mut(*r) {
                p.set_uint(&AttrId::global("exported"), 0);
            }
        }
    }
}

impl Stage for Splitting {
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

    fn reset(&mut self, ctx: &mut StageContext) {
        self.clear_exported(ctx);
        self.result = None;
    }

    fn load(&mut self, ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), crate::Error> {
        common::load_configuration(&mut self.base, node);
        let attendees = self.base.attendees.clone();
        common::retrieve_attendees(&mut self.base, ctx, attendees);
        Ok(())
    }

    fn save(&self, _ctx: &StageContext, w: &mut XmlWriter) {
        w.start(StageKind::Splitting.xml_name());
        self.save_configuration(w);
        w.end();
    }

    fn save_configuration(&self, w: &mut XmlWriter) {
        common::save_configuration(&self.base, w, false);
    }

    fn is_over(&self, _ctx: &StageContext) -> bool {
        true
    }

    fn classification(&self, ctx: &StageContext) -> Vec<RankEntry> {
        let result = self.result.clone().unwrap_or_else(|| self.remaining(ctx.players));
        let rank = |r| self.previous_rank(ctx.players, r);
        common::rank_entries(ctx.players, self.owner(), &result, &rank)
    }

    fn output(&self, ctx: &StageContext) -> Vec<PlayerRef> {
        self.result.clone().unwrap_or_else(|| self.remaining(ctx.players))
    }

    fn lock(&mut self, ctx: &mut StageContext) {
        self.base.locked = true;
        let ranks: Vec<(PlayerRef, u32)> =
            self.base.attendees.iter().map(|r| (*r, self.previous_rank(ctx.players, *r))).collect();
        common::write_ranks(self.owner(), ctx, &ranks);
        self.result = Some(self.remaining(ctx.players));
    }

    fn unlock(&mut self, ctx: &mut StageContext) {
        self.result = None;
        self.base.locked = false;
        self.clear_exported(ctx);
    }

    fn remap_players(&mut self, map: &std::collections::HashMap<PlayerRef, PlayerRef>) {
        let tr = |r: &mut PlayerRef| {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        };
        self.base.attendees.iter_mut().for_each(tr);
        self.result.iter_mut().flatten().for_each(tr);
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
        StageKind::Splitting => Box::new(Splitting::new(id)),
        _ => Box::new(crate::stage::RawStage::new(kind, id)),
    }
}

/// Attribut global `exported` (absent = faux).
fn exported(players: &PlayerStore, r: PlayerRef) -> bool {
    players.get(r).and_then(|p| p.attr("exported")).map(|v| v.as_bool()).unwrap_or(false)
}
