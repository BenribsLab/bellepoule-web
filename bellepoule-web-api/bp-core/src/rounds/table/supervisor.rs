//! Phase de tableaux (port de `rounds/tableau/table_supervisor.cpp`).
//!
//! La phase gère un arbre de suites de tableaux : la suite principale
//! (places 1…) et, selon l'option `PlacesTirees`, les suites de classement
//! qui reçoivent les battus (places 3, 5, 9… puis 7, 11…). Les suites sont
//! rangées en ordre préfixe dans `table_sets()` ; leur identifiant est leur
//! chemin dans l'arbre (`0`, `0:0`, `0:1`, `0:1:0`…).

use std::any::Any;
use std::collections::HashMap;

use crate::i18n::tr;
use crate::player::{AttrId, Owner, PlayerRef, PlayerStore};
use crate::stage::{
    load_attendee, save_attendees, ErrorLevel, PreviousStage, RankEntry, Stage, StageBase, StageContext, StageError,
    StageKind,
};
use crate::xml::XmlWriter;

use super::common::{self, codes, compare_players};
use super::table_set::{Env, NodeId, Roadmap, TableSet};

/// Option « places tirées » (`PlacesTirees`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencedPlaces {
    /// Aucun match de classement (0).
    None,
    /// Selon le quota (1) : pas de match de classement.
    Quota,
    /// Match pour la 3e place (3).
    ThirdPlaces,
    /// Toutes les places tirées (99).
    AllPlaces,
}

impl FencedPlaces {
    pub fn value(self) -> u32 {
        match self {
            FencedPlaces::None => 0,
            FencedPlaces::Quota => 1,
            FencedPlaces::ThirdPlaces => 3,
            FencedPlaces::AllPlaces => 99,
        }
    }

    pub fn from_value(v: u32) -> Self {
        match v {
            1 => FencedPlaces::Quota,
            3 => FencedPlaces::ThirdPlaces,
            99 => FencedPlaces::AllPlaces,
            _ => FencedPlaces::None,
        }
    }
}

/// Icône d'état d'une suite dans la liste des suites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSetStatus {
    Over,
    Error,
    Running,
    Empty,
}

pub struct TableSupervisor {
    base: StageBase,
    /// Valeur brute de `PlacesTirees` (voir `FencedPlaces`).
    pub fenced_places: u32,
    sets: Vec<TableSet>,
    displayed: Option<usize>,
    previous: Option<PreviousStage>,
    result: Option<Vec<PlayerRef>>,
    output_short_list: Option<Vec<PlayerRef>>,
}

impl TableSupervisor {
    pub fn new(id: u32) -> Self {
        let mut base = StageBase::new(StageKind::TableSupervisor, id);
        base.max_score = Some(15);
        base.net_id = common::new_net_id();
        base.filter = codes(&["stage_start_rank", "name", "first_name", "club"]);
        base.classification_filter = codes(&["rank", "status", "name", "first_name", "club"]);
        TableSupervisor {
            base,
            fenced_places: 0,
            sets: Vec::new(),
            displayed: None,
            previous: None,
            result: None,
            output_short_list: None,
        }
    }

    // ----------------------------------------------------------------- accès

    pub fn fenced_places_mode(&self) -> FencedPlaces {
        FencedPlaces::from_value(self.fenced_places)
    }

    pub fn max_score(&self) -> u32 {
        self.base.max_score.unwrap_or(15)
    }

    /// Toutes les suites, en ordre préfixe.
    pub fn table_sets(&self) -> &[TableSet] {
        &self.sets
    }

    pub fn table_set(&self, i: usize) -> &TableSet {
        &self.sets[i]
    }

    pub fn table_set_mut(&mut self, i: usize) -> &mut TableSet {
        &mut self.sets[i]
    }

    pub fn table_set_by_id(&self, id: &str) -> Option<usize> {
        self.sets.iter().position(|s| s.id == id)
    }

    /// Suites visibles (affichées dans la liste, enregistrées, classées).
    pub fn visible_table_sets(&self) -> Vec<usize> {
        (0..self.sets.len()).filter(|i| self.sets[*i].visible).collect()
    }

    pub fn displayed_table_set(&self) -> Option<usize> {
        self.displayed
    }

    /// Sélection d'une suite dans la liste (`OnTableSetSelected`).
    pub fn select_table_set(&mut self, i: Option<usize>) {
        self.displayed = i.filter(|i| *i < self.sets.len());
    }

    /// État d'une suite (`OnTableSetStatusUpdated`).
    pub fn table_set_status(&self, i: usize) -> TableSetStatus {
        let s = &self.sets[i];
        if s.is_over {
            TableSetStatus::Over
        } else if s.has_error() {
            TableSetStatus::Error
        } else if !s.tables.is_empty() {
            TableSetStatus::Running
        } else {
            TableSetStatus::Empty
        }
    }

    /// `TableSetIsFenced`.
    pub fn table_set_is_fenced(&self, i: usize) -> bool {
        if self.sets[i].parent.is_none() {
            return true;
        }
        let first = self.sets[i].relative_place(self.sets[i].first_place);
        match self.fenced_places_mode() {
            FencedPlaces::AllPlaces => true,
            FencedPlaces::ThirdPlaces => first <= 3,
            _ => false,
        }
    }

    fn owner(&self) -> Owner {
        self.base.owner()
    }

    fn player_tag(ctx_team: bool) -> &'static str {
        if ctx_team {
            "Equipe"
        } else {
            "Tireur"
        }
    }

    // --------------------------------------------------------- suites

    fn delete_table_sets(&mut self) {
        self.sets.clear();
        self.displayed = None;
    }

    fn feed_table_set_store(&mut self, from_place: u32, nb_tables: u32, parent: Option<usize>, nb_players: u32) {
        self.feed_group_table_sets(from_place, nb_tables, parent, nb_players, None, 1);
    }

    /// Arbre des suites ; `group`/`base_place` pour l'entraînement des
    /// jeunes (une suite principale par groupe, places à la suite).
    fn feed_group_table_sets(&mut self, from_place: u32, nb_tables: u32, parent: Option<usize>, nb_players: u32, group: Option<u32>, base_place: u32) {
        let idx = self.sets.len();
        let id = match (parent, group) {
            (None, None) => "0".to_string(),
            (None, Some(g)) => format!("G{g}"),
            (Some(p), _) => format!("{}:{}", self.sets[p].id, self.sets[p].children.len()),
        };
        let mut set = TableSet::new(id, from_place, parent);
        set.group = group;
        set.base_place = base_place;
        self.sets.push(set);
        if let Some(p) = parent {
            self.sets[p].children.push(idx);
        }
        let mut place_offset = 1u32;
        for i in 0..nb_tables.saturating_sub(1) {
            place_offset <<= 1;
            if from_place + place_offset + 1 < nb_players + base_place {
                self.feed_group_table_sets(from_place + place_offset, i + 1, Some(idx), nb_players, group, base_place);
            }
        }
    }

    /// Groupes de l'entraînement des jeunes parmi les participants, dans
    /// l'ordre d'entrée (`None` hors entraînement des jeunes).
    fn youth_groups(&self, players: &PlayerStore) -> Option<Vec<(u32, Vec<PlayerRef>)>> {
        let group_of = |p: &PlayerRef| players.get(*p).and_then(|pl| pl.attr("youth_group")).map(|v| v.as_uint());
        if self.base.attendees.is_empty() || !self.base.attendees.iter().all(|p| group_of(p).is_some()) {
            return None;
        }
        let mut groups: Vec<(u32, Vec<PlayerRef>)> = Vec::new();
        for p in &self.base.attendees {
            let g = group_of(p).unwrap_or(1);
            match groups.iter_mut().find(|(n, _)| *n == g) {
                Some((_, list)) => list.push(*p),
                None => groups.push((g, vec![*p])),
            }
        }
        groups.sort_by_key(|(g, _)| *g);
        Some(groups)
    }

    /// `CreateTableSets` : arbre des suites et suite principale garnie.
    fn create_table_sets(&mut self, players: &PlayerStore) {
        if let Some(groups) = self.youth_groups(players) {
            self.create_youth_table_sets(players, groups);
            return;
        }
        let nb_players = self.base.attendees.len() as u64;
        if nb_players > 0 {
            let mut nb_tables = 0;
            for i in 0..32u32 {
                if (1u64 << i) >= nb_players {
                    nb_tables = i;
                    break;
                }
            }
            if nb_tables > 0 {
                self.sets.clear();
                self.feed_table_set_store(1, nb_tables, None, nb_players as u32);
                let mut attendees = self.base.attendees.clone();
                let id = AttrId::local("stage_start_rank", self.owner());
                attendees.sort_by(|a, b| compare_players(players, *a, *b, &id, 0));
                self.set_set_attendees(0, attendees.into_iter().map(Some).collect(), None, players);
                self.displayed = Some(0);
            }
        }
        self.set_table_sets_state();
    }

    /// Entraînement des jeunes : une suite principale par groupe, les places
    /// des groupes se suivant (groupe 1 : 1 à 8, groupe 2 : 9 à 16…).
    fn create_youth_table_sets(&mut self, players: &PlayerStore, groups: Vec<(u32, Vec<PlayerRef>)>) {
        self.sets.clear();
        let id = AttrId::local("stage_start_rank", self.owner());
        let mut base = 1u32;
        for (group, mut members) in groups {
            let n = members.len() as u32;
            let nb_tables = (0..32u32).find(|i| (1u64 << i) >= n as u64).unwrap_or(0);
            if nb_tables == 0 {
                // Un seul tireur : il est premier de son groupe, sans tableau.
                base += n;
                continue;
            }
            let root = self.sets.len();
            self.feed_group_table_sets(base, nb_tables, None, n, Some(group), base);
            members.sort_by(|a, b| compare_players(players, *a, *b, &id, 0));
            self.set_set_attendees(root, members.into_iter().map(Some).collect(), None, players);
            base += n;
        }
        self.displayed = if self.sets.is_empty() { None } else { Some(0) };
        self.set_table_sets_state();
    }

    /// `SetTableSetsState` : suites actives et visibles selon l'option.
    fn set_table_sets_state(&mut self) {
        for i in 0..self.sets.len() {
            if self.table_set_is_fenced(i) {
                self.sets[i].active = true;
                if self.sets[i].has_attendees() {
                    self.sets[i].visible = true;
                } else {
                    self.hide_table_set(i);
                }
            } else {
                self.sets[i].active = false;
                self.hide_table_set(i);
            }
        }
        if self.displayed.is_none() && !self.sets.is_empty() {
            self.displayed = Some(0);
        }
    }

    fn hide_table_set(&mut self, i: usize) {
        self.sets[i].visible = false;
        if self.displayed == Some(i) {
            self.displayed = None;
        }
    }

    fn env<'a>(&self, players: &'a PlayerStore, short_list: &'a [PlayerRef], tag: &'a str) -> Env<'a> {
        Env {
            players,
            owner: self.owner(),
            previous_rank: common::previous_rank_id(self.previous, self.owner()),
            short_list,
            player_tag: tag,
        }
    }

    /// `TableSet::SetAttendees` avec réinitialisation des suites des battus.
    fn set_set_attendees(&mut self, i: usize, attendees: Vec<Option<PlayerRef>>, withdrawals: Option<Vec<PlayerRef>>, players: &PlayerStore) {
        if let Some(mut w) = withdrawals {
            let short = self.base.attendees.clone();
            let env = self.env(players, &short, "Tireur");
            self.sets[i].sort_withdrawals(&env, &mut w);
            self.sets[i].withdrawals = w;
        }
        let defeated: Vec<usize> = self.sets[i].tables.iter().filter_map(|t| t.defeated_table_set).collect();
        for d in defeated {
            self.set_set_attendees(d, Vec::new(), Some(Vec::new()), players);
        }
        let max = self.max_score();
        self.sets[i].apply_attendees(attendees, max);
    }

    /// `OnTableOver` : les battus du tableau rejoignent leur suite de classement.
    fn on_table_over(&mut self, set: usize, t: usize, players: &PlayerStore) {
        let nb = self.sets[set].tables.len();
        let number = self.sets[set].tables[t].number as usize;
        if nb < number + 3 {
            return;
        }
        let Some(&child) = self.sets[set].children.get(nb - number - 3) else { return };
        self.sets[set].tables[t].defeated_table_set = Some(child);
        let short = self.base.attendees.clone();
        let (loosers, withdrawals, _, count) = {
            let env = self.env(players, &short, "Tireur");
            self.sets[set].loosers(t, &env)
        };
        if count > 0 {
            self.set_set_attendees(child, loosers, Some(withdrawals), players);
            if self.table_set_is_fenced(child) {
                self.sets[child].visible = true;
            }
        } else {
            self.set_set_attendees(child, Vec::new(), Some(withdrawals), players);
            self.hide_table_set(child);
        }
    }

    /// Suite de `OnNewScore` : propagation, état, tableau terminé.
    fn on_new_score(&mut self, set: usize, node: NodeId, players: &PlayerStore) {
        self.sets[set].spread_winners();
        self.sets[set].refresh_table_status();
        if let Some(t) = self.sets[set].managing_table(node) {
            if self.sets[set].tables[t].is_over {
                self.on_table_over(set, t, players);
            }
        }
    }

    fn check_over_tables(&mut self, set: usize, players: &PlayerStore) {
        for t in 1..self.sets[set].tables.len() {
            if self.sets[set].tables[t].is_over {
                self.on_table_over(set, t, players);
            }
        }
    }

    // --------------------------------------------------------------- saisie

    /// Saisie d'un score (`V`, `W3`, `12`, vide) pour `fencer` dans le match
    /// du nœud `node`. Renvoie vrai si la saisie est complète. Sans effet si
    /// la phase est verrouillée.
    pub fn set_score(&mut self, ctx: &mut StageContext, set: usize, node: NodeId, fencer: PlayerRef, value: &str) -> bool {
        if self.base.locked {
            return false;
        }
        let complete = self.sets[set].input_score(node, fencer, value);
        self.on_new_score(set, node, ctx.players);
        complete
    }

    /// Changement de statut d'un tireur dans un match (`OnStatusChanged`) :
    /// `A` abandon, `E` exclusion, `F` forfait ou `Q` (rétablissement).
    pub fn set_fencer_status(&mut self, ctx: &mut StageContext, set: usize, node: NodeId, fencer: PlayerRef, code: char) {
        if self.base.locked {
            return;
        }
        if let Some(m) = self.sets[set].nodes[node].bout.as_mut() {
            if code != 'Q' {
                m.drop_fencer(fencer, code);
            } else {
                m.restore_fencer(fencer);
            }
            super::table_set::timestamp(m);
        }
        let owner = self.owner();
        if let Some(p) = ctx.players.get_mut(fencer) {
            p.set_str(&AttrId::local("status", owner), &code.to_string());
        }
        self.on_new_score(set, node, ctx.players);
    }

    /// Statut affiché pour le vainqueur d'une case dans le match suivant.
    pub fn current_status(&self, set: usize, node: NodeId, fencer: PlayerRef) -> char {
        self.sets[set]
            .bout(node)
            .filter(|m| m.is_dropped())
            .and_then(|m| m.score(fencer))
            .and_then(|s| s.drop_reason())
            .unwrap_or('Q')
    }

    /// Scores aléatoires pour le premier tableau affiché de la suite affichée
    /// (`OnStuffClicked`, option « ScoreAleatoire »).
    pub fn stuff<R: rand::Rng>(&mut self, ctx: &mut StageContext, rng: &mut R) {
        let Some(set) = self.displayed else { return };
        if self.base.locked {
            return;
        }
        let max = self.max_score();
        self.sets[set].stuff(rng, max);
        self.check_over_tables(set, ctx.players);
    }

    pub fn display_previous(&mut self, set: usize) {
        self.sets[set].display_previous();
    }

    pub fn display_next(&mut self, set: usize) {
        self.sets[set].display_next();
    }

    // -------------------------------------------------------- configuration

    /// Score maximum (`ScoreMax`) : appliqué aussi aux matchs existants.
    pub fn set_max_score(&mut self, max: u32) {
        self.base.max_score = Some(max);
        for s in &mut self.sets {
            s.set_max_score(max);
        }
    }

    /// `ApplyConfig` pour l'option des places tirées.
    pub fn set_fenced_places(&mut self, mode: FencedPlaces) {
        self.fenced_places = mode.value();
        self.set_table_sets_state();
    }

    /// Lignes de configuration imprimées (`DrawConfig`).
    pub fn config_lines(&self) -> Vec<String> {
        let places = match self.fenced_places_mode() {
            FencedPlaces::AllPlaces => tr("All places fenced"),
            FencedPlaces::ThirdPlaces => tr("Fence off for third place"),
            _ => tr("No fence off for third place"),
        };
        let qualified = match self.base.nb_qualified {
            Some(n) => format!("{} : {}", tr("Qualified"), n),
            None => format!("{} : 100%", tr("Qualified")),
        };
        vec![places, format!("{} : {}", tr("Max score"), self.max_score()), qualified]
    }

    /// Nom à imprimer (`GetPrintName`).
    pub fn print_name(&self) -> Option<String> {
        self.displayed.map(|d| self.sets[d].print_name(&self.base.name))
    }

    /// `GetFullName` : « Tableau\nnom ».
    pub fn full_name(&self) -> String {
        format!("{}\n{}", tr(StageKind::TableSupervisor.class_name()), self.base.name)
    }

    // -------------------------------------------------------------- réseau

    /// Retrouve un match par l'identifiant réseau de son tableau (`batch`)
    /// et le sien (`source`).
    pub fn find_match_by_net_id(&self, batch: u32, source: u32) -> Option<(usize, NodeId)> {
        for (s, set) in self.sets.iter().enumerate() {
            for t in set.tables.iter().skip(1) {
                if t.net_id != batch {
                    continue;
                }
                for n in &t.match_list {
                    if set.nodes[*n].bout.as_ref().map(|m| m.net_id) == Some(source) {
                        return Some((s, *n));
                    }
                }
            }
        }
        None
    }

    /// `BellePoule2D::Roadmap` : affecte (ou efface avec `None`) piste,
    /// heure et arbitres d'un match.
    pub fn apply_roadmap(&mut self, set: usize, node: NodeId, roadmap: Option<&Roadmap>) {
        self.sets[set].apply_roadmap(node, roadmap);
    }

    /// `ClearRoadmaps` d'un tableau.
    pub fn clear_roadmaps(&mut self, set: usize, table: usize) {
        self.sets[set].clear_roadmaps(table);
    }

    /// `SmartPoule::Score` : relit les deux `<Tireur>` d'un `<Match>` reçu.
    pub fn load_remote_score(&mut self, ctx: &mut StageContext, set: usize, path: &str, xml: &str) -> bool {
        let Some(node) = self.sets[set].node_from_path(path) else { return false };
        let Ok(doc) = crate::xml::parse(xml) else { return false };
        let root = doc.root_element();
        let children: Vec<_> = root.children().filter(|n| n.is_element()).collect();
        if root.tag_name().name() != "Match" || children.len() != 2 {
            return false;
        }
        if let Some(m) = self.sets[set].nodes[node].bout.as_mut() {
            for (i, c) in children.iter().enumerate() {
                if let Some(f) = m.opponent(i) {
                    m.load_fencer(*c, f);
                }
            }
            m.after_load();
        }
        let over = self.sets[set].bout(node).map(|m| m.is_over()).unwrap_or(false);
        if over {
            self.sets[set].spread_winners();
        }
        self.sets[set].refresh_table_status();
        let _ = ctx;
        true
    }

    /// En-tête `<PhaseDeTableaux …>` (laissé ouvert, `SaveHeader`).
    pub fn save_header(&self, w: &mut XmlWriter) {
        w.start(StageKind::TableSupervisor.xml_name());
        self.write_configuration(w);
    }

    /// Feuille de match envoyée à un arbitre (`SmartPoule::ScoreSheetCall`) :
    /// `<PhaseDeTableaux><SuiteDeTableaux><Tableau><Match/>` refermés. Le
    /// contexte de compétition (`Contest::SaveHeader`) est à écrire avant.
    pub fn write_score_sheet(&self, players: &PlayerStore, set: usize, node: NodeId, w: &mut XmlWriter) {
        let s = &self.sets[set];
        self.save_header(w);
        s.save_header(w);
        if let Some(t) = s.managing_table(node) {
            s.save_table_header(t, w, None);
            if let Some(m) = s.bout(node) {
                m.save(players, w);
            }
            w.end();
        }
        w.end();
        w.end();
    }

    // ---------------------------------------------------------- classement

    /// Classement courant : ordre et rang (`GetCurrentClassification`), sans
    /// modifier les joueurs.
    pub fn compute_classification(&self, players: &PlayerStore) -> (Vec<PlayerRef>, HashMap<PlayerRef, u32>) {
        let short = self.base.attendees.clone();
        let env = self.env(players, &short, "Tireur");
        let mut result: Vec<PlayerRef> = Vec::new();
        let mut black: Vec<PlayerRef> = Vec::new();
        let mut ranks = HashMap::new();
        for (i, set) in self.sets.iter().enumerate() {
            if !set.visible {
                continue;
            }
            let fourth = set.tables.len() > 2
                && set.tables[2].defeated_table_set.map(|d| self.sets[d].active).unwrap_or(false);
            let _ = i;
            for (p, r) in set.classification(&env, fourth) {
                ranks.insert(p, r);
                result.retain(|x| *x != p);
                result.push(p);
            }
            for p in set.blackcardeds(&env) {
                if !black.contains(&p) {
                    black.insert(0, p);
                }
                result.retain(|x| *x != p);
            }
        }
        let rank_id = AttrId::local("rank", self.owner());
        result.sort_by(|a, b| {
            let ra = ranks.get(a).copied();
            let rb = ranks.get(b).copied();
            match (ra, rb) {
                (Some(x), Some(y)) => x.cmp(&y),
                _ => compare_players(players, *a, *b, &rank_id, 0),
            }
        });
        result.extend(black);
        (result, ranks)
    }

    /// Écrit les rangs (`rank`) calculés, pour l'affichage du classement.
    pub fn update_classification(&self, ctx: &mut StageContext) {
        let (_, ranks) = self.compute_classification(ctx.players);
        let ranks: Vec<_> = ranks.into_iter().collect();
        common::write_ranks(self.owner(), ctx, &ranks);
    }

    /// Export HTML de la suite affichée (`DumpToHTML`).
    pub fn dump_to_html(&self, players: &PlayerStore) -> String {
        // Entraînement des jeunes : le tableau principal de chaque groupe.
        if self.sets.iter().any(|s| s.group.is_some()) {
            return self
                .sets
                .iter()
                .filter(|s| s.parent.is_none())
                .map(|s| format!("<h3>{}</h3>\n{}", s.name().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"), s.dump_to_html(players)))
                .collect::<Vec<_>>()
                .join("\n");
        }
        self.displayed.map(|d| self.sets[d].dump_to_html(players)).unwrap_or_default()
    }

    fn write_configuration(&self, w: &mut XmlWriter) {
        common::save_configuration(&self.base, w, true);
        w.attr_fmt("PlacesTirees", self.fenced_places);
    }

    fn rank_lookup<'a>(ranks: &'a HashMap<PlayerRef, u32>, players: &'a PlayerStore, owner: Owner) -> impl Fn(PlayerRef) -> u32 + 'a {
        move |r| {
            ranks.get(&r).copied().unwrap_or_else(|| {
                players.get(r).and_then(|p| p.get(&AttrId::local("rank", owner))).map(|v| v.as_uint()).unwrap_or(0)
            })
        }
    }
}

impl Stage for TableSupervisor {
    fn base(&self) -> &StageBase {
        &self.base
    }

    /// `Supervisor::DumpToHTML` : la suite affichée (ou chaque groupe).
    fn dump_to_html(&self, ctx: &StageContext, html: &mut String) {
        html.push_str(&TableSupervisor::dump_to_html(self, ctx.players));
    }

    fn base_mut(&mut self) -> &mut StageBase {
        &mut self.base
    }

    fn set_previous(&mut self, previous: Option<PreviousStage>) {
        self.previous = previous;
    }

    fn bout_max_score(&self) -> Option<u32> {
        Some(self.max_score())
    }

    fn set_attendees(&mut self, ctx: &mut StageContext, attendees: Vec<PlayerRef>) {
        common::retrieve_attendees(&mut self.base, ctx, attendees);
    }

    fn garnish(&mut self, ctx: &mut StageContext) {
        self.delete_table_sets();
        self.create_table_sets(ctx.players);
    }

    fn reset(&mut self, ctx: &mut StageContext) {
        common::reset_statuses(&self.base, ctx);
        self.delete_table_sets();
        self.result = None;
        self.output_short_list = None;
        self.base.quota_exceedance = 0;
    }

    fn load(&mut self, ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), crate::Error> {
        common::load_configuration(&mut self.base, node);
        if let Some(v) = node.attribute("PlacesTirees") {
            self.fenced_places = crate::attribute::parse_leading_int(v) as u32;
        }
        let tag = Self::player_tag(ctx.settings.team_event);
        let fill = self.base.attendees.is_empty();
        let owner = self.owner();
        for n in node.children().filter(|n| n.is_element()) {
            let name = n.tag_name().name();
            if name == tag {
                if let Some(r) = load_attendee(&self.base, ctx, n, owner) {
                    if fill {
                        self.base.attendees.push(r);
                    }
                }
            } else if name == "SuiteDeTableaux" {
                if self.sets.is_empty() {
                    self.create_table_sets(ctx.players);
                }
                let Some(i) = n.attribute("ID").and_then(|id| self.table_set_by_id(id)) else { continue };
                let short = self.base.attendees.clone();
                {
                    let env = self.env(ctx.players, &short, tag);
                    let mut set = std::mem::replace(&mut self.sets[i], TableSet::new(String::new(), 0, None));
                    set.load_tables(n, &env);
                    self.sets[i] = set;
                }
                self.sets[i].spread_winners();
                self.sets[i].refresh_table_status();
                self.check_over_tables(i, ctx.players);
            } else {
                break;
            }
        }
        Ok(())
    }

    fn save(&self, ctx: &StageContext, w: &mut XmlWriter) {
        self.save_header(w);
        save_attendees(&self.base, ctx, w, self.owner());
        let ids = |i: usize| self.sets[i].id.clone();
        for set in self.sets.iter().filter(|s| s.visible) {
            set.save(ctx.players, w, &ids);
        }
        w.end();
    }

    fn save_configuration(&self, w: &mut XmlWriter) {
        self.write_configuration(w);
    }

    fn is_over(&self, _ctx: &StageContext) -> bool {
        self.sets.iter().filter(|s| s.visible && s.parent.is_none()).all(|s| s.is_over)
    }

    fn error(&self, _ctx: &StageContext) -> Option<StageError> {
        for set in self.sets.iter().filter(|s| s.visible) {
            if let Some(n) = set.first_error {
                let name = set.match_name(n).unwrap_or_default();
                return Some(StageError {
                    level: ErrorLevel::Error,
                    message: tr("No winner!"),
                    guilty: Some(tr("Match %s").replace("%s", &name)),
                });
            }
        }
        None
    }

    fn classification(&self, ctx: &StageContext) -> Vec<RankEntry> {
        let (result, ranks) = match &self.result {
            Some(r) => (r.clone(), self.compute_classification(ctx.players).1),
            None => self.compute_classification(ctx.players),
        };
        let lookup = Self::rank_lookup(&ranks, ctx.players, self.owner());
        common::rank_entries(ctx.players, self.owner(), &result, &lookup)
    }

    fn output(&self, ctx: &StageContext) -> Vec<PlayerRef> {
        if let Some(o) = &self.output_short_list {
            return o.clone();
        }
        let (result, ranks) = self.compute_classification(ctx.players);
        let lookup = Self::rank_lookup(&ranks, ctx.players, self.owner());
        common::compute_output(&self.base, ctx.players, &result, &lookup, true).output
    }

    fn lock(&mut self, ctx: &mut StageContext) {
        self.base.locked = true;
        for s in &mut self.sets {
            s.locked = true;
        }
        let previous_output = self.output_short_list.take().unwrap_or_default();
        common::restore_not_qualified(self.owner(), ctx, &previous_output);
        let (result, ranks) = self.compute_classification(ctx.players);
        let list: Vec<_> = ranks.iter().map(|(p, r)| (*p, *r)).collect();
        common::write_ranks(self.owner(), ctx, &list);
        let c = {
            let lookup = Self::rank_lookup(&ranks, ctx.players, self.owner());
            common::compute_output(&self.base, ctx.players, &result, &lookup, true)
        };
        common::apply_output(&mut self.base, ctx, &c);
        self.result = Some(result);
        self.output_short_list = Some(c.output);
    }

    fn unlock(&mut self, _ctx: &mut StageContext) {
        self.result = None;
        self.output_short_list = None;
        self.base.quota_exceedance = 0;
        self.base.locked = false;
        for s in &mut self.sets {
            s.locked = false;
        }
    }

    fn remap_players(&mut self, map: &HashMap<PlayerRef, PlayerRef>) {
        let tr = |r: &mut PlayerRef| {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        };
        self.base.attendees.iter_mut().for_each(tr);
        self.result.iter_mut().flatten().for_each(tr);
        self.output_short_list.iter_mut().flatten().for_each(tr);
        for set in &mut self.sets {
            set.remap_fencers(map);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
