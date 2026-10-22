//! BellePoule — cœur métier (port Rust de BellePoule 5.7).
//!
//! Aucune dépendance graphique : tout ce qui est ici est testable en ligne de
//! commande. Voir `ARCHITECTURE.md` à la racine pour le découpage.

pub mod attribute;
pub mod bout;
pub mod officiating;
pub mod relay;
pub mod relay_sheet;
pub mod i18n;
pub mod sort;
pub mod player;
pub mod stage;
pub mod xml;

// --- Modules portés en parallèle (voir ARCHITECTURE.md pour les propriétaires) ---

// Compétition, tournoi, planning, phases génériques
pub mod contest;
pub mod tournament;
pub mod schedule;
pub mod weapon;
pub mod category;
pub mod fie_time;
pub mod filter;
pub mod rounds;

// Formats d'import/export
pub mod formats;
pub mod net;
pub mod group_sheets;
pub mod smartpoule;

pub use player::{Owner, Player, PlayerClass, PlayerRef, PlayerStore};
pub use stage::{Stage, StageContext, StageKind};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("erreur d'entrée/sortie : {0}")]
    Io(#[from] std::io::Error),
    #[error("XML invalide : {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("format non reconnu : {0}")]
    Format(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
