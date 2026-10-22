//! Réservation commune de plusieurs pistes et arbitres pour une poule.

use crate::{
    hall::Hall,
    slot::{JobKey, PisteKey},
    time::add,
};
use bp_core::i18n::tr;

impl Hall {
    /// Catalogue partagé, indépendant de la présence de compétitions ouvertes.
    pub fn save_referee_catalog(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let mut w = bp_core::xml::XmlWriter::new();
        w.start("Arbitres");
        for referee in self.referees.referees.values() {
            referee.player.save(&mut w, false);
        }
        w.end();
        let xml = w.finish();
        if std::fs::read(path).ok().as_deref() == Some(xml.as_bytes()) {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("xml.tmp");
        std::fs::write(&temporary, xml)?;
        std::fs::rename(temporary, path)
    }

    pub fn load_referee_catalog(&mut self, path: &std::path::Path) -> bool {
        let Ok(xml) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(doc) = bp_core::xml::parse(&xml) else {
            return false;
        };
        if !doc.root_element().has_tag_name("Arbitres") {
            return false;
        }
        for node in doc
            .root_element()
            .children()
            .filter(|n| n.has_tag_name("Arbitre"))
        {
            let mut player = bp_core::Player::new(bp_core::PlayerClass::Referee);
            player.load(node);
            let weapons = player.str_attr("weapon").unwrap_or_default().to_string();
            let lists: Vec<_> = self
                .referees
                .lists
                .iter()
                .enumerate()
                .filter(|(_, l)| weapons.contains(l.weapon.xml))
                .map(|(i, _)| i)
                .collect();
            for list in lists {
                self.add_referee_to_list(list, player.clone());
            }
        }
        true
    }

    /// Affectation d'arbitres sans obligation de créer ni de choisir une piste.
    pub fn set_job_referees(&mut self, job: JobKey, referees: &[u32]) -> Result<(), String> {
        let j = self.jobs.get(&job).ok_or_else(|| tr("Pool not found."))?;
        let (competition, batch, existing) = (j.competition, j.batch, j.slot);
        let mut selected = Vec::new();
        for r in referees {
            if !selected.contains(r) {
                selected.push(*r);
            }
        }
        for r in &selected {
            if !self.pool_referee_available(job, *r) {
                return Err(tr("Referee unavailable:"));
            }
        }
        let sk = if let Some(sk) = existing {
            sk
        } else {
            if selected.is_empty() {
                return Ok(());
            }
            let duration = j.duration();
            let sk = self.slot_insert(crate::slot::Slot::new(
                None,
                self.timeline.cursor_time(false),
                None,
                duration,
            ));
            self.slot_add_job(sk, job);
            sk
        };
        let old = self
            .slots
            .get(&sk)
            .map(|s| s.referees.clone())
            .unwrap_or_default();
        if self
            .slots
            .get(&sk)
            .is_some_and(|s| !s.additional_pistes.is_empty() && selected.len() < s.pistes().len())
        {
            return Err(tr("Assign at least one referee to each piste."));
        }
        for r in &old {
            if !selected.contains(r) {
                self.slot_remove_referee(sk, *r);
            }
        }
        for r in &selected {
            if !old.contains(r) {
                self.slot_add_referee(sk, *r);
            }
        }
        if let Some(slot) = self.slots.get_mut(&sk) {
            slot.referees = selected.clone();
        }
        self.job_make_referee_parcel(job);
        self.job_make_piste_parcel(job);
        self.job_spread(job);
        self.batch_spread(competition, batch);
        for r in old.iter().chain(selected.iter()) {
            self.refresh_referee_workloads(*r);
        }
        self.refresh_availability();
        self.redraw();
        Ok(())
    }

    /// Disponibilité sur tout le créneau de la poule, en ignorant sa propre
    /// réservation pour conserver les arbitres qui lui sont déjà affectés.
    pub fn pool_referee_available(&self, job: JobKey, reference: u32) -> bool {
        let Some(j) = self.jobs.get(&job) else {
            return false;
        };
        let sk = j.slot;
        let fallback =
            crate::slot::Slot::new(None, self.timeline.cursor_time(false), None, j.duration());
        let slot = sk.and_then(|key| self.slots.get(&key)).unwrap_or(&fallback);
        let Some(competition) = self.competition(j.competition) else {
            return false;
        };
        let Some(r) = self.referees.get(reference) else {
            return false;
        };
        r.player
            .str_attr("weapon")
            .is_some_and(|w| w.contains(&competition.weapon))
            && (r.player.attending() || slot.referees.contains(&reference))
            && !r
                .slots
                .iter()
                .filter(|key| Some(**key) != sk)
                .filter_map(|key| self.slots.get(key))
                .any(|other| {
                    slot.start < add(other.start, other.duration)
                        && other.start < add(slot.start, slot.duration)
                })
    }

    /// Les poules planifiées restent glissables pour ajouter une piste.
    pub fn batch_draggable_jobs(
        &self,
        competition: u32,
        batch: u32,
        selection: &[JobKey],
    ) -> Vec<JobKey> {
        let pending = self.batch_pending_selected(competition, batch, selection);
        let mut jobs: Vec<_> = selection
            .iter()
            .copied()
            .filter(|key| {
                pending.contains(key)
                    || self.jobs.get(key).is_some_and(|job| {
                        job.competition == competition
                            && job.batch == batch
                            && job.is_pool
                            && job.slot.is_some()
                    })
            })
            .collect();
        jobs.sort_by_key(|key| self.jobs.get(key).map(|j| j.position));
        jobs
    }

    /// Une piste ajoutée reprend l'horaire de la poule, indépendamment du curseur.
    pub fn can_add_pool_piste(&self, job: JobKey, piste: PisteKey) -> bool {
        let Some(j) = self.jobs.get(&job).filter(|j| j.is_pool) else {
            return false;
        };
        let Some(slot) = j.slot.and_then(|sk| self.slots.get(&sk)) else {
            return false;
        };
        let Some(target) = self.piste(piste) else {
            return false;
        };
        !target.blocked
            && !slot.pistes().contains(&piste)
            && !target
                .slots
                .iter()
                .filter_map(|sk| self.slots.get(sk))
                .any(|other| {
                    slot.start < add(other.start, other.duration)
                        && other.start < add(slot.start, slot.duration)
                })
    }

    /// Met à jour atomiquement les ressources, après vérification des conflits.
    pub fn set_pool_resources(
        &mut self,
        job: JobKey,
        pistes: &[PisteKey],
        referees: &[u32],
    ) -> Result<(), String> {
        self.set_pool_resources_optional(job, pistes, referees, true)
    }

    pub fn set_pool_resources_optional(
        &mut self,
        job: JobKey,
        pistes: &[PisteKey],
        referees: &[u32],
        require_referees: bool,
    ) -> Result<(), String> {
        let j = self.jobs.get(&job).ok_or_else(|| tr("Pool not found."))?;
        if !j.is_pool {
            return Err(tr("These resources are only available for pools."));
        }
        let sk = j
            .slot
            .ok_or_else(|| tr("Place the pool on a piste first."))?;
        let old = self
            .slots
            .get(&sk)
            .cloned()
            .ok_or_else(|| tr("Pool not found."))?;
        let (competition, batch) = (j.competition, j.batch);
        let unique = |items: &[u32]| {
            let mut out = Vec::new();
            for &v in items {
                if !out.contains(&v) {
                    out.push(v);
                }
            }
            out
        };
        let pistes = unique(pistes);
        let referees = unique(referees);
        if pistes.is_empty() {
            return Err(tr("Select at least one piste."));
        }
        if require_referees && pistes.len() > 1 && referees.len() < pistes.len() {
            return Err(tr("Assign at least one referee to each piste."));
        }
        let overlaps = |other: &crate::slot::Slot| {
            old.start < add(other.start, other.duration)
                && other.start < add(old.start, old.duration)
        };
        for key in &pistes {
            let p = self.piste(*key).ok_or_else(|| tr("Piste not found."))?;
            if (p.blocked && !old.pistes().contains(key))
                || p.slots
                    .iter()
                    .any(|k| *k != sk && self.slots.get(k).is_some_and(&overlaps))
            {
                return Err(format!("{} {}", tr("Piste unavailable:"), p.id));
            }
        }
        for reference in &referees {
            let r = self
                .referees
                .get(*reference)
                .ok_or_else(|| tr("Referee not found."))?;
            if require_referees && !self.pool_referee_available(job, *reference) {
                return Err(format!(
                    "{} {}",
                    tr("Referee unavailable:"),
                    r.player.name()
                ));
            }
        }
        // Toutes les validations précèdent la première modification.
        for key in old.pistes() {
            if let Some(p) = self.piste_mut(key) {
                p.slots.retain(|k| *k != sk);
            }
        }
        if let Some(slot) = self.slots.get_mut(&sk) {
            slot.piste = Some(pistes[0]);
            slot.additional_pistes = pistes[1..].to_vec();
        }
        for key in &pistes {
            let slots = &self.slots;
            if let Some(p) = self.pistes.iter_mut().find(|p| p.key == *key) {
                p.slots.push(sk);
                p.slots.sort_by_key(|k| slots.get(k).map(|s| s.start));
            }
        }
        for r in &old.referees {
            if !referees.contains(r) {
                self.slot_remove_referee(sk, *r);
            }
        }
        for r in &referees {
            if !old.referees.contains(r) {
                self.slot_add_referee(sk, *r);
            }
        }
        if let Some(slot) = self.slots.get_mut(&sk) {
            slot.referees = referees.clone();
        }
        self.job_make_referee_parcel(job);
        self.job_make_piste_parcel(job);
        self.refresh_job_status(job);
        self.job_spread(job);
        self.batch_spread(competition, batch);
        for r in old.referees.iter().chain(referees.iter()) {
            self.refresh_referee_workloads(*r);
        }
        self.refresh_availability();
        self.redraw();
        Ok(())
    }

    /// Heure de début choisie à la main (double-clic sur une piste ou un
    /// match). Les pistes et les arbitres du match doivent être libres à la
    /// nouvelle heure ; un match sans créneau en reçoit un, sans piste.
    pub fn set_job_start(&mut self, job: JobKey, start: crate::time::Time) -> Result<(), String> {
        let j = self.jobs.get(&job).ok_or_else(|| tr("Pool not found."))?;
        let (competition, batch, duration) = (j.competition, j.batch, j.duration());
        let Some(sk) = j.slot else {
            let sk = self.slot_insert(crate::slot::Slot::new(None, start, Some(add(start, duration)), duration));
            self.slot_add_job(sk, job);
            self.job_spread(job);
            self.batch_spread(competition, batch);
            self.redraw();
            return Ok(());
        };
        let slot = self.slots.get(&sk).cloned().ok_or_else(|| tr("Pool not found."))?;
        let end = add(start, slot.duration);
        let overlaps = |other: &crate::slot::Slot| start < add(other.start, other.duration) && other.start < end;
        for key in slot.pistes() {
            let Some(p) = self.piste(key) else { continue };
            if p.slots.iter().any(|k| *k != sk && self.slots.get(k).is_some_and(&overlaps)) {
                return Err(format!("{} {}", tr("Piste unavailable:"), p.id));
            }
        }
        for r in &slot.referees {
            let Some(referee) = self.referees.get(*r) else { continue };
            if referee.slots.iter().any(|k| *k != sk && self.slots.get(k).is_some_and(&overlaps)) {
                return Err(format!("{} {}", tr("Referee unavailable:"), referee.player.name()));
            }
        }
        // Toutes les vérifications précèdent la modification.
        if let Some(s) = self.slots.get_mut(&sk) {
            s.start = start;
            s.end = Some(end);
        }
        let starts: std::collections::HashMap<crate::slot::SlotKey, crate::time::Time> =
            self.slots.iter().map(|(k, s)| (*k, s.start)).collect();
        for key in slot.pistes() {
            if let Some(p) = self.piste_mut(key) {
                p.slots.sort_by_key(|k| starts.get(k).copied());
            }
        }
        for r in &slot.referees {
            if let Some(referee) = self.referees.referees.get_mut(r) {
                referee.slots.sort_by_key(|k| starts.get(k).copied());
            }
        }
        let piste_id = slot.piste.and_then(|k| self.piste(k)).map(|p| p.id).unwrap_or(0);
        for j in &slot.jobs {
            self.job_set_piste(*j, piste_id, start);
            self.job_make_piste_parcel(*j);
            self.refresh_job_status(*j);
            self.job_spread(*j);
        }
        self.batch_spread(competition, batch);
        self.refresh_availability();
        self.redraw();
        Ok(())
    }

    pub(crate) fn job_make_piste_parcel(&mut self, job: JobKey) {
        let pistes: Vec<_> = self
            .jobs
            .get(&job)
            .and_then(|j| j.slot)
            .and_then(|sk| self.slots.get(&sk))
            .map(|s| {
                s.pistes()
                    .into_iter()
                    .filter_map(|k| self.piste(k).map(|p| p.id))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(parcel) = self.jobs.get_mut(&job).and_then(|j| j.parcel.as_mut()) {
            parcel.set_uint("piste", pistes.first().copied().unwrap_or(0));
            parcel.set_list("pistes", &pistes);
        }
    }
}
