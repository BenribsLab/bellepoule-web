//! Classement général (port de `rounds/classification/general_classification.cpp`
//! et de `supervisor/classification.cpp`).
//!
//! Dernière phase d'une formule : chaque présent du pointage reçoit un rang
//! final (`final_rank`). Les qualifiés de la phase précédente prennent leur
//! rang dans cette phase ; les éliminés ont reçu le leur au fil des phases
//! (`Schedule::give_eliminated_a_final_rank`). Les exclus (carton noir) et
//! les tireurs exportés vers une autre compétition sont retirés, et les
//! rangs suivants sont décalés d'autant.
//!
//! Le calcul a besoin des présents du pointage et des rangs de la phase
//! précédente : `Schedule` les fournit par `set_context` puis appelle
//! `refresh` (qui écrit les rangs finaux).

pub mod fff;

use std::any::Any;
use std::collections::HashMap;

use crate::attribute::Value;
use crate::contest::players::sort_refs;
use crate::filter::Filter;
use crate::player::{AttrId, PlayerRef};
use crate::stage::{RankEntry, Stage, StageBase, StageContext, StageKind};
use crate::xml::XmlWriter;

/// Fabrique (signature fixe, appelée par `rounds::create_stage`).
pub fn new_stage(kind: StageKind, id: u32) -> Box<dyn Stage> {
    Box::new(GeneralClassificationStage::new(kind, id))
}

/// Formats d'export proposés (`ExportType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportType {
    Pdf,
    Fff,
    Html,
    Frd,
}

impl ExportType {
    /// Extension (minuscules) et libellé du filtre de fichiers (msgid).
    pub fn pattern(self) -> (&'static str, &'static str) {
        match self {
            ExportType::Fff => ("fff", "All FFF files (.FFF)"),
            ExportType::Pdf => ("pdf", "All PDF files (.PDF)"),
            ExportType::Html => ("html", "All HTML files (.HTML)"),
            ExportType::Frd => ("frd", "All AskFred files (.FRD)"),
        }
    }
}

const EXCLUDED: &[&str] = &[
    "ref",
    "plugin_ID",
    "incident",
    "IP",
    "password",
    "cyphered_password",
    "HS",
    "attending",
    "exported",
    "global_status",
    "indice",
    "level",
    "workload_rate",
    "pool_nr",
    "stage_start_rank",
    "promoted",
    "rank",
    "status",
    "team",
    "victories_count",
    "bouts_count",
    "victories_ratio",
    "strip",
    "time",
];

pub struct GeneralClassificationStage {
    base: StageBase,
    /// Colonnes affichées (même filtre pour la liste et le classement).
    pub filter: Filter,
    /// Présents du pointage (`_attendees->GetPresents ()`).
    presents: Vec<PlayerRef>,
    /// Rangs des qualifiés dans la phase précédente.
    previous_ranks: HashMap<PlayerRef, u32>,
    /// Classement calculé par `refresh`.
    result: Vec<PlayerRef>,
}

impl GeneralClassificationStage {
    pub fn new(kind: StageKind, id: u32) -> Self {
        let mut filter = Filter::excluding("GeneralClassification", EXCLUDED);
        for c in ["final_rank", "name", "first_name", "club", "league", "region", "country"] {
            filter.show_attribute(c);
        }
        let mut base = StageBase::new(kind, id);
        base.filter = filter.selected_codes();
        base.classification_filter = filter.selected_codes();
        GeneralClassificationStage { base, filter, presents: Vec::new(), previous_ranks: HashMap::new(), result: Vec::new() }
    }

    /// Données venant des autres phases : présents du pointage et rang de
    /// chaque participant (qualifié) dans la phase précédente.
    pub fn set_context(&mut self, presents: Vec<PlayerRef>, previous_ranks: HashMap<PlayerRef, u32>) {
        self.presents = presents;
        self.previous_ranks = previous_ranks;
    }

    /// `GiveShortListAFinalRank` puis `GetCurrentClassification` : écrit les
    /// rangs finaux et mémorise le classement.
    pub fn refresh(&mut self, ctx: &mut StageContext) {
        let final_rank = AttrId::global("final_rank");
        for r in &self.base.attendees {
            if let (Some(p), Some(rank)) = (ctx.players.get_mut(*r), self.previous_ranks.get(r)) {
                p.set_uint(&final_rank, *rank);
            }
        }
        let mut full = self.presents.clone();
        full.retain(|r| ctx.players.get(*r).is_some());
        sort_refs(ctx.players, &mut full, &final_rank, self.base.rand_seed);
        let fencer_count = full.len() as u32;
        let mut excluded = 0u32;
        let mut result = Vec::new();
        for r in full {
            let p = ctx.players.get_mut(r).unwrap();
            if p.attr("exported").map(Value::as_bool).unwrap_or(false) {
                excluded += 1;
                continue;
            }
            let Some(status) = p.str_attr("global_status").map(str::to_string) else { continue };
            if status.starts_with('E') {
                excluded += 1;
                continue;
            }
            let value = match p.get(&final_rank).map(Value::as_uint) {
                Some(v) => v.wrapping_sub(excluded),
                None => fencer_count - excluded,
            };
            p.set_uint(&final_rank, value);
            result.push(r);
        }
        self.result = result;
    }

    /// Classement final (après `refresh`).
    pub fn result(&self) -> &[PlayerRef] {
        &self.result
    }

    /// Nom de fichier proposé pour un export (`OnExportToolbuttonClicked`) :
    /// nom par défaut de la compétition sans `.cotcot`, suffixé du nom de la
    /// phase pour le HTML.
    pub fn export_file_name(default_file_name: &str, export: ExportType) -> String {
        let stem = default_file_name.strip_suffix(".cotcot").unwrap_or(default_file_name);
        let modifier = if export == ExportType::Html { crate::i18n::tr("General classification") } else { String::new() };
        format!("{stem}{modifier}.{}", export.pattern().0)
    }

    /// Décalage de places saisi pour l'export FFF (le C++ retranche 1).
    pub fn place_shifting_from_entry(text: &str) -> u32 {
        let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
        crate::attribute::parse_leading_int(&digits).max(0) as u32
    }
}

impl Stage for GeneralClassificationStage {
    fn base(&self) -> &StageBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut StageBase {
        &mut self.base
    }

    fn load(&mut self, _ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), crate::Error> {
        self.base.load_configuration(node);
        self.filter.load(node, "");
        self.filter.load(node, "Classement");
        self.base.filter = self.filter.selected_codes();
        self.base.classification_filter = self.filter.selected_codes();
        Ok(())
    }

    /// `<ClassementGeneral PhaseID ID [NetID] Affichage AffichageClassement/>`.
    fn save(&self, _ctx: &StageContext, w: &mut XmlWriter) {
        w.start(StageKind::GeneralClassification.xml_name());
        self.save_configuration(w);
        w.end();
    }

    fn save_configuration(&self, w: &mut XmlWriter) {
        let b = &self.base;
        w.attr_fmt("PhaseID", b.id);
        w.attr("ID", &b.name);
        if b.net_id != 0 {
            w.attr("NetID", &format!("{:x}", b.net_id));
        }
        if let Some(n) = b.nb_qualified {
            w.attr_fmt("NbQualifiesParIndice", n);
        }
        self.filter.save(w, "");
        self.filter.save(w, "Classement");
    }

    fn is_over(&self, _ctx: &StageContext) -> bool {
        true
    }

    fn classification(&self, ctx: &StageContext) -> Vec<RankEntry> {
        let final_rank = AttrId::global("final_rank");
        self.result
            .iter()
            .map(|r| RankEntry {
                player: *r,
                rank: ctx.players.get(*r).and_then(|p| p.get(&final_rank)).map(Value::as_uint).unwrap_or(0),
                status: 'Q',
            })
            .collect()
    }

    fn reset(&mut self, ctx: &mut StageContext) {
        let _ = ctx;
        self.result.clear();
    }

    fn remap_players(&mut self, map: &HashMap<PlayerRef, PlayerRef>) {
        let tr = |r: &mut PlayerRef| {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        };
        self.base.attendees.iter_mut().for_each(tr);
        self.presents.iter_mut().for_each(tr);
        self.result.iter_mut().for_each(tr);
        self.previous_ranks = self.previous_ranks.drain().map(|(k, v)| (*map.get(&k).unwrap_or(&k), v)).collect();
    }

    fn dump_to_html(&self, ctx: &StageContext, html: &mut String) {
        crate::contest::html::players_table_with_borders(
            html,
            ctx.players,
            &self.result,
            &self.filter,
            self.base.owner(),
            crate::contest::html::is_power_of_two_border,
        );
    }

    fn has_its_own_ranking(&self) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
