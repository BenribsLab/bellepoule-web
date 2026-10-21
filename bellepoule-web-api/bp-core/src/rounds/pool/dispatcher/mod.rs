//! Ordre des matchs d'une poule (port de `rounds/poule/dispatcher/`).
//!
//! Deux cas, comme en C++ (`Dispatcher::SetAffinityCriteria`) :
//! - aucun groupe de coéquipiers significatif : on applique la table FIE
//!   (`table::fencing_pairs`) pour les poules de 2 à 17 tireurs ;
//! - des tireurs partagent un critère d'affinité (club, pays…) : les
//!   coéquipiers sont « verrouillés » pour se rencontrer en premier, puis
//!   l'algorithme glouton répartit les assauts en maximisant le repos.
//!
//! Le « repos » (fitness) d'un assaut est le nombre d'assauts écoulés depuis
//! le dernier assaut de chacun des deux tireurs ; 0 signifie deux assauts
//! consécutifs (`rest_error`).

mod table;

pub use table::fencing_pairs;

/// Taille maximale d'une poule gérée par les tables FIE (`_MAX_POOL_SIZE`).
pub const MAX_POOL_SIZE: usize = 17;

#[derive(Debug, Clone)]
struct Opponent {
    /// Numéro dans la poule, à partir de 1.
    id: usize,
    /// Groupe d'affinité (0 = aucun).
    quark: u32,
    /// Nombre de coéquipiers restant à rencontrer en priorité.
    lock: i64,
    fitness: i32,
    fitness_profile: Vec<u32>,
    /// Adversaires restant à rencontrer (indices dans `opponents`).
    opponent_list: Vec<usize>,
}

impl Opponent {
    fn new(id: usize, quark: u32) -> Self {
        Opponent { id, quark, lock: 0, fitness: -1, fitness_profile: Vec::new(), opponent_list: Vec::new() }
    }
}

#[derive(Debug, Clone)]
struct Pair {
    a: usize,
    b: usize,
    a_fitness: i32,
    b_fitness: i32,
}

impl Pair {
    fn has_opponent(&self, o: usize) -> bool {
        self.a == o || self.b == o
    }
}

/// Répartiteur des matchs d'une poule.
#[derive(Debug, Clone, Default)]
pub struct Dispatcher {
    name: String,
    pool_size: usize,
    opponents: Vec<Opponent>,
    pairs: Vec<Pair>,
}

/// Profil de repos d'un tireur : `profile[i]` = nombre de ses assauts
/// précédés de `i` assauts de repos (`Opponent::Dump`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestProfile {
    pub id: usize,
    pub profile: Vec<u32>,
}

impl Dispatcher {
    /// Répartiteur vide (`Dispatcher (name)`).
    pub fn new(name: &str) -> Self {
        Dispatcher { name: name.to_string(), ..Default::default() }
    }

    /// Répartiteur construit à partir d'une séquence explicite d'assauts
    /// (`Dispatcher (pool_size, name, ...)`, utilisé par les tests).
    pub fn from_pairs(pool_size: usize, name: &str, pairs: &[(usize, usize)]) -> Self {
        let mut d = Dispatcher::new(name);
        d.pool_size = pool_size;
        d.opponents = (1..=pool_size).map(|i| Opponent::new(i, 0)).collect();
        let count = d.pair_count();
        for (i, (a, b)) in pairs.iter().take(count).enumerate() {
            d.add_pair(i as i32, a - 1, b - 1);
        }
        d.refresh_fitness();
        d
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn pool_size(&self) -> usize {
        self.pool_size
    }

    fn pair_count(&self) -> usize {
        if self.pool_size > 0 {
            self.pool_size * (self.pool_size - 1) / 2
        } else {
            0
        }
    }

    fn reset(&mut self) {
        self.pairs.clear();
        self.opponents.clear();
    }

    /// `new Pair (iteration, a, b)` : fixe la fitness des deux tireurs et les
    /// retire mutuellement de leurs listes d'adversaires (`Opponent::Use`).
    fn add_pair(&mut self, iteration: i32, a: usize, b: usize) {
        self.opponents[a].fitness = iteration;
        self.opponents[b].fitness = iteration;
        {
            let qa = self.opponents[a].quark;
            let qb = self.opponents[b].quark;
            if qa != 0 && qa == qb {
                self.opponents[a].lock -= 1;
                self.opponents[b].lock -= 1;
            }
            if let Some(p) = self.opponents[a].opponent_list.iter().position(|x| *x == b) {
                self.opponents[a].opponent_list.remove(p);
            }
            if let Some(p) = self.opponents[b].opponent_list.iter().position(|x| *x == a) {
                self.opponents[b].opponent_list.remove(p);
            }
        }
        self.pairs.push(Pair { a, b, a_fitness: -1, b_fitness: -1 });
    }

    /// Calcule l'ordre des matchs pour la liste de tireurs donnée
    /// (`SetAffinityCriteria`). `affinity` donne, pour chaque tireur dans
    /// l'ordre de la poule, l'image du critère d'affinité (`None` si absent
    /// ou si aucun critère n'est retenu).
    pub fn set_affinity_criteria(&mut self, affinity: &[Option<String>]) {
        self.reset();
        self.pool_size = affinity.len();

        let mut quarks: Vec<String> = Vec::new();
        let mut distribution: Vec<(u32, usize)> = Vec::new();
        let mut biggest_team_size = 0usize;

        for (i, a) in affinity.iter().enumerate() {
            let mut opponent = Opponent::new(i + 1, 0);
            if let Some(image) = a {
                let quark = match quarks.iter().position(|q| q == image) {
                    Some(p) => p as u32 + 1,
                    None => {
                        quarks.push(image.clone());
                        quarks.len() as u32
                    }
                };
                opponent.quark = quark;
                match distribution.iter_mut().find(|(q, _)| *q == quark) {
                    Some((_, count)) => *count += 1,
                    None => distribution.push((quark, 1)),
                }
                let size = distribution.iter().find(|(q, _)| *q == quark).map(|(_, c)| *c).unwrap_or(0);
                biggest_team_size = biggest_team_size.max(size);
            }
            self.opponents.push(opponent);
        }

        if biggest_team_size < 2 || biggest_team_size > self.pool_size / 2 {
            if self.pool_size >= 2 && self.pool_size <= MAX_POOL_SIZE {
                let table = fencing_pairs(self.pool_size);
                for (i, (a, b)) in table.iter().enumerate().take(self.pair_count()) {
                    self.add_pair(i as i32 + 1, *a as usize - 1, *b as usize - 1);
                }
                self.refresh_fitness();
            }
        } else {
            // Verrouillage par affinité
            for (quark, count) in &distribution {
                self.lock_opponents(*quark, *count);
            }
            self.spread_opponents();
            self.create_pairs();
            self.refresh_fitness();
        }
    }

    fn lock_opponents(&mut self, quark: u32, count: usize) {
        if quark != 0 && count > 1 {
            for o in &mut self.opponents {
                if o.quark == quark {
                    o.lock = count as i64 - 1;
                }
            }
        }
    }

    fn spread_opponents(&mut self) {
        let n = self.opponents.len();
        for i in 0..n {
            self.opponents[i].opponent_list = (0..n).filter(|j| *j != i).collect();
        }
    }

    fn is_compatible(&self, target: usize, with: usize) -> bool {
        let t = &self.opponents[target];
        let w = &self.opponents[with];
        if t.quark == w.quark {
            true
        } else {
            t.lock == 0 && w.lock == 0
        }
    }

    /// `Opponent::GetBestOpponent` : trie (de façon stable) les adversaires
    /// restants par fitness croissante et renvoie le premier compatible.
    fn best_opponent(&mut self, a: usize) -> Option<usize> {
        let mut list = std::mem::take(&mut self.opponents[a].opponent_list);
        list.sort_by_key(|o| self.opponents[*o].fitness);
        let best = list.iter().copied().find(|t| self.is_compatible(*t, a));
        self.opponents[a].opponent_list = list;
        best
    }

    fn create_pairs(&mut self) {
        let mut working: Vec<usize> = (0..self.opponents.len()).collect();
        let mut current = 0usize;
        let mut iteration: i32 = 1;

        while current < working.len() {
            let a = working[current];
            match self.best_opponent(a) {
                Some(b) if self.opponents[b].fitness == -1 || self.opponents[b].fitness < iteration - 1 => {
                    self.add_pair(iteration, a, b);
                    working.sort_by_key(|o| self.opponents[*o].fitness);
                    current = 0;
                    iteration += 1;
                }
                _ => current += 1,
            }
        }
    }

    fn refresh_fitness(&mut self) {
        let size = self.pair_count();
        for o in &mut self.opponents {
            o.fitness_profile = vec![0; size];
        }
        for idx in (0..self.pairs.len()).rev() {
            self.pairs[idx].a_fitness = -1;
            self.pairs[idx].b_fitness = -1;
            for (i, prev_idx) in (0..idx).rev().enumerate() {
                let (pa, pb) = (self.pairs[prev_idx].a, self.pairs[prev_idx].b);
                let prev = Pair { a: pa, b: pb, a_fitness: 0, b_fitness: 0 };
                let pair = &mut self.pairs[idx];
                if pair.a_fitness == -1 && prev.has_opponent(pair.a) {
                    pair.a_fitness = i as i32;
                    if let Some(c) = self.opponents[pair.a].fitness_profile.get_mut(i) {
                        *c += 1;
                    }
                }
                let pair = &mut self.pairs[idx];
                if pair.b_fitness == -1 && prev.has_opponent(pair.b) {
                    pair.b_fitness = i as i32;
                    if let Some(c) = self.opponents[pair.b].fitness_profile.get_mut(i) {
                        *c += 1;
                    }
                }
            }
        }
    }

    /// Nombre d'assauts effectivement générés.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// `GetPlayerPair` : numéros (à partir de 1) des deux tireurs du match
    /// `match_index` et indicateur d'assauts consécutifs (`rest_error`).
    pub fn player_pair(&self, match_index: usize) -> Option<(usize, usize, bool)> {
        let p = self.pairs.get(match_index)?;
        let rest_error = p.a_fitness == 0 || p.b_fitness == 0;
        Some((self.opponents[p.a].id, self.opponents[p.b].id, rest_error))
    }

    /// Tous les assauts dans l'ordre.
    pub fn pairs(&self) -> Vec<(usize, usize)> {
        self.pairs.iter().map(|p| (self.opponents[p.a].id, self.opponents[p.b].id)).collect()
    }

    /// Repos avant chaque assaut pour chacun des deux tireurs
    /// (`GetAFitness`/`GetBFitness`, 0 si premier assaut).
    pub fn pair_fitness(&self, match_index: usize) -> Option<(u32, u32)> {
        let p = self.pairs.get(match_index)?;
        Some((p.a_fitness.max(0) as u32, p.b_fitness.max(0) as u32))
    }

    /// Profils de repos de chaque tireur (données de `Dump`).
    pub fn rest_profiles(&self) -> Vec<RestProfile> {
        self.opponents.iter().map(|o| RestProfile { id: o.id, profile: o.fitness_profile.clone() }).collect()
    }

    /// Distribution globale des repos (`Dump`, « Global digest ») :
    /// `digest[i]` = nombre de demi-assauts précédés de `i` assauts de repos.
    pub fn digest(&self) -> Vec<u32> {
        let count = self.pair_count();
        let mut max_fitness = 0usize;
        let mut table = vec![0u32; count.max(1)];
        for i in 0..self.pairs.len() {
            let (a, b) = self.pair_fitness(i).unwrap_or((0, 0));
            for f in [a as usize, b as usize] {
                if f < count {
                    table[f] += 1;
                }
                max_fitness = max_fitness.max(f);
            }
        }
        table.truncate(max_fitness + 1);
        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chaque couple de tireurs doit se rencontrer une et une seule fois,
    /// quelle que soit la taille de la poule.
    #[test]
    fn every_pair_meets_exactly_once() {
        for n in 2..=MAX_POOL_SIZE {
            let d = Dispatcher::new("test");
            let _ = d;
            let pairs: Vec<(u8, u8)> = table::fencing_pairs(n).to_vec();
            assert_eq!(pairs.len(), n * (n - 1) / 2, "taille {n}");
            let mut seen = std::collections::HashSet::new();
            for (a, b) in pairs {
                assert!(a != b && a >= 1 && b >= 1 && a as usize <= n && b as usize <= n, "taille {n}");
                assert!(seen.insert((a.min(b), a.max(b))), "taille {n} : couple répété {a}-{b}");
            }
        }
    }

    /// Ordre officiel FIE pour une poule de 7 (`dispatcher/test.cpp`).
    #[test]
    fn fie_order_for_seven() {
        let expected: [(u8, u8); 21] = [
            (1, 4), (2, 5), (3, 6), (7, 1), (5, 4), (2, 3), (6, 7), (5, 1), (4, 3), (6, 2), (5, 7),
            (3, 1), (4, 6), (7, 2), (3, 5), (1, 6), (2, 4), (7, 3), (6, 5), (1, 2), (4, 7),
        ];
        assert_eq!(table::fencing_pairs(7), &expected);
    }
}
