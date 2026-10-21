//! Une poule (port de `rounds/poule/pool.cpp`, sans la partie GooCanvas).
//!
//! La poule ne possède que des références de joueurs ; les résultats
//! calculés (victoires, indice, touches données…) sont écrits comme
//! attributs locaux des joueurs, comme en C++ :
//! - `rank_owner` : l'allocateur (numéro de poule, piste, heure, rang de départ) ;
//! - `current_owner` : le tour courant (`_current_round_owner` du superviseur) ;
//! - `combined_owner` : le superviseur (cumul avec le tour de poules précédent) ;
//! - `previous_combined` : le superviseur du tour de poules précédent, s'il y en a un.
//!
//! Les données propres à la poule (`Victories`, `HR`, `Rank`, attachées au
//! joueur par `SetData` en C++) sont gardées dans `results`.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use rand::{Rng, SeedableRng};

use crate::attribute;
use crate::bout::{Match, StartTime};
use crate::i18n::tr;
use crate::player::{random_key, AttrId, Owner, PlayerClass, PlayerRef, PlayerStore};
use crate::xml::{self, XmlWriter};

use super::dispatcher::Dispatcher;
use super::neo_swapper::criteria_image;

/// Politique de comparaison (`Pool::ComparisonPolicy`).
pub const WITH_POOL_NR: u32 = 0x1;
pub const WITH_CALCULUS: u32 = 0x2;
pub const WITH_RANDOM: u32 = 0x4;

// --- Petits accès aux attributs ---------------------------------------------

pub(crate) fn get_uint(players: &PlayerStore, p: PlayerRef, name: &str, owner: Owner) -> Option<u32> {
    players.get(p)?.get(&AttrId::local(name, owner)).map(|v| v.as_uint())
}

pub(crate) fn get_int(players: &PlayerStore, p: PlayerRef, name: &str, owner: Owner) -> Option<i32> {
    players.get(p)?.get(&AttrId::local(name, owner)).map(|v| v.as_int())
}

pub(crate) fn get_str(players: &PlayerStore, p: PlayerRef, name: &str, owner: Owner) -> Option<String> {
    players.get(p)?.get(&AttrId::local(name, owner)).map(|v| v.xml_image())
}

pub(crate) fn status_char(players: &PlayerStore, p: PlayerRef, owner: Owner) -> Option<char> {
    get_str(players, p, "status", owner).and_then(|s| s.chars().next())
}

pub(crate) fn set_uint(players: &mut PlayerStore, p: PlayerRef, name: &str, owner: Owner, v: u32) {
    if let Some(pl) = players.get_mut(p) {
        pl.set_uint(&AttrId::local(name, owner), v);
    }
}

pub(crate) fn set_str(players: &mut PlayerStore, p: PlayerRef, name: &str, owner: Owner, v: &str) {
    if let Some(pl) = players.get_mut(p) {
        pl.set_str(&AttrId::local(name, owner), v);
    }
}

pub(crate) fn remove_attr(players: &mut PlayerStore, p: PlayerRef, name: &str, owner: Owner) {
    if let Some(pl) = players.get_mut(p) {
        pl.remove(&AttrId::local(name, owner));
    }
}

/// Le tireur a été déclaré absent (`incident` = `A`).
pub fn fencer_is_absent(players: &PlayerStore, p: PlayerRef) -> bool {
    players.get(p).and_then(|pl| pl.str_attr("incident")).map(|s| s.starts_with('A')).unwrap_or(false)
}

/// Tri aléatoire reproductible (`Player::RandomCompare`).
pub fn random_compare(a: PlayerRef, b: PlayerRef, seed: u32) -> Ordering {
    random_key(a, seed).cmp(&random_key(b, seed)).then(a.cmp(&b))
}

/// `Pool::CompareStatus` : Q < N < A < E < F.
fn compare_status(a: char, b: char) -> Ordering {
    const ORDER: [char; 5] = ['Q', 'N', 'A', 'E', 'F'];
    let ia = ORDER.iter().position(|c| *c == a).unwrap_or(5);
    let ib = ORDER.iter().position(|c| *c == b).unwrap_or(5);
    if ia < ib {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

/// `Pool::ComparePlayer` : comparaison de deux tireurs selon les résultats
/// enregistrés pour `data_owner` (ratio de victoires, indice, touches
/// données), puis tirage au sort reproductible.
pub fn compare_players(players: &PlayerStore, a: PlayerRef, b: PlayerRef, data_owner: Option<Owner>, rand_seed: u32, policy: u32) -> Ordering {
    if policy & WITH_CALCULUS != 0 {
        if let Some(owner) = data_owner {
            let pool_a = get_uint(players, a, "pool_nr", owner).unwrap_or(0);
            let pool_b = get_uint(players, b, "pool_nr", owner).unwrap_or(0);
            let ratio_a = get_uint(players, a, "victories_ratio", owner).unwrap_or(0);
            let ratio_b = get_uint(players, b, "victories_ratio", owner).unwrap_or(0);
            let ind_a = get_int(players, a, "indice", owner).unwrap_or(0);
            let ind_b = get_int(players, b, "indice", owner).unwrap_or(0);
            let hs_a = get_uint(players, a, "HS", owner).unwrap_or(0);
            let hs_b = get_uint(players, b, "HS", owner).unwrap_or(0);
            let st_a = status_char(players, a, owner).unwrap_or('Q');
            let st_b = status_char(players, b, owner).unwrap_or('Q');

            if st_a != st_b {
                if st_a == 'E' {
                    return Ordering::Greater;
                }
                if st_b == 'E' {
                    return Ordering::Less;
                }
            }
            if policy & WITH_POOL_NR != 0 && pool_a != pool_b {
                return pool_a.cmp(&pool_b);
            }
            if st_a != st_b {
                return compare_status(st_a, st_b);
            }
            if ratio_a != ratio_b {
                return ratio_b.cmp(&ratio_a);
            }
            if ind_a != ind_b {
                return ind_b.cmp(&ind_a);
            }
            if hs_a != hs_b {
                return hs_b.cmp(&hs_a);
            }
        }
    }
    if policy & WITH_RANDOM != 0 {
        return random_compare(a, b, rand_seed);
    }
    Ordering::Equal
}

/// Traduit « Pool #%02d ».
pub fn pool_name(number: u32) -> String {
    tr("Pool #%02d").replace("%02d", &format!("{number:02}")).replace(' ', "\u{a0}")
}

/// État affiché d'une poule (`RefreshStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolStatus {
    /// Tous les matchs sont terminés (`GTK_STOCK_APPLY`).
    Over,
    /// Un match est incohérent (`GTK_STOCK_DIALOG_WARNING`).
    Error,
    /// Connexion d'un arbitre rompue (`Broken`) ou en attente (`Waiting`).
    RefereeConnection(&'static str),
    /// En cours (`GTK_STOCK_EXECUTE`).
    Ongoing,
}

/// Données propres à un tireur dans la poule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FencerResult {
    pub victories: u32,
    /// Touches reçues, en négatif (`HR`).
    pub hits_received: i32,
    /// Place dans la poule (0 = non calculée).
    pub rank: u32,
}

/// Un match de poule.
#[derive(Debug, Clone)]
pub struct PoolMatch {
    pub bout: Match,
    /// Le match suit immédiatement un autre match d'un des deux tireurs.
    pub rest_error: bool,
}

/// Affectation commune calculée pour un match, sans changer son identifiant FIE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchAssignment {
    pub match_index: usize,
    pub wave: u32,
    pub piste: u32,
    pub referee: Option<PlayerRef>,
}

impl PoolMatch {
    /// Nom affiché (`M1`, `M2`…).
    pub fn name(&self) -> String {
        format!("M{}", self.bout.number)
    }
}

#[derive(Debug, Clone)]
pub struct Pool {
    number: u32,
    name: String,
    pub(crate) max_score: u32,
    rand_seed: u32,
    pub net_id: u32,
    pub(crate) workload: u32,
    xml_player_tag: String,
    rank_owner: Owner,
    current_owner: Option<Owner>,
    combined_owner: Option<Owner>,
    previous_combined: Option<Owner>,
    fencer_list: Vec<PlayerRef>,
    sorted_fencer_list: Vec<PlayerRef>,
    referee_list: Vec<PlayerRef>,
    strip: u32,
    additional_strips: Vec<u32>,
    piste_referees: Vec<(u32, PlayerRef)>,
    start_time: Option<StartTime>,
    duration_sec: u32,
    matches: Vec<PoolMatch>,
    dispatcher: Dispatcher,
    is_over: bool,
    has_error: bool,
    locked: bool,
    nb_drop: i32,
    results: BTreeMap<PlayerRef, FencerResult>,
    strength: u32,
    strength_contributors: u32,
    /// Graine des scores aléatoires (`Stuff`), incrémentée à chaque usage.
    stuff_count: u64,
    /// Groupe (entraînement des jeunes).
    group: Option<u32>,
}

impl Pool {
    /// `Pool::Pool`. `rank_owner` est l'allocateur.
    pub fn new(max_score: u32, workload: u32, number: u32, xml_player_tag: &str, rand_seed: u32, rank_owner: Owner) -> Self {
        let name = pool_name(number);
        Pool {
            number,
            dispatcher: Dispatcher::new(&name),
            name,
            max_score,
            rand_seed,
            net_id: 0,
            workload,
            xml_player_tag: xml_player_tag.to_string(),
            rank_owner,
            current_owner: None,
            combined_owner: None,
            previous_combined: None,
            fencer_list: Vec::new(),
            sorted_fencer_list: Vec::new(),
            referee_list: Vec::new(),
            strip: 0,
            additional_strips: Vec::new(),
            piste_referees: Vec::new(),
            start_time: None,
            duration_sec: 0,
            matches: Vec::new(),
            is_over: false,
            has_error: false,
            locked: false,
            nb_drop: 0,
            results: BTreeMap::new(),
            strength: 0,
            strength_contributors: 0,
            stuff_count: 0,
            group: None,
        }
    }

    /// Entraînement des jeunes : rattache la poule au groupe `group`, dont
    /// elle est la poule `local` (« Groupe 2 · Poule n° 01 »).
    pub fn set_group(&mut self, group: u32, local: u32) {
        self.group = Some(group);
        self.name = format!("{} {} · {}", tr("Group"), group, pool_name(local));
    }

    pub fn group(&self) -> Option<u32> {
        self.group
    }

    // --- Accesseurs -----------------------------------------------------------

    pub fn number(&self) -> u32 {
        self.number
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn max_score(&self) -> u32 {
        self.max_score
    }

    pub fn set_max_score(&mut self, max_score: u32) {
        self.max_score = max_score;
        for m in &mut self.matches {
            m.bout.max_score = max_score;
        }
    }

    pub fn piste(&self) -> u32 {
        self.strip
    }

    pub fn start_time(&self) -> Option<StartTime> {
        self.start_time
    }

    pub fn duration_sec(&self) -> u32 {
        self.duration_sec
    }

    pub fn workload(&self) -> u32 {
        self.workload
    }

    /// Tous les tireurs, absents compris, par rang de départ (`GetFencerList`).
    pub fn fencers(&self) -> &[PlayerRef] {
        &self.fencer_list
    }

    /// Tireurs présents dans l'ordre de la poule (numéros 1, 2…)
    /// (`GetSortedFencerList`).
    pub fn sorted_fencers(&self) -> &[PlayerRef] {
        &self.sorted_fencer_list
    }

    /// Arbitres (le dernier ajouté en tête, comme `g_slist_prepend`).
    pub fn referees(&self) -> &[PlayerRef] {
        &self.referee_list
    }

    pub fn nb_players(&self) -> usize {
        self.sorted_fencer_list.len()
    }

    pub fn matches(&self) -> &[PoolMatch] {
        &self.matches
    }

    pub fn has_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    pub fn dispatcher(&self) -> &Dispatcher {
        &self.dispatcher
    }

    pub fn is_over(&self) -> bool {
        self.is_over
    }

    pub fn has_error(&self) -> bool {
        self.has_error
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn strength(&self) -> u32 {
        self.strength
    }

    pub fn set_strength_contributors(&mut self, count: u32) {
        self.strength_contributors = count;
    }

    /// Résultats d'un tireur dans la poule.
    pub fn result(&self, player: PlayerRef) -> FencerResult {
        self.results.get(&player).copied().unwrap_or_default()
    }

    pub fn current_owner(&self) -> Option<Owner> {
        self.current_owner
    }

    pub fn combined_owner(&self) -> Option<Owner> {
        self.combined_owner
    }

    pub fn rank_owner(&self) -> Owner {
        self.rank_owner
    }


    /// Position (à partir de 1) d'un tireur dans la poule.
    pub fn position_of(&self, player: PlayerRef) -> Option<usize> {
        self.sorted_fencer_list.iter().position(|p| *p == player).map(|i| i + 1)
    }

    // --- Composition ----------------------------------------------------------

    /// `SetDataOwner` : rattache la poule au tour courant et au cumul.
    pub fn set_data_owner(&mut self, players: &mut PlayerStore, current: Owner, combined: Owner, previous_combined: Option<Owner>) {
        self.current_owner = Some(current);
        self.combined_owner = Some(combined);
        self.previous_combined = previous_combined;
        self.nb_drop = 0;
        for p in self.fencer_list.clone() {
            set_uint(players, p, "pool_nr", current, self.number);
            set_uint(players, p, "pool_nr", combined, self.number);
        }
    }

    fn compare_start_rank(&self, players: &PlayerStore, a: PlayerRef, b: PlayerRef) -> Ordering {
        let id = AttrId::local("stage_start_rank", self.rank_owner);
        let (va, vb) = (players.get(a).and_then(|p| p.get(&id)), players.get(b).and_then(|p| p.get(&id)));
        let o = attribute::compare_values(attribute::desc("stage_start_rank"), va, vb);
        if o != Ordering::Equal {
            return o;
        }
        random_compare(a, b, self.rand_seed)
    }

    /// `AddFencer` : `position` est donné lors du chargement d'un fichier
    /// (les tireurs arrivent alors dans l'ordre de la poule).
    pub fn add_fencer(&mut self, players: &mut PlayerStore, player: PlayerRef, position: Option<usize>) {
        if self.fencer_list.contains(&player) {
            return;
        }
        set_uint(players, player, "pool_nr", self.rank_owner, self.number);
        self.set_roadmap_to(players, player, self.strip, self.start_time);

        let pos = self
            .fencer_list
            .iter()
            .position(|f| self.compare_start_rank(players, player, *f) != Ordering::Greater)
            .unwrap_or(self.fencer_list.len());
        self.fencer_list.insert(pos, player);

        if !fencer_is_absent(players, player) {
            if position.is_some() {
                self.sorted_fencer_list.push(player);
            } else {
                self.sort_players(players);
            }
        }
        self.refresh_strength(players);
    }

    /// `RemoveFencer`.
    pub fn remove_fencer(&mut self, players: &mut PlayerStore, player: PlayerRef) {
        if let Some(i) = self.fencer_list.iter().position(|p| *p == player) {
            self.set_roadmap_to(players, player, 0, None);
            self.fencer_list.remove(i);
            self.sort_players(players);
            self.refresh_strength(players);
        }
    }

    /// `DismissFencer` : le tireur reste dans la poule mais ne tire pas (absent).
    pub fn dismiss_fencer(&mut self, players: &mut PlayerStore, player: PlayerRef) {
        if let Some(i) = self.sorted_fencer_list.iter().position(|p| *p == player) {
            self.set_roadmap_to(players, player, 0, None);
            self.sorted_fencer_list.remove(i);
            self.refresh_strength(players);
        }
    }

    /// `AllowFencer` : réintègre un tireur écarté par `dismiss_fencer`.
    pub fn allow_fencer(&mut self, players: &mut PlayerStore, player: PlayerRef) {
        if !self.sorted_fencer_list.contains(&player) {
            let seed = self.rand_seed;
            let pos = self
                .sorted_fencer_list
                .iter()
                .position(|f| random_compare(player, *f, seed) != Ordering::Greater)
                .unwrap_or(self.sorted_fencer_list.len());
            self.sorted_fencer_list.insert(pos, player);
            self.set_roadmap_to(players, player, self.strip, self.start_time);
            self.refresh_strength(players);
        }
    }

    /// `SortPlayers` : l'ordre dans la poule est tiré au sort (graine de la phase).
    fn sort_players(&mut self, players: &PlayerStore) {
        let seed = self.rand_seed;
        let mut sorted: Vec<PlayerRef> = self.fencer_list.iter().copied().filter(|f| !fencer_is_absent(players, *f)).collect();
        crate::sort::glib_sort_by(&mut sorted, |a, b| random_compare(*a, *b, seed));
        self.sorted_fencer_list = sorted;
    }

    /// `RefreshStrength` (somme des rangs de départ des meilleurs tireurs ;
    /// n'est affichée qu'en mode mise au point en C++).
    fn refresh_strength(&mut self, players: &PlayerStore) {
        self.strength = self
            .fencer_list
            .iter()
            .take(self.strength_contributors as usize)
            .filter_map(|p| get_uint(players, *p, "stage_start_rank", self.rank_owner))
            .sum();
    }

    /// `AddReferee` (ajout en tête de liste).
    pub fn add_referee(&mut self, referee: PlayerRef) {
        if !self.referee_list.contains(&referee) {
            self.referee_list.insert(0, referee);
        }
    }

    /// Ajoute un arbitre lu dans un fichier : l'ordre du fichier est conservé
    /// (l'arbitre principal reste le premier ; `add_referee` inverserait la
    /// liste à chaque enregistrement).
    pub(crate) fn append_referee(&mut self, referee: PlayerRef) {
        if !self.referee_list.contains(&referee) {
            self.referee_list.push(referee);
        }
    }

    pub fn remove_referee(&mut self, referee: PlayerRef) {
        self.referee_list.retain(|r| *r != referee);
        self.piste_referees.retain(|(_, r)| *r != referee);
    }

    pub fn remove_all_referees(&mut self) {
        self.referee_list.clear();
        self.piste_referees.clear();
    }

    // --- Piste et horaire -----------------------------------------------------

    fn set_roadmap_to(&self, players: &mut PlayerStore, fencer: PlayerRef, strip: u32, start: Option<StartTime>) {
        set_uint(players, fencer, "strip", self.rank_owner, strip);
        let image = start.map(|t| t.0.format("%k:%M").to_string()).unwrap_or_default();
        set_str(players, fencer, "time", self.rank_owner, &image);
    }

    /// `SetRoadmap` : piste et heure de début (message `BellePoule2D::Roadmap`).
    pub fn set_roadmap(&mut self, players: &mut PlayerStore, strip: u32, start_time: Option<StartTime>) {
        self.strip = strip;
        self.additional_strips.clear();
        self.piste_referees.clear();
        self.start_time = start_time;
        for f in self.sorted_fencer_list.clone() {
            self.set_roadmap_to(players, f, strip, start_time);
        }
    }

    /// Liste commune des pistes d'une poule ; `strip()` reste la première.
    pub fn strips(&self) -> Vec<u32> {
        (self.strip != 0).then_some(self.strip).into_iter().chain(self.additional_strips.iter().copied()).collect()
    }

    pub fn set_roadmap_pistes(&mut self, players: &mut PlayerStore, pistes: &[u32], start: Option<StartTime>) {
        let previous = self.piste_referees.clone();
        let mut list = Vec::new();
        for &piste in pistes { if piste != 0 && !list.contains(&piste) { list.push(piste); } }
        self.set_roadmap(players, list.first().copied().unwrap_or(0), start);
        self.additional_strips = list.into_iter().skip(1).collect();
        let strips = self.strips();
        self.piste_referees = if strips.len() > 1 && self.referee_list.len() != strips.len() {
            previous.into_iter().filter(|(p, r)| strips.contains(p) && self.referee_list.contains(r)).collect()
        } else { strips.into_iter().zip(self.referee_list.iter().copied()).collect() };
    }

    pub fn piste_referees(&self) -> &[(u32, PlayerRef)] { &self.piste_referees }

    /// Vagues de matchs sans tireur engagé deux fois simultanément. Les pistes
    /// les moins chargées sont prioritaires ; l'ordre FIE départage les matchs.
    pub fn match_assignments(&self) -> Vec<MatchAssignment> {
        let pistes = self.strips();
        if pistes.is_empty() { return Vec::new(); }
        let mut counts = vec![0usize; pistes.len()];
        let mut pending: Vec<_> = (0..self.matches.len()).collect();
        let mut result = Vec::new();
        let mut wave = 0;
        while !pending.is_empty() {
            wave += 1;
            let mut used = Vec::new();
            let mut order: Vec<_> = (0..pistes.len()).collect();
            order.sort_by_key(|i| (counts[*i], *i));
            for i in order {
                let next = pending.iter().position(|index| {
                    (0..2).filter_map(|k| self.matches[*index].bout.opponent(k)).all(|p| !used.contains(&p))
                });
                let Some(next) = next else { break };
                let match_index = pending.remove(next);
                used.extend((0..2).filter_map(|k| self.matches[match_index].bout.opponent(k)));
                let piste = pistes[i];
                let referee = self.piste_referees.iter().find(|(p, _)| *p == piste).map(|(_, r)| *r)
                    .or_else(|| if pistes.len() == 1 { self.referee_list.first().copied() } else { None });
                result.push(MatchAssignment { match_index, wave, piste, referee });
                counts[i] += 1;
            }
        }
        result
    }

    /// Image de l'heure de début (`FieTime::GetImage`, `%k:%M`).
    pub fn start_time_image(&self) -> Option<String> {
        self.start_time.map(|t| t.0.format("%k:%M").to_string())
    }

    /// Libellé de piste de la feuille de poule (`"%02d  @  hh:mm"`).
    pub fn piste_label(&self) -> Option<String> {
        if self.strip == 0 {
            return None;
        }
        let time = self.start_time_image().unwrap_or_default();
        let pistes = self.strips().iter().map(|p| format!("{p:02}")).collect::<Vec<_>>().join(", ");
        Some(format!("{}\u{a0}\u{a0}@\u{a0}\u{a0}{}", pistes, time))
    }

    /// `Timestamp` : durée écoulée depuis l'heure de début.
    pub fn timestamp(&mut self) {
        if let Some(t) = self.start_time {
            let now = crate::bout::now_rounded();
            let span = (now - t.0).num_seconds();
            self.duration_sec = if span < 0 { 1 } else { span as u32 };
        }
    }

    // --- Matchs ---------------------------------------------------------------

    /// `CreateMatchs` : génère les matchs dans l'ordre du répartiteur. Le
    /// premier critère de séparation sert d'affinité (coéquipiers).
    pub fn create_matches(&mut self, players: &PlayerStore, affinity_criteria: Option<&str>) {
        if !self.matches.is_empty() {
            return;
        }
        let affinity: Vec<Option<String>> = self
            .sorted_fencer_list
            .iter()
            .map(|p| affinity_criteria.and_then(|c| criteria_image(players, *p, c)))
            .collect();
        self.dispatcher.set_affinity_criteria(&affinity);

        let n = self.nb_players();
        let nb_matchs = (n * n).saturating_sub(n) / 2;
        for i in 0..nb_matchs {
            if let Some((a, b, rest_error)) = self.dispatcher.player_pair(i) {
                let pa = self.sorted_fencer_list[a - 1];
                let pb = self.sorted_fencer_list[b - 1];
                let mut bout = Match::with_opponents(pa, pb, self.max_score);
                bout.number = i as u32 + 1;
                bout.name = format!("M{}", i + 1);
                self.matches.push(PoolMatch { bout, rest_error });
            }
        }
    }

    /// `DeleteMatchs`.
    pub fn delete_matches(&mut self, players: &mut PlayerStore) {
        self.duration_sec = 0;
        self.clean_scores(players);
        self.matches.clear();
        if let Some(owner) = self.current_owner {
            for f in self.fencer_list.clone() {
                remove_attr(players, f, "status", owner);
            }
        }
    }

    /// Index du match entre deux tireurs (`GetMatch (A, B)`).
    pub fn match_index(&self, a: PlayerRef, b: PlayerRef) -> Option<usize> {
        self.matches.iter().position(|m| m.bout.has_fencer(a) && m.bout.has_fencer(b))
    }

    pub fn match_between(&self, a: PlayerRef, b: PlayerRef) -> Option<&PoolMatch> {
        self.match_index(a, b).map(|i| &self.matches[i])
    }

    /// Match par numéro d'ordre (à partir de 0) du répartiteur (`GetMatch (i)`).
    pub fn match_by_order(&self, i: usize) -> Option<usize> {
        let (a, b, _) = self.dispatcher.player_pair(i)?;
        let pa = *self.sorted_fencer_list.get(a - 1)?;
        let pb = *self.sorted_fencer_list.get(b - 1)?;
        self.match_index(pa, pb)
    }

    /// Saisie d'un score (`ScoreCollector`) : `value` vaut `V`, `W3`, `4` ou
    /// vide. Renvoie vrai si la saisie est complète. Les résultats sont
    /// recalculés.
    pub fn set_score_str(&mut self, players: &mut PlayerStore, match_index: usize, fencer: PlayerRef, value: &str) -> bool {
        if self.locked {
            return false;
        }
        let Some(m) = self.matches.get_mut(match_index) else { return false };
        let complete = m.bout.set_score_str(fencer, value);
        self.on_new_score(players);
        complete
    }

    /// Saisie numérique d'un score.
    pub fn set_score(&mut self, players: &mut PlayerStore, match_index: usize, fencer: PlayerRef, score: u32, is_the_best: bool) {
        if self.locked {
            return;
        }
        if let Some(m) = self.matches.get_mut(match_index) {
            m.bout.set_score(fencer, score, is_the_best);
        }
        self.on_new_score(players);
    }

    /// Saisie d'un score par la cellule de la grille : ligne `a`, colonne
    /// `b` (tireurs), score de `a` contre `b`.
    pub fn set_cell(&mut self, players: &mut PlayerStore, a: PlayerRef, b: PlayerRef, value: &str) -> bool {
        match self.match_index(a, b) {
            Some(i) => self.set_score_str(players, i, a, value),
            None => false,
        }
    }

    /// `OnNewScore`.
    fn on_new_score(&mut self, players: &mut PlayerStore) {
        self.refresh_score_data(players);
        if self.is_over {
            self.timestamp();
        }
    }

    /// Message `SmartPoule::Score` : charge les scores d'un match depuis
    /// `<Match><Tireur …/><Tireur …/></Match>`. `bout` commence à 1.
    pub fn load_match_xml(&mut self, players: &mut PlayerStore, bout: usize, xml_data: &str) -> bool {
        if bout == 0 {
            return false;
        }
        let Some(i) = self.match_by_order(bout - 1) else { return false };
        let Ok(doc) = xml::parse(xml_data) else { return false };
        let root = doc.root_element();
        if root.tag_name().name() != "Match" {
            return false;
        }
        let nodes: Vec<_> = root.children().filter(|n| n.is_element()).collect();
        if nodes.len() == 2 {
            let m = &mut self.matches[i].bout;
            for (k, node) in nodes.iter().enumerate() {
                if let Some(f) = m.opponent(k) {
                    m.load_fencer(*node, f);
                }
            }
            m.after_load();
            self.refresh_score_data(players);
        }
        true
    }

    /// `Stuff` : scores aléatoires (ScoreAleatoire), reproductibles.
    pub fn stuff(&mut self, players: &mut PlayerStore) {
        let seed = (self.rand_seed as u64) << 32 | (self.number as u64) << 16 | (self.stuff_count & 0xFFFF);
        self.stuff_count += 1;
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        let max = self.max_score;
        for m in &mut self.matches {
            let (Some(a), Some(b)) = (m.bout.opponent(0), m.bout.opponent(1)) else { continue };
            let score = if max > 0 { rng.gen_range(0..max) } else { 0 };
            if rng.gen_bool(0.5) {
                m.bout.set_score(a, max, true);
                m.bout.set_score(b, score, false);
            } else {
                m.bout.set_score(a, score, false);
                m.bout.set_score(b, max, true);
            }
        }
        self.refresh_score_data(players);
        self.timestamp();
    }

    /// `CleanScores`.
    pub fn clean_scores(&mut self, players: &mut PlayerStore) {
        for m in &mut self.matches {
            m.bout.clean_score();
        }
        if !self.matches.is_empty() {
            self.refresh_score_data(players);
        }
        self.is_over = false;
    }

    pub fn lock(&mut self, players: &mut PlayerStore) {
        self.locked = true;
        self.refresh_score_data(players);
    }

    pub fn unlock(&mut self) {
        self.locked = false;
    }

    // --- Abandons, exclusions, forfaits ----------------------------------------

    /// `CopyPlayersStatus` : reprend les statuts enregistrés pour `from`
    /// (l'allocateur) et applique abandons/exclusions/forfaits.
    pub fn copy_players_status(&mut self, players: &mut PlayerStore, from: Owner) {
        for p in self.sorted_fencer_list.clone() {
            let status = get_str(players, p, "status", from).unwrap_or_else(|| "Q".into());
            self.restore_player(players, p);
            if matches!(status.chars().next(), Some('A' | 'F' | 'E')) {
                self.drop_player(players, p, &status);
            }
        }
        self.refresh_score_data(players);
    }

    fn refresh_attr_str(&self, players: &mut PlayerStore, p: PlayerRef, name: &str, value: &str) {
        if let Some(o) = self.current_owner {
            set_str(players, p, name, o, value);
        }
        if let Some(o) = self.combined_owner {
            set_str(players, p, name, o, value);
        }
    }

    /// `DropPlayer` : abandon (`A`), exclusion (`E`) ou forfait (`F`).
    pub fn drop_player(&mut self, players: &mut PlayerStore, player: PlayerRef, reason: &str) {
        if let Some(o) = self.current_owner {
            if status_char(players, player, o) == Some('Q') {
                self.nb_drop += 1;
            }
        }
        self.refresh_attr_str(players, player, "status", reason);
        let r = reason.chars().next().unwrap_or('A');
        for opponent in self.sorted_fencer_list.clone() {
            if opponent != player {
                if let Some(i) = self.match_index(opponent, player) {
                    self.matches[i].bout.drop_fencer(player, r);
                }
            }
        }
    }

    /// `RestorePlayer`.
    pub fn restore_player(&mut self, players: &mut PlayerStore, player: PlayerRef) {
        let dropped = self
            .current_owner
            .and_then(|o| status_char(players, player, o))
            .map(|c| c != 'Q' && c != 'N')
            .unwrap_or(false);
        self.refresh_attr_str(players, player, "status", "Q");
        if dropped {
            self.nb_drop -= 1;
            for opponent in self.sorted_fencer_list.clone() {
                if opponent != player {
                    if let Some(i) = self.match_index(opponent, player) {
                        self.matches[i].bout.restore_fencer(player);
                    }
                }
            }
        }
    }

    /// `OnStatusChanged` : choix du statut d'un tireur dans la feuille
    /// (`Q`, `A`, `E`, `F`). Les résultats sont recalculés.
    pub fn set_status(&mut self, players: &mut PlayerStore, player: PlayerRef, code: &str) {
        if self.locked {
            return;
        }
        if !code.is_empty() && !code.starts_with('Q') {
            self.drop_player(players, player, code);
        } else {
            self.restore_player(players, player);
        }
        self.refresh_score_data(players);
    }

    /// Statut courant d'un tireur dans la poule.
    pub fn status_of(&self, players: &PlayerStore, player: PlayerRef) -> char {
        self.current_owner.and_then(|o| status_char(players, player, o)).unwrap_or('Q')
    }

    /// `GetNbMatchs (of)` : nombre de matchs disputés par un tireur.
    pub fn nb_matches_of(&self, players: &PlayerStore, of: PlayerRef) -> u32 {
        let mut nb = self.nb_players() as i64 - 1;
        if let Some(owner) = self.current_owner {
            for p in &self.fencer_list {
                if let Some(c) = status_char(players, *p, owner) {
                    if matches!(c, 'A' | 'F' | 'E') || fencer_is_absent(players, *p) {
                        nb -= 1;
                        if *p == of {
                            return 0;
                        }
                    }
                }
            }
        }
        nb.max(0) as u32
    }

    /// `GetNbMatchs ()`.
    pub fn nb_matches(&self) -> usize {
        let n = self.nb_players();
        (n * n).saturating_sub(n) / 2
    }

    // --- Calculs ---------------------------------------------------------------

    fn refresh_attr(&self, players: &mut PlayerStore, p: PlayerRef, name: &str, value: u32, op: CombinedOperation, combined_value: u32) {
        if let Some(o) = self.current_owner {
            set_uint(players, p, name, o, value);
        }
        let Some(combined) = self.combined_owner else { return };
        match self.previous_combined {
            None => set_uint(players, p, name, combined, value),
            Some(prev) => {
                if let Some(source) = get_uint(players, p, name, prev) {
                    let v = match op {
                        CombinedOperation::Sum => source.wrapping_add(value),
                        CombinedOperation::None => combined_value,
                    };
                    set_uint(players, p, name, combined, v);
                }
            }
        }
    }

    /// `RefreshScoreData` : victoires, V/M, TD, TR, indice et place dans la
    /// poule pour chaque tireur.
    pub fn refresh_score_data(&mut self, players: &mut PlayerStore) {
        let n = self.nb_players();
        self.is_over = true;
        self.has_error = false;

        for ia in 0..n {
            let a = self.sorted_fencer_list[ia];
            let mut victories = 0u32;
            let mut hits_scored = 0u32;
            let mut hits_received = 0i32;
            for ib in 0..n {
                if ia == ib {
                    continue;
                }
                let b = self.sorted_fencer_list[ib];
                if let Some(m) = self.match_between(a, b) {
                    let sa = *m.bout.score(a).unwrap();
                    let sb = *m.bout.score(b).unwrap();
                    if sa.is_known() {
                        hits_scored += sa.get();
                    }
                    if sb.is_known() {
                        hits_received -= sb.get() as i32;
                    }
                    if sa.is_known() && sb.is_known() && sa.is_the_best() {
                        victories += 1;
                    }
                    let (match_over, match_error) = (m.bout.is_over(), m.bout.has_error());
                    if !match_over {
                        self.is_over = false;
                    }
                    if match_error {
                        self.has_error = true;
                    }
                }
            }
            let entry = self.results.entry(a).or_default();
            entry.victories = victories;
            entry.hits_received = hits_received;

            let bouts = (n as i64 - self.nb_drop as i64 - 1) as u32;
            self.refresh_attr(players, a, "victories_count", victories, CombinedOperation::Sum, 0);
            self.refresh_attr(players, a, "bouts_count", bouts, CombinedOperation::Sum, 0);
            self.refresh_attr(players, a, "indice", (hits_scored as i32 + hits_received) as u32, CombinedOperation::Sum, 0);
            self.refresh_attr(players, a, "HS", hits_scored, CombinedOperation::Sum, 0);

            let active = n as i64 - self.nb_drop as i64;
            let current_ratio = if active > 1 { (victories as u64 * 1000 / (active - 1) as u64) as u32 } else { 0 };
            let mut combined_ratio = current_ratio;
            if let Some(prev) = self.previous_combined {
                let pv = get_uint(players, a, "victories_count", prev);
                let pb = get_uint(players, a, "bouts_count", prev);
                if let (Some(pv), Some(pb)) = (pv, pb) {
                    let total_v = pv + victories;
                    let total_b = pb.wrapping_add(bouts);
                    if total_b != 0 {
                        combined_ratio = (total_v as u64 * 1000 / total_b as u64) as u32;
                    }
                }
            }
            self.refresh_attr(players, a, "victories_ratio", current_ratio, CombinedOperation::None, combined_ratio);
        }

        // Place dans la poule
        let owner = self.current_owner;
        let seed = self.rand_seed;
        let mut ranking = self.sorted_fencer_list.clone();
        crate::sort::glib_sort_by(&mut ranking, |a, b| compare_players(players, *a, *b, owner, seed, WITH_CALCULUS | WITH_RANDOM));
        let mut previous: Option<PlayerRef> = None;
        let mut previous_rank = 0;
        for (i, p) in ranking.iter().enumerate() {
            let same = match (previous, owner) {
                (Some(prev), Some(o)) => ["victories_ratio", "indice", "HS"].iter().all(|name| {
                    let id = AttrId::local(name, o);
                    let va = players.get(prev).and_then(|x| x.get(&id));
                    let vb = players.get(*p).and_then(|x| x.get(&id));
                    attribute::compare_values(attribute::desc(name), va, vb) == Ordering::Equal
                }),
                _ => false,
            };
            let rank = if same {
                previous_rank
            } else {
                previous_rank = i as u32 + 1;
                previous_rank
            };
            self.results.entry(*p).or_default().rank = rank;
            previous = Some(*p);
        }
    }

    /// `RefreshStatus` : état affiché, selon les matchs et la connexion des
    /// arbitres.
    pub fn status(&self, referees: &PlayerStore) -> PoolStatus {
        if self.is_over {
            return PoolStatus::Over;
        }
        if self.has_error {
            return PoolStatus::Error;
        }
        for r in &self.referee_list {
            match referees.get(*r).and_then(|p| p.str_attr("connection")) {
                Some("Broken") => return PoolStatus::RefereeConnection("Broken"),
                Some("Waiting") => return PoolStatus::RefereeConnection("Waiting"),
                _ => {}
            }
        }
        PoolStatus::Ongoing
    }

    // --- XML -------------------------------------------------------------------

    /// `Pool::Save` : `<Poule>` avec ses tireurs, arbitres et matchs.
    pub fn save(&self, players: &PlayerStore, w: &mut XmlWriter) {
        w.start("Poule");
        w.attr_fmt("ID", self.number);
        w.attr("NetID", &format!("{:x}", self.net_id));
        if self.strip != 0 {
            w.attr_fmt("Piste", self.strip);
        }
        if !self.additional_strips.is_empty() {
            w.attr("Pistes", &self.strips().iter().map(u32::to_string).collect::<Vec<_>>().join(","));
        }
        if !self.piste_referees.is_empty() {
            w.attr("ArbitresPistes", &self.piste_referees.iter().map(|(p, r)| format!("{p}:{r}")).collect::<Vec<_>>().join(","));
        }
        if let Some(t) = &self.start_time {
            w.attr("Date", &t.xml_date());
            w.attr("Heure", &t.xml_time());
        }
        if self.duration_sec > 0 {
            w.attr_fmt("Duree", self.duration_sec);
        }

        let mut fencers = self.sorted_fencer_list.clone();
        for f in &self.fencer_list {
            if !self.sorted_fencer_list.contains(f) {
                fencers.push(*f);
            }
        }
        for (i, f) in fencers.iter().enumerate() {
            let tag = players.get(*f).map(|p| p.class.xml_tag()).unwrap_or("Tireur");
            let result = self.result(*f);
            w.start(tag);
            w.attr_fmt("REF", *f);
            w.attr_fmt("NoDansLaPoule", i + 1);
            w.attr_fmt("NbVictoires", result.victories);
            w.attr_fmt("NbMatches", self.nb_matches_of(players, *f));
            if let Some(o) = self.current_owner {
                if let Some(hs) = get_uint(players, *f, "HS", o) {
                    let indice = get_int(players, *f, "indice", o).unwrap_or(0);
                    w.attr_fmt("TD", hs);
                    w.attr_fmt("TR", hs as i32 - indice);
                }
            }
            w.attr_fmt("RangPoule", result.rank);
            w.end();
        }

        for r in &self.referee_list {
            w.start(PlayerClass::Referee.xml_tag());
            w.attr_fmt("REF", *r);
            w.end();
        }

        for m in &self.matches {
            m.bout.save(players, w);
        }
        w.end();
    }

    /// Image XML autonome de la poule (`FeedParcel`).
    pub fn to_xml_string(&self, players: &PlayerStore) -> String {
        let mut w = XmlWriter::new();
        self.save(players, &mut w);
        w.finish()
    }

    /// `Pool::Load (xml_node)` : attributs de l'élément `<Poule>`.
    pub fn load_header(&mut self, node: roxmltree::Node) {
        if let Some(v) = node.attribute("NetID") {
            self.net_id = u32::from_str_radix(v, 16).unwrap_or(0);
        }
        if node.attribute("Piste").is_some() {
            self.strip = xml::attr_u32(node, "Piste", 0);
        }
        self.additional_strips.clear();
        self.piste_referees = node.attribute("ArbitresPistes").unwrap_or_default().split(',').filter_map(|pair| {
            let (p, r) = pair.split_once(':')?; Some((p.parse().ok()?, r.parse().ok()?))
        }).collect();
        if let Some(pistes) = node.attribute("Pistes") {
            for piste in pistes.split(',').filter_map(|p| p.trim().parse::<u32>().ok()) {
                if piste > 0 && piste != self.strip && !self.additional_strips.contains(&piste) {
                    if self.strip == 0 { self.strip = piste; } else { self.additional_strips.push(piste); }
                }
            }
        }
        if let (Some(d), Some(t)) = (node.attribute("Date"), node.attribute("Heure")) {
            self.start_time = StartTime::parse(d, t);
        }
        if node.attribute("Duree").is_some() {
            self.duration_sec = xml::attr_u32(node, "Duree", 0);
        }
    }

    /// `Pool::Load (xml_node, player_list)` : scores des `<Match>` (les
    /// matchs doivent avoir été créés). L'ordre des tireurs dans le fichier
    /// peut différer : on retrouve leur position initiale.
    pub fn load_matches(&mut self, poule: roxmltree::Node) {
        self.is_over = true;
        let tag = self.xml_player_tag.clone();
        for m in xml::children(poule, "Match") {
            let fencers: Vec<_> = m.children().filter(|n| n.is_element() && n.tag_name().name() == tag).collect();
            if fencers.len() < 2 {
                continue;
            }
            let (mut na, mut nb) = (fencers[0], fencers[1]);
            let ra = xml::attr_u32(na, "REF", 0);
            let rb = xml::attr_u32(nb, "REF", 0);
            let Some(i) = self.match_index(ra, rb) else { continue };
            let mut pa = ra;
            let mut pb = rb;
            if self.matches[i].bout.opponent(0) != Some(pa) {
                std::mem::swap(&mut pa, &mut pb);
                std::mem::swap(&mut na, &mut nb);
            }
            let bout = &mut self.matches[i].bout;
            bout.load_fencer(na, pa);
            bout.load_fencer(nb, pb);
            bout.after_load();
            if !bout.is_over() {
                self.is_over = false;
            }
            if bout.has_error() {
                self.has_error = true;
            }
        }
    }

    // --- Données imprimables ---------------------------------------------------

    /// Feuille de poule (`Draw`). `for_referees` : feuille vierge pour les
    /// arbitres (numéros de matchs dans les cases, colonne signature).
    pub fn sheet(&self, players: &PlayerStore, referees: &PlayerStore, for_referees: bool) -> PoolSheet {
        let names: Vec<_> = self.referee_list.iter().filter_map(|r| referees.get(*r)).map(|p| {
            if self.referee_list.len() == 1 { (p.name(), p.str_attr("first_name").map(str::to_string)) }
            else { (format!("{} {}", p.name(), p.first_name()).trim().to_string(), None) }
        }).collect();
        let referee = if names.len() <= 1 { names.into_iter().next() }
            else { Some((names.into_iter().map(|(n, _)| n).collect::<Vec<_>>().join(", "), None)) };
        let n = self.nb_players();
        let mut rows = Vec::with_capacity(n);
        for (i, a) in self.sorted_fencer_list.iter().enumerate() {
            let mut cells = Vec::with_capacity(n);
            for (j, b) in self.sorted_fencer_list.iter().enumerate() {
                if i == j {
                    cells.push(SheetCell { text: String::new(), grey: true, match_index: None });
                    continue;
                }
                match self.match_index(*a, *b) {
                    Some(mi) => {
                        let m = &self.matches[mi];
                        let text = if for_referees { m.name() } else { m.bout.score(*a).map(|s| s.image(self.max_score)).unwrap_or_default() };
                        cells.push(SheetCell { text, grey: m.bout.is_dropped(), match_index: Some(mi) });
                    }
                    None => cells.push(SheetCell { text: String::new(), grey: true, match_index: None }),
                }
            }
            let result = self.result(*a);
            let dashboard = self.current_owner.map(|o| Dashboard {
                status: self.status_of(players, *a),
                victories: result.victories.to_string(),
                victories_ratio: format!("{:.3}", get_uint(players, *a, "victories_ratio", o).unwrap_or(0) as f64 / 1000.0),
                hits_scored: get_uint(players, *a, "HS", o).unwrap_or(0).to_string(),
                hits_received: (-result.hits_received).to_string(),
                indice: format!("{:+}", get_int(players, *a, "indice", o).unwrap_or(0)),
                rank: result.rank.to_string(),
            });
            rows.push(SheetRow { position: i + 1, player: *a, identity: player_identity(players, *a), cells, dashboard });
        }
        let assignments = self.match_assignments();
        let mut matches: Vec<_> = self
            .matches
            .iter()
            .enumerate()
            .map(|(match_index, m)| {
                let opp = |k: usize| {
                    m.bout.opponent(k).map(|p| MatchSheetFencer {
                        player: p,
                        position: self.position_of(p).unwrap_or(0),
                        identity: player_identity(players, p),
                        score: m.bout.score_at(k).image(self.max_score),
                    })
                };
                let assignment = assignments.iter().find(|a| a.match_index == match_index);
                let resource = if self.strips().len() > 1 {
                    assignment.map(|a| {
                        let referee = a.referee.and_then(|r| referees.get(r)).map(|p| format!("{} {}", p.name(), p.first_name()))
                            .unwrap_or_else(|| tr("Not assigned"));
                        format!("{} · {}\n{} {}", tr("Wave %d").replace("%d", &a.wave.to_string()),
                            tr("Piste %d").replace("%d", &a.piste.to_string()), tr("Referee"), referee)
                    })
                } else { None };
                MatchSheet { match_index, resource, assignment: assignment.cloned(), name: m.name(), rest_error: m.rest_error, opponents: [opp(0), opp(1)], score_boxes: if self.max_score <= 15 { self.max_score } else { 0 } }
            })
            .collect();
        if self.strips().len() > 1 {
            matches.sort_by_key(|m| m.assignment.as_ref().map(|a| (a.wave, a.piste)));
        }
        PoolSheet {
            title: self.name.clone(),
            referee,
            piste: self.piste_label(),
            for_referees,
            rows,
            matches,
            headers: if for_referees {
                vec![tr("Signature")]
            } else {
                ["Status", "Victories", "Vict./Matchs", "H. scored", "H. received", "Index", "Place"].iter().map(|s| tr(s)).collect()
            },
        }
    }

    /// `DumpToHTML` : lignes `<tr>` de la poule pour l'export HTML.
    pub fn dump_to_html(&self, players: &PlayerStore, grid_size: usize) -> String {
        let mut s = String::new();
        let n = self.nb_players();
        s.push_str("          <tr class=\"PoolName\">\n");
        s.push_str(&format!("            <td>{}</td>\n", self.name));
        for _ in 0..4 {
            s.push_str("            <td class=\"GridSeparator\"></td>\n");
        }
        for i in 0..n {
            s.push_str(&format!("            <td class=\"GridSeparator\" style=\"text-align: center\";>{}</td>\n", i + 1));
        }
        s.push_str("            <td class=\"GridSeparator\"></td>\n");
        if n < grid_size {
            s.push_str("            <td class=\"GridSeparator\"></td>\n");
        }
        for h in ["#", "V", "+/-", "+"] {
            s.push_str(&format!("            <td class=\"GridSeparator\" style=\"text-align: center\";>{h}</td>\n"));
        }
        s.push_str("          </tr>\n");

        for (i, a) in self.sorted_fencer_list.iter().enumerate() {
            s.push_str("          <tr class=\"EvenRow\">\n");
            for (code, look) in [("name", attribute::Look::LongText), ("first_name", attribute::Look::LongText), ("country", attribute::Look::ShortText)] {
                if let Some(v) = players.get(*a).and_then(|p| p.attr(code)) {
                    let image = attribute::desc(code).user_image(&v.xml_image(), look);
                    s.push_str("            <td>\n");
                    s.push_str(&format!("              <span class=\"{code}\">{image} </span>"));
                    s.push_str("            </td>\n");
                }
            }
            s.push_str("            <td class=\"GridSeparator\"></td>\n");
            s.push_str(&format!("            <td class=\"GridSeparator\">{}</td>\n", i + 1));
            for (j, b) in self.sorted_fencer_list.iter().enumerate() {
                match (i != j).then(|| self.match_between(*a, *b)).flatten() {
                    Some(m) if !m.bout.is_dropped() => {
                        let image = m.bout.score(*a).map(|sc| sc.image(self.max_score)).unwrap_or_default();
                        s.push_str(&format!("            <td class=\"PoolCell\">{image}</td>\n"));
                    }
                    _ => s.push_str("            <td class=\"NoScoreCell\"></td>\n"),
                }
            }
            s.push_str("            <td class=\"GridSeparator\"></td>\n");
            if n < grid_size {
                s.push_str("            <td class=\"GridSeparator\"></td>\n");
            }
            let o = self.current_owner;
            let ratio = o.and_then(|o| get_uint(players, *a, "victories_ratio", o)).unwrap_or(0);
            let indice = o.and_then(|o| get_int(players, *a, "indice", o)).unwrap_or(0);
            let hs = o.and_then(|o| get_uint(players, *a, "HS", o)).unwrap_or(0);
            s.push_str(&format!("            <td class=\"DashBoardCell\">{}</td>\n", self.result(*a).rank));
            s.push_str(&format!("            <td class=\"DashBoardCell\">{:.3}</td>\n", ratio as f64 / 1000.0));
            s.push_str(&format!("            <td class=\"DashBoardCell\">{indice}</td>\n"));
            s.push_str(&format!("            <td class=\"DashBoardCell\">{hs}</td>\n"));
            s.push_str("          </tr>\n");
        }
        s
    }
}

#[derive(Debug, Clone, Copy)]
enum CombinedOperation {
    None,
    Sum,
}

/// Identité d'un joueur telle qu'affichée dans les feuilles
/// (`GetPlayerImage` : nom, prénom, club, ligue, région, pays).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerIdentity {
    pub name: String,
    pub first_name: String,
    pub club: String,
    pub league: String,
    pub region: String,
    pub country: String,
}

pub fn player_identity(players: &PlayerStore, p: PlayerRef) -> PlayerIdentity {
    let Some(pl) = players.get(p) else { return PlayerIdentity::default() };
    let get = |c: &str| pl.str_attr(c).unwrap_or_default().to_string();
    PlayerIdentity {
        name: get("name"),
        first_name: get("first_name"),
        club: get("club"),
        league: get("league"),
        region: get("region"),
        country: get("country"),
    }
}

/// Case de la grille d'une feuille de poule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetCell {
    /// Score (`V`, `V4`, `3`) ou nom du match pour les arbitres.
    pub text: String,
    /// Case grisée (diagonale, match d'un tireur sorti).
    pub grey: bool,
    /// Match correspondant (saisie des scores).
    pub match_index: Option<usize>,
}

/// Colonnes de résultats d'une ligne.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dashboard {
    pub status: char,
    pub victories: String,
    pub victories_ratio: String,
    pub hits_scored: String,
    pub hits_received: String,
    pub indice: String,
    pub rank: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetRow {
    pub position: usize,
    pub player: PlayerRef,
    pub identity: PlayerIdentity,
    pub cells: Vec<SheetCell>,
    /// Absent pour une feuille d'arbitre ou une poule non encore supervisée.
    pub dashboard: Option<Dashboard>,
}

/// Tireur d'une feuille de match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchSheetFencer {
    pub player: PlayerRef,
    pub position: usize,
    pub identity: PlayerIdentity,
    pub score: String,
}

/// Feuille de match (liste sous la grille).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchSheet {
    /// Indice réel du match, indépendant de son ordre d'affichage par vague.
    pub match_index: usize,
    pub resource: Option<String>,
    pub assignment: Option<MatchAssignment>,
    pub name: String,
    /// Icône horloge : assauts consécutifs.
    pub rest_error: bool,
    pub opponents: [Option<MatchSheetFencer>; 2],
    /// Nombre de cases à cocher par tireur (`GetScoreTable`, 0 au-delà de 15).
    pub score_boxes: u32,
}

/// Données d'une feuille de poule, prêtes à dessiner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolSheet {
    pub title: String,
    /// Noms des arbitres du bandeau (détails par match en multi-pistes).
    pub referee: Option<(String, Option<String>)>,
    pub piste: Option<String>,
    pub for_referees: bool,
    pub rows: Vec<SheetRow>,
    pub matches: Vec<MatchSheet>,
    /// En-têtes des colonnes de résultats (déjà traduits).
    pub headers: Vec<String>,
}

/// Classe XML du joueur (`Tireur`/`Equipe`).
pub fn xml_player_tag(team_event: bool) -> &'static str {
    if team_event {
        PlayerClass::Team.xml_tag()
    } else {
        PlayerClass::Fencer.xml_tag()
    }
}

impl Pool {
    /// Renumérotation des tireurs par le pointage (les arbitres ont leur
    /// propre numérotation et ne changent pas).
    pub(crate) fn remap_fencers(&mut self, map: &std::collections::HashMap<PlayerRef, PlayerRef>) {
        let tr = |r: &mut PlayerRef| {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        };
        self.fencer_list.iter_mut().for_each(tr);
        self.sorted_fencer_list.iter_mut().for_each(tr);
        for m in &mut self.matches {
            for o in &mut m.bout.opponents {
                if let Some(f) = o.fencer.as_mut() {
                    tr(f);
                }
            }
        }
        self.results = std::mem::take(&mut self.results)
            .into_iter()
            .map(|(r, v)| (map.get(&r).copied().unwrap_or(r), v))
            .collect();
    }
}
