//! État partagé d'un tour de poules.
//!
//! En C++, `Pool::Supervisor` garde un pointeur sur son `Pool::Allocator`
//! (fournisseur d'entrée) et tous deux partagent le nom, le score maximum,
//! le nombre de qualifiés et les poules. Ici ces données vivent dans un
//! `PoolRound` partagé (`Arc<Mutex<…>>`) entre les deux phases ; chaque
//! phase ne garde que son `StageBase`.
//!
//! Propriétaires des attributs locaux :
//! - allocateur : `Owner::Stage(id de l'allocateur)` (rang de départ, n° de
//!   poule, piste, heure, statut lu dans le fichier) ;
//! - tour courant : `Owner::Other(CURRENT_ROUND_TAG | id du superviseur)` ;
//! - superviseur (cumul et classement) : `Owner::Stage(id du superviseur)`.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::attribute::{self, Scope, Uniqueness};
use crate::i18n::tr;
use crate::player::{random_key, AttrId, Owner, PlayerRef, PlayerStore};
use crate::stage::{ErrorLevel, RankEntry, StageError};
use crate::xml::{self, XmlWriter};

use super::dispatcher::MAX_POOL_SIZE;
use super::neo_swapper::NeoSwapper;
use super::pool::{self, compare_players, fencer_is_absent, get_str, get_uint, set_str, set_uint, Pool, WITH_CALCULUS, WITH_POOL_NR, WITH_RANDOM};

/// Marqueur du propriétaire « tour courant » (`_current_round_owner`).
pub const CURRENT_ROUND_TAG: u64 = 0x504F_4F4C_0000_0000;
/// Marqueur du propriétaire « allocateur » quand le superviseur est seul.
pub const VIRTUAL_ALLOCATOR_TAG: u64 = 0x414C_4C4F_0000_0000;

/// Une formule de poules proposée (`Allocator::Configuration`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolConfig {
    pub nb_pool: u32,
    pub size: u32,
    /// Nombre de poules d'un tireur de plus (`size + 1`).
    pub nb_overloaded: u32,
}

impl PoolConfig {
    /// Libellé « taille » de la liste déroulante (`"6"` ou `"6 or 7"`).
    pub fn size_label(&self) -> String {
        if self.nb_overloaded > 0 {
            format!("{} {} {}", self.size, tr("or"), self.size + 1)
        } else {
            self.size.to_string()
        }
    }

    /// Plus grande poule (`GetBiggestPoolSize`).
    pub fn biggest_size(&self) -> u32 {
        if self.nb_overloaded > 0 {
            self.size + 1
        } else {
            self.size
        }
    }

    /// Nombre total de matchs (`GetNbMatchs`).
    pub fn nb_matches(&self) -> u32 {
        let s = self.size;
        s * (s.saturating_sub(1)) / 2 * (self.nb_pool - self.nb_overloaded) + self.nb_overloaded * s * (s + 1) / 2
    }
}

/// Origine d'une modification de présence (`PillowDialog`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PillowKind {
    /// Dialogue des retardataires (liste des absents).
    Latecomer,
    /// Dialogue des absents (liste des présents).
    Absent,
}

/// Raison d'un refus de déplacement manuel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    Locked,
    UnknownPool,
    UnknownFencer,
}

#[derive(Debug, Clone)]
pub struct PoolRound {
    pub(crate) alloc_id: Option<u32>,
    pub(crate) sup_id: Option<u32>,
    pub(crate) allocator_linked: bool,
    pub(crate) supervisor_linked: bool,

    pub name: String,
    pub max_score: u32,
    pub bout_timing: crate::officiating::BoutTiming,
    pub(crate) nb_qualified: u32,
    pub(crate) nb_qualified_valid: bool,
    seeding_balanced: bool,
    swapping: Option<String>,
    swapping_criteria: Vec<String>,

    configs: Vec<PoolConfig>,
    best_config: Option<usize>,
    selected_config: Option<usize>,
    pools: Vec<Pool>,
    swapper: NeoSwapper,
    original_pool: HashMap<PlayerRef, u32>,
    /// Numéro de poule mémorisé par le dialogue des absents (`RevertContext`).
    absent_revert_pool: HashMap<PlayerRef, u32>,

    /// Participants de l'allocateur (sa « short list »).
    pub(crate) attendees: Vec<PlayerRef>,
    pub(crate) team_event: bool,

    pub allocator_filter: Vec<String>,
    pub list_filter: Vec<String>,
    pub supervisor_filter: Vec<String>,
    pub classification_filter: Vec<String>,

    /// Un Marshaller est connecté : piste et arbitres sont exigés.
    pub has_marshaller: bool,
    pub(crate) allocator_locked: bool,
    pub(crate) supervisor_locked: bool,
    pub(crate) managed: bool,
    pub(crate) allocator_loaded: bool,
    pub(crate) supervisor_loaded: bool,
    pub(crate) alloc_rand_seed: u32,
    pub(crate) sup_rand_seed: u32,
    pub(crate) allocator_net_id: u32,
    /// Superviseur du tour de poules précédent (cumul des résultats).
    pub(crate) previous_round: Option<Option<u32>>,

    /// Classement figé au verrouillage (`_result`).
    pub(crate) result: Option<Vec<PlayerRef>>,
    /// Qualifiés figés au verrouillage (`_output_short_list`).
    pub(crate) output: Option<Vec<PlayerRef>>,
    pub(crate) quota_exceedance: u32,
    /// Les références ont été renumérotées : la composition est à re-vérifier.
    pub(crate) distribution_dirty: bool,
    /// Sortie croisée (entraînement des jeunes) : qualifiés rangés par place
    /// dans la poule puis par numéro de poule, pour que le tableau oppose le
    /// 1er de la poule 1 au dernier de la poule 2, etc.
    pub cross_output: bool,
    /// Entraînement des jeunes : nombre de poules de chaque groupe (les
    /// groupes sont donnés par l'attribut `youth_group` des tireurs).
    pub youth_groups: Option<Vec<u32>>,
}

/// Attributs exclus des listes d'affichage (`CreateExcludingList`).
const STAGE_EXCLUDED: &[&str] = &[
    "ref", "plugin_ID", "IP", "password", "cyphered_password", "HS", "attending", "exported", "final_rank", "global_status", "indice", "level",
    "workload_rate", "pool_nr", "promoted", "rank", "status", "team", "victories_count", "bouts_count", "victories_ratio", "strip", "time",
];
const CLASSIFICATION_EXCLUDED: &[&str] = &[
    "ref", "plugin_ID", "IP", "password", "cyphered_password", "attending", "exported", "final_rank", "global_status", "level", "workload_rate",
    "promoted", "team", "strip", "time",
];
const LIST_EXCLUDED: &[&str] = &[
    "ref", "IP", "password", "cyphered_password", "HS", "attending", "exported", "final_rank", "global_status", "indice", "level", "workload_rate",
    "promoted", "rank", "status", "team", "victories_count", "bouts_count", "victories_ratio",
];

/// Liste d'affichage (`Filter`) : attributs autorisés.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// Affichage de l'allocateur (`Affichage`).
    Allocator,
    /// Liste des tireurs de l'allocateur (`AffichageListe`).
    List,
    /// Feuilles de poule (`AffichageScores`).
    Supervisor,
    /// Classement du tour (`AffichageClassement`).
    Classification,
}

impl FilterKind {
    fn excluded(self) -> &'static [&'static str] {
        match self {
            FilterKind::Allocator | FilterKind::Supervisor => STAGE_EXCLUDED,
            FilterKind::List => LIST_EXCLUDED,
            FilterKind::Classification => CLASSIFICATION_EXCLUDED,
        }
    }

    /// Codes des attributs proposés par cette liste.
    pub fn available(self) -> Vec<&'static str> {
        attribute::registry().all().iter().map(|d| d.code_name).filter(|c| !self.excluded().contains(c)).collect()
    }

    fn defaults(self) -> Vec<String> {
        let list: &[&str] = match self {
            FilterKind::Allocator => &["stage_start_rank", "name", "first_name", "club"],
            FilterKind::List => &["name", "first_name", "club", "pool_nr", "strip", "time"],
            FilterKind::Supervisor => &["name", "first_name"],
            FilterKind::Classification => &["rank", "status", "name", "first_name", "club", "pool_nr", "victories_ratio", "indice", "HS"],
        };
        list.iter().map(|s| s.to_string()).collect()
    }

    /// `Filter::Load` : noms XML séparés par des virgules.
    fn parse(self, value: &str) -> Vec<String> {
        let available = self.available();
        let mut out = Vec::new();
        for item in value.split(',') {
            let desc = attribute::registry().by_xml(item, Scope::Local).or_else(|| attribute::registry().by_code(item));
            let code = desc.map(|d| d.code_name).unwrap_or(item);
            if available.contains(&code) && !out.iter().any(|c: &String| c == code) {
                out.push(code.to_string());
            }
        }
        out
    }
}

/// `Filter::Save`.
fn filter_image(list: &[String]) -> String {
    list.iter().map(|c| attribute::registry().by_code(c).map(|d| d.xml_name).unwrap_or(c.as_str())).collect::<Vec<_>>().join(",")
}

/// Critères de séparation « réguliers » (cases à cocher) : attributs
/// permutables à valeurs multiples (pays, région, ligue, club).
pub fn regular_swapping_criteria() -> Vec<&'static str> {
    attribute::registry().swappable().filter(|d| d.uniqueness == Uniqueness::NotSingular).map(|d| d.code_name).collect()
}

/// Critères « supplémentaires » proposés dans la liste déroulante.
pub fn extra_swapping_criteria() -> Vec<&'static str> {
    let swappable: Vec<&str> = attribute::registry().swappable().map(|d| d.code_name).collect();
    FilterKind::List
        .available()
        .into_iter()
        .filter(|c| {
            let d = attribute::desc(c);
            d.rights == attribute::Rights::Public && d.uniqueness == Uniqueness::NotSingular && !swappable.contains(c)
        })
        .collect()
}

fn derive_net_id(a: u32, b: u32) -> u32 {
    let v = (random_key(a, b ^ 0x4E45_5449) & 0xFFFF_FFFF) as u32;
    if v == 0 {
        1
    } else {
        v
    }
}

impl Default for PoolRound {
    fn default() -> Self {
        Self::new()
    }
}

impl PoolRound {
    pub fn new() -> Self {
        PoolRound {
            alloc_id: None,
            sup_id: None,
            allocator_linked: false,
            supervisor_linked: false,
            name: String::new(),
            max_score: 5,
            bout_timing: crate::officiating::BoutTiming::default(),
            nb_qualified: 0,
            nb_qualified_valid: false,
            seeding_balanced: true,
            swapping: None,
            swapping_criteria: Vec::new(),
            configs: Vec::new(),
            best_config: None,
            selected_config: None,
            pools: Vec::new(),
            swapper: NeoSwapper::new(),
            original_pool: HashMap::new(),
            absent_revert_pool: HashMap::new(),
            attendees: Vec::new(),
            team_event: false,
            allocator_filter: FilterKind::Allocator.defaults(),
            list_filter: FilterKind::List.defaults(),
            supervisor_filter: FilterKind::Supervisor.defaults(),
            classification_filter: FilterKind::Classification.defaults(),
            has_marshaller: false,
            allocator_locked: false,
            supervisor_locked: false,
            managed: false,
            allocator_loaded: false,
            supervisor_loaded: false,
            alloc_rand_seed: 0,
            sup_rand_seed: 0,
            allocator_net_id: 0,
            previous_round: None,
            result: None,
            output: None,
            quota_exceedance: 0,
            distribution_dirty: false,
            cross_output: false,
            youth_groups: None,
        }
    }

    // --- Propriétaires -----------------------------------------------------------

    fn sup_id_value(&self) -> u32 {
        self.sup_id.unwrap_or_else(|| self.alloc_id.map(|a| a + 1).unwrap_or(0))
    }

    /// Propriétaire des attributs de l'allocateur.
    pub fn allocator_owner(&self) -> Owner {
        match (self.allocator_linked, self.alloc_id) {
            (true, Some(id)) => Owner::Stage(id),
            _ => Owner::Other(VIRTUAL_ALLOCATOR_TAG | self.sup_id_value() as u64),
        }
    }

    /// Propriétaire des attributs du superviseur (cumul, classement).
    pub fn supervisor_owner(&self) -> Owner {
        Owner::Stage(self.sup_id_value())
    }

    /// Propriétaire des attributs du tour courant.
    pub fn current_round_owner(&self) -> Owner {
        Owner::Other(CURRENT_ROUND_TAG | self.sup_id_value() as u64)
    }

    pub(crate) fn xml_tag(&self) -> &'static str {
        pool::xml_player_tag(self.team_event)
    }

    /// Identifiant réseau de l'allocateur (`NetID` du `<TourDePoules>`).
    pub fn net_id(&self) -> u32 {
        if self.allocator_net_id != 0 {
            self.allocator_net_id
        } else {
            derive_net_id(self.alloc_id.unwrap_or(0) + 0x100, self.alloc_rand_seed)
        }
    }

    // --- Accesseurs ---------------------------------------------------------------

    pub fn pools(&self) -> &[Pool] {
        &self.pools
    }

    pub fn pools_mut(&mut self) -> &mut [Pool] {
        &mut self.pools
    }

    pub fn pool(&self, index: usize) -> Option<&Pool> {
        self.pools.get(index)
    }

    pub fn nb_pools(&self) -> usize {
        self.pools.len()
    }

    pub fn attendees(&self) -> &[PlayerRef] {
        &self.attendees
    }

    pub fn configs(&self) -> &[PoolConfig] {
        &self.configs
    }

    pub fn best_config(&self) -> Option<usize> {
        self.best_config
    }

    pub fn selected_config(&self) -> Option<usize> {
        self.selected_config
    }

    pub fn selected(&self) -> Option<PoolConfig> {
        self.selected_config.map(|i| self.configs[i])
    }

    pub fn seeding_is_balanced(&self) -> bool {
        self.seeding_balanced
    }

    pub fn swapping_string(&self) -> Option<&str> {
        self.swapping.as_deref()
    }

    /// Critères de séparation actifs, le premier étant prioritaire.
    pub fn swapping_criteria(&self) -> &[String] {
        &self.swapping_criteria
    }

    pub fn swapper(&self) -> &NeoSwapper {
        &self.swapper
    }

    pub fn nb_qualified(&self) -> Option<u32> {
        self.nb_qualified_valid.then_some(self.nb_qualified)
    }

    /// Fixe le nombre de qualifiés (`None` : tous).
    pub fn set_nb_qualified(&mut self, n: Option<u32>) {
        match n {
            Some(n) => {
                self.nb_qualified = n;
                self.nb_qualified_valid = true;
            }
            None => {
                self.nb_qualified_valid = false;
                self.nb_qualified = self.attendees.len() as u32;
            }
        }
    }

    /// Qualifiés exprimés en pourcentage des participants (arrondi supérieur).
    pub fn set_qualified_ratio(&mut self, percent: u32) {
        let n = (self.attendees.len() as u64 * percent as u64).div_ceil(100) as u32;
        self.set_nb_qualified(Some(n));
    }

    /// `GetBiggestPoolSize`.
    pub fn biggest_pool_size(&self) -> u32 {
        self.selected().map(|c| c.biggest_size()).unwrap_or(0)
    }

    /// `GetWorkload`.
    pub fn workload(&self) -> u32 {
        let b = self.biggest_pool_size();
        if b > 0 {
            self.max_score * (b * (b - 1) / 2)
        } else {
            0
        }
    }

    /// Nombre total de matchs de la formule choisie.
    pub fn nb_matches(&self) -> u32 {
        self.selected().map(|c| c.nb_matches()).unwrap_or(0)
    }

    pub fn set_max_score(&mut self, max_score: u32) {
        self.max_score = max_score;
        for p in &mut self.pools {
            p.set_max_score(max_score);
        }
    }

    /// Indice de la poule d'un tireur (`GetPoolOf`).
    pub fn pool_of(&self, player: PlayerRef) -> Option<usize> {
        self.pools.iter().position(|p| p.fencers().contains(&player))
    }

    /// Poule d'origine (avant déplacement manuel) d'un tireur.
    pub fn original_pool(&self, player: PlayerRef) -> Option<u32> {
        self.original_pool.get(&player).copied()
    }

    /// Le tireur a été déplacé à la main (icône « rafraîchir ») : ni
    /// retardataire, ni dans sa poule d'origine.
    pub fn is_moved(&self, players: &PlayerStore, player: PlayerRef) -> bool {
        let Some(pool) = self.pool_of(player) else { return false };
        let latecomer = players.get(player).and_then(|p| p.str_attr("incident")).map(|s| s.starts_with('R')).unwrap_or(false);
        !(latecomer || self.original_pool(player) == Some(self.pools[pool].number()))
    }

    /// La poule a la taille prévue par la formule (`is_balanced`).
    pub fn pool_is_balanced(&self, index: usize) -> bool {
        if self.youth_groups.is_some() {
            return self.pools.get(index).map(|p| p.nb_players() >= 2).unwrap_or(false);
        }
        let Some(c) = self.selected() else { return false };
        let Some(p) = self.pools.get(index) else { return false };
        let size = p.nb_players() as u32;
        size == c.size || (c.nb_overloaded > 0 && size == c.size + 1)
    }

    // --- Participants ---------------------------------------------------------------

    /// `Stage::RetrieveAttendees` pour l'allocateur.
    pub(crate) fn retrieve_allocator_attendees(&mut self, players: &mut PlayerStore, attendees: Vec<PlayerRef>) {
        let owner = self.allocator_owner();
        for (i, p) in attendees.iter().enumerate() {
            set_uint(players, *p, "stage_start_rank", owner, i as u32 + 1);
            set_str(players, *p, "status", owner, "Q");
            if let Some(pl) = players.get_mut(*p) {
                pl.set_str(&AttrId::global("global_status"), "Q");
            }
        }
        if !self.nb_qualified_valid {
            self.nb_qualified = attendees.len() as u32;
        }
        self.attendees = attendees;
    }

    // --- Formules et composition ------------------------------------------------------

    /// `Allocator::Setup` : formules possibles pour le nombre de présents.
    pub fn setup(&mut self, players: &PlayerStore) {
        let nb_players = self.attendees.iter().filter(|p| !fencer_is_absent(players, **p)).count() as u32;
        self.configs.clear();
        self.selected_config = None;
        self.best_config = None;

        let max = MAX_POOL_SIZE as u32;
        let max_pool_size = if nb_players % max != 0 { max - 1 } else { max };
        let mut size = 2;
        while size <= max_pool_size.min(nb_players) {
            if nb_players % size == 0 {
                self.register_config(PoolConfig { nb_pool: nb_players / size, size, nb_overloaded: 0 });
            }
            for p in 1..nb_players / size {
                if (nb_players - size * p) % (size + 1) == 0 {
                    let overloaded = (nb_players - size * p) / (size + 1);
                    self.register_config(PoolConfig { nb_pool: p + overloaded, size, nb_overloaded: overloaded });
                    break;
                }
            }
            size += 1;
        }
    }

    fn register_config(&mut self, config: PoolConfig) {
        self.configs.push(config);
        if config.size < 7 || (config.size == 7 && config.nb_overloaded == 0) {
            self.best_config = Some(self.configs.len() - 1);
        }
    }

    /// `SelectConfig` : formule à `nb_pool` poules.
    pub fn select_config_by_nb_pool(&mut self, nb_pool: u32) -> bool {
        match self.configs.iter().position(|c| c.nb_pool == nb_pool) {
            Some(i) => {
                self.selected_config = Some(i);
                true
            }
            None => false,
        }
    }

    fn new_pool(&self, number: u32) -> Pool {
        let mut p = Pool::new(self.max_score, self.workload(), number, self.xml_tag(), self.alloc_rand_seed, self.allocator_owner());
        if let Some(c) = self.selected() {
            p.set_strength_contributors(c.size - c.size % 2);
        }
        p.net_id = derive_net_id(self.alloc_id.unwrap_or(0) << 8 | number, self.alloc_rand_seed);
        p
    }

    /// `DeletePools`.
    pub fn delete_pools(&mut self) {
        self.pools.clear();
    }

    /// `CreatePools` : répartition en serpentin (avec séparation) ou par
    /// niveau selon `RepartitionEquilibre`.
    pub fn create_pools(&mut self, players: &mut PlayerStore) {
        if self.youth_groups.is_some() {
            self.create_youth_pools(players);
            return;
        }
        let Some(config) = self.selected() else { return };
        let nb_pool = config.nb_pool as usize;
        self.pools = (0..nb_pool).map(|i| self.new_pool(i as u32 + 1)).collect();
        let owner = self.allocator_owner();
        self.swapper.configure(nb_pool, &self.swapping_criteria, owner);
        let short_list = self.attendees.clone();

        if self.seeding_balanced {
            let distribution = self.swapper.swap(players, &short_list);
            self.store_swapping(players, distribution);
        } else {
            let mut pool = 0usize;
            for player in short_list {
                let number = pool as u32 + 1;
                let n = self.pools[pool].nb_players() as u32;
                let full = if config.nb_overloaded > 0 {
                    if number <= config.nb_overloaded {
                        n > config.size
                    } else {
                        n >= config.size
                    }
                } else {
                    n >= config.size
                };
                if full && pool + 1 < self.pools.len() {
                    pool += 1;
                }
                self.original_pool.insert(player, pool as u32 + 1);
                self.pools[pool].add_fencer(players, player, None);
            }
        }
    }

    /// Groupes de l'entraînement des jeunes, dans l'ordre des participants :
    /// (numéro du groupe, tireurs).
    pub fn youth_members(&self, players: &PlayerStore) -> Vec<(u32, Vec<PlayerRef>)> {
        let mut groups: Vec<(u32, Vec<PlayerRef>)> = Vec::new();
        for p in &self.attendees {
            if fencer_is_absent(players, *p) {
                continue;
            }
            let g = players.get(*p).and_then(|pl| pl.attr("youth_group")).map(|v| v.as_uint()).unwrap_or(1);
            match groups.iter_mut().find(|(n, _)| *n == g) {
                Some((_, list)) => list.push(*p),
                None => groups.push((g, vec![*p])),
            }
        }
        groups.sort_by_key(|(g, _)| *g);
        groups
    }

    /// Entraînement des jeunes : les poules de chaque groupe, numérotées à
    /// la suite (serpentin avec décalage, ou par niveau).
    fn create_youth_pools(&mut self, players: &mut PlayerStore) {
        let nb_pools = self.youth_groups.clone().unwrap_or_default();
        let groups = self.youth_members(players);
        let owner = self.allocator_owner();
        self.pools.clear();
        let mut number = 1u32;
        for (gi, (group, members)) in groups.iter().enumerate() {
            let k = nb_pools.get(gi).copied().unwrap_or(1).clamp(1, members.len().max(1) as u32) as usize;
            let first = self.pools.len();
            for local in 0..k {
                let mut pool = self.new_pool(number);
                pool.set_group(*group, local as u32 + 1);
                self.pools.push(pool);
                number += 1;
            }
            let distribution: Vec<Vec<PlayerRef>> = if self.seeding_balanced {
                let mut swapper = NeoSwapper::new();
                swapper.configure(k, &self.swapping_criteria, owner);
                swapper.swap(players, members)
            } else {
                // Par niveau : les meilleurs dans la première poule.
                let n = members.len();
                let mut it = members.iter().copied();
                (0..k).map(|i| it.by_ref().take(n / k + usize::from(i < n % k)).collect()).collect()
            };
            for (i, fencers) in distribution.into_iter().enumerate() {
                for f in fencers {
                    self.original_pool.insert(f, (first + i) as u32 + 1);
                    if let Some(pool) = self.pools.get_mut(first + i) {
                        pool.add_fencer(players, f, None);
                    }
                }
            }
        }
        self.swapper.configure(self.pools.len(), &self.swapping_criteria, owner);
    }

    /// Après relecture d'un fichier : groupe et nom des poules d'après leurs tireurs.
    fn name_youth_pools(&mut self, players: &PlayerStore) {
        if self.youth_groups.is_none() {
            return;
        }
        let mut local: Vec<(u32, u32)> = Vec::new();
        for pool in &mut self.pools {
            let g = pool
                .fencers()
                .first()
                .and_then(|f| players.get(*f))
                .and_then(|p| p.attr("youth_group"))
                .map(|v| v.as_uint())
                .unwrap_or(1);
            let n = match local.iter_mut().find(|(group, _)| *group == g) {
                Some((_, n)) => {
                    *n += 1;
                    *n
                }
                None => {
                    local.push((g, 1));
                    1
                }
            };
            pool.set_group(g, n);
        }
    }

    fn store_swapping(&mut self, players: &mut PlayerStore, distribution: Vec<Vec<PlayerRef>>) {
        for (i, fencers) in distribution.into_iter().enumerate() {
            for f in fencers {
                self.original_pool.insert(f, i as u32 + 1);
                if let Some(pool) = self.pools.get_mut(i) {
                    pool.add_fencer(players, f, None);
                }
            }
        }
    }

    /// `RefreshDisplay` : recompose les poules (hors verrouillage).
    pub fn refresh_composition(&mut self, players: &mut PlayerStore) {
        if !self.allocator_locked {
            self.delete_pools();
            self.create_pools(players);
        }
    }

    /// `Allocator::Garnish`.
    pub fn garnish(&mut self, players: &mut PlayerStore) {
        if self.pools.is_empty() {
            self.setup(players);
            if self.youth_groups.is_some() {
                self.selected_config = self.best_config;
                self.create_pools(players);
            } else if let Some(best) = self.best_config {
                self.selected_config = Some(best);
                self.create_pools(players);
            }
        }
    }

    /// Choix d'une formule dans la liste (`OnComboboxChanged`).
    pub fn select_config(&mut self, players: &mut PlayerStore, index: usize) -> bool {
        if index >= self.configs.len() || self.allocator_locked {
            return false;
        }
        self.selected_config = Some(index);
        self.refresh_composition(players);
        true
    }

    /// Répartition équilibrée (serpentin) ou par niveau (`ApplyConfig`).
    pub fn set_seeding_balanced(&mut self, players: &mut PlayerStore, balanced: bool) {
        if self.seeding_balanced != balanced {
            self.seeding_balanced = balanced;
            if !balanced {
                self.swapping_criteria.clear();
            }
            self.refresh_composition(players);
        }
    }

    /// Entraînement des jeunes : serpentin (avec décalage) ou par niveau,
    /// fixé avant la composition des poules.
    pub fn set_youth_seeding(&mut self, balanced: bool, swapping: &[String]) {
        self.seeding_balanced = balanced;
        let regular = regular_swapping_criteria();
        let extra = extra_swapping_criteria();
        let mut list: Vec<String> = Vec::new();
        let mut image = String::from("/");
        for code in swapping {
            if regular.contains(&code.as_str()) || extra.contains(&code.as_str()) {
                list.push(code.clone());
                image.push_str(code);
                image.push('/');
            }
        }
        self.swapping_criteria = if balanced { list } else { Vec::new() };
        self.swapping = Some(image);
    }

    /// Critères de séparation (`OnSwappingToggled`) : `regular` parmi
    /// `regular_swapping_criteria()` (pris dans l'ordre du registre),
    /// `extras` parmi `extra_swapping_criteria()` (prioritaires, dans l'ordre
    /// donné ; les doublons sont ignorés).
    pub fn set_swapping_criteria(&mut self, players: &mut PlayerStore, regular: &[&str], extras: &[&str]) {
        let available = extra_swapping_criteria();
        let mut list: Vec<String> = Vec::new();
        for e in extras {
            if available.contains(e) && !list.iter().any(|c| c == e) {
                list.push(e.to_string());
            }
        }
        let extra_count = list.len();
        for code in regular_swapping_criteria() {
            if regular.contains(&code) {
                list.push(code.to_string());
            }
        }
        // Image : réguliers puis supplémentaires (relue par `parse_swapping`).
        let mut image = String::from("/");
        for code in list[extra_count..].iter().chain(list[..extra_count].iter()) {
            image.push_str(code);
            image.push('/');
        }
        self.swapping_criteria = list;
        self.swapping = Some(image);
        self.refresh_composition(players);
    }

    fn parse_swapping(&mut self) {
        self.swapping_criteria.clear();
        let Some(s) = self.swapping.clone() else { return };
        let regular = regular_swapping_criteria();
        let extra = extra_swapping_criteria();
        let mut extras: Vec<String> = Vec::new();
        for token in s.split('/').filter(|t| !t.is_empty()) {
            if regular.contains(&token) {
                self.swapping_criteria.push(token.to_string());
            } else if extra.contains(&token) && !extras.iter().any(|e| e == token) {
                extras.push(token.to_string());
            }
        }
        // Les critères supplémentaires passent en premier, dans leur ordre.
        extras.append(&mut self.swapping_criteria);
        self.swapping_criteria = extras;
    }

    fn pools_distribution(&self) -> Vec<Vec<PlayerRef>> {
        self.pools.iter().map(|p| p.fencers().to_vec()).collect()
    }

    /// Reconfigure le répartiteur et analyse la composition courante.
    pub fn check_distribution(&mut self, players: &PlayerStore) {
        let owner = self.allocator_owner();
        self.swapper.configure(self.pools.len(), &self.swapping_criteria, owner);
        let d = self.pools_distribution();
        self.swapper.check_current_distribution(players, &d);
    }

    /// Glisser-déposer d'un tireur vers la poule `to` (indice) (`DropObject`).
    pub fn move_fencer(&mut self, players: &mut PlayerStore, player: PlayerRef, to: usize) -> Result<(), MoveError> {
        if self.allocator_locked {
            return Err(MoveError::Locked);
        }
        if to >= self.pools.len() {
            return Err(MoveError::UnknownPool);
        }
        let from = self.pool_of(player);
        if from.is_none() && !self.attendees.contains(&player) {
            return Err(MoveError::UnknownFencer);
        }
        if let Some(f) = from {
            self.pools[f].remove_fencer(players, player);
        }
        // Entraînement des jeunes : changer de poule peut changer de groupe.
        if let Some(g) = self.pools[to].group() {
            if let Some(p) = players.get_mut(player) {
                p.set_uint(&AttrId::global("youth_group"), g);
            }
        }
        self.pools[to].add_fencer(players, player, None);
        self.swapper.move_fencer(player, from, Some(to));
        Ok(())
    }

    /// Le déplacement d'un joueur est interdit (`DragingIsForbidden`).
    pub fn dragging_is_forbidden(&self, players: &PlayerStore, player: PlayerRef) -> bool {
        match players.get(player).map(|p| p.class) {
            Some(crate::player::PlayerClass::Fencer) | Some(crate::player::PlayerClass::Team) => self.allocator_locked,
            _ => true,
        }
    }

    /// Message `BellePoule2D::Roadmap` (`OnPoolRoadmap`) : piste, heure et
    /// arbitres d'une poule. `start` = (date `jj.mm.aaaa`, heure `hh:mm`).
    pub fn set_pool_roadmap(&mut self, players: &mut PlayerStore, index: usize, piste: Option<(u32, &str, &str)>, referees: &[PlayerRef]) {
        let Some(pool) = self.pools.get_mut(index) else { return };
        pool.set_roadmap(players, 0, None);
        if let Some((strip, date, time)) = piste {
            pool.set_roadmap(players, strip, crate::bout::StartTime::parse(date, time));
        }
        pool.remove_all_referees();
        for r in referees {
            if players.get(*r).is_some() {
                pool.add_referee(*r);
            }
        }
    }

    // --- Retardataires et absents -------------------------------------------------------

    /// Les dialogues retardataires/absents sont proposés (`Display`).
    pub fn pillows_allowed(&self) -> bool {
        !self.team_event && self.alloc_id == Some(1) && !self.allocator_locked
    }

    /// `OnAttendeeToggled` : un tireur devient absent (`attending = false`)
    /// ou présent. Renvoie faux si la formule de poules ne permet pas
    /// l'opération (la modification doit alors être annulée).
    pub fn on_attendee_toggled(&mut self, players: &mut PlayerStore, from: PillowKind, attendee: PlayerRef, attending: bool, nb_pools: u32) -> bool {
        let had_incident = players.get(attendee).and_then(|p| p.str_attr("incident")).is_some();
        if let Some(p) = players.get_mut(attendee) {
            p.remove(&AttrId::global("incident"));
            p.set(&AttrId::global("attending"), attribute::Value::Bool(attending));
        }

        let zone;
        if !attending {
            zone = self.pool_of(attendee);
            let Some(z) = zone else { return false };
            if !had_incident {
                if let Some(p) = players.get_mut(attendee) {
                    p.set_str(&AttrId::global("incident"), "A");
                }
                self.pools[z].dismiss_fencer(players, attendee);
            } else {
                self.pools[z].remove_fencer(players, attendee);
                self.attendees.retain(|a| *a != attendee);
            }
            if from == PillowKind::Absent {
                self.absent_revert_pool.insert(attendee, self.pools[z].number());
            }
        } else {
            if !had_incident {
                if let Some(p) = players.get_mut(attendee) {
                    p.set_str(&AttrId::global("incident"), "R");
                }
            }
            let mut list = self.attendees.clone();
            if !list.contains(&attendee) {
                list.push(attendee);
            }
            self.retrieve_allocator_attendees(players, list);

            let pool_id = if from == PillowKind::Latecomer { 0 } else { self.absent_revert_pool.get(&attendee).copied().unwrap_or(0) };
            let distribution = self.swapper.inject_fencer(players, attendee, pool_id as usize);
            self.store_swapping(players, distribution);
            zone = self.pool_of(attendee);
            if had_incident {
                if let Some(z) = zone {
                    self.pools[z].allow_fencer(players, attendee);
                }
            }
        }

        self.check_distribution(players);

        self.setup(players);
        if !self.select_config_by_nb_pool(nb_pools) {
            return false;
        }
        true
    }

    // --- Verrouillage de l'allocateur ------------------------------------------------------

    /// `Allocator::OnLocked` : création des matchs de chaque poule.
    pub(crate) fn lock_allocator(&mut self, players: &PlayerStore) {
        self.allocator_locked = true;
        let affinity = self.swapping_criteria.first().cloned();
        for p in &mut self.pools {
            p.create_matches(players, affinity.as_deref());
        }
    }

    /// `Allocator::IsOver`.
    pub fn allocator_is_over(&self) -> bool {
        (0..self.pools.len()).all(|i| self.pool_is_balanced(i))
    }

    /// `Allocator::GetError`.
    pub fn allocator_error(&self) -> Option<StageError> {
        for (i, p) in self.pools.iter().enumerate() {
            if !self.pool_is_balanced(i) {
                return Some(StageError { level: ErrorLevel::Error, message: tr("Wrong fencers count!"), guilty: Some(p.name().to_string()) });
            }
        }
        if self.has_marshaller {
            for p in &self.pools {
                if p.piste() == 0 || p.referees().is_empty() {
                    return Some(StageError { level: ErrorLevel::Warning, message: tr("Referees allocation \n ongoing"), guilty: None });
                }
            }
        }
        None
    }

    // --- Supervision -------------------------------------------------------------------

    fn detect_previous_round(&self, players: &PlayerStore) -> Option<Owner> {
        if let Some(explicit) = self.previous_round {
            return explicit.map(Owner::Stage);
        }
        if !self.allocator_linked {
            return None;
        }
        let prev = self.alloc_id?.checked_sub(1)?;
        let owner = Owner::Stage(prev);
        let found = self.attendees.iter().any(|p| get_uint(players, *p, "bouts_count", owner).is_some() && get_uint(players, *p, "victories_count", owner).is_some());
        found.then_some(owner)
    }

    /// `RetrievePools`/`Manage` : rattache les poules au superviseur et
    /// reprend les statuts de l'allocateur.
    pub fn manage_pools(&mut self, players: &mut PlayerStore) {
        let current = self.current_round_owner();
        let combined = self.supervisor_owner();
        let alloc = self.allocator_owner();
        let previous = self.detect_previous_round(players);
        for p in &mut self.pools {
            p.set_data_owner(players, current, combined, previous);
            p.copy_players_status(players, alloc);
        }
        self.managed = true;
    }

    /// Une poule au moins a des matchs.
    pub fn has_matches(&self) -> bool {
        self.pools.iter().any(|p| p.has_matches())
    }

    /// `Supervisor::IsOver`.
    pub fn supervisor_is_over(&self) -> bool {
        self.pools.iter().all(|p| p.is_over())
    }

    /// `Supervisor::GetError`.
    pub fn supervisor_error(&self) -> Option<StageError> {
        self.pools.iter().find(|p| p.has_error()).map(|p| StageError {
            level: ErrorLevel::Error,
            message: tr("Bout without winner!"),
            guilty: Some(p.name().to_string()),
        })
    }

    /// Saisie aléatoire des scores de toutes les poules (`OnStuffClicked`).
    pub fn stuff(&mut self, players: &mut PlayerStore) {
        for p in &mut self.pools {
            p.clean_scores(players);
            p.stuff(players);
        }
    }

    fn evaluate(&self, players: &PlayerStore, list: &mut [PlayerRef], sort_owner: Owner, rank_owner: Owner, policy: u32) -> Vec<u32> {
        let seed = self.sup_rand_seed;
        crate::sort::glib_sort_by(&mut *list, |a, b| compare_players(players, *a, *b, Some(sort_owner), seed, policy));
        let mut ranks = Vec::with_capacity(list.len());
        let mut previous_rank = 0;
        for i in 0..list.len() {
            if i > 0 && compare_players(players, list[i], list[i - 1], Some(rank_owner), 0, WITH_CALCULUS) == Ordering::Equal {
                ranks.push(previous_rank);
            } else {
                previous_rank = i as u32 + 1;
                ranks.push(previous_rank);
            }
        }
        ranks
    }

    /// `GetCurrentClassification` sans écriture : (classement du tour courant,
    /// classement cumulé), chacun sous forme (joueur, place).
    pub fn compute_classification(&self, players: &PlayerStore) -> (Vec<(PlayerRef, u32)>, Vec<(PlayerRef, u32)>) {
        let mut list: Vec<PlayerRef> = Vec::new();
        for p in &self.pools {
            for f in p.sorted_fencers() {
                list.insert(0, *f);
            }
        }
        let mut policy = WITH_CALCULUS | WITH_RANDOM;
        if !self.seeding_balanced {
            policy |= WITH_POOL_NR;
        }
        let current = self.current_round_owner();
        let combined = self.supervisor_owner();
        let r1 = self.evaluate(players, &mut list, current, current, policy);
        let current_list: Vec<(PlayerRef, u32)> = list.iter().copied().zip(r1).collect();
        let sort_owner = if self.seeding_balanced { combined } else { current };
        let r2 = self.evaluate(players, &mut list, sort_owner, combined, policy);
        (current_list, list.iter().copied().zip(r2).collect())
    }

    /// Calcule et enregistre les places (`rank`) du tour courant, du cumul
    /// et de l'allocateur. Renvoie le classement cumulé.
    pub fn refresh_classification(&mut self, players: &mut PlayerStore) -> Vec<PlayerRef> {
        let (current, combined) = self.compute_classification(players);
        let (co, so, ao) = (self.current_round_owner(), self.supervisor_owner(), self.allocator_owner());
        for (p, r) in &current {
            set_uint(players, *p, "rank", co, *r);
        }
        for (p, r) in &combined {
            set_uint(players, *p, "rank", so, *r);
            set_uint(players, *p, "rank", ao, *r);
        }
        combined.into_iter().map(|(p, _)| p).collect()
    }

    /// `SetOutputShortlist` : qualifiés, en appliquant les abandons et le
    /// quota. `apply` écrit les statuts (`N`, statut global des sortants).
    pub(crate) fn output_short_list(&self, players: &mut Option<&mut PlayerStore>, view: &PlayerStore, result: &[PlayerRef], ranks: &HashMap<PlayerRef, u32>) -> (Vec<PlayerRef>, u32) {
        let so = self.supervisor_owner();
        let mut list: Vec<PlayerRef> = Vec::new();
        let status_of = |players: &Option<&mut PlayerStore>, p: PlayerRef| -> Option<String> {
            match players {
                Some(store) => get_str(store, p, "status", so),
                None => get_str(view, p, "status", so),
            }
        };
        for p in result {
            if let Some(st) = status_of(players, *p) {
                if !st.starts_with('Q') && !st.starts_with('N') {
                    if let Some(store) = players.as_deref_mut() {
                        if let Some(pl) = store.get_mut(*p) {
                            pl.set_str(&AttrId::global("global_status"), &st);
                        }
                    }
                } else {
                    list.push(*p);
                }
            }
        }

        // Tireurs non promus (barrage)
        while let Some(last) = list.last().copied() {
            let promoted = match players {
                Some(store) => store.get(last).and_then(|pl| pl.get(&AttrId::local("promoted", so)).map(|v| v.as_bool())),
                None => view.get(last).and_then(|pl| pl.get(&AttrId::local("promoted", so)).map(|v| v.as_bool())),
            };
            if promoted == Some(false) {
                if let Some(store) = players.as_deref_mut() {
                    set_str(store, last, "status", so, "N");
                    if let Some(pl) = store.get_mut(last) {
                        pl.set_str(&AttrId::global("global_status"), "N");
                    }
                }
                list.pop();
            } else {
                break;
            }
        }

        let mut exceedance = 0;
        if self.nb_qualified_valid && self.nb_qualified > 0 {
            let n = self.nb_qualified as usize;
            if let Some(last_qualified) = list.get(n - 1).copied() {
                let last_rank = ranks.get(&last_qualified).copied().unwrap_or(0);
                while let Some(p) = list.last().copied() {
                    if ranks.get(&p).copied().unwrap_or(0) <= last_rank {
                        break;
                    }
                    if status_of(players, p).map(|s| s.starts_with('Q')).unwrap_or(false) {
                        if let Some(store) = players.as_deref_mut() {
                            set_str(store, p, "status", so, "N");
                            if let Some(pl) = store.get_mut(p) {
                                pl.set_str(&AttrId::global("global_status"), "N");
                            }
                        }
                    }
                    list.pop();
                }
            }
            if list.len() > n {
                exceedance = list.len() as u32;
                let mut last_rank: Option<u32> = None;
                for (i, p) in list.iter().rev().enumerate() {
                    let r = ranks.get(p).copied().unwrap_or(0);
                    match last_rank {
                        None => last_rank = Some(r),
                        Some(l) if r < l => {
                            exceedance = i as u32;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        (list, exceedance)
    }

    /// `Stage::SetResult` du superviseur.
    pub(crate) fn set_supervisor_result(&mut self, players: &mut PlayerStore) {
        let so = self.supervisor_owner();
        if let Some(previous) = self.output.clone() {
            for p in previous {
                if get_str(players, p, "status", so).map(|s| s.starts_with('N')).unwrap_or(false) {
                    set_str(players, p, "status", so, "Q");
                }
                if let Some(pl) = players.get_mut(p) {
                    if pl.str_attr("global_status").map(|s| s.starts_with('N')).unwrap_or(false) {
                        pl.set_str(&AttrId::global("global_status"), "Q");
                    }
                }
            }
        }
        let result = self.refresh_classification(players);
        let ranks: HashMap<PlayerRef, u32> = result.iter().map(|p| (*p, get_uint(players, *p, "rank", so).unwrap_or(0))).collect();
        let view = players.clone();
        let (output, exceedance) = self.output_short_list(&mut Some(players), &view, &result, &ranks);
        let output = self.cross_order(output);
        self.result = Some(result);
        self.output = Some(output);
        self.quota_exceedance = exceedance;
    }

    /// Classement du superviseur (verrouillé ou calculé à la volée).
    pub fn supervisor_classification(&self, players: &PlayerStore) -> Vec<RankEntry> {
        let so = self.supervisor_owner();
        let entries: Vec<(PlayerRef, u32)> = match &self.result {
            Some(result) => result.iter().map(|p| (*p, get_uint(players, *p, "rank", so).unwrap_or(0))).collect(),
            None => self.compute_classification(players).1,
        };
        entries
            .into_iter()
            .map(|(p, rank)| RankEntry { player: p, rank, status: pool::status_char(players, p, so).unwrap_or('Q') })
            .collect()
    }

    /// Qualifiés du superviseur (figés au verrouillage, sinon simulés).
    pub fn supervisor_output(&self, players: &PlayerStore) -> Vec<PlayerRef> {
        if let Some(o) = &self.output {
            return o.clone();
        }
        let (_, combined) = self.compute_classification(players);
        let ranks: HashMap<PlayerRef, u32> = combined.iter().copied().collect();
        let result: Vec<PlayerRef> = combined.iter().map(|(p, _)| *p).collect();
        self.cross_order(self.output_short_list(&mut None, players, &result, &ranks).0)
    }

    /// Sortie croisée : par place dans la poule, puis par numéro de poule
    /// (l'ordre du classement départage les ex æquo).
    fn cross_order(&self, list: Vec<PlayerRef>) -> Vec<PlayerRef> {
        if !self.cross_output {
            return list;
        }
        // Groupe d'abord (entraînement des jeunes), puis place, puis poule.
        let mut keyed: Vec<(u32, u32, usize, usize, PlayerRef)> = list
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let pool = self.pool_of(*p).unwrap_or(usize::MAX);
                let group = self.pools.get(pool).and_then(|pl| pl.group()).unwrap_or(0);
                let place = self.pools.get(pool).map(|pl| pl.result(*p).rank).filter(|r| *r > 0).unwrap_or(u32::MAX);
                (group, place, pool, i, *p)
            })
            .collect();
        keyed.sort();
        keyed.into_iter().map(|(_, _, _, _, p)| p).collect()
    }

    /// `GiveEliminatedAFinalRank` : place finale des éliminés.
    /// `next_has_own_ranking` : la phase suivante établit son propre classement.
    pub fn give_eliminated_a_final_rank(&self, players: &mut PlayerStore, next_has_own_ranking: bool) {
        let (Some(result), Some(output)) = (&self.result, &self.output) else { return };
        let so = self.supervisor_owner();
        let qualified = output.len() as u32;
        let mut fix = 0;
        for p in result {
            let rank = get_uint(players, *p, "rank", so).unwrap_or(0);
            let final_rank = if !next_has_own_ranking {
                Some(rank)
            } else {
                match pool::status_char(players, *p, so) {
                    Some('A') if rank <= qualified => {
                        fix += 1;
                        Some(qualified + fix)
                    }
                    Some('A') | Some('N') | Some('E') => Some(rank),
                    _ => None,
                }
            };
            if let (Some(fr), Some(pl)) = (final_rank, players.get_mut(*p)) {
                pl.set_uint(&AttrId::global("final_rank"), fr);
            }
        }
    }

    /// Densité d'impression des feuilles de poule (`SetPrintPoolDensity`) :
    /// (poules par colonne, poules par ligne).
    pub fn print_density(&self) -> (u32, u32) {
        let size = self.biggest_pool_size().max(1);
        ((21 / size).clamp(1, 4), (14 / size).clamp(1, 2))
    }

    /// Nombre de pages pour imprimer toutes les poules (`PreparePrint`).
    pub fn print_page_count(&self, for_referees: bool) -> u32 {
        let (v, h) = if for_referees { (1, 1) } else { self.print_density() };
        let density = (v * h) as usize;
        self.pools.len().div_ceil(density) as u32
    }

    /// Export HTML des poules (`Supervisor::DumpToHTML`).
    pub fn dump_to_html(&self, players: &PlayerStore) -> String {
        let mut s = String::from("        <div>\n          <table>\n");
        for p in &self.pools {
            s.push_str(&p.dump_to_html(players, self.biggest_pool_size() as usize));
            s.push_str("          <tr class=\"PoolSeparator\">\n");
        }
        s.push_str("          </table>\n\n        </div>\n");
        s
    }

    // --- XML -------------------------------------------------------------------------

    /// `Allocator::SaveConfiguration` (attributs du `<TourDePoules>`).
    pub fn save_configuration(&self, w: &mut XmlWriter) {
        self.bout_timing.save(w);
        w.attr_fmt("PhaseID", self.sup_id_value());
        w.attr("ID", &self.name);
        w.attr("NetID", &format!("{:x}", self.net_id()));
        w.attr_fmt("ScoreMax", self.max_score);
        if self.nb_qualified_valid {
            w.attr_fmt("NbQualifiesParIndice", self.nb_qualified);
        }
        w.attr("Affichage", &filter_image(&self.allocator_filter));
        w.attr("AffichageScores", &filter_image(&self.supervisor_filter));
        w.attr("AffichageClassement", &filter_image(&self.classification_filter));
        w.attr("AffichageListe", &filter_image(&self.list_filter));
        w.attr_fmt("RepartitionEquilibre", self.seeding_balanced as u32);
        if self.cross_output {
            w.attr("SortieCroisee", "1");
        }
        if let Some(g) = &self.youth_groups {
            w.attr("GroupesJeunes", &g.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","));
        }
        if let Some(s) = &self.swapping {
            w.attr("Decalage", s);
        }
    }

    /// `SaveHeader` : ouvre `<TourDePoules>` (à fermer par l'appelant).
    pub fn save_header(&self, w: &mut XmlWriter) {
        w.start("TourDePoules");
        self.save_configuration(w);
        if !self.pools.is_empty() {
            w.attr_fmt("NbDePoules", self.pools.len());
        }
        let next = if self.allocator_linked { self.alloc_id.unwrap_or(0) + 2 } else { self.sup_id_value() + 1 };
        w.attr_fmt("PhaseSuivanteDesQualifies", next);
    }

    /// `Allocator::Save` : élément `<TourDePoules>` complet.
    pub fn save(&self, players: &PlayerStore, w: &mut XmlWriter) {
        self.save_header(w);
        let (ao, so) = (self.allocator_owner(), self.supervisor_owner());
        for p in &self.attendees {
            let Some(pl) = players.get(*p) else { continue };
            w.start(pl.class.xml_tag());
            w.attr_fmt("REF", *p);
            if let Some(v) = pl.get(&AttrId::local("stage_start_rank", ao)) {
                w.attr_fmt("RangInitial", v.as_uint());
            }
            if let Some(v) = pl.get(&AttrId::local("rank", so)) {
                w.attr_fmt("RangFinal", v.as_uint());
            }
            if let Some(v) = pl.get(&AttrId::local("status", so)) {
                w.attr("Statut", &v.xml_image());
            }
            w.end();
        }
        for p in &self.pools {
            p.save(players, w);
        }
        w.end();
    }

    /// `LoadConfiguration` (allocateur et superviseur).
    pub(crate) fn load_configuration(&mut self, node: roxmltree::Node) {
        self.bout_timing.load(node);
        if let Some(name) = node.attribute("ID") {
            self.name = name.to_string();
        }
        if let Some(v) = node.attribute("ScoreMax") {
            self.set_max_score(attribute::parse_leading_int(v).max(0) as u32);
        }
        if let Some(v) = node.attribute("NbQualifiesParIndice") {
            self.nb_qualified = attribute::parse_leading_int(v).max(0) as u32;
            self.nb_qualified_valid = true;
        }
        if let Some(v) = node.attribute("NetID") {
            self.allocator_net_id = u32::from_str_radix(v, 16).unwrap_or(0);
        }
        if let Some(v) = node.attribute("Affichage") {
            self.allocator_filter = FilterKind::Allocator.parse(v);
        }
        if let Some(v) = node.attribute("AffichageScores") {
            self.supervisor_filter = FilterKind::Supervisor.parse(v);
        }
        if let Some(v) = node.attribute("AffichageClassement") {
            self.classification_filter = FilterKind::Classification.parse(v);
        }
        if let Some(v) = node.attribute("AffichageListe") {
            self.list_filter = FilterKind::List.parse(v);
        }
        if let Some(v) = node.attribute("RepartitionEquilibre") {
            self.seeding_balanced = attribute::parse_leading_int(v) == 1;
        }
        self.cross_output = node.attribute("SortieCroisee") == Some("1");
        self.youth_groups = node
            .attribute("GroupesJeunes")
            .map(|v| v.split(',').filter_map(|n| n.trim().parse::<u32>().ok()).collect());
        self.swapping = node.attribute("Decalage").map(str::to_string);
        self.parse_swapping();
    }

    /// `Allocator::Load` : configuration, participants et poules.
    pub(crate) fn load(&mut self, players: &mut PlayerStore, node: roxmltree::Node) {
        self.load_configuration(node);
        let nb_pool = xml::attr_u32(node, "NbDePoules", 0);
        let tag = self.xml_tag();
        let ao = self.allocator_owner();
        let mut attendees = Vec::new();
        self.pools.clear();

        for child in node.children().filter(|n| n.is_element()) {
            let name = child.tag_name().name();
            if name == tag {
                let Some(r) = child.attribute("REF").map(|v| attribute::parse_leading_int(v) as PlayerRef) else { continue };
                let Some(p) = players.get_mut(r) else { continue };
                p.set_uint(&AttrId::local("stage_start_rank", ao), xml::attr_u32(child, "RangInitial", 0));
                let status = child.attribute("Statut").unwrap_or("Q");
                p.set_str(&AttrId::local("status", ao), status);
                p.set_str(&AttrId::global("global_status"), status);
                if !attendees.contains(&r) {
                    attendees.push(r);
                }
            } else if name == "Poule" {
                if self.configs.is_empty() || self.attendees != attendees {
                    self.attendees = attendees.clone();
                    if !self.nb_qualified_valid {
                        self.nb_qualified = attendees.len() as u32;
                    }
                    self.setup(players);
                    self.select_config_by_nb_pool(nb_pool);
                    // Poule d'origine de chaque tireur
                    self.create_pools(players);
                    self.delete_pools();
                }
                self.load_pool(players, child);
            }
        }
        if self.attendees != attendees {
            self.attendees = attendees;
            if !self.nb_qualified_valid {
                self.nb_qualified = self.attendees.len() as u32;
            }
        }
        if !self.pools.is_empty() {
            self.name_youth_pools(players);
            self.check_distribution(players);
        }
    }

    fn load_pool(&mut self, players: &mut PlayerStore, node: roxmltree::Node) {
        let number = self.pools.len() as u32 + 1;
        let mut pool = self.new_pool(number);
        pool.load_header(node);
        let tag = self.xml_tag();
        let affinity = self.swapping_criteria.first().cloned();
        let mut matches_loaded = false;
        for child in node.children().filter(|n| n.is_element()) {
            let name = child.tag_name().name();
            if name == tag {
                let r = xml::attr_u32(child, "REF", 0);
                if players.get(r).is_some() {
                    let position = child.attribute("NoDansLaPoule").map(|v| attribute::parse_leading_int(v).max(0) as usize);
                    pool.add_fencer(players, r, Some(position.unwrap_or(0)));
                }
            } else if name == "Arbitre" {
                let r = xml::attr_u32(child, "REF", 0);
                // Les arbitres ont un magasin distinct de celui des tireurs.
                if r != 0 {
                    pool.append_referee(r);
                }
            } else if name == "Match" && !matches_loaded {
                pool.create_matches(players, affinity.as_deref());
                pool.load_matches(node);
                matches_loaded = true;
            }
        }
        self.pools.push(pool);
    }
}

impl PoolRound {
    /// Renumérotation des tireurs par le pointage (voir `Pool::remap_fencers`).
    /// La composition est re-vérifiée ensuite par `recheck_distribution`.
    pub(crate) fn remap_refs(&mut self, map: &HashMap<PlayerRef, PlayerRef>) {
        let tr = |r: &PlayerRef| map.get(r).copied().unwrap_or(*r);
        self.attendees = self.attendees.iter().map(tr).collect();
        for p in &mut self.pools {
            p.remap_fencers(map);
        }
        self.original_pool = std::mem::take(&mut self.original_pool).into_iter().map(|(r, v)| (tr(&r), v)).collect();
        self.absent_revert_pool = std::mem::take(&mut self.absent_revert_pool).into_iter().map(|(r, v)| (tr(&r), v)).collect();
        if let Some(r) = self.result.as_mut() {
            *r = r.iter().map(tr).collect();
        }
        if let Some(o) = self.output.as_mut() {
            *o = o.iter().map(tr).collect();
        }
        self.distribution_dirty = true;
    }

    /// Analyse de nouveau la composition si les références ont changé.
    pub(crate) fn recheck_distribution(&mut self, players: &PlayerStore) {
        if std::mem::take(&mut self.distribution_dirty) && !self.pools.is_empty() {
            self.check_distribution(players);
        }
    }

    /// `Allocator::Reset` : supprime les poules et les formules.
    pub(crate) fn reset_allocator(&mut self) {
        self.delete_pools();
        self.configs.clear();
        self.best_config = None;
        self.selected_config = None;
        self.original_pool.clear();
        self.allocator_locked = false;
    }

    /// `Allocator::OnUnLocked` : les poules redeviennent modifiables.
    pub(crate) fn unlock_allocator(&mut self) {
        self.allocator_locked = false;
    }

    /// `Supervisor::OnLocked` : verrouille les poules.
    pub(crate) fn lock_supervisor(&mut self, players: &mut PlayerStore) {
        self.supervisor_locked = true;
        for p in &mut self.pools {
            p.lock(players);
        }
    }

    /// `Supervisor::OnUnLocked` + `Stage::FreeResult`.
    pub(crate) fn unlock_supervisor(&mut self) {
        self.supervisor_locked = false;
        for p in &mut self.pools {
            p.unlock();
        }
        self.result = None;
        self.output = None;
        self.quota_exceedance = 0;
    }

    /// `Supervisor::OnUnPlugged` : efface les scores et les matchs de toutes les poules.
    pub(crate) fn unplug_supervisor(&mut self, players: &mut PlayerStore) {
        for p in &mut self.pools {
            p.clean_scores(players);
            p.delete_matches(players);
        }
        self.managed = false;
    }
}

#[cfg(test)]
mod swapping_tests {
    use super::*;

    #[test]
    fn several_extra_criteria_are_kept_in_order() {
        let extra = extra_swapping_criteria();
        let regular = regular_swapping_criteria();
        assert!(extra.len() >= 2 && !regular.is_empty());
        let mut round = PoolRound::new();
        round.swapping = Some(format!("/{}/{}/{}/", regular[0], extra[1], extra[0]));
        round.parse_swapping();
        assert_eq!(round.swapping_criteria(), [extra[1].to_string(), extra[0].to_string(), regular[0].to_string()]);
    }
}
