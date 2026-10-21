//! Phase de tableaux à élimination directe (port de `rounds/tableau/`).
//!
//! - [`TableSupervisor`] : la phase (`PhaseDeTableaux`), implémente `Stage` ;
//! - [`TableSet`] : une suite de tableaux (principale ou de classement) ;
//! - [`Table`] : un tableau de 2^n places ;
//! - [`layout`] : données de dessin (cases, scores, connecteurs) ;
//! - [`print`] : pagination des tableaux et des feuilles de match ;
//! - export HTML : [`TableSet::dump_to_html`].
//!
//! Utilisation typique par l'interface : `garnish`, puis pour la suite
//! affichée `table_set(i).drawing(...)`, saisie par
//! `TableSupervisor::set_score` / `set_fencer_status`, et relecture du
//! modèle après chaque action.

pub mod common;
pub mod html;
pub mod layout;
pub mod print;
pub mod supervisor;
pub mod table;
pub mod table_set;

pub use layout::{Bounds, Connector, LayoutMetrics, NodeCell, TableSetDrawing};
pub use print::{Page, PrintSession, PrintType, ScoreSheet, SheetCompositor};
pub use supervisor::{FencedPlaces, TableSetStatus, TableSupervisor};
pub use table::Table;
pub use table_set::{simplify_looser_tree, Node, NodeId, QuickSearchMatch, QuickSearchTable, Roadmap, TableSet};

use crate::stage::{RawStage, Stage, StageKind};

/// Fabrique des phases de ce module (signature fixe, appelée par `rounds::create_stage`).
pub fn new_stage(kind: StageKind, id: u32) -> Box<dyn Stage> {
    match kind {
        StageKind::TableSupervisor => Box::new(TableSupervisor::new(id)),
        _ => Box::new(RawStage::new(kind, id)),
    }
}
