//! Suite de tableaux (port de la logique de `rounds/tableau/table_set.cpp`).
//!
//! Une suite est un arbre binaire complet de nœuds. La racine est la case du
//! vainqueur ; chaque nœud porte le match dont le vainqueur occupe la case
//! (`Node::bout`). Les feuilles (plus grand tableau) portent un « match »
//! fictif tireur contre exempt. Un nœud dont le match est `None` est une
//! place morte (pas de tireur possible dans ce sous-arbre).
//!
//! La suite principale (`first_place == 1`) place les tireurs selon la
//! formule FIE des têtes de série ; les suites de classement (places 3, 5,
//! 9…) reçoivent les battus d'un tableau dans l'ordre du tableau.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::bout::{Match, StartTime};
use crate::i18n::tr;
use crate::player::{AttrId, Owner, PlayerClass, PlayerRef, PlayerStore};
use crate::xml::XmlWriter;

use super::common::{compare_players, new_net_id, status_of};
use super::table::Table;

/// Indice d'un nœud dans sa suite de tableaux.
pub type NodeId = usize;

/// Une case de l'arbre (`NodeData`).
#[derive(Debug, Clone)]
pub struct Node {
    /// Rang de tête de série attendu pour le vainqueur de la case.
    pub expected_winner_rank: u32,
    /// Indice `t` du tableau (taille `2^t`) qui contient la case.
    pub table: usize,
    /// Position de la case dans son tableau (0 = en haut).
    pub table_index: u32,
    /// Match dont le vainqueur occupe la case ; `None` pour une place morte.
    pub bout: Option<Match>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// Feuille de match déjà imprimée.
    pub printed: bool,
}

/// Contexte de calcul passé par la phase.
pub(crate) struct Env<'a> {
    pub players: &'a PlayerStore,
    /// Propriétaire des attributs `status`/`rank` (la phase).
    pub owner: Owner,
    /// Rang de la phase précédente (départage du classement).
    pub previous_rank: AttrId,
    /// Participants de la phase (`GetFencerFromRef`).
    pub short_list: &'a [PlayerRef],
    /// Balise des tireurs (`Tireur` ou `Equipe`).
    pub player_tag: &'a str,
}

/// Affectation d'un match par le gestionnaire de pistes (`BellePoule2D::Roadmap`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roadmap {
    pub piste: u32,
    pub start_time: Option<StartTime>,
    pub duration_span: u32,
    pub referees: Vec<PlayerRef>,
}

/// Entrée de la recherche rapide : un tableau et ses matchs prêts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickSearchTable {
    pub table: usize,
    pub image: String,
    pub matches: Vec<QuickSearchMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickSearchMatch {
    pub node: NodeId,
    /// Nom du match (`1-8.3`).
    pub name: String,
    /// « NOM A / NOM B ».
    pub label: String,
    /// Chemin `tableau:match` (identifiant des messages SmartPoule / HTTP).
    pub path: String,
}

/// Place un tireur (ou un exempt) dans un match (`Match::SetOpponent` C++ :
/// un exempt donne la victoire 0-0 à l'adversaire).
pub(crate) fn put_opponent(m: &mut Match, pos: usize, fencer: Option<PlayerRef>) {
    let max = m.max_score;
    m.opponents[pos].fencer = fencer;
    m.opponents[pos].is_known = true;
    if fencer.is_none() {
        m.opponents[pos].score.set(0, false, max);
        m.opponents[1 - pos].score.set(0, true, max);
    }
}

/// Retire un tireur d'un match (`Match::RemoveOpponent` C++).
pub(crate) fn take_opponent(m: &mut Match, pos: usize) {
    m.opponents[pos].fencer = None;
    m.opponents[pos].is_known = false;
    m.opponents[0].score.clean();
    m.opponents[1].score.clean();
}

/// Ajoute un arbitre en tête de liste s'il n'y est pas (`Match::AddReferee`).
pub(crate) fn add_referee(m: &mut Match, referee: PlayerRef) {
    if !m.referees.contains(&referee) {
        m.referees.insert(0, referee);
    }
}

/// `Match::Compare` : piste, heure puis numéro.
pub fn compare_matches(a: &Match, b: &Match) -> Ordering {
    a.piste.cmp(&b.piste).then_with(|| match (a.start_time, b.start_time) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    })
    .then_with(|| a.number.cmp(&b.number))
}

/// `Match::Timestamp` : durée du match terminé depuis son heure de début.
pub(crate) fn timestamp(m: &mut Match) {
    if let Some(start) = m.start_time {
        if m.is_over() {
            let now = crate::bout::now_rounded();
            let span = (now - start.0).num_seconds();
            m.duration_sec = if span < 0 { 1 } else { span as u32 };
        }
    }
}

/// `Match::AdjustRoadmap` : un match qui suit un match « à portée
/// multiple » hérite de sa piste, de ses arbitres et d'une heure décalée.
pub(crate) fn adjust_roadmap(m: &mut Match, according_to: &Match) -> bool {
    if m.opponents[0].is_known && m.opponents[1].is_known && m.start_time.is_none() && according_to.duration_span > 1 {
        if let Some(start) = according_to.start_time {
            let t = start.0 + chrono::Duration::minutes(1) + chrono::Duration::seconds(according_to.duration_sec as i64);
            m.start_time = Some(StartTime(t));
            m.duration_span = according_to.duration_span - 1;
            m.piste = according_to.piste;
            m.referees = according_to.referees.clone();
            return true;
        }
    }
    false
}

#[derive(Debug, Clone)]
pub struct TableSet {
    pub(crate) id: String,
    pub(crate) first_place: u32,
    pub(crate) parent: Option<usize>,
    pub(crate) children: Vec<usize>,
    pub(crate) visible: bool,
    pub(crate) active: bool,
    pub(crate) locked: bool,
    pub(crate) attendees: Vec<Option<PlayerRef>>,
    pub(crate) withdrawals: Vec<PlayerRef>,
    pub(crate) tables: Vec<Table>,
    pub(crate) nodes: Vec<Node>,
    pub(crate) root: Option<NodeId>,
    pub(crate) from_table: usize,
    pub(crate) to_table: usize,
    pub(crate) is_over: bool,
    pub(crate) first_error: Option<NodeId>,
    /// Compteur de numéros de match par tableau (entrées de recherche rapide).
    next_number: Vec<u32>,
    /// Entraînement des jeunes : groupe de la suite (sinon `None`).
    pub(crate) group: Option<u32>,
    /// Première place de la suite principale de son groupe (1 hors groupes).
    pub(crate) base_place: u32,
}

impl TableSet {
    pub(crate) fn new(id: String, first_place: u32, parent: Option<usize>) -> Self {
        TableSet {
            id,
            first_place,
            parent,
            children: Vec::new(),
            visible: false,
            active: false,
            locked: false,
            attendees: Vec::new(),
            withdrawals: Vec::new(),
            tables: Vec::new(),
            nodes: Vec::new(),
            root: None,
            from_table: 0,
            to_table: 0,
            is_over: false,
            first_error: None,
            next_number: Vec::new(),
            group: None,
            base_place: 1,
        }
    }

    /// Place dans le groupe (ou dans la phase hors groupes).
    pub fn relative_place(&self, place: u32) -> u32 {
        place + 1 - self.base_place
    }

    /// Groupe (entraînement des jeunes).
    pub fn group(&self) -> Option<u32> {
        self.group
    }

    fn place_label(&self) -> String {
        match self.group {
            Some(g) => format!("{} {} · {}{}", tr("Group"), g, tr("Place #"), self.relative_place(self.first_place)),
            None => format!("{}{}", tr("Place #"), self.first_place),
        }
    }

    // ---------------------------------------------------------------- accès

    /// Identifiant (chemin `0`, `0:1`… dans l'arbre des suites).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Première place attribuée par cette suite (1, 3, 5, 9…).
    pub fn first_place(&self) -> u32 {
        self.first_place
    }

    /// Nom court « Place #N » (`_short_name`, aussi écrit dans `Titre`).
    pub fn name(&self) -> String {
        self.place_label()
    }

    pub fn parent(&self) -> Option<usize> {
        self.parent
    }

    /// Suites filles (battus des demi-finales, des quarts…).
    pub fn children(&self) -> &[usize] {
        &self.children
    }

    /// Visible dans la liste des suites (et donc enregistrée, classée).
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Places tirées selon l'option `PlacesTirees`.
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn has_attendees(&self) -> bool {
        !self.attendees.is_empty()
    }

    /// Participants dans l'ordre de placement (`None` = place vide).
    pub fn attendees(&self) -> &[Option<PlayerRef>] {
        &self.attendees
    }

    pub fn withdrawals(&self) -> &[PlayerRef] {
        &self.withdrawals
    }

    pub fn nb_tables(&self) -> usize {
        self.tables.len()
    }

    /// Tableaux, indice `t` = taille `2^t`.
    pub fn tables(&self) -> &[Table] {
        &self.tables
    }

    pub fn table(&self, t: usize) -> &Table {
        &self.tables[t]
    }

    /// Indice du tableau de taille donnée.
    pub fn table_of_size(&self, size: u32) -> Option<usize> {
        self.tables.iter().position(|t| t.size == size)
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    pub fn bout(&self, id: NodeId) -> Option<&Match> {
        self.nodes.get(id).and_then(|n| n.bout.as_ref())
    }

    pub fn is_over(&self) -> bool {
        self.is_over
    }

    pub fn has_error(&self) -> bool {
        self.first_error.is_some()
    }

    /// Nœud du premier match en erreur (« pas de vainqueur »).
    pub fn first_error(&self) -> Option<NodeId> {
        self.first_error
    }

    /// Premier tableau affiché (colonne de gauche).
    pub fn from_table(&self) -> usize {
        self.from_table
    }

    /// Dernier tableau affiché (colonne de droite, au plus 3 tableaux après).
    pub fn to_table(&self) -> usize {
        self.to_table
    }

    /// Nom du match (`1-8.3`), `None` pour les matchs non numérotés.
    pub fn match_name(&self, id: NodeId) -> Option<String> {
        let m = self.bout(id)?;
        (m.number != 0).then(|| m.name.clone())
    }

    /// Tableau qui gère le match du nœud (tableau de gauche).
    pub fn managing_table(&self, id: NodeId) -> Option<usize> {
        let t = self.nodes[id].table + 1;
        (t < self.tables.len()).then_some(t)
    }

    /// Position du nœud parmi les enfants de son parent.
    pub fn child_position(&self, id: NodeId) -> usize {
        match self.nodes[id].parent {
            Some(p) => self.nodes[p].children.iter().position(|c| *c == id).unwrap_or(0),
            None => 0,
        }
    }

    /// Chemin `tableau:match` du match d'un nœud (recherche rapide).
    pub fn match_path(&self, id: NodeId) -> Option<String> {
        let m = self.bout(id)?;
        if m.number == 0 {
            return None;
        }
        let table_number = self.tables[self.nodes[id].table].number;
        Some(format!("{}:{}", table_number.checked_sub(1)?, m.number - 1))
    }

    /// Retrouve un match par son chemin `tableau:match`.
    pub fn node_from_path(&self, path: &str) -> Option<NodeId> {
        (0..self.nodes.len()).find(|n| self.match_path(*n).as_deref() == Some(path))
    }

    /// Référence du code QR d'un match (`#competition/phase/suite.chemin`, hexadécimal).
    pub fn flash_ref(&self, id: NodeId, contest_net_id: u32, stage_net_id: u32) -> Option<String> {
        Some(format!("#{:x}/{:x}/{}.{}", contest_net_id, stage_net_id, self.id, self.match_path(id)?))
    }

    /// Nom à imprimer (`GetPrintName`).
    pub fn print_name(&self, stage_name: &str) -> String {
        if stage_name.is_empty() {
            format!("{} - {}", tr("Table"), self.place_label())
        } else {
            format!("{} - {} - {}", tr("Table"), stage_name, self.place_label())
        }
    }

    /// Bouton « précédent » actif.
    pub fn can_display_previous(&self) -> bool {
        !self.tables.is_empty() && self.from_table + 1 < self.tables.len()
    }

    /// Bouton « suivant » actif : le tableau suivant est prêt ou terminé.
    pub fn can_display_next(&self) -> bool {
        self.from_table > 0 && {
            let next = &self.tables[self.from_table - 1];
            next.ready_to_fence || next.is_over
        }
    }

    /// Espace de noms des matchs du premier tableau affiché (`1-16.`).
    pub fn search_namespace(&self) -> String {
        let size = self.tables.get(self.from_table).map(|t| t.size).unwrap_or(0);
        format!("{}-{}.", self.first_place, size)
    }

    // ---------------------------------------------------------- parcours

    /// Parcours postfixe (enfants de gauche à droite, puis le nœud).
    pub fn post_order(&self) -> Vec<NodeId> {
        fn walk(s: &TableSet, n: NodeId, out: &mut Vec<NodeId>) {
            for c in &s.nodes[n].children {
                walk(s, *c, out);
            }
            out.push(n);
        }
        let mut out = Vec::new();
        if let Some(r) = self.root {
            walk(self, r, &mut out);
        }
        out
    }

    /// Parcours en largeur depuis la racine.
    pub fn level_order(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        if let Some(r) = self.root {
            out.push(r);
            let mut i = 0;
            while i < out.len() {
                let n = out[i];
                out.extend(self.nodes[n].children.iter().copied());
                i += 1;
            }
        }
        out
    }

    // ------------------------------------------------------------- arbre

    pub(crate) fn delete_tree(&mut self) {
        self.nodes.clear();
        self.root = None;
        self.tables.clear();
        self.next_number.clear();
        self.is_over = false;
    }

    /// `CreateTree` : tableaux, nœuds, placement des tireurs et exempts.
    pub(crate) fn create_tree(&mut self, max_score: u32) {
        let nb_players = self.attendees.len() as u64;
        let mut nb_tables = 0;
        for i in 0..32u32 {
            if (1u64 << i) >= nb_players {
                nb_tables = i as usize;
                break;
            }
        }
        nb_tables += 1;

        self.tables = (0..nb_tables).map(|t| Table::new(1 << t, (nb_tables - t - 1) as u32)).collect();
        self.next_number = vec![0; nb_tables];
        self.add_fork(None, max_score);

        // Places mortes : nœuds dont aucun enfant n'a de match
        for n in self.post_order() {
            if self.nodes[n].children.is_empty() {
                continue;
            }
            if self.nodes[n].bout.is_some() {
                let a = self.nodes[n].children[0];
                let b = self.nodes[n].children[1];
                if self.nodes[a].bout.is_none() && self.nodes[b].bout.is_none() {
                    self.drop_match(n);
                }
            }
        }

        self.change_from_table_forced(nb_tables - 1);
    }

    fn add_fork(&mut self, to: Option<NodeId>, max_score: u32) {
        let id = self.nodes.len();
        let (table, table_index, expected) = match to {
            None => (0, 0, 1),
            Some(to) => {
                let to_node = &self.nodes[to];
                let table = to_node.table + 1;
                let n = to_node.children.len() as u32;
                let table_index = if n == 0 { to_node.table_index * 2 } else { to_node.table_index * 2 + 1 };
                let expected = if n != to_node.expected_winner_rank % 2 {
                    to_node.expected_winner_rank
                } else {
                    (self.tables[table].size + 1) - to_node.expected_winner_rank
                };
                (table, table_index, expected)
            }
        };

        let mut bout = Match::new(max_score);
        let name_space = format!("{}-{}.", self.first_place, self.tables[table].size * 2);
        self.nodes.push(Node {
            expected_winner_rank: expected,
            table,
            table_index,
            bout: None,
            parent: to,
            children: Vec::new(),
            printed: false,
        });
        if let Some(to) = to {
            self.nodes[to].children.push(id);
        } else {
            self.root = Some(id);
        }
        self.tables[table].add_node(id);

        let left = table + 1;
        if left < self.tables.len() {
            self.next_number[left] += 1;
            bout.number = self.next_number[left];
            bout.name = format!("{name_space}{}", bout.number);
            bout.net_id = new_net_id();
            self.nodes[id].bout = Some(bout);
            // ManageMatch : insertion triée par numéro
            let number = self.next_number[left];
            let list = &mut self.tables[left].match_list;
            let pos = list
                .iter()
                .position(|n| self.nodes[*n].bout.as_ref().map(|m| m.number).unwrap_or(0) > number)
                .unwrap_or(list.len());
            list.insert(pos, id);

            self.add_fork(Some(id), max_score);
            self.add_fork(Some(id), max_score);
        } else {
            bout.name = name_space;
            self.nodes[id].bout = Some(bout);
            // Suite principale (de la phase ou d'un groupe) : têtes de série.
            let player = if self.parent.is_none() {
                self.attendees.get(expected as usize - 1).copied().flatten()
            } else {
                self.attendees.get(table_index as usize).copied().flatten()
            };
            if let Some(p) = player {
                let m = self.nodes[id].bout.as_mut().unwrap();
                put_opponent(m, 0, Some(p));
                put_opponent(m, 1, None);
                if let Some(to) = to {
                    let pos = self.child_position(id);
                    if let Some(tm) = self.nodes[to].bout.as_mut() {
                        put_opponent(tm, pos, Some(p));
                    }
                }
            } else if to.is_some() {
                self.drop_match(id);
            }
        }
    }

    /// `DropMatch` : la case devient une place morte, le parent a un exempt.
    fn drop_match(&mut self, id: NodeId) {
        let Some(parent) = self.nodes[id].parent else { return };
        let left = self.nodes[id].table + 1;
        if left < self.tables.len() {
            self.tables[left].match_list.retain(|n| *n != id);
        }
        self.nodes[id].bout = None;
        if self.child_position(id) == 0 {
            if let Some(pm) = self.nodes[parent].bout.as_mut() {
                put_opponent(pm, 0, None);
            }
        } else {
            let a = self.nodes[parent].children[0];
            let a_winner = self.nodes[a].bout.as_ref().map(|m| m.winner());
            if let Some(pm) = self.nodes[parent].bout.as_mut() {
                if let Some(w) = a_winner {
                    put_opponent(pm, 0, w);
                }
                put_opponent(pm, 1, None);
            }
        }
    }

    /// `Garnish` : reconstruit l'arbre et propage les vainqueurs.
    pub(crate) fn garnish(&mut self, max_score: u32) {
        self.delete_tree();
        self.create_tree(max_score);
        self.spread_winners();
    }

    /// Suite des `SetAttendees` C++ (hors réinitialisation des suites filles,
    /// faite par la phase) : arbre, tableaux affichés, statut.
    pub(crate) fn apply_attendees(&mut self, attendees: Vec<Option<PlayerRef>>, max_score: u32) {
        self.attendees = attendees;
        self.garnish(max_score);
        let nb = self.tables.len();
        let active = self.attendees.iter().filter(|a| a.is_some()).count() as u32;
        for t in 0..nb {
            let table = nb - t - 1;
            if self.tables[table].size / 2 < active {
                self.change_from_table(table);
                break;
            }
        }
        self.refresh_table_status();
    }

    // -------------------------------------------------------- propagation

    /// `SpreadWinners` : chaque match terminé envoie son vainqueur au match suivant.
    pub(crate) fn spread_winners(&mut self) {
        for n in self.post_order() {
            let Some(parent) = self.nodes[n].parent else { continue };
            let Some(m) = self.nodes[n].bout.clone() else { continue };
            let pos = self.child_position(n);
            let Some(pm) = self.nodes[parent].bout.as_mut() else { continue };
            if m.is_over() {
                put_opponent(pm, pos, m.winner());
                adjust_roadmap(pm, &m);
            } else {
                take_opponent(pm, pos);
            }
        }
    }

    /// `RefreshTableStatus` : état de chaque tableau et de la suite.
    pub(crate) fn refresh_table_status(&mut self) {
        if self.root.is_none() {
            return;
        }
        self.first_error = None;
        self.is_over = true;
        for t in &mut self.tables {
            t.is_over = true;
            t.ready_to_fence = false;
            t.first_error = None;
        }
        for n in self.post_order() {
            let left = self.nodes[n].table + 1;
            if left >= self.tables.len() {
                continue;
            }
            let Some(m) = self.nodes[n].bout.as_ref() else { continue };
            if !m.is_over() {
                if m.opponent(0).is_some() && m.opponent(1).is_some() {
                    if m.has_error() {
                        self.tables[left].first_error = Some(n);
                    }
                    self.tables[left].ready_to_fence = true;
                }
                self.tables[left].is_over = false;
            }
        }
        let nb = self.tables.len();
        for t in 0..nb {
            let table = &self.tables[nb - t - 1];
            if table.first_error.is_some() {
                self.first_error = table.first_error;
                self.is_over = false;
            } else if !table.is_over {
                self.is_over = false;
            }
        }
    }

    // ---------------------------------------------------------- affichage

    fn change_from_table_forced(&mut self, table: usize) {
        self.from_table = table;
        self.to_table = table;
        for _ in 0..3 {
            if self.to_table == 0 {
                break;
            }
            self.to_table -= 1;
        }
        for t in &mut self.tables {
            t.hide();
        }
        let mut column = 0;
        let mut t = self.from_table as isize;
        while t >= self.to_table as isize {
            let header = t > self.to_table as isize;
            self.tables[t as usize].show(column, header);
            column += 1;
            t -= 1;
        }
    }

    /// `ChangeFromTable` : choisit la première colonne affichée.
    pub fn change_from_table(&mut self, table: usize) {
        if table < self.tables.len() && (table != self.from_table || !self.tables[table].displayed) {
            self.change_from_table_forced(table);
        }
    }

    /// Affiche le tableau précédent (plus grand).
    pub fn display_previous(&mut self) {
        if self.from_table + 1 < self.tables.len() {
            self.change_from_table(self.from_table + 1);
        }
    }

    /// Affiche le tableau suivant (plus petit).
    pub fn display_next(&mut self) {
        if self.from_table > 0 {
            self.change_from_table(self.from_table - 1);
        }
    }

    // ------------------------------------------------------------ saisie

    /// Saisie d'un score textuel (`V`, `W3`, `12`…) pour un tireur du match
    /// d'un nœud. Renvoie vrai si la saisie est complète. La phase propage
    /// ensuite le résultat (`TableSupervisor::set_score`).
    pub(crate) fn input_score(&mut self, node: NodeId, fencer: PlayerRef, value: &str) -> bool {
        let Some(m) = self.nodes[node].bout.as_mut() else { return false };
        let complete = m.set_score_str(fencer, value);
        timestamp(m);
        complete
    }

    /// `Stuff` : scores aléatoires pour les matchs du premier tableau affiché.
    pub(crate) fn stuff<R: rand::Rng>(&mut self, rng: &mut R, max_score: u32) {
        if self.root.is_none() || self.from_table == 0 {
            return;
        }
        let target = self.from_table - 1;
        for n in self.post_order() {
            if self.nodes[n].table != target {
                continue;
            }
            let Some(m) = self.nodes[n].bout.as_mut() else { continue };
            let winner = match (m.opponent(0), m.opponent(1)) {
                (Some(a), Some(b)) => {
                    let w = if rng.gen::<bool>() {
                        m.set_score(a, max_score, true);
                        m.set_score(b, rng.gen_range(0..max_score.max(1)), false);
                        a
                    } else {
                        m.set_score(a, rng.gen_range(0..max_score.max(1)), false);
                        m.set_score(b, max_score, true);
                        b
                    };
                    timestamp(m);
                    Some(w)
                }
                _ => m.winner(),
            };
            if let (Some(w), Some(parent)) = (winner, self.nodes[n].parent) {
                let pos = self.child_position(n);
                let m = self.nodes[n].bout.clone().unwrap();
                if let Some(pm) = self.nodes[parent].bout.as_mut() {
                    put_opponent(pm, pos, Some(w));
                    adjust_roadmap(pm, &m);
                }
            }
        }
        self.refresh_table_status();
    }

    /// Affectation de piste/heure/arbitres, ou effacement si `None`.
    pub(crate) fn apply_roadmap(&mut self, node: NodeId, roadmap: Option<&Roadmap>) {
        let Some(m) = self.nodes[node].bout.as_mut() else { return };
        m.referees.clear();
        match roadmap {
            Some(r) => {
                m.start_time = r.start_time;
                m.duration_span = r.duration_span;
                m.piste = r.piste;
                for referee in &r.referees {
                    add_referee(m, *referee);
                }
            }
            None => {
                m.piste = 0;
                m.start_time = None;
            }
        }
    }

    /// `Table::ClearRoadmaps`.
    pub(crate) fn clear_roadmaps(&mut self, table: usize) {
        for n in self.tables[table].match_list.clone() {
            if let Some(m) = self.nodes[n].bout.as_mut() {
                m.piste = 0;
                m.start_time = None;
                m.duration_span = 1;
                m.referees.clear();
            }
        }
    }

    pub(crate) fn set_max_score(&mut self, max_score: u32) {
        for n in &mut self.nodes {
            if let Some(m) = n.bout.as_mut() {
                m.max_score = max_score;
            }
        }
    }

    // ------------------------------------------------------- classement

    /// `Table::GetLoosers` : battus d'un tableau dans l'ordre du tableau
    /// (`None` pour une place vide), abandons, exclus, nombre de battus.
    pub(crate) fn loosers(&self, table: usize, env: &Env) -> (Vec<Option<PlayerRef>>, Vec<PlayerRef>, Vec<PlayerRef>, u32) {
        let mut loosers = Vec::new();
        let mut withdrawals = Vec::new();
        let mut blackcardeds = Vec::new();
        let mut count = 0;
        for n in &self.tables[table].match_list {
            let Some(m) = self.nodes[*n].bout.as_ref() else { continue };
            let looser = m.looser();
            match looser {
                Some(l) if m.is_dropped() => {
                    match status_of(env.players, l, env.owner) {
                        Some('A') => withdrawals.push(l),
                        Some('E') => blackcardeds.push(l),
                        _ => {}
                    }
                    loosers.push(None);
                }
                _ => {
                    if looser.is_some() {
                        count += 1;
                    }
                    loosers.push(looser);
                }
            }
        }
        (simplify_looser_tree(loosers), withdrawals, blackcardeds, count)
    }

    /// Exclus (carton noir) de tous les tableaux (`GetBlackcardeds`).
    pub(crate) fn blackcardeds(&self, env: &Env) -> Vec<PlayerRef> {
        let mut out = Vec::new();
        for t in 0..self.tables.len() {
            out.extend(self.loosers(t, env).2);
        }
        out
    }

    fn compare_player(&self, env: &Env, a: PlayerRef, b: PlayerRef, best: &HashMap<PlayerRef, usize>) -> Ordering {
        if let (Some(sa), Some(sb)) = (status_of(env.players, a, env.owner), status_of(env.players, b, env.owner)) {
            if sa == 'E' && sa != sb {
                return Ordering::Greater;
            }
            if sb == 'E' && sb != sa {
                return Ordering::Less;
            }
        }
        if let (Some(ta), Some(tb)) = (best.get(&a), best.get(&b)) {
            if ta != tb {
                return self.tables[*tb].number.cmp(&self.tables[*ta].number);
            }
        }
        compare_players(env.players, a, b, &env.previous_rank, 0)
    }

    /// Trie des abandons avant de les ranger dans la suite.
    pub(crate) fn sort_withdrawals(&self, env: &Env, list: &mut [PlayerRef]) {
        let best = HashMap::new();
        list.sort_by(|a, b| self.compare_player(env, *a, *b, &best));
    }

    /// `GetCurrentClassification` : classement de la suite et rang de chacun.
    /// `fourth_place_fenced` : la place 4 est tirée (match pour la 3e place).
    pub(crate) fn classification(&self, env: &Env, fourth_place_fenced: bool) -> Vec<(PlayerRef, u32)> {
        if self.root.is_none() {
            return Vec::new();
        }
        let mut list: Vec<PlayerRef> = Vec::new();
        let mut best: HashMap<PlayerRef, usize> = HashMap::new();
        for n in self.level_order() {
            if let Some(w) = self.nodes[n].bout.as_ref().and_then(|m| m.winner()) {
                if !list.contains(&w) {
                    list.push(w);
                    best.insert(w, self.nodes[n].table);
                }
            }
        }
        list.sort_by(|a, b| self.compare_player(env, *a, *b, &best));
        list.extend(self.withdrawals.iter().copied());

        let place_is_fenced = |place: u32| self.relative_place(place) != 4 || self.tables.len() <= 2 || fourth_place_fenced;
        let mut out = Vec::with_capacity(list.len());
        let mut previous: Option<PlayerRef> = None;
        let mut previous_rank = 0;
        for (i, p) in list.iter().enumerate() {
            let place = self.first_place + i as u32;
            let tie = previous.map(|q| self.compare_player(env, *p, q, &best) == Ordering::Equal).unwrap_or(false);
            if tie || !place_is_fenced(place) {
                out.push((*p, previous_rank));
            } else {
                out.push((*p, place));
                previous_rank = place;
            }
            previous = Some(*p);
        }
        out
    }

    // ------------------------------------------------------ recherche rapide

    /// Recherche rapide : tableaux (du plus grand au plus petit) et leurs
    /// matchs dont les deux tireurs sont connus.
    pub fn quick_search(&self, players: &PlayerStore) -> Vec<QuickSearchTable> {
        let mut out = Vec::new();
        for t in (1..self.tables.len()).rev() {
            let mut matches = Vec::new();
            let mut nodes: Vec<NodeId> = (0..self.nodes.len())
                .filter(|n| self.nodes[*n].table + 1 == t && self.nodes[*n].bout.as_ref().map(|m| m.number).unwrap_or(0) != 0)
                .collect();
            nodes.sort_by_key(|n| self.nodes[*n].bout.as_ref().map(|m| m.number));
            for n in nodes {
                let m = self.nodes[n].bout.as_ref().unwrap();
                if let (Some(a), Some(b)) = (m.opponent(0), m.opponent(1)) {
                    let name = |r| players.get(r).map(|p| p.name()).unwrap_or_default();
                    matches.push(QuickSearchMatch {
                        node: n,
                        name: m.name.clone(),
                        label: format!("{} / {}", name(a), name(b)),
                        path: self.match_path(n).unwrap_or_default(),
                    });
                }
            }
            out.push(QuickSearchTable { table: t, image: self.tables[t].image(), matches });
        }
        out
    }

    /// `OnGotoMatch` : match prêt du premier tableau affiché portant ce numéro.
    pub fn goto_match(&self, number: u32) -> Option<NodeId> {
        let t = self.from_table;
        (0..self.nodes.len()).find(|n| {
            self.nodes[*n].table + 1 == t
                && self.nodes[*n]
                    .bout
                    .as_ref()
                    .map(|m| m.number == number && m.opponent(0).is_some() && m.opponent(1).is_some())
                    .unwrap_or(false)
        })
    }

    // ---------------------------------------------------------------- XML

    /// En-tête `<SuiteDeTableaux>` (laissé ouvert).
    pub fn save_header(&self, w: &mut XmlWriter) {
        w.start("SuiteDeTableaux");
        w.attr("ID", &self.id);
        w.attr("Titre", &self.name());
        if !self.tables.is_empty() {
            w.attr_fmt("NbDeTableaux", self.tables.len() - 1);
        }
    }

    /// En-tête `<Tableau>` (laissé ouvert). `defeated_id` : identifiant de
    /// la suite des battus.
    pub fn save_table_header(&self, t: usize, w: &mut XmlWriter, defeated_id: Option<&str>) {
        let table = &self.tables[t];
        w.start("Tableau");
        w.attr("ID", &format!("A{}", table.size));
        w.attr("NetID", &format!("{:x}", table.net_id));
        w.attr("Titre", &table.image());
        w.attr_fmt("Taille", table.size);
        if let Some(id) = defeated_id {
            w.attr("DestinationDesElimines", id);
        }
    }

    pub(crate) fn save(&self, players: &PlayerStore, w: &mut XmlWriter, set_id: &dyn Fn(usize) -> String) {
        self.save_header(w);
        if self.root.is_some() {
            for t in (1..self.tables.len()).rev() {
                let defeated = self.tables[t].defeated_table_set.map(set_id);
                self.save_table_header(t, w, defeated.as_deref());
                for n in &self.tables[t].match_list {
                    if let Some(m) = self.nodes[*n].bout.as_ref() {
                        m.save(players, w);
                    }
                }
                w.end();
            }
        }
        w.end();
    }

    /// `LoadNode` : lit les `<Tableau>` d'une `<SuiteDeTableaux>`.
    pub(crate) fn load_tables(&mut self, node: roxmltree::Node, env: &Env) {
        for n in node.children().filter(|n| n.is_element()) {
            if n.tag_name().name() != "Tableau" {
                return;
            }
            let Some(size) = n.attribute("Taille") else { continue };
            let size = crate::attribute::parse_leading_int(size) as u32;
            if let Some(t) = self.table_of_size(size) {
                self.load_table(t, n, env);
            }
        }
    }

    fn load_table(&mut self, t: usize, node: roxmltree::Node, env: &Env) {
        if let Some(v) = node.attribute("NetID") {
            self.tables[t].net_id = u32::from_str_radix(v, 16).unwrap_or(0);
        }
        for n in node.children().filter(|n| n.is_element()) {
            if n.tag_name().name() != "Match" {
                return;
            }
            let Some(id) = n.attribute("ID") else { continue };
            let number = crate::attribute::parse_leading_int(id) as u32;
            let found = self.tables[t]
                .match_list
                .iter()
                .copied()
                .find(|x| self.nodes[*x].bout.as_ref().map(|m| m.number) == Some(number));
            if let Some(x) = found {
                let m = self.nodes[x].bout.as_mut().unwrap();
                if let Some(v) = n.attribute("NetID") {
                    m.net_id = u32::from_str_radix(v, 16).unwrap_or(0);
                }
                load_match(m, n, env);
            }
        }
    }
}

/// `Table::LoadMatch` : horaires, arbitres et scores d'un `<Match>`.
pub(crate) fn load_match(m: &mut Match, node: roxmltree::Node, env: &Env) {
    use crate::xml::attr_u32;
    if let Some(v) = node.attribute("Piste") {
        m.piste = crate::attribute::parse_leading_int(v) as u32;
    }
    if let (Some(d), Some(t)) = (node.attribute("Date"), node.attribute("Heure")) {
        m.start_time = StartTime::parse(d, t);
    }
    if node.attribute("Duree").is_some() {
        m.duration_sec = attr_u32(node, "Duree", 0);
    }
    if node.attribute("Portee").is_some() {
        m.duration_span = attr_u32(node, "Portee", 1);
    }
    let mut a: Option<roxmltree::Node> = None;
    for n in node.children().filter(|n| n.is_element()) {
        let tag = n.tag_name().name();
        if tag == "Arbitre" {
            if let Some(r) = n.attribute("REF") {
                let r = crate::attribute::parse_leading_int(r) as PlayerRef;
                if env.players.get(r).map(|p| p.class == PlayerClass::Referee).unwrap_or(false) {
                    add_referee(m, r);
                }
            }
        } else if tag == env.player_tag {
            match a {
                None => a = Some(n),
                Some(an) => {
                    let lookup = |x: roxmltree::Node| {
                        x.attribute("REF")
                            .map(|r| crate::attribute::parse_leading_int(r) as PlayerRef)
                            .filter(|r| env.short_list.contains(r))
                    };
                    if let (Some(fa), Some(fb)) = (lookup(an), lookup(n)) {
                        m.opponents[0].fencer = Some(fa);
                        m.load_fencer(an, fa);
                        m.opponents[1].fencer = Some(fb);
                        m.load_fencer(n, fb);
                        m.after_load();
                        put_opponent(m, 0, Some(fa));
                        put_opponent(m, 1, Some(fb));
                    }
                    return;
                }
            }
        }
    }
}

impl TableSet {
    /// Renumérotation des tireurs par le pointage.
    pub(crate) fn remap_fencers(&mut self, map: &HashMap<PlayerRef, PlayerRef>) {
        let tr = |r: &mut PlayerRef| {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        };
        self.attendees.iter_mut().flatten().for_each(tr);
        self.withdrawals.iter_mut().for_each(tr);
        for node in &mut self.nodes {
            if let Some(b) = node.bout.as_mut() {
                for o in &mut b.opponents {
                    if let Some(f) = o.fencer.as_mut() {
                        tr(f);
                    }
                }
            }
        }
    }
}

/// `Table::SimplifyLooserTree` : retire les branches vides en haut et en
/// bas de la liste des battus pour obtenir un tableau compact.
pub fn simplify_looser_tree(list: Vec<Option<PlayerRef>>) -> Vec<Option<PlayerRef>> {
    let mut simplified: Vec<Option<PlayerRef>> = Vec::new();
    let mut i = 0;
    while i < list.len() {
        let even = list[i];
        let odd = list.get(i + 1).copied();
        if even.is_some() || odd.flatten().is_some() {
            simplified = list[i..].to_vec();
            if odd.is_some() && even.is_none() {
                simplified.swap(0, 1);
            }
            break;
        }
        i += 2;
    }
    if let Some(ultimate) = simplified.iter().rposition(|x| x.is_some()) {
        simplified.truncate(ultimate + 1);
        if simplified.len() % 2 != 0 {
            simplified.insert(ultimate, None);
        }
    }
    simplified
}
