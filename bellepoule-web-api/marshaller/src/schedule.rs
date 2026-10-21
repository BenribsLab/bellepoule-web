//! Planning : créneaux réservés, matchs et état des lots
//! (port des parties de `slot.cpp`, `job.cpp`, `batch.cpp`, `piste.cpp` et
//! `competition.cpp` qui relient ces objets entre eux).

use crate::hall::Hall;
use crate::model::{recall, spread, Batch, BatchStatus, JobStatus};
use crate::slot::{self, JobKey, PisteKey, Slot, SlotKey, OVERLAP_TOLERANCE};
use crate::time::{self, add, diff, insert_sorted, Span, Time, MINUTE};

impl Hall {
    // --------------------------------------------------------------------------------
    // Arène des créneaux

    /// Enregistre un créneau (libre ou nouveau) dans l'arène.
    pub fn slot_insert(&mut self, s: Slot) -> SlotKey {
        let k = self.next_slot;
        self.next_slot += 1;
        self.slots.insert(k, s);
        k
    }

    /// Supprime les créneaux qui ne sont plus réservés sur aucune piste
    /// (équivalent des `Release` du C++).
    pub fn gc_slots(&mut self) {
        let used: std::collections::BTreeSet<SlotKey> =
            self.pistes.iter().flat_map(|p| p.slots.iter().copied()).collect();
        self.slots.retain(|k, s| used.contains(k) || !s.jobs.is_empty());
    }

    /// Créneaux réservés d'une piste, dans l'ordre.
    pub fn piste_slots(&self, piste: PisteKey) -> Vec<&Slot> {
        self.piste(piste).map(|p| p.slots.iter().filter_map(|k| self.slots.get(k)).collect()).unwrap_or_default()
    }

    /// `Piste::GetFreeSlots` (aucun si la piste est bloquée).
    pub fn piste_free_slots(&self, piste: PisteKey, from: Time, duration: Span) -> Vec<Slot> {
        match self.piste(piste) {
            Some(p) if !p.blocked => slot::free_slots(Some(piste), &self.piste_slots(piste), from, duration),
            _ => Vec::new(),
        }
    }

    /// `Piste::GetFreeSlot`.
    pub fn piste_free_slot(&self, piste: PisteKey, from: Time, duration: Span) -> Option<Slot> {
        match self.piste(piste) {
            Some(p) if !p.blocked => slot::free_slot(Some(piste), &self.piste_slots(piste), from, duration),
            _ => None,
        }
    }

    /// `Piste::GetSlotAt`.
    pub fn piste_slot_at(&self, piste: PisteKey, t: Time) -> Option<SlotKey> {
        let p = self.piste(piste)?;
        slot::slot_at(t, p.slots.iter().filter_map(|k| self.slots.get(k).map(|s| (*k, s))))
    }

    /// `Piste::GetSlotAfter` / `EnlistedReferee::GetSlotAfter`.
    fn slot_after(list: &[SlotKey], sk: SlotKey) -> Option<SlotKey> {
        let i = list.iter().position(|k| *k == sk)?;
        list.get(i + 1).copied()
    }

    /// `Slot::IsOver`.
    pub fn slot_is_over(&self, sk: SlotKey) -> bool {
        match self.slots.get(&sk) {
            Some(s) => !s.jobs.is_empty() && s.jobs.iter().all(|j| self.jobs.get(j).map(|j| j.over).unwrap_or(false)),
            None => false,
        }
    }

    /// `Slot::SetDuration`.
    pub fn slot_set_duration(&mut self, sk: SlotKey, duration: Span) {
        if !self.slot_is_over(sk) {
            if let Some(s) = self.slots.get_mut(&sk) {
                s.duration = duration;
                s.end = Some(add(s.start, duration));
            }
        }
    }

    /// `Slot::AddJob`.
    pub fn slot_add_job(&mut self, sk: SlotKey, job: JobKey) {
        let Some(s) = self.slots.get(&sk) else { return };
        if s.jobs.contains(&job) {
            return;
        }
        let Some(duration) = self.jobs.get(&job).map(|j| j.duration()) else { return };
        self.slot_set_duration(sk, duration);

        let s = self.slots.get_mut(&sk).unwrap();
        let assigned = !s.jobs.is_empty();
        s.jobs.push(job);
        let (piste, start, has_referees) = (s.piste, s.start, !s.referees.is_empty());
        if let Some(j) = self.jobs.get_mut(&job) {
            j.slot = Some(sk);
        }
        let piste_id = piste.and_then(|p| self.piste(p)).map(|p| p.id).unwrap_or(0);
        self.job_set_piste(job, piste_id, start);
        self.job_make_piste_parcel(job);
        if has_referees {
            self.job_make_referee_parcel(job);
        }
        self.refresh_job_status(job);

        if !assigned {
            for pk in self.slots.get(&sk).map(|s| s.pistes()).unwrap_or_default() {
                let slots = &self.slots;
                if let Some(p) = self.pistes.iter_mut().find(|p| p.key == pk) {
                    insert_sorted(&mut p.slots, sk, |a, b| {
                        slots.get(a).map(|s| s.start).cmp(&slots.get(b).map(|s| s.start))
                    });
                }
            }
        }
    }

    /// `Slot::RemoveJob`.
    pub fn slot_remove_job(&mut self, sk: SlotKey, job: JobKey) {
        while let Some(r) = self.slots.get(&sk).and_then(|s| s.referees.first().copied()) {
            self.slot_remove_referee(sk, r);
        }
        let Some(s) = self.slots.get_mut(&sk) else { return };
        s.jobs.retain(|j| *j != job);
        if let Some(j) = self.jobs.get_mut(&job) {
            if j.slot == Some(sk) {
                j.slot = None;
            }
        }
        self.refresh_job_status(job);
        let s = self.slots.get_mut(&sk).unwrap();
        s.duration = 0;
        if s.jobs.is_empty() {
            // `Piste::OnSlotRetracted`
            let pistes = s.pistes();
            for pk in pistes {
                if let Some(p) = self.piste_mut(pk) {
                    p.slots.retain(|k| *k != sk);
                }
            }
        }
    }

    /// `Slot::AddReferee`.
    pub fn slot_add_referee(&mut self, sk: SlotKey, referee: u32) {
        let Some(s) = self.slots.get_mut(&sk) else { return };
        if s.referees.contains(&referee) { return; }
        s.referees.insert(0, referee);
        let jobs = s.jobs.clone();
        self.referee_add_slot(referee, sk);
        for j in jobs {
            self.job_make_referee_parcel(j);
            self.refresh_job_status(j);
            self.job_spread(j);
        }
    }

    /// `Slot::RemoveReferee`.
    pub fn slot_remove_referee(&mut self, sk: SlotKey, referee: u32) {
        let Some(s) = self.slots.get_mut(&sk) else { return };
        s.referees.retain(|r| *r != referee);
        let jobs = s.jobs.clone();
        for j in jobs {
            self.job_make_referee_parcel(j);
            self.referee_removed_from_slot(referee, sk);
            self.refresh_job_status(j);
            self.job_spread(j);
        }
    }

    /// `EnlistedReferee::AddSlot`.
    pub(crate) fn referee_add_slot(&mut self, referee: u32, sk: SlotKey) {
        let Some(slot) = self.slots.get(&sk) else { return };
        let (duration, start) = (slot.duration, slot.start);
        let slots = &self.slots;
        if let Some(r) = self.referees.referees.get_mut(&referee) {
            r.workload += (duration / MINUTE) as i32;
            insert_sorted(&mut r.slots, sk, |a, b| {
                let sa = if *a == sk { Some(start) } else { slots.get(a).map(|s| s.start) };
                let sb = if *b == sk { Some(start) } else { slots.get(b).map(|s| s.start) };
                sa.cmp(&sb)
            });
        }
    }

    /// `EnlistedReferee::OnRemovedFromSlot`.
    pub(crate) fn referee_removed_from_slot(&mut self, referee: u32, sk: SlotKey) {
        let duration = self.slots.get(&sk).map(|s| s.duration).unwrap_or(0);
        if let Some(r) = self.referees.referees.get_mut(&referee) {
            r.slots.retain(|k| *k != sk);
            r.workload -= (duration / MINUTE) as i32;
        }
    }

    /// `Slot::Cancel`.
    pub fn slot_cancel(&mut self, sk: SlotKey) {
        let jobs = self.slots.get(&sk).map(|s| s.jobs.clone()).unwrap_or_default();
        for j in jobs {
            self.slot_remove_job(sk, j);
            self.job_reset_roadmap(j);
        }
    }

    /// `Slot::Swap` : échange matchs et arbitres de deux créneaux.
    pub fn slot_swap(&mut self, a: SlotKey, b: SlotKey) {
        // Les réservations communes se modifient ensemble, pas par échange
        // automatique d'une seule piste.
        if [a, b].iter().any(|k| self.slots.get(k).is_some_and(|s| !s.additional_pistes.is_empty())) { return; }
        let (ja, ra) = self.slots.get(&a).map(|s| (s.jobs.clone(), s.referees.clone())).unwrap_or_default();
        let (jb, rb) = self.slots.get(&b).map(|s| (s.jobs.clone(), s.referees.clone())).unwrap_or_default();
        self.slot_cancel(a);
        self.slot_cancel(b);
        for j in ja {
            self.slot_add_job(b, j);
        }
        for j in jb {
            self.slot_add_job(a, j);
        }
        for r in ra {
            self.slot_add_referee(b, r);
        }
        for r in rb {
            self.slot_add_referee(a, r);
        }
    }

    /// `Slot::FixOverlaps` : décale les créneaux suivants (piste puis premier
    /// arbitre) qui commencent plus de 5 minutes avant la fin de celui-ci.
    pub fn slot_fix_overlaps(&mut self, sk: SlotKey, dry_run: bool) {
        let Some(s) = self.slots.get(&sk).cloned() else { return };
        let Some(end) = s.end else { return };
        let mut followers = Vec::new();
        for pk in s.pistes() {
            if let Some(p) = self.piste(pk) {
                followers.push(Self::slot_after(&p.slots, sk));
            }
        }
        for reference in &s.referees {
            if let Some(r) = self.referees.referees.get(reference) {
                followers.push(Self::slot_after(&r.slots, sk));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for next in followers.into_iter().flatten() {
            if !seen.insert(next) { continue; }
            let Some(n) = self.slots.get(&next).cloned() else { continue };
            if diff(end, n.start) > OVERLAP_TOLERANCE {
                if dry_run {
                    if let Some(j) = s.jobs.first().and_then(|j| self.jobs.get(j)) {
                        let (c, b) = (j.competition, j.batch);
                        self.raise_overlap_warning(c, b);
                    }
                } else {
                    if let Some(ns) = self.slots.get_mut(&next) {
                        ns.start = end;
                    }
                    self.slot_set_duration(next, n.duration);
                    if let Some(dep) = n.jobs.first().copied() {
                        let piste_id = n.piste.and_then(|p| self.piste(p)).map(|p| p.id).unwrap_or(0);
                        self.job_set_piste(dep, piste_id, end);
                        self.job_spread(dep);
                    }
                }
            }
        }
    }

    // --------------------------------------------------------------------------------
    // Matchs

    /// `Job::SetPiste`.
    pub(crate) fn job_set_piste(&mut self, job: JobKey, piste_id: u32, start: Time) {
        if let Some(p) = self.jobs.get_mut(&job).and_then(|j| j.parcel.as_mut()) {
            p.set_uint("piste", piste_id);
            p.set_str("start_date", &time::xml_date(start));
            p.set_str("start_time", &time::xml_time(start));
        }
    }

    /// `Job::MakeRefereeParcel`.
    pub(crate) fn job_make_referee_parcel(&mut self, job: JobKey) {
        let referees: Vec<u32> = self
            .jobs
            .get(&job)
            .and_then(|j| j.slot)
            .and_then(|s| self.slots.get(&s))
            .map(|s| s.referees.clone())
            .unwrap_or_default();
        if let Some(p) = self.jobs.get_mut(&job).and_then(|j| j.parcel.as_mut()) {
            if referees.is_empty() {
                p.remove("referees");
            } else {
                p.set_list("referees", &referees);
            }
        }
    }

    /// `Job::ResetRoadMap`.
    pub(crate) fn job_reset_roadmap(&mut self, job: JobKey) {
        let outbox = &mut self.outbox;
        if let Some(p) = self.jobs.get_mut(&job).and_then(|j| j.parcel.as_mut()) {
            recall(outbox, p);
            p.remove("piste");
            p.remove("pistes");
            p.remove("start_date");
            p.remove("start_time");
            p.remove("referees");
        }
    }

    pub(crate) fn job_spread(&mut self, job: JobKey) {
        self.job_make_piste_parcel(job);
        let outbox = &mut self.outbox;
        if let Some(p) = self.jobs.get_mut(&job).and_then(|j| j.parcel.as_mut()) {
            spread(outbox, p);
        }
    }

    pub(crate) fn job_recall(&mut self, job: JobKey) {
        let outbox = &mut self.outbox;
        if let Some(p) = self.jobs.get_mut(&job).and_then(|j| j.parcel.as_mut()) {
            recall(outbox, p);
        }
    }

    /// `Job::HasPiste`.
    pub fn job_has_piste(&self, job: JobKey) -> bool {
        self.jobs
            .get(&job)
            .and_then(|j| j.slot)
            .and_then(|s| self.slots.get(&s))
            .map(|s| s.piste.is_some())
            .unwrap_or(false)
    }

    /// `Job::GetKinship (Player *referee)` : pire parenté avec les tireurs.
    pub fn job_kinship_with(&self, job: JobKey, referee: u32) -> u32 {
        let (Some(j), Some(r)) = (self.jobs.get(&job), self.referees.referees.get(&referee)) else { return 0 };
        j.fencers
            .iter()
            .map(|f| f.affinities.kinship_with(Some(&r.affinities), &self.affinities))
            .max()
            .unwrap_or(0)
    }

    /// `Job::RefreshStatus`.
    pub(crate) fn job_refresh_status(&mut self, job: JobKey) {
        let referees = self
            .jobs
            .get(&job)
            .and_then(|j| j.slot)
            .and_then(|s| self.slots.get(&s))
            .map(|s| s.referees.clone())
            .unwrap_or_default();
        let kinship = referees.iter().map(|r| self.job_kinship_with(job, *r)).max().unwrap_or(0);
        if let Some(j) = self.jobs.get_mut(&job) {
            j.kinship = kinship;
        }
    }

    /// `Slot::RefreshJobStatus`.
    pub(crate) fn refresh_job_status(&mut self, job: JobKey) {
        self.job_refresh_status(job);
        if let Some((c, b)) = self.jobs.get(&job).map(|j| (j.competition, j.batch)) {
            self.batch_on_new_affinities_rule(c, b, job);
            self.batch_refresh_control_panel(c, b);
        }
    }

    /// `Job::SetRealDuration`.
    pub fn job_set_real_duration(&mut self, job: JobKey, duration: Span) {
        let Some(j) = self.jobs.get(&job) else { return };
        let (Some(sk), regular, over) = (j.slot, j.regular_duration, j.over) else { return };
        if duration > 0 {
            if duration < regular {
                self.slot_set_duration(sk, duration);
            }
            self.jobs.get_mut(&job).unwrap().over = true;
        } else if over {
            self.jobs.get_mut(&job).unwrap().over = false;
            self.slot_set_duration(sk, regular);
        }
    }

    // --------------------------------------------------------------------------------
    // Lots

    pub fn batch(&self, competition: u32, batch: u32) -> Option<&Batch> {
        self.competition(competition)?.batch(batch)
    }

    pub fn batch_mut(&mut self, competition: u32, batch: u32) -> Option<&mut Batch> {
        self.competition_mut(competition)?.batch_mut(batch)
    }

    /// `Batch::OnNewAffinitiesRule`.
    pub(crate) fn batch_on_new_affinities_rule(&mut self, c: u32, b: u32, job: JobKey) {
        let has_piste = self.job_has_piste(job);
        let has_referees = self
            .jobs
            .get(&job)
            .and_then(|j| j.slot)
            .and_then(|s| self.slots.get(&s))
            .map(|s| !s.referees.is_empty())
            .unwrap_or(false);
        let kinship_value = self.jobs.get(&job).map(|j| j.kinship).unwrap_or(0);
        let start_of = |h: &Hall, k: &JobKey| -> Option<Time> {
            h.jobs.get(k).and_then(|j| j.slot).and_then(|s| h.slots.get(&s)).map(|s| s.start)
        };
        let starts: std::collections::HashMap<JobKey, Option<Time>> =
            self.jobs.keys().map(|k| (*k, start_of(self, k))).collect();

        let Some(batch) = self.batch_mut(c, b) else { return };
        if batch.row(job).is_none() {
            return;
        }
        let mut kinship = 0;
        let mut status = JobStatus::None;
        let mut to_spread = Vec::new();
        let mut to_recall = Vec::new();

        let in_pending = batch.pending.contains(&job);
        if has_piste {
            if in_pending {
                batch.pending.retain(|j| *j != job);
                insert_sorted(&mut batch.scheduled, job, |x, y| match (starts.get(x), starts.get(y)) {
                    (Some(Some(a)), Some(Some(b))) => a.cmp(b),
                    _ => std::cmp::Ordering::Equal,
                });
                to_spread.push(job);
            }
        } else if !in_pending {
            batch.scheduled.retain(|j| *j != job);
            batch.partly.retain(|j| *j != job);
            batch.pending.insert(0, job);
            status = JobStatus::Question;
            to_recall.push(job);
        }

        let in_partly = batch.partly.contains(&job);
        if has_referees {
            kinship = kinship_value;
            if in_partly {
                batch.partly.retain(|j| *j != job);
                if status != JobStatus::None {
                    status = JobStatus::Error;
                }
                to_spread.push(job);
            }
        } else if !in_partly && has_piste {
            batch.partly.insert(0, job);
            to_spread.push(job);
        }

        let warning = self.affinities.color_of_kinship(kinship);
        let batch = self.batch_mut(c, b).unwrap();
        if let Some(row) = batch.rows.iter_mut().find(|r| r.job == job) {
            row.status = status;
            row.warning = warning;
        }
        for j in to_recall {
            self.job_recall(j);
        }
        for j in to_spread {
            self.job_spread(j);
        }
    }

    /// `Batch::RefreshControlPanel`.
    pub(crate) fn batch_refresh_control_panel(&mut self, c: u32, b: u32) {
        let outbox = &mut self.outbox;
        let Some(batch) = self.competitions.iter_mut().find(|x| x.id == c).and_then(|x| x.batch_mut(b)) else {
            return;
        };
        batch.assign_sensitive = true;
        batch.cancel_sensitive = true;
        if batch.pending.is_empty() && batch.partly.is_empty() {
            batch.assign_sensitive = false;
            batch.status = BatchStatus::Complete;
        } else {
            batch.status = BatchStatus::Incomplete;
        }
        if batch.scheduled.is_empty() {
            batch.cancel_sensitive = false;
        }
        batch.eob.spread(outbox);
        self.competition_on_new_batch_status(c);
    }

    /// `Competition::OnNewBatchStatus`.
    pub(crate) fn competition_on_new_batch_status(&mut self, c: u32) {
        if let Some(comp) = self.competition_mut(c) {
            comp.batch_image = comp.batches.iter().any(|b| b.panel.visible && b.status == BatchStatus::Incomplete);
        }
    }

    /// `Batch::Mute` / `UnMute`.
    pub fn batch_mute(&mut self, c: u32, b: u32, mute: bool) {
        if let Some(batch) = self.batch_mut(c, b) {
            batch.eob.suspended = mute;
        }
    }

    /// `Batch::Spread` : matchs planifiés puis fin de rafale.
    pub fn batch_spread(&mut self, c: u32, b: u32) {
        let jobs = self.batch(c, b).map(|x| x.scheduled.clone()).unwrap_or_default();
        for j in jobs {
            self.job_spread(j);
        }
        let outbox = &mut self.outbox;
        if let Some(batch) = self.competitions.iter_mut().find(|x| x.id == c).and_then(|x| x.batch_mut(b)) {
            batch.eob.spread(outbox);
        }
    }

    /// `Batch::Recall`.
    pub fn batch_recall(&mut self, c: u32, b: u32) {
        let jobs = self.batch(c, b).map(|x| x.scheduled.clone()).unwrap_or_default();
        for j in jobs {
            self.job_recall(j);
        }
        let outbox = &mut self.outbox;
        if let Some(batch) = self.competitions.iter_mut().find(|x| x.id == c).and_then(|x| x.batch_mut(b)) {
            batch.eob.spread(outbox);
        }
    }

    /// `Batch::RetreivePendingJobs` : en attente et partiellement planifiés,
    /// triés par position.
    pub fn batch_pending_jobs(&self, c: u32, b: u32) -> Vec<JobKey> {
        let Some(batch) = self.batch(c, b) else { return Vec::new() };
        let mut list: Vec<JobKey> = batch.pending.iter().chain(batch.partly.iter()).copied().collect();
        list.sort_by_key(|j| self.jobs.get(j).map(|x| x.position).unwrap_or(0));
        list
    }

    /// `Batch::RetreivePendingSelected` : parmi les matchs sélectionnés.
    pub fn batch_pending_selected(&self, c: u32, b: u32, selection: &[JobKey]) -> Vec<JobKey> {
        let Some(batch) = self.batch(c, b) else { return Vec::new() };
        let mut list: Vec<JobKey> = selection
            .iter()
            .copied()
            .filter(|j| batch.pending.contains(j) || batch.partly.contains(j))
            .collect();
        list.sort_by_key(|j| self.jobs.get(j).map(|x| x.position).unwrap_or(0));
        list
    }

    /// `Batch::FixOverlapWarnings`.
    pub fn batch_fix_overlap_warnings(&mut self, c: u32, b: u32) {
        let jobs = self.batch(c, b).map(|x| x.scheduled.clone()).unwrap_or_default();
        for j in jobs {
            if let Some(sk) = self.jobs.get(&j).and_then(|x| x.slot) {
                self.slot_fix_overlaps(sk, false);
            }
        }
    }

    /// `Batch::RaiseOverlapWarning` → `Hall::OnJobOverlapWarning`.
    pub fn raise_overlap_warning(&mut self, c: u32, b: u32) {
        let Some(comp) = self.competition(c) else { return };
        let Some(batch) = comp.batch(b) else { return };
        let warning = crate::hall::OverlapWarning {
            competition: c,
            batch: b,
            message: format!("{} {}", comp.title(), batch.name),
            color: comp.color,
        };
        match self.overlaps.iter_mut().find(|w| w.competition == c && w.batch == b) {
            Some(w) => *w = warning,
            None => self.overlaps.push(warning),
        }
    }

    /// `Hall::OnOverlapWarningActivated` (réponse « oui »).
    pub fn fix_overlap_warning(&mut self, index: usize) {
        if index < self.overlaps.len() {
            let w = self.overlaps.remove(index);
            self.batch_fix_overlap_warnings(w.competition, w.batch);
        }
    }

    // --------------------------------------------------------------------------------
    // Affichage

    /// `Hall::Redraw` : réaffiche les pistes à leur heure et recalcule la
    /// disponibilité des arbitres.
    pub fn redraw(&mut self) {
        for p in self.pistes.iter_mut() {
            p.blurred = false;
        }
        self.redraw_scheduled = true;
        self.refresh_availability();
        self.gc_slots();
    }

    /// Durée écoulée d'un créneau (barre de progression des pistes).
    pub fn slot_progress(&self, sk: SlotKey) -> f64 {
        let Some(s) = self.slots.get(&sk) else { return 0.0 };
        let elapsed = diff(self.clock.now(), s.start);
        if elapsed > 0 && s.duration > 0 {
            elapsed as f64 / s.duration as f64
        } else {
            0.0
        }
    }

    /// Le créneau affiché par une piste (créneau à son heure d'affichage).
    pub fn piste_displayed_slot(&self, piste: PisteKey) -> Option<SlotKey> {
        let p = self.piste(piste)?;
        self.piste_slot_at(piste, p.display_time)
    }

    /// Titre d'une piste en markup Pango (`OnSlotUpdated`).
    pub fn piste_title_markup(&self, sk: SlotKey) -> String {
        let Some(s) = self.slots.get(&sk) else { return String::new() };
        let mut out = String::new();
        let mut last = None;
        for (i, j) in s.jobs.iter().filter_map(|j| self.jobs.get(j)).enumerate() {
            if i == 0 {
                let batch_name = self.batch(j.competition, j.batch).map(|b| b.name.clone()).unwrap_or_default();
                out.push_str(&glib_escape(&batch_name));
                out.push_str(" - <b>");
                out.push_str(&glib_escape(&j.name));
            } else {
                if i == 1 {
                    out.push_str(" ... ");
                }
                last = Some(j.name.clone());
            }
        }
        if let Some(l) = last {
            out.push_str(&glib_escape(&l));
        }
        if !out.is_empty() {
            out.push_str("</b>");
        }
        out
    }

    /// Pas de la frise utilisé pour arrondir une heure.
    pub fn round_time(t: Time) -> Time {
        use chrono::Timelike;
        let span = t.minute() as Span * MINUTE + t.second() as Span * time::SECOND;
        add(t, crate::model::Timeline::rectification(span))
    }
}

/// Échappement markup minimal.
pub fn glib_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
