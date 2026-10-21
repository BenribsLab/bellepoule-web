//! Séparation des tireurs d'un même club, d'une même ligue, région ou pays
//! dans les poules (port de `rounds/poule/neo_swapper/`).
//!
//! Le C++ manipule des « proxys » de poules et de tireurs ; on garde la même
//! structure avec des indices dans des arènes. Le répartiteur ne modifie pas
//! les poules lui-même : `swap`, `inject_fencer` renvoient la composition
//! calculée (`StoreSwapping`), que l'allocateur applique.
//!
//! Écart assumé : les tables de hachage GLib (ordre d'itération non
//! spécifié) sont remplacées par des listes dans l'ordre d'apparition des
//! critères ; seul l'ordre des erreurs de même rang peut en être affecté.

use crate::attribute::{self, Look};
use crate::player::{AttrId, Owner, PlayerRef, PlayerStore};

/// Image « utilisateur » d'un critère pour un joueur (`GetUserImage
/// (LONG_TEXT)`), utilisée pour regrouper les tireurs (quark GLib).
pub fn criteria_image(players: &PlayerStore, player: PlayerRef, code_name: &str) -> Option<String> {
    let p = players.get(player)?;
    let id = AttrId::for_desc(code_name, Owner::Global);
    let value = p.get(&id)?;
    let xml = value.xml_image();
    Some(match attribute::registry().by_code(code_name) {
        Some(desc) => desc.user_image(&xml, Look::LongText),
        None => xml,
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct Profile {
    score: u32,
    pool_count: u32,
}

#[derive(Debug, Clone)]
struct Criteria {
    depth: usize,
    pool_count: usize,
    fencer_count: u32,
    big: Profile,
    small: Profile,
    score_table: Vec<u32>,
    /// `_pool_table` : poules déjà touchées par ce critère.
    pool_seen: Vec<bool>,
}

impl Criteria {
    fn new(pool_count: usize, depth: usize) -> Self {
        Criteria {
            depth,
            pool_count,
            fencer_count: 0,
            big: Profile::default(),
            small: Profile::default(),
            score_table: vec![0; pool_count],
            pool_seen: vec![false; pool_count],
        }
    }

    fn use_one(&mut self) {
        self.fencer_count += 1;
        let pc = self.pool_count.max(1) as u32;
        if self.fencer_count % pc != 0 {
            self.big.pool_count = self.fencer_count % pc;
            self.small.pool_count = pc - self.big.pool_count;
            self.small.score = self.fencer_count / pc;
            self.big.score = self.small.score + 1;
        } else {
            self.big.pool_count = pc;
            self.big.score = self.fencer_count / pc;
            self.small.pool_count = 0;
            self.small.score = 0;
        }
    }

    fn change_score(&mut self, delta: i32, pool: usize) {
        self.pool_seen[pool] = true;
        self.score_table[pool] = (self.score_table[pool] as i64 + delta as i64).max(0) as u32;
    }

    fn free_place_in(&self, pool: usize) -> bool {
        let score = self.score_table[pool];
        if self.small.score != 0 {
            if score < self.small.score {
                return true;
            }
        } else if score < self.big.score {
            return true;
        }
        if self.big.score > 0 && score == self.big.score - 1 {
            let mut over_populated = 0;
            for s in &self.score_table {
                if *s >= self.big.score {
                    over_populated += 1;
                    if over_populated >= self.big.pool_count {
                        return false;
                    }
                }
            }
            return true;
        }
        false
    }
}

#[derive(Debug, Clone)]
struct FencerProxy {
    player: PlayerRef,
    rank: u32,
    original_pool: Option<usize>,
    new_pool: Option<usize>,
    criterias: Vec<Option<usize>>,
}

#[derive(Debug, Clone, Default)]
struct PoolProxy {
    fencers: Vec<usize>,
    size: u32,
}

/// Répartiteur « NeoSwapper ».
#[derive(Debug, Clone, Default)]
pub struct NeoSwapper {
    /// Propriétaire du rang de départ (`stage_start_rank`) : l'allocateur.
    owner: Option<Owner>,
    criteria_list: Vec<String>,
    proxies: Vec<FencerProxy>,
    criterias: Vec<Criteria>,
    /// Critères par profondeur et par image, dans l'ordre d'apparition.
    distributions: Vec<Vec<(String, usize)>>,
    pools: Vec<PoolProxy>,
    fencer_list: Vec<usize>,
    error_list: Vec<usize>,
}

impl NeoSwapper {
    pub fn new() -> Self {
        Self::default()
    }

    fn criteria_count(&self) -> usize {
        self.criteria_list.len()
    }

    /// `Configure` : nombre de poules et critères de séparation (codes
    /// d'attributs, le premier étant le plus important).
    pub fn configure(&mut self, nb_pools: usize, criteria_list: &[String], owner: Owner) {
        self.owner = Some(owner);
        self.criteria_list = criteria_list.to_vec();
        self.proxies.clear();
        self.criterias.clear();
        self.distributions = vec![Vec::new(); criteria_list.len()];
        self.pools = vec![PoolProxy::default(); nb_pools];
        self.fencer_list.clear();
        self.error_list.clear();
    }

    fn nb_pools(&self) -> usize {
        self.pools.len()
    }

    fn rank_of(&self, players: &PlayerStore, player: PlayerRef) -> Option<u32> {
        let owner = self.owner?;
        players.get(player)?.get(&AttrId::local("stage_start_rank", owner)).map(|v| v.as_uint())
    }

    fn add_to_pool(&mut self, f: usize, pool: usize) {
        self.proxies[f].new_pool = Some(pool);
        self.pools[pool].fencers.push(f);
        self.pools[pool].size += 1;
        for d in 0..self.criteria_count() {
            if let Some(c) = self.proxies[f].criterias[d] {
                self.criterias[c].change_score(1, pool);
            }
        }
    }

    fn insert_into_pool(&mut self, f: usize, pool: usize) {
        self.add_to_pool(f, pool);
        let mut list = std::mem::take(&mut self.pools[pool].fencers);
        list.sort_by_key(|x| self.proxies[*x].rank);
        self.pools[pool].fencers = list;
    }

    fn remove_from_pool(&mut self, f: usize, pool: usize) {
        self.proxies[f].new_pool = None;
        if let Some(p) = self.pools[pool].fencers.iter().position(|x| *x == f) {
            self.pools[pool].fencers.remove(p);
        }
        self.pools[pool].size = self.pools[pool].size.saturating_sub(1);
        for d in 0..self.criteria_count() {
            if let Some(c) = self.proxies[f].criterias[d] {
                self.criterias[c].change_score(-1, pool);
            }
        }
    }

    /// `ManageFencer` : crée le proxy d'un tireur et enregistre ses critères.
    fn manage_fencer(&mut self, players: &PlayerStore, player: PlayerRef, pool: Option<usize>) -> usize {
        let count = self.criteria_count();
        let rank = self.rank_of(players, player);
        let idx = self.proxies.len();
        self.proxies.push(FencerProxy {
            player,
            rank: rank.unwrap_or(0xFF),
            original_pool: pool,
            new_pool: pool,
            criterias: vec![None; count],
        });
        if rank.is_none() {
            return idx;
        }
        self.fencer_list.push(idx);
        for depth in 0..count {
            let code = self.criteria_list[depth].clone();
            if let Some(image) = criteria_image(players, player, &code) {
                let c = match self.distributions[depth].iter().find(|(k, _)| *k == image) {
                    Some((_, c)) => *c,
                    None => {
                        self.criterias.push(Criteria::new(self.pools.len(), depth));
                        let c = self.criterias.len() - 1;
                        self.distributions[depth].push((image, c));
                        c
                    }
                };
                self.proxies[idx].criterias[depth] = Some(c);
                self.criterias[c].use_one();
            }
        }
        if let Some(p) = pool {
            self.add_to_pool(idx, p);
        }
        idx
    }

    /// `CheckCurrentDistribution` : analyse une composition existante.
    pub fn check_current_distribution(&mut self, players: &PlayerStore, pools: &[Vec<PlayerRef>]) {
        for (i, fencers) in pools.iter().enumerate().take(self.nb_pools()) {
            for f in fencers {
                self.manage_fencer(players, *f, Some(i));
            }
        }
        self.find_errors(self.criteria_count());
        self.fencer_list.clear();
    }

    /// `Swap` : répartition en serpentin puis séparation selon les critères.
    /// Renvoie la composition de chaque poule.
    pub fn swap(&mut self, players: &PlayerStore, fencer_list: &[PlayerRef]) -> Vec<Vec<PlayerRef>> {
        let nb = self.nb_pools();
        if nb == 0 {
            return Vec::new();
        }
        for (i, f) in fencer_list.iter().enumerate() {
            let pool_index = if (i / nb) % 2 == 0 { i % nb } else { nb - 1 - i % nb };
            self.manage_fencer(players, *f, Some(pool_index));
        }

        let count = self.criteria_count();
        for _trial in 0..2 {
            for depth in 1..=count {
                self.find_errors(depth);
                self.dispatch_errors(depth);
            }

            // Placement des erreurs restantes sans échange
            let errors = self.error_list.clone();
            for error in errors {
                let Some(error_pool) = self.proxies[error].new_pool else { continue };
                self.remove_from_pool(error, error_pool);
                for i in 0..nb {
                    if self.fencer_can_go_to(error, i, count - 1) {
                        self.insert_into_pool(error, i);
                        break;
                    }
                    if i == nb - 1 {
                        self.insert_into_pool(error, error_pool);
                    }
                }
            }
            self.find_errors(count);

            // Dernière chance : on force un déplacement
            if let Some(first) = self.error_list.first().copied() {
                self.dispatch_fencer(first, count, true);
            }
        }

        self.find_errors(count);
        let result = self.store_swapping();
        self.fencer_list.clear();
        result
    }

    fn store_swapping(&self) -> Vec<Vec<PlayerRef>> {
        self.pools.iter().map(|p| p.fencers.iter().map(|f| self.proxies[*f].player).collect()).collect()
    }

    /// `InjectFencer` : ajoute un retardataire, dans la poule `pool_id`
    /// (à partir de 1) ou dans la plus petite poule compatible.
    pub fn inject_fencer(&mut self, players: &PlayerStore, player: PlayerRef, pool_id: usize) -> Vec<Vec<PlayerRef>> {
        let shadow = self.manage_fencer(players, player, None);
        let count = self.criteria_count();
        if pool_id > 0 && pool_id <= self.nb_pools() {
            self.insert_into_pool(shadow, pool_id - 1);
        } else if self.nb_pools() > 0 {
            let mut by_size: Vec<usize> = Vec::new();
            for i in 0..self.nb_pools() {
                let size = self.pools[i].size;
                let pos = by_size.iter().position(|p| size <= self.pools[*p].size).unwrap_or(by_size.len());
                by_size.insert(pos, i);
            }
            let smallest = by_size[0];
            for p in by_size.clone() {
                if count == 0 || self.fencer_can_go_to(shadow, p, count - 1) {
                    if self.pools[p].size == self.pools[smallest].size {
                        self.insert_into_pool(shadow, p);
                    } else {
                        self.insert_into_pool(shadow, smallest);
                    }
                    break;
                }
            }
        }
        self.store_swapping()
    }

    /// `MoveFencer` : déplacement manuel d'un tireur (glisser-déposer).
    pub fn move_fencer(&mut self, player: PlayerRef, from: Option<usize>, to: Option<usize>) {
        if self.criteria_count() == 0 {
            return;
        }
        if let Some(from) = from {
            if from >= self.pools.len() {
                return;
            }
            let found = self.pools[from].fencers.iter().copied().find(|f| self.proxies[*f].player == player);
            if let Some(f) = found {
                self.remove_from_pool(f, from);
                if let Some(to) = to.filter(|t| *t < self.pools.len()) {
                    self.insert_into_pool(f, to);
                }
            }
        }
        self.find_errors(self.criteria_count());
    }

    fn get_errors(&self, c: usize, errors: &mut Vec<usize>) {
        let crit = &self.criterias[c];
        let mut own_errors: Vec<usize> = Vec::new();
        let mut over_populated: Vec<usize> = Vec::new();

        for i in 0..crit.pool_count {
            if crit.score_table[i] > crit.big.score && crit.pool_seen[i] {
                // `GetFencers` : liste construite par ajout en tête
                let mut found = 0;
                let mut pool_errors = Vec::new();
                for f in &self.pools[i].fencers {
                    if self.proxies[*f].criterias[crit.depth] == Some(c) {
                        if found >= crit.big.score {
                            pool_errors.insert(0, *f);
                        }
                        found += 1;
                    }
                }
                own_errors.extend(pool_errors);
            }
            if crit.small.score != 0 && crit.score_table[i] > crit.small.score && crit.pool_seen[i] {
                over_populated.insert(0, i);
            }
        }

        let mut remaining = 0usize;
        if over_populated.len() > crit.big.pool_count as usize {
            remaining = over_populated.len() - crit.big.pool_count as usize;
            if remaining > own_errors.len() {
                remaining -= own_errors.len();
            } else if !own_errors.is_empty() {
                remaining = 0;
            }
        }
        for pool in over_populated.iter().take(remaining) {
            // `GetFencer`
            let mut found = 0;
            let mut fencer = None;
            for f in &self.pools[*pool].fencers {
                if self.proxies[*f].criterias[crit.depth] == Some(c) {
                    found += 1;
                    if found >= crit.big.score {
                        fencer = Some(*f);
                        break;
                    }
                }
            }
            if let Some(f) = fencer {
                own_errors.insert(0, f);
            }
        }

        for f in own_errors {
            if !errors.contains(&f) {
                errors.insert(0, f);
            }
        }
    }

    fn find_errors(&mut self, depth: usize) {
        let mut errors = Vec::new();
        for d in 0..depth.min(self.distributions.len()) {
            for (_, c) in &self.distributions[d] {
                self.get_errors(*c, &mut errors);
            }
        }
        errors.sort_by_key(|f| self.proxies[*f].rank);
        self.error_list = errors;
    }

    fn dispatch_errors(&mut self, depth: usize) {
        let mut i = 0;
        while i < self.error_list.len() {
            let error = self.error_list[i];
            if self.dispatch_fencer(error, depth, false) {
                self.find_errors(depth);
                i = 0;
            } else {
                i += 1;
            }
        }
    }

    fn has_same_criteria(&self, a: usize, b: usize, depth: usize) -> bool {
        match (self.proxies[a].criterias[depth], self.proxies[b].criterias[depth]) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    fn dispatch_fencer(&mut self, error: usize, depth: usize, force: bool) -> bool {
        let Some(error_pool) = self.proxies[error].new_pool else { return false };
        self.remove_from_pool(error, error_pool);

        // `ListCrawler` : alternance droite/gauche autour du rang du tireur
        let list = self.fencer_list.clone();
        let start = self.proxies[error].rank as i64 - 1;
        let (mut left, mut right): (Option<i64>, Option<i64>) = if start >= 0 && (start as usize) < list.len() {
            (
                if start > 0 { Some(start - 1) } else { None },
                if (start as usize + 1) < list.len() { Some(start + 1) } else { None },
            )
        } else {
            (None, None)
        };
        let mut request_count = 0u32;
        let mut placed = false;

        loop {
            let next = match (left, right) {
                (None, None) => None,
                (None, Some(r)) => {
                    right = if (r as usize + 1) < list.len() { Some(r + 1) } else { None };
                    Some(r)
                }
                (Some(l), None) => {
                    left = if l > 0 { Some(l - 1) } else { None };
                    Some(l)
                }
                (Some(l), Some(r)) => {
                    request_count += 1;
                    if request_count % 2 == 1 {
                        right = if (r as usize + 1) < list.len() { Some(r + 1) } else { None };
                        Some(r)
                    } else {
                        left = if l > 0 { Some(l - 1) } else { None };
                        Some(l)
                    }
                }
            };
            let Some(n) = next else { break };
            let x = list[n as usize];
            let Some(x_pool) = self.proxies[x].new_pool else { continue };
            if error_pool != x_pool
                && !self.has_same_criteria(error, x, depth - 1)
                && self.proxies[x].rank as usize != x_pool + 1
            {
                self.remove_from_pool(x, x_pool);
                if self.fencer_can_go_to(error, x_pool, depth - 1)
                    && (force || self.fencer_can_go_to(x, error_pool, depth - 1))
                {
                    self.insert_into_pool(x, error_pool);
                    self.insert_into_pool(error, x_pool);
                    placed = true;
                    break;
                }
                self.insert_into_pool(x, x_pool);
            }
        }

        if !placed {
            self.insert_into_pool(error, error_pool);
        }
        placed
    }

    fn fencer_can_go_to(&self, fencer: usize, pool: usize, depth: usize) -> bool {
        for d in 0..=depth.min(self.criteria_count().saturating_sub(1)) {
            if let Some(c) = self.proxies[fencer].criterias.get(d).copied().flatten() {
                if !self.criterias[c].free_place_in(pool) {
                    return false;
                }
            }
        }
        true
    }

    /// Vrai s'il reste des tireurs mal séparés (`HasErrors`).
    pub fn has_errors(&self) -> bool {
        !self.error_list.is_empty()
    }

    /// Nombre de tireurs mal séparés (`GetMoved`).
    pub fn moved(&self) -> usize {
        self.error_list.len()
    }

    /// `IsAnError`.
    pub fn is_an_error(&self, player: PlayerRef) -> bool {
        self.error_list.iter().any(|e| self.proxies[*e].player == player)
    }

    /// Tireurs mal séparés, par rang de départ.
    pub fn errors(&self) -> Vec<PlayerRef> {
        self.error_list.iter().map(|e| self.proxies[*e].player).collect()
    }

    /// Poule d'origine (numéro à partir de 1) attribuée par le dernier
    /// calcul, pour chaque tireur suivi.
    pub fn original_pool(&self, player: PlayerRef) -> Option<usize> {
        self.proxies.iter().rev().find(|p| p.player == player).and_then(|p| p.original_pool.map(|o| o + 1))
    }
}
