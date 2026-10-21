//! Échanges avec AskFred (fichiers `.frd`, http://www.askfred.net) :
//! lecture des pré-inscriptions et écriture des résultats.

pub mod reader;
pub mod writer;

/// Espace de noms des fichiers AskFred.
pub const NAMESPACE: &str = "http://www.askfred.net";

pub use reader::{AskFredFile, Club, Competitor, Event};
pub use writer::{dump_to_frd, AskFredScheme, FrdInput};
