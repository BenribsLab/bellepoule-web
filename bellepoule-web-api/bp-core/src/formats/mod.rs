//! Formats d'import/export : FFF, XML FIE, CSV, classements, AskFred, HTML,
//! ainsi que la persistance annexe (JSON, configuration utilisateur) et le
//! chargement des valeurs discrètes depuis `resources/`.
//!
//! Tout est indépendant du module `contest` (porté en parallèle) : les
//! imports produisent des `Player` prêts à être ajoutés à un `PlayerStore`
//! (voir [`PlayerList`]) et les exports prennent un [`ContestHeader`], un
//! `PlayerStore` et des listes ordonnées de références.
//!
//! Sources C++ : `actors/checkin.cpp` (ImportFFF, ImportCSV,
//! GuessPlayerOrganization), `supervisor/classification.cpp` (DumpToFFF),
//! `supervisor/rounds/checkin/rank_importer.cpp`, `supervisor/askfred/*`,
//! `common/util/{xml_scheme,json_file,user_config,attribute_desc}.cpp`,
//! `supervisor/contest.cpp` (DumpToHTML, DumpToFRD, en-tête XML),
//! `actors/players_list.cpp` (DumpToHTML).

pub mod archive;
pub mod askfred;
pub mod csv;
pub mod discrete;
pub mod encoding;
pub mod fff;
pub mod fie_xml;
pub mod header;
pub mod html;
pub mod json_file;
pub mod key_file;
pub mod ranking;
pub mod scheme;
pub mod team;
pub mod user_config;

pub use discrete::{DiscreteCatalog, DiscreteItem};
pub use header::ContestHeader;
pub use team::{LoadedPlayer, PlayerList};
