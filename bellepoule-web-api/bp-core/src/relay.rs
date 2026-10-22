//! Rencontres par équipes en relais : ordre des relais, feuille de match et
//! journal de la rencontre (scores cumulés relais après relais).
//!
//! Deux types de rencontre :
//! - **chacun contre chacun** : chaque tireur d'une équipe rencontre chaque
//!   tireur de l'autre. L'équipe du haut porte les numéros 1 à N, celle du
//!   bas N+1 à 2N (3 contre 3 : 3-6, 1-5, 2-4, 1-6, 3-4, 2-5, 1-4, 2-6, 3-5) ;
//! - **relais simple** : chaque tireur ne fait qu'un match ; le haut porte les
//!   numéros impairs, le bas les pairs (1-2, puis 3-4, puis 5-6…), quelle que
//!   soit la taille des équipes.
//!
//! Le score est cumulé : le relais n se termine quand une équipe atteint
//! n × (points par relais) ou à la fin de son temps.

use serde::{Deserialize, Serialize};

use crate::xml::XmlWriter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RelayKind {
    /// Chaque tireur rencontre chaque tireur de l'autre équipe.
    #[default]
    EachVsEach,
    /// Chaque tireur ne fait qu'un match.
    Single,
}

impl RelayKind {
    fn xml(self) -> &'static str {
        match self {
            RelayKind::EachVsEach => "croise",
            RelayKind::Single => "simple",
        }
    }
    fn from_xml(text: &str) -> Option<RelayKind> {
        match text {
            "croise" => Some(RelayKind::EachVsEach),
            "simple" => Some(RelayKind::Single),
            _ => None,
        }
    }
}

/// Format des rencontres par équipes d'une phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayFormat {
    pub kind: RelayKind,
    /// Tireurs par équipe (réglage de la compétition, fixé à l'appel).
    pub team_size: u32,
    /// Points de chaque relais (5 : 5, 10, 15… 45 en 3 contre 3).
    pub points: u32,
    /// Remplaçants par équipe (réglage de la compétition).
    pub substitutes: u32,
}

impl Default for RelayFormat {
    fn default() -> Self {
        RelayFormat { kind: RelayKind::EachVsEach, team_size: 3, points: 5, substitutes: 1 }
    }
}

impl RelayFormat {
    pub fn validate(&self) -> Result<(), String> {
        let max_size = match self.kind {
            RelayKind::EachVsEach => 6,
            RelayKind::Single => 20,
        };
        if !(1..=max_size).contains(&self.team_size) || !(1..=15).contains(&self.points) {
            return Err(format!("relais : 1 à {max_size} tireurs par équipe, 1 à 15 points par relais"));
        }
        Ok(())
    }

    /// Ordre des relais : (numéro du haut, numéro du bas).
    pub fn bouts(&self) -> Vec<(u32, u32)> {
        let n = self.team_size.max(1);
        match self.kind {
            RelayKind::Single => (0..n).map(|k| (2 * k + 1, 2 * k + 2)).collect(),
            RelayKind::EachVsEach => match n {
                2 => vec![(1, 4), (2, 3), (1, 3), (2, 4)],
                3 => vec![(3, 6), (1, 5), (2, 4), (1, 6), (3, 4), (2, 5), (1, 4), (2, 6), (3, 5)],
                // Rotation : à chaque tour, chaque tireur du haut rencontre le
                // tireur suivant du bas ; jamais deux relais de suite pour un tireur.
                _ => (0..n).flat_map(|r| (0..n).map(move |i| (i + 1, n + 1 + (i + r) % n))).collect(),
            },
        }
    }

    pub fn relays(&self) -> usize {
        self.bouts().len()
    }

    /// Score à atteindre à la fin du relais `index` (0 = premier).
    pub fn target(&self, index: usize) -> u32 {
        self.points * (index as u32 + 1)
    }

    /// Score de la rencontre (dernier relais).
    pub fn total(&self) -> u32 {
        self.target(self.relays().saturating_sub(1))
    }

    /// Numéros de l'équipe du haut, puis du bas.
    pub fn numbers(&self) -> [Vec<u32>; 2] {
        let n = self.team_size.max(1);
        match self.kind {
            RelayKind::EachVsEach => [(1..=n).collect(), (n + 1..=2 * n).collect()],
            RelayKind::Single => [(0..n).map(|k| 2 * k + 1).collect(), (0..n).map(|k| 2 * k + 2).collect()],
        }
    }

    pub fn label(&self) -> String {
        match self.kind {
            RelayKind::EachVsEach => format!("{0} contre {0}, chacun contre chacun, relais de {1} points", self.team_size, self.points),
            RelayKind::Single => format!("Relais simple, équipes de {}, relais de {} points", self.team_size, self.points),
        }
    }

    pub fn load(&mut self, node: roxmltree::Node) {
        let value = RelayFormat {
            kind: node.attribute("TypeRelais").and_then(RelayKind::from_xml).unwrap_or(self.kind),
            team_size: self.team_size,
            points: crate::xml::attr_u32(node, "PointsRelais", self.points),
            substitutes: self.substitutes,
        };
        if value.validate().is_ok() {
            *self = value;
        }
    }

    pub fn save(&self, w: &mut XmlWriter) {
        w.attr("TypeRelais", self.kind.xml());
        w.attr_fmt("PointsRelais", self.points);
    }
}

/// Journal d'une rencontre par équipes, saisi par l'arbitre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RelayLog {
    pub revision: u64,
    pub format: RelayFormat,
    /// Équipe du haut : 0 = première équipe de la feuille, 1 = seconde.
    pub top: u8,
    /// Tireur de chaque numéro (indice = numéro - 1), choisi au début de la rencontre.
    pub lineup: Vec<String>,
    /// Scores cumulés [haut, bas] à la fin de chaque relais terminé (un
    /// relais arrêté au temps peut finir sous son objectif).
    pub relays: Vec<[u32; 2]>,
    /// Égalité à la fin : vainqueur désigné par l'arbitre (équipe de la feuille, 0 ou 1).
    pub winner: Option<u8>,
}

impl RelayLog {
    pub fn validate(&self) -> Result<(), String> {
        self.format.validate()?;
        if self.top > 1 || self.winner.is_some_and(|w| w > 1) || self.relays.len() > self.format.relays() || self.lineup.len() > 2 * self.format.team_size as usize {
            return Err("journal des relais invalide".into());
        }
        let total = self.format.total();
        let mut previous = [0u32, 0u32];
        for (i, r) in self.relays.iter().enumerate() {
            if r[0] < previous[0] || r[1] < previous[1] {
                return Err(format!("relais {} : un score cumulé ne peut pas baisser", i + 1));
            }
            if r[0] > self.format.target(i) || r[1] > self.format.target(i) || r[0] > total || r[1] > total {
                return Err(format!("relais {} : score au-delà de {}", i + 1, self.format.target(i)));
            }
            previous = *r;
        }
        Ok(())
    }

    /// La rencontre est finie : tous les relais tirés, ou le score total atteint.
    pub fn is_over(&self) -> bool {
        let total = self.format.total();
        self.relays.len() == self.format.relays() || self.relays.last().is_some_and(|r| r[0] >= total || r[1] >= total)
    }

    /// Scores finaux par équipe de la feuille (première, seconde).
    pub fn scores(&self) -> Option<[u32; 2]> {
        let last = *self.relays.last()?;
        Some(if self.top == 0 { last } else { [last[1], last[0]] })
    }

    /// Valeurs du score à enregistrer (« W45 » pour le vainqueur, « 38 »),
    /// une fois la rencontre finie sur un écart.
    pub fn values(&self) -> Option<[String; 2]> {
        if !self.is_over() {
            return None;
        }
        let s = self.scores()?;
        let w = match s[0].cmp(&s[1]) {
            std::cmp::Ordering::Greater => 0,
            std::cmp::Ordering::Less => 1,
            std::cmp::Ordering::Equal => self.winner? as usize,
        };
        Some([0, 1].map(|k| if k == w { format!("W{}", s[k]) } else { s[k].to_string() }))
    }

    /// Une ligne par relais : « 3-6 · 5/3 ».
    pub fn summary(&self) -> String {
        self.format
            .bouts()
            .iter()
            .zip(self.relays.iter())
            .map(|((a, b), r)| format!("{a}-{b} · {}/{}", r[0], r[1]))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_of_the_relays() {
        let three = RelayFormat::default();
        assert_eq!(three.bouts(), vec![(3, 6), (1, 5), (2, 4), (1, 6), (3, 4), (2, 5), (1, 4), (2, 6), (3, 5)]);
        assert_eq!(three.total(), 45);
        assert_eq!(three.numbers(), [vec![1, 2, 3], vec![4, 5, 6]]);
        let two = RelayFormat { team_size: 2, points: 4, ..RelayFormat::default() };
        assert_eq!(two.bouts(), vec![(1, 4), (2, 3), (1, 3), (2, 4)]);
        assert_eq!(two.total(), 16);
        let single = RelayFormat { kind: RelayKind::Single, team_size: 4, points: 3, substitutes: 1 };
        assert_eq!(single.bouts(), vec![(1, 2), (3, 4), (5, 6), (7, 8)]);
        assert_eq!(single.numbers(), [vec![1, 3, 5, 7], vec![2, 4, 6, 8]]);
        // Quatre contre quatre : chacun contre chacun, une seule fois.
        let four = RelayFormat { team_size: 4, ..RelayFormat::default() };
        let bouts = four.bouts();
        assert_eq!(bouts.len(), 16);
        let mut seen = std::collections::BTreeSet::new();
        for (a, b) in &bouts {
            assert!((1..=4).contains(a) && (5..=8).contains(b));
            assert!(seen.insert((*a, *b)));
        }
        for pair in bouts.windows(2) {
            assert!(pair[0].0 != pair[1].0 && pair[0].1 != pair[1].1, "un tireur ne tire pas deux relais de suite");
        }
    }

    #[test]
    fn cumulative_scores_give_the_result() {
        let mut log = RelayLog { revision: 1, top: 1, ..Default::default() };
        log.relays = vec![[5, 3], [8, 10]];
        log.validate().unwrap();
        assert!(!log.is_over());
        assert_eq!(log.values(), None);
        log.relays = vec![[5, 3], [8, 10], [15, 12], [20, 18], [25, 21], [30, 27], [35, 31], [40, 38], [45, 41]];
        log.validate().unwrap();
        assert!(log.is_over());
        // L'équipe du haut est la seconde de la feuille.
        assert_eq!(log.scores(), Some([41, 45]));
        assert_eq!(log.values(), Some(["41".to_string(), "W45".to_string()]));
        // Arrêt au temps : moins de 45, égalité départagée par l'arbitre.
        log.relays.truncate(8);
        log.relays.push([39, 39]);
        assert!(log.is_over());
        assert_eq!(log.values(), None);
        log.winner = Some(0);
        assert_eq!(log.values(), Some(["W39".to_string(), "39".to_string()]));
        log.relays[1] = [4, 10];
        assert!(log.validate().is_err(), "un score cumulé ne baisse pas");
    }
}

impl crate::contest::Contest {
    /// Format des relais de la phase `index` : type et points de la phase,
    /// tireurs et remplaçants de la compétition (fixés à l'appel).
    pub fn relay_format(&self, index: usize) -> RelayFormat {
        let mut f = self.schedule.stage(index).map(|s| s.bout_timing().relay).unwrap_or_default();
        f.team_size = self.settings.team_size.clamp(1, 20);
        f.substitutes = self.settings.substitutes.min(10);
        f
    }

    /// Tireurs et remplaçants des équipes : le score maximum des rencontres
    /// de chaque phase suit (total des relais).
    pub fn set_team_size(&mut self, team_size: u32, substitutes: u32) -> bool {
        let (team_size, substitutes) = (team_size.clamp(1, 20), substitutes.min(10));
        if (self.settings.team_size, self.settings.substitutes) == (team_size, substitutes) {
            return false;
        }
        self.settings.team_size = team_size;
        self.settings.substitutes = substitutes;
        if self.settings.team_event {
            for i in 0..self.schedule.len() {
                let total = self.relay_format(i).total();
                let Some(stage) = self.schedule.stage_mut(i) else { continue };
                if stage.base().locked {
                    continue;
                }
                let any = stage.as_any_mut();
                if let Some(s) = any.downcast_mut::<crate::rounds::table::supervisor::TableSupervisor>() {
                    s.set_max_score(total);
                } else if let Some(s) = any.downcast_mut::<crate::rounds::pool::PoolSupervisorStage>() {
                    s.round().set_max_score(total);
                }
            }
        }
        self.dirty = true;
        true
    }

    /// Réglages du chrono et des relais d'une phase. Pour une compétition par
    /// équipes, le score maximum des rencontres devient le total des relais
    /// (45 en 3 contre 3, relais de 5 points).
    pub fn set_stage_timing(&mut self, index: usize, mut timing: crate::officiating::BoutTiming) -> Result<bool, String> {
        timing.relay.team_size = self.settings.team_size.clamp(1, 20);
        timing.relay.substitutes = self.settings.substitutes.min(10);
        timing.validate()?;
        timing.relay.validate()?;
        let team = self.settings.team_event;
        let stage = self.schedule.stage_mut(index).ok_or("phase introuvable")?;
        if stage.base().locked {
            return Err("phase verrouillée".into());
        }
        if stage.bout_timing() == timing {
            return Ok(false);
        }
        stage.set_bout_timing(timing);
        if team {
            let total = timing.relay.total();
            let any = stage.as_any_mut();
            if let Some(s) = any.downcast_mut::<crate::rounds::table::supervisor::TableSupervisor>() {
                s.set_max_score(total);
            } else if let Some(s) = any.downcast_mut::<crate::rounds::pool::PoolSupervisorStage>() {
                s.round().set_max_score(total);
            }
        }
        self.dirty = true;
        Ok(true)
    }
}
