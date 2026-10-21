//! Un tableau de 2^n places (port de `rounds/tableau/table.cpp`).
//!
//! Dans une suite de tableaux, le tableau d'indice `t` a `2^t` places : `t = 0`
//! est la case du vainqueur, `t = 1` la finale, etc. Un tableau de taille `S`
//! « gère » les `S/2` matchs de ce tour : ce sont les matchs portés par les
//! nœuds du tableau de taille `S/2` (le tableau de droite).

use crate::i18n::tr;

use super::common::{new_net_id, undivadable};
use super::table_set::NodeId;

#[derive(Debug, Clone)]
pub struct Table {
    pub(crate) size: u32,
    pub(crate) number: u32,
    pub(crate) column: u32,
    pub(crate) displayed: bool,
    pub(crate) header_displayed: bool,
    /// Nœuds de ce tableau, de haut en bas (`_node_table`).
    pub(crate) nodes: Vec<NodeId>,
    /// Matchs gérés, triés par numéro (`_match_list`).
    pub(crate) match_list: Vec<NodeId>,
    pub(crate) is_over: bool,
    pub(crate) ready_to_fence: bool,
    pub(crate) first_error: Option<NodeId>,
    /// Suite de tableaux qui reçoit les battus de ce tableau (`DestinationDesElimines`).
    pub(crate) defeated_table_set: Option<usize>,
    pub(crate) net_id: u32,
}

impl Table {
    pub(crate) fn new(size: u32, number: u32) -> Self {
        Table {
            size,
            number,
            column: number,
            displayed: true,
            header_displayed: false,
            nodes: Vec::new(),
            match_list: Vec::new(),
            is_over: false,
            ready_to_fence: false,
            first_error: None,
            defeated_table_set: None,
            net_id: new_net_id(),
        }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    /// Numéro du tableau : 0 pour le plus grand, `nb_tables - 1` pour la case du vainqueur.
    pub fn number(&self) -> u32 {
        self.number
    }

    /// Colonne d'affichage (0 = première colonne affichée).
    pub fn column(&self) -> u32 {
        self.column
    }

    pub fn is_displayed(&self) -> bool {
        self.displayed
    }

    /// Faux pour la dernière colonne affichée (pas d'en-tête ni de score).
    pub fn is_header_displayed(&self) -> bool {
        self.header_displayed
    }

    pub fn is_over(&self) -> bool {
        self.is_over
    }

    /// Au moins un match prêt à être tiré (deux tireurs connus, non terminé).
    pub fn ready_to_fence(&self) -> bool {
        self.ready_to_fence
    }

    pub fn has_error(&self) -> bool {
        self.first_error.is_some()
    }

    pub fn net_id(&self) -> u32 {
        self.net_id
    }

    pub fn set_net_id(&mut self, id: u32) {
        self.net_id = id;
    }

    /// Indice (dans la compétition) de la suite qui reçoit les battus.
    pub fn defeated_table_set(&self) -> Option<usize> {
        self.defeated_table_set
    }

    /// Nœuds du tableau, de haut en bas.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Nœuds dont le match est géré par ce tableau, par numéro de match.
    pub fn matches(&self) -> &[NodeId] {
        &self.match_list
    }

    /// Nom court `T<taille>` (`GetMiniName`).
    pub fn mini_name(&self) -> String {
        format!("T{}", self.size)
    }

    /// Nom affiché : « Vainqueur », « Finale », « Demi-finale » ou
    /// « Tableau de N » (espaces insécables, `GetImage`).
    pub fn image(&self) -> String {
        let image = match self.size {
            1 => tr("Winner"),
            2 => tr("Final"),
            4 => tr("Semi-final"),
            s => tr("Table of %d").replace("%d", &s.to_string()),
        };
        undivadable(&image)
    }

    /// Annonce de fin de tableau (`GetAnnounce`).
    pub fn announce(&self) -> String {
        format!("{} {}", self.image(), tr("OVER"))
    }

    /// Ligne d'affichage d'un nœud (`GetRow`) : la ligne 0 est la première
    /// sous l'en-tête.
    pub fn row(&self, index: u32) -> u32 {
        let first_index_row = (1u32 << self.column) - 1;
        let delta_row = 2u32 << self.column;
        first_index_row + index * delta_row
    }

    pub(crate) fn show(&mut self, column: u32, header: bool) {
        self.displayed = true;
        self.header_displayed = header;
        self.column = column;
    }

    pub(crate) fn hide(&mut self) {
        self.displayed = false;
        self.header_displayed = false;
    }

    pub(crate) fn add_node(&mut self, node: NodeId) {
        if (self.nodes.len() as u32) < self.size {
            self.nodes.push(node);
        }
    }
}
