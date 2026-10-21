//! Les deux phases d'un tour de poules : l'allocateur (composition des
//! poules, fournisseur d'entrée) et le superviseur (saisie des scores et
//! classement du tour). Elles partagent un `PoolRound`.
//!
//! Dans le fichier, les deux phases n'ont qu'un élément `<TourDePoules>`,
//! écrit par l'allocateur (`Allocator::Save`) ; le superviseur n'écrit rien
//! (`Supervisor::Save` est vide).

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::player::PlayerRef;
use crate::stage::{RankEntry, Stage, StageBase, StageContext, StageError, StageKind};
use crate::xml::XmlWriter;
use crate::Error;

use super::round::PoolRound;

/// Tour de poules partagé entre l'allocateur et le superviseur.
pub type SharedRound = Arc<Mutex<PoolRound>>;

fn lock(round: &SharedRound) -> MutexGuard<'_, PoolRound> {
    round.lock().unwrap_or_else(|e| e.into_inner())
}

// ================================================================================
// Allocateur
// ================================================================================

pub struct PoolAllocatorStage {
    base: StageBase,
    round: SharedRound,
}

impl PoolAllocatorStage {
    pub fn new(kind: StageKind, id: u32) -> Self {
        let mut round = PoolRound::new();
        round.alloc_id = Some(id);
        PoolAllocatorStage { base: StageBase::new(kind, id), round: Arc::new(Mutex::new(round)) }
    }

    /// Le tour de poules, pour l'interface (composition, glisser-déposer…).
    pub fn round(&self) -> MutexGuard<'_, PoolRound> {
        lock(&self.round)
    }

    pub fn shared_round(&self) -> SharedRound {
        self.round.clone()
    }
}

impl Stage for PoolAllocatorStage {
    fn base(&self) -> &StageBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut StageBase {
        &mut self.base
    }

    fn is_input_provider(&self) -> bool {
        true
    }

    fn id_changed(&mut self) {
        lock(&self.round).alloc_id = Some(self.base.id);
    }

    fn name(&self) -> String {
        lock(&self.round).name.clone()
    }

    fn set_name(&mut self, name: &str) {
        lock(&self.round).name = name.to_string();
    }

    fn set_attendees(&mut self, ctx: &mut StageContext, attendees: Vec<PlayerRef>) {
        let mut round = lock(&self.round);
        round.team_event = ctx.settings.team_event;
        round.alloc_rand_seed = self.base.rand_seed;
        if !round.nb_qualified_valid {
            round.nb_qualified = attendees.len() as u32;
        }
        round.attendees = attendees.clone();
        self.base.attendees = attendees;
    }

    fn garnish(&mut self, ctx: &mut StageContext) {
        let mut round = lock(&self.round);
        round.team_event = ctx.settings.team_event;
        round.alloc_rand_seed = self.base.rand_seed;
        round.garnish(ctx.players);
    }

    fn reset(&mut self, ctx: &mut StageContext) {
        lock(&self.round).reset_allocator();
        crate::stage::default_reset(&self.base, ctx);
    }

    fn load(&mut self, ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), Error> {
        let mut round = lock(&self.round);
        round.team_event = ctx.settings.team_event;
        round.alloc_rand_seed = self.base.rand_seed;
        round.load(ctx.players, node);
        round.allocator_loaded = true;
        self.base.attendees = round.attendees.clone();
        Ok(())
    }

    fn save(&self, ctx: &StageContext, w: &mut XmlWriter) {
        lock(&self.round).save(ctx.players, w);
    }

    fn save_configuration(&self, w: &mut XmlWriter) {
        lock(&self.round).save_configuration(w);
    }

    fn is_over(&self, _ctx: &StageContext) -> bool {
        lock(&self.round).allocator_is_over()
    }

    fn error(&self, _ctx: &StageContext) -> Option<StageError> {
        lock(&self.round).allocator_error()
    }

    /// `GetCurrentClassification` : copie de la liste des participants.
    fn classification(&self, _ctx: &StageContext) -> Vec<RankEntry> {
        lock(&self.round)
            .attendees()
            .iter()
            .enumerate()
            .map(|(i, p)| RankEntry { player: *p, rank: i as u32 + 1, status: 'Q' })
            .collect()
    }

    fn output(&self, _ctx: &StageContext) -> Vec<PlayerRef> {
        lock(&self.round).attendees().to_vec()
    }

    fn lock(&mut self, ctx: &mut StageContext) {
        self.base.locked = true;
        lock(&self.round).lock_allocator(ctx.players);
    }

    fn unlock(&mut self, _ctx: &mut StageContext) {
        self.base.locked = false;
        lock(&self.round).unlock_allocator();
    }

    /// Le tour partagé est renuméroté par le superviseur, une seule fois.
    fn remap_players(&mut self, map: &HashMap<PlayerRef, PlayerRef>) {
        for r in self.base.attendees.iter_mut() {
            if let Some(n) = map.get(r) {
                *r = *n;
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

// ================================================================================
// Superviseur
// ================================================================================

pub struct PoolSupervisorStage {
    base: StageBase,
    round: SharedRound,
    linked: bool,
}

impl PoolSupervisorStage {
    pub fn new(kind: StageKind, id: u32) -> Self {
        let mut round = PoolRound::new();
        round.sup_id = Some(id);
        PoolSupervisorStage { base: StageBase::new(kind, id), round: Arc::new(Mutex::new(round)), linked: false }
    }

    pub fn round(&self) -> MutexGuard<'_, PoolRound> {
        lock(&self.round)
    }

    pub fn shared_round(&self) -> SharedRound {
        self.round.clone()
    }

    /// `OnStuffClicked` : scores aléatoires dans toutes les poules (tests).
    pub fn stuff(&mut self, ctx: &mut StageContext) {
        lock(&self.round).stuff(ctx.players);
    }
}

impl Stage for PoolSupervisorStage {
    fn base(&self) -> &StageBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut StageBase {
        &mut self.base
    }

    fn input_provider(&self) -> Option<StageKind> {
        Some(StageKind::PoolAllocator)
    }

    /// `SetInputProvider` : l'allocateur et le superviseur partagent le tour.
    fn attach_input_provider(&mut self, provider: &mut dyn Stage) {
        if let Some(allocator) = provider.as_any_mut().downcast_mut::<PoolAllocatorStage>() {
            self.round = allocator.round.clone();
            let mut round = lock(&self.round);
            round.allocator_linked = true;
            round.supervisor_linked = true;
            round.alloc_id = Some(allocator.base.id);
            round.sup_id = Some(self.base.id);
            self.linked = true;
        }
    }

    fn id_changed(&mut self) {
        lock(&self.round).sup_id = Some(self.base.id);
    }

    fn bout_max_score(&self) -> Option<u32> {
        Some(lock(&self.round).max_score)
    }

    fn qualified_quota(&self) -> Option<u32> {
        lock(&self.round).nb_qualified()
    }

    fn name(&self) -> String {
        lock(&self.round).name.clone()
    }

    fn set_name(&mut self, name: &str) {
        lock(&self.round).name = name.to_string();
    }

    /// `OnPlugged` → `RetrievePools` : les poules passent sous la
    /// responsabilité du superviseur.
    fn on_plugged(&mut self, ctx: &mut StageContext) {
        let mut round = lock(&self.round);
        round.team_event = ctx.settings.team_event;
        round.sup_rand_seed = self.base.rand_seed;
        round.recheck_distribution(ctx.players);
        round.manage_pools(ctx.players);
    }

    fn set_attendees(&mut self, ctx: &mut StageContext, attendees: Vec<PlayerRef>) {
        let _ = ctx;
        self.base.attendees = attendees;
    }

    fn garnish(&mut self, ctx: &mut StageContext) {
        let mut round = lock(&self.round);
        round.sup_rand_seed = self.base.rand_seed;
        if !round.managed {
            round.manage_pools(ctx.players);
        }
    }

    /// `OnUnPlugged` : scores et matchs effacés.
    fn reset(&mut self, ctx: &mut StageContext) {
        lock(&self.round).unplug_supervisor(ctx.players);
        crate::stage::default_reset(&self.base, ctx);
    }

    fn load(&mut self, ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), Error> {
        let mut round = lock(&self.round);
        round.team_event = ctx.settings.team_event;
        round.sup_rand_seed = self.base.rand_seed;
        round.load_configuration(node);
        round.supervisor_loaded = true;
        Ok(())
    }

    /// Rien : l'allocateur a écrit l'élément `<TourDePoules>`.
    fn save(&self, _ctx: &StageContext, w: &mut XmlWriter) {
        if !self.linked {
            // Superviseur sans allocateur (ne se produit pas dans une
            // compétition) : on garde au moins sa configuration.
            w.start("TourDePoules");
            lock(&self.round).save_configuration(w);
            w.end();
        }
    }

    fn save_configuration(&self, w: &mut XmlWriter) {
        lock(&self.round).save_configuration(w);
    }

    fn is_over(&self, _ctx: &StageContext) -> bool {
        lock(&self.round).supervisor_is_over()
    }

    fn error(&self, _ctx: &StageContext) -> Option<StageError> {
        lock(&self.round).supervisor_error()
    }

    fn classification(&self, ctx: &StageContext) -> Vec<RankEntry> {
        lock(&self.round).supervisor_classification(ctx.players)
    }

    fn output(&self, ctx: &StageContext) -> Vec<PlayerRef> {
        lock(&self.round).supervisor_output(ctx.players)
    }

    /// `Stage::Lock` : poules verrouillées, puis classement et qualifiés figés.
    fn lock(&mut self, ctx: &mut StageContext) {
        self.base.locked = true;
        let mut round = lock(&self.round);
        round.sup_rand_seed = self.base.rand_seed;
        if !round.managed {
            round.manage_pools(ctx.players);
        }
        round.lock_supervisor(ctx.players);
        round.set_supervisor_result(ctx.players);
        self.base.quota_exceedance = round.quota_exceedance;
    }

    fn unlock(&mut self, _ctx: &mut StageContext) {
        self.base.locked = false;
        self.base.quota_exceedance = 0;
        lock(&self.round).unlock_supervisor();
    }

    fn remap_players(&mut self, map: &HashMap<PlayerRef, PlayerRef>) {
        for r in self.base.attendees.iter_mut() {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        }
        lock(&self.round).remap_refs(map);
    }

    fn on_loading_completed(&mut self, ctx: &mut StageContext) {
        lock(&self.round).recheck_distribution(ctx.players);
    }

    fn dump_to_html(&self, ctx: &StageContext, html: &mut String) {
        html.push_str(&lock(&self.round).dump_to_html(ctx.players));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
