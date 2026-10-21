//! Créneaux horaires (port de `marshaller/slot.cpp`, partie algorithmique).
//!
//! En C++ un `Slot` est un objet à compteur de références partagé par la
//! piste, les arbitres et les matchs. Ici les créneaux réservés vivent dans une
//! arène du modèle (`Hall::slots`, clé `SlotKey`) ; les créneaux libres calculés
//! (`free_slots`) sont de simples valeurs qui deviennent des créneaux réservés
//! lorsqu'un match y est placé.

use std::cmp::Ordering;

use crate::time::{add, diff, Span, Time, MINUTE};

pub type SlotKey = u32;
pub type PisteKey = u32;
pub type JobKey = u32;
pub type RefereeRef = u32;

#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    /// Piste (absente pour les créneaux libres d'un arbitre).
    pub piste: Option<PisteKey>,
    /// Pistes supplémentaires réservées au même horaire pour une poule.
    pub additional_pistes: Vec<PisteKey>,
    pub start: Time,
    /// Fin : pour un créneau réservé `start + duration` ; pour un créneau libre
    /// la limite imposée par le créneau réservé suivant (ou aucune).
    pub end: Option<Time>,
    pub duration: Span,
    pub jobs: Vec<JobKey>,
    /// Arbitres (`g_list_prepend` : le dernier ajouté est en tête).
    pub referees: Vec<RefereeRef>,
}

impl Slot {
    pub fn new(piste: Option<PisteKey>, start: Time, end: Option<Time>, duration: Span) -> Self {
        Slot { piste, additional_pistes: Vec::new(), start, end, duration, jobs: Vec::new(), referees: Vec::new() }
    }

    pub fn pistes(&self) -> Vec<PisteKey> {
        self.piste.into_iter().chain(self.additional_pistes.iter().copied()).collect()
    }

    /// `TimeIsInside`.
    pub fn time_is_inside(&self, time: Time) -> bool {
        self.start <= time && time < add(self.start, self.duration)
    }

    /// `CanWrap` : le créneau `with` (libre) peut-il contenir celui-ci ?
    pub fn can_wrap(&self, with: &Slot) -> bool {
        if diff(with.start, self.start) > 0 {
            return false;
        }
        if let Some(with_end) = with.end {
            if add(self.start, self.duration) > with_end {
                return false;
            }
        }
        true
    }

    /// `Equals`.
    pub fn equals(&self, to: &Slot) -> bool {
        if self.start != to.start {
            return false;
        }
        if let (Some(a), Some(b)) = (self.end, to.end) {
            if a != b {
                return false;
            }
        }
        true
    }

    /// `CompareAvailbility`.
    pub fn compare_availability(a: &Slot, b: &Slot) -> Ordering {
        a.start.cmp(&b.start)
    }
}

fn latest(a: Time, b: Option<Time>) -> Time {
    match b {
        Some(b) if b > a => b,
        _ => a,
    }
}

/// `Slot::GetFreeSlots` : créneaux libres à partir de `from` entre les
/// créneaux réservés `booked` (triés par heure de début).
pub fn free_slots(piste: Option<PisteKey>, booked: &[&Slot], from: Time, mut duration: Span) -> Vec<Slot> {
    let mut slots = Vec::new();

    // Avant les créneaux réservés
    match booked.first() {
        None => slots.push(Slot::new(piste, from, None, duration)),
        Some(first) => {
            if first.start >= add(from, duration) {
                slots.push(Slot::new(piste, from, Some(first.start), duration));
            }
        }
    }

    for (i, current) in booked.iter().enumerate() {
        match booked.get(i + 1) {
            None => {
                // Après les créneaux réservés
                slots.push(Slot::new(piste, latest(from, current.end), None, duration));
            }
            Some(next) => {
                // Entre deux créneaux réservés
                if duration == 0 {
                    if let Some(next_end) = next.end {
                        duration = diff(next_end, current.start);
                    }
                }
                let max_start = add(next.start, -duration);
                let after_current = current.end.map(|e| max_start >= e).unwrap_or(true);
                if after_current && max_start >= from {
                    slots.push(Slot::new(piste, latest(from, current.end), Some(next.start), duration));
                }
            }
        }
    }
    slots
}

/// `Slot::GetFreeSlot` : créneau libre commençant exactement à `from`.
pub fn free_slot(piste: Option<PisteKey>, booked: &[&Slot], from: Time, duration: Span) -> Option<Slot> {
    free_slots(piste, booked, from, duration).into_iter().find(|s| s.duration >= duration && s.start == from)
}

/// `Slot::GetSlotAt`.
pub fn slot_at<'a, K: Copy>(time: Time, slots: impl IntoIterator<Item = (K, &'a Slot)>) -> Option<K> {
    slots.into_iter().find(|(_, s)| s.time_is_inside(time)).map(|(k, _)| k)
}

/// Débordement toléré avant de signaler un retard (`FixOverlaps`).
pub const OVERLAP_TOLERANCE: Span = 5 * MINUTE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{parse_fie, HOUR};

    fn t(h: &str) -> Time {
        parse_fie("01.01.2024", h).unwrap()
    }

    fn booked(start: &str, dur: Span) -> Slot {
        let s = t(start);
        Slot::new(Some(1), s, Some(add(s, dur)), dur)
    }

    #[test]
    fn free_slots_on_empty_piste() {
        let f = free_slots(Some(1), &[], t("10:00"), HOUR);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].start, t("10:00"));
        assert_eq!(f[0].end, None);
        assert!(free_slot(Some(1), &[], t("10:00"), HOUR).is_some());
    }

    #[test]
    fn free_slots_between_bookings() {
        let a = booked("10:00", HOUR);
        let b = booked("12:00", HOUR);
        let list = [&a, &b];
        // 9:00 + 1h tient avant 10:00, puis 11:00-12:00, puis après 13:00.
        let f = free_slots(Some(1), &list, t("09:00"), HOUR);
        let starts: Vec<Time> = f.iter().map(|s| s.start).collect();
        assert_eq!(starts, vec![t("09:00"), t("11:00"), t("13:00")]);
        assert_eq!(f[1].end, Some(t("12:00")));
        // 1h30 ne tient pas entre les deux.
        let f = free_slots(Some(1), &list, t("09:00"), 90 * MINUTE);
        let starts: Vec<Time> = f.iter().map(|s| s.start).collect();
        assert_eq!(starts, vec![t("13:00")]);
        // Pas de créneau libre commençant à 10:30 (occupé).
        assert!(free_slot(Some(1), &list, t("10:30"), HOUR).is_none());
        assert!(free_slot(Some(1), &list, t("11:00"), HOUR).is_some());
    }

    #[test]
    fn wrap_and_inside() {
        let job = Slot::new(Some(1), t("10:00"), None, HOUR);
        let free = Slot::new(None, t("09:00"), Some(t("11:00")), HOUR);
        assert!(job.can_wrap(&free));
        let short = Slot::new(None, t("09:00"), Some(t("10:30")), HOUR);
        assert!(!job.can_wrap(&short));
        let late = Slot::new(None, t("10:15"), None, HOUR);
        assert!(!job.can_wrap(&late));
        assert!(job.time_is_inside(t("10:59")));
        assert!(!job.time_is_inside(t("11:00")));
    }
}
