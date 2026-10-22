//! Entraînement des jeunes : placement d'un groupe entier (ses poules) sur
//! une ou plusieurs pistes avec un ou plusieurs arbitres, et contrôle des
//! arbitres du même club que les tireurs.

use crate::{
    hall::Hall,
    slot::{JobKey, PisteKey},
    time::add,
};
use bp_core::i18n::tr;

/// Arbitre du même club qu'au moins un tireur d'un match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClubConflict {
    pub referee: u32,
    pub referee_name: String,
    pub club: String,
    pub job_name: String,
    pub fencers: Vec<String>,
}

impl ClubConflict {
    /// « DUPONT Jean (YERRES) : même club que MARTIN Lina, DURAND Noé (Poule n° 2) ».
    pub fn message(&self) -> String {
        format!("{} ({}) : même club que {} ({})", self.referee_name, self.club, self.fencers.join(", "), self.job_name)
    }
}

impl Hall {
    /// Poules du groupe `group` du lot, dans l'ordre d'affichage.
    pub fn group_pools(&self, competition: u32, batch: u32, group: u32) -> Vec<JobKey> {
        let mut pools: Vec<_> = self
            .jobs
            .values()
            .filter(|j| j.competition == competition && j.batch == batch && j.is_pool && j.group == Some(group))
            .map(|j| (j.position, j.key))
            .collect();
        pools.sort();
        pools.into_iter().map(|(_, k)| k).collect()
    }

    /// Groupes du lot (numéros croissants).
    pub fn batch_groups(&self, competition: u32, batch: u32) -> Vec<u32> {
        let mut groups: Vec<u32> = self
            .jobs
            .values()
            .filter(|j| j.competition == competition && j.batch == batch && j.is_pool)
            .filter_map(|j| j.group)
            .collect();
        groups.sort();
        groups.dedup();
        groups
    }

    /// Un arbitre peut-il être donné au groupe ? Présent, de l'arme, et
    /// libre du curseur jusqu'à la fin du groupe joué sur une seule piste
    /// (ses propres créneaux dans le groupe ne comptent pas).
    pub fn group_referee_available(&self, competition: u32, batch: u32, group: u32, reference: u32) -> bool {
        let pools = self.group_pools(competition, batch, group);
        let Some(comp) = self.competition(competition) else { return false };
        let Some(r) = self.referees.get(reference) else { return false };
        let own: Vec<_> = pools.iter().filter_map(|j| self.jobs.get(j).and_then(|j| j.slot)).collect();
        if !(r.player.str_attr("weapon").is_some_and(|w| w.contains(comp.weapon.as_str())) && (r.player.attending() || own.iter().any(|sk| self.slots.get(sk).is_some_and(|s| s.referees.contains(&reference))))) {
            return false;
        }
        let start = self.timeline.cursor_time(true);
        let total: crate::time::Span = pools.iter().filter_map(|j| self.jobs.get(j)).map(|j| j.duration()).sum();
        let end = add(start, total);
        !r.slots
            .iter()
            .filter(|sk| !own.contains(sk))
            .filter_map(|sk| self.slots.get(sk))
            .any(|other| start < add(other.start, other.duration) && other.start < end)
    }

    fn take_off_job(&mut self, job: JobKey) {
        if let Some(sk) = self.jobs.get(&job).and_then(|j| j.slot) {
            self.slot_remove_job(sk, job);
            self.gc_slots();
        }
    }

    /// Place toutes les poules d'un groupe ensemble, à partir du curseur de
    /// la frise : un seul créneau sur les pistes du groupe, avec ses arbitres.
    /// BellePoule entrelace alors les matchs des poules (M1 poule 1, M2 poule 2…),
    /// les répartit sur les pistes et fait alterner les arbitres.
    pub fn place_group(&mut self, competition: u32, batch: u32, group: u32, pistes: &[PisteKey], referees: &[u32]) -> Result<(), String> {
        let pools = self.group_pools(competition, batch, group);
        if pools.is_empty() {
            return Err(tr("No pool in this group."));
        }
        let mut list: Vec<PisteKey> = Vec::new();
        for p in pistes {
            if !list.contains(p) {
                list.push(*p);
            }
        }
        if list.is_empty() {
            return Err(tr("Select at least one piste."));
        }
        let mut refs: Vec<u32> = Vec::new();
        for r in referees {
            if !refs.contains(r) {
                refs.push(*r);
            }
        }
        // Durée : les matchs du groupe se partagent les pistes.
        let total: crate::time::Span = pools.iter().filter_map(|j| self.jobs.get(j)).map(|j| j.duration()).sum();
        let duration = (total + list.len() as crate::time::Span - 1) / list.len() as crate::time::Span;
        let start = self.timeline.cursor_time(true);
        let end = add(start, duration);
        let own: Vec<_> = pools.iter().filter_map(|j| self.jobs.get(j).and_then(|j| j.slot)).collect();
        let overlaps = |other: &crate::slot::Slot| start < add(other.start, other.duration) && other.start < end;

        // Vérifications avant toute modification.
        for key in &list {
            let p = self.piste(*key).ok_or_else(|| tr("Piste not found."))?;
            if p.blocked || p.slots.iter().any(|k| !own.contains(k) && self.slots.get(k).is_some_and(&overlaps)) {
                return Err(format!("{} {}", tr("Piste unavailable:"), p.id));
            }
        }
        for r in &refs {
            let referee = self.referees.get(*r).ok_or_else(|| tr("Referee not found."))?;
            if referee.slots.iter().any(|k| !own.contains(k) && self.slots.get(k).is_some_and(&overlaps)) {
                return Err(format!("{} {}", tr("Referee unavailable:"), referee.player.name()));
            }
        }

        for job in &pools {
            self.take_off_job(*job);
        }
        let mut slot = crate::slot::Slot::new(Some(list[0]), start, Some(end), duration);
        slot.additional_pistes = list[1..].to_vec();
        let sk = self.slot_insert(slot);
        for job in &pools {
            self.slot_add_job(sk, *job);
        }
        self.slot_set_duration(sk, duration);
        for r in refs.iter().rev() {
            self.slot_add_referee(sk, *r);
        }
        if let Some(s) = self.slots.get_mut(&sk) {
            s.referees = refs.clone();
        }
        for job in &pools {
            self.job_make_referee_parcel(*job);
            self.job_make_piste_parcel(*job);
            self.refresh_job_status(*job);
            self.job_spread(*job);
        }
        self.batch_spread(competition, batch);
        for r in &refs {
            self.refresh_referee_workloads(*r);
        }
        self.refresh_availability();
        self.redraw();
        Ok(())
    }

    /// Arbitres du même club qu'un tireur du match (information à faire
    /// confirmer avant l'affectation).
    pub fn club_conflicts(&self, job: JobKey, referees: &[u32]) -> Vec<ClubConflict> {
        let Some(j) = self.jobs.get(&job) else { return Vec::new() };
        let norm = |s: &str| s.trim().to_lowercase();
        let mut out = Vec::new();
        for r in referees {
            let Some(referee) = self.referees.get(*r) else { continue };
            let club = referee.player.str_attr("club").unwrap_or_default().to_string();
            if norm(&club).is_empty() {
                continue;
            }
            let fencers: Vec<String> = j
                .fencers
                .iter()
                .filter(|f| {
                    std::iter::once(&f.player).chain(f.members.iter().map(|(p, _)| p)).any(|p| norm(p.str_attr("club").unwrap_or_default()) == norm(&club))
                })
                .map(|f| format!("{} {}", f.player.name(), f.player.first_name()).trim().to_string())
                .collect();
            if !fencers.is_empty() {
                out.push(ClubConflict {
                    referee: *r,
                    referee_name: format!("{} {}", referee.player.name(), referee.player.first_name()).trim().to_string(),
                    club,
                    job_name: j.name.clone(),
                    fencers,
                });
            }
        }
        out
    }

    /// Même contrôle pour l'arbitre déposé sur une piste (créneau sous le curseur).
    pub fn referee_drop_conflicts(&self, referee: u32, piste: PisteKey) -> Vec<ClubConflict> {
        let cursor = self.timeline.cursor_time(false);
        let Some(slot) = self.piste_slot_at(piste, cursor).and_then(|sk| self.slots.get(&sk)) else { return Vec::new() };
        slot.jobs.iter().flat_map(|job| self.club_conflicts(*job, &[referee])).collect()
    }

    /// Même contrôle pour toutes les poules d'un groupe.
    pub fn group_club_conflicts(&self, competition: u32, batch: u32, group: u32, referees: &[u32]) -> Vec<ClubConflict> {
        self.group_pools(competition, batch, group).into_iter().flat_map(|job| self.club_conflicts(job, referees)).collect()
    }
}
