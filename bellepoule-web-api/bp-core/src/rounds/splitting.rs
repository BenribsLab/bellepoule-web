//! Point de scission (port de `rounds/splitting/splitting.cpp`) : partage
//! des tireurs en divisions.
//!
//! Chaque tireur reste dans la compétition (division 0) ou part dans une
//! division (1, 2…) ; au verrouillage, l'application crée une nouvelle
//! compétition par division utilisée (`exported_players`), les autres
//! continuent avec leur rang précédent. L'attribut global `exported` (case
//! « exporté » de BellePoule) suit : vrai pour un tireur qui part.
//!
//! Les règles de la formule ([`SplitConfig`]) préremplissent la répartition
//! à l'entrée dans la phase (au classement, ou selon la catégorie et l'année
//! de naissance) ; elle reste modifiable à la main.

use std::any::Any;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::player::{AttrId, Owner, PlayerRef, PlayerStore};
use crate::rounds::table::common::{self, codes, compare_players};
use crate::stage::{PreviousStage, RankEntry, Stage, StageBase, StageContext, StageKind};

use crate::xml::XmlWriter;

/// Nombre de places au classement : fixe ou en pourcentage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    Count(u32),
    Percent(u32),
}

impl Size {
    /// Places sur `total` (pourcentage arrondi au plus proche).
    pub fn places(self, total: usize) -> usize {
        match self {
            Size::Count(n) => (n as usize).min(total),
            Size::Percent(p) => ((total * p.min(100) as usize + 50) / 100).min(total),
        }
    }
}

/// Critère d'une division.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Criterion {
    /// Une des catégories générales (calculées d'après l'année de naissance).
    Category { values: Vec<String> },
    /// Né une de ces années.
    BirthYears { years: Vec<i32> },
    /// Né cette année ou avant.
    BornBefore { year: i32 },
    /// Né cette année ou après.
    BornFrom { year: i32 },
}

impl Criterion {
    fn matches(&self, players: &PlayerStore, r: PlayerRef) -> bool {
        let Some(p) = players.get(r) else { return false };
        match self {
            // Catégorie générale, calculée d'après la date de naissance.
            Criterion::Category { values } => {
                let c = crate::categories::general_category(p.str_attr("birth_date")).unwrap_or_default().to_lowercase();
                !c.is_empty() && values.iter().any(|v| v.trim().to_lowercase() == c)
            }
            Criterion::BirthYears { years } => birth_year(p.str_attr("birth_date")).is_some_and(|y| years.contains(&y)),
            Criterion::BornBefore { year } => birth_year(p.str_attr("birth_date")).is_some_and(|y| y <= *year),
            Criterion::BornFrom { year } => birth_year(p.str_attr("birth_date")).is_some_and(|y| y >= *year),
        }
    }
}

pub use crate::categories::birth_year;

/// Façon de préremplir les divisions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitMode {
    /// Les tireurs sont choisis à la main au point de scission.
    #[default]
    Manual,
    /// Au classement : `keep` premiers ici, puis chaque division prend les
    /// suivants (sa taille ; sans taille, tout le reste).
    Ranking,
    /// Chaque tireur va dans la première division dont le critère convient,
    /// sinon il reste.
    Criteria,
}

/// Une division : nom de la compétition créée et règle de remplissage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Division {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Size>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criterion: Option<Criterion>,
}

/// Réglages du point de scission (formule).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitConfig {
    #[serde(default)]
    pub mode: SplitMode,
    /// Mode au classement : places gardées dans cette compétition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<Size>,
    /// Divisions créées (au moins une).
    pub divisions: Vec<Division>,
}

impl Default for SplitConfig {
    fn default() -> Self {
        SplitConfig { mode: SplitMode::Manual, keep: None, divisions: vec![Division { name: "Division 2".into(), ..Default::default() }] }
    }
}

pub struct Splitting {
    base: StageBase,
    previous: Option<PreviousStage>,
    result: Option<Vec<PlayerRef>>,
    /// Réglages (formule).
    pub config: SplitConfig,
    /// Division de chaque tireur qui part (1 = première division).
    assignments: BTreeMap<PlayerRef, u32>,
}

impl Splitting {
    pub fn new(id: u32) -> Self {
        let mut base = StageBase::new(StageKind::Splitting, id);
        base.filter = codes(&[
            "exported",
            "stage_start_rank",
            "name",
            "first_name",
            "birth_date",
            "gender",
            "club",
            "country",
            "licence",
        ]);
        Splitting { base, previous: None, result: None, config: SplitConfig::default(), assignments: BTreeMap::new() }
    }

    fn owner(&self) -> Owner {
        self.base.owner()
    }

    /// Coche ou décoche un tireur à extraire (première division).
    pub fn set_exported(&mut self, ctx: &mut StageContext, r: PlayerRef, value: bool) {
        self.set_division(ctx, r, value as u32);
    }

    /// Division d'un tireur : 0 = il reste dans cette compétition.
    pub fn set_division(&mut self, ctx: &mut StageContext, r: PlayerRef, division: u32) {
        if self.base.locked {
            return;
        }
        let division = division.min(self.config.divisions.len().max(1) as u32);
        if division == 0 {
            self.assignments.remove(&r);
        } else {
            self.assignments.insert(r, division);
        }
        if let Some(p) = ctx.players.get_mut(r) {
            p.set_uint(&AttrId::global("exported"), (division > 0) as u32);
        }
    }

    /// Division d'un tireur (0 = il reste). Un tireur coché sans division
    /// connue (fichier d'une version antérieure) part dans la première.
    pub fn division_of(&self, players: &PlayerStore, r: PlayerRef) -> u32 {
        match self.assignments.get(&r) {
            Some(d) => *d,
            None => exported(players, r) as u32,
        }
    }

    /// Nom d'une division (1 = première), ou « Division n » par défaut.
    pub fn division_name(&self, division: u32) -> String {
        self.config
            .divisions
            .get(division.saturating_sub(1) as usize)
            .map(|d| d.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("Division {}", division + 1))
    }

    /// Catégorie (code des fichiers : « M13 », « S », « V1 »…) d'une division
    /// définie par catégories : la plus âgée de ses catégories.
    pub fn division_category(&self, division: u32) -> Option<String> {
        if self.config.mode != SplitMode::Criteria {
            return None;
        }
        let d = self.config.divisions.get(division.checked_sub(1)? as usize)?;
        let Some(Criterion::Category { values }) = &d.criterion else { return None };
        let model = crate::categories::active();
        let oldest = values
            .iter()
            .filter_map(|v| model.find(v).map(|i| (i, model.categories[i].name.clone())))
            .max_by_key(|(i, _)| *i)
            .map(|(_, name)| name)
            .or_else(|| values.first().cloned())?;
        Some(crate::categories::contest_code(&oldest))
    }

    /// Divisions qui ont au moins un tireur, dans l'ordre.
    pub fn divisions_in_use(&self, players: &PlayerStore) -> Vec<u32> {
        let mut used: Vec<u32> = self.base.attendees.iter().map(|r| self.division_of(players, *r)).filter(|d| *d > 0).collect();
        used.sort_unstable();
        used.dedup();
        used
    }

    /// Libellé du point de scission (liste « pour GREG ») : « N1 » ou « N2 »
    /// prépare deux divisions N1 et N2, « Tableau A » ou « Tableau B » deux
    /// divisions Tableau A et Tableau B (tailles et critères conservés).
    pub fn set_label(&mut self, ctx: &mut StageContext, label: &str) {
        if self.base.locked {
            return;
        }
        self.base.name = label.to_string();
        let pair: Option<[&str; 2]> = match label.trim() {
            "N1" | "N2" => Some(["N1", "N2"]),
            "Tableau A" | "Tableau B" => Some(["Tableau A", "Tableau B"]),
            _ => None,
        };
        let Some(pair) = pair else { return };
        let mut config = self.config.clone();
        config.divisions.resize_with(2, Division::default);
        for (division, name) in config.divisions.iter_mut().zip(pair) {
            division.name = name.to_string();
        }
        if config != self.config {
            self.set_config(ctx, config);
        }
    }

    /// Nouveaux réglages ; la répartition est refaite d'après eux.
    pub fn set_config(&mut self, ctx: &mut StageContext, config: SplitConfig) {
        if self.base.locked {
            return;
        }
        self.config = config;
        if self.config.divisions.is_empty() {
            self.config.divisions = SplitConfig::default().divisions;
        }
        self.apply_rules(ctx);
    }

    /// Préremplit la répartition d'après les règles (sans effet à la main).
    pub fn apply_rules(&mut self, ctx: &mut StageContext) {
        if self.base.locked || self.config.mode == SplitMode::Manual {
            return;
        }
        let order = self.ranked_attendees(ctx.players);
        let mut plan: Vec<(PlayerRef, u32)> = Vec::new();
        match self.config.mode {
            SplitMode::Manual => {}
            SplitMode::Ranking => {
                let total = order.len();
                let mut at = self.config.keep.map(|k| k.places(total)).unwrap_or(0);
                for r in &order[..at.min(total)] {
                    plan.push((*r, 0));
                }
                let last = self.config.divisions.len();
                for (i, d) in self.config.divisions.iter().enumerate() {
                    // La dernière division (ou une sans taille) prend le reste.
                    let rest = total.saturating_sub(at);
                    let take = if i + 1 == last { rest } else { d.size.map(|s| s.places(total)).unwrap_or(rest) };
                    let end = (at + take).min(total);
                    for r in &order[at..end] {
                        plan.push((*r, i as u32 + 1));
                    }
                    at = end;
                }
                for r in &order[at.min(total)..] {
                    plan.push((*r, 0));
                }
            }
            SplitMode::Criteria => {
                for r in &order {
                    let division = self
                        .config
                        .divisions
                        .iter()
                        .position(|d| d.criterion.as_ref().is_some_and(|c| c.matches(ctx.players, *r)))
                        .map(|i| i as u32 + 1)
                        .unwrap_or(0);
                    plan.push((*r, division));
                }
            }
        }
        for (r, d) in plan {
            self.set_division(ctx, r, d);
        }
    }

    /// Participants par rang de la phase précédente.
    fn ranked_attendees(&self, players: &PlayerStore) -> Vec<PlayerRef> {
        let mut list = self.base.attendees.clone();
        list.sort_by_key(|r| {
            let rank = self.previous_rank(players, *r);
            (rank == 0, rank)
        });
        list
    }

    /// Tireurs d'une division, avec leur rang d'entrée (position dans la
    /// liste + 1), comme `OnLocked`. L'application les duplique (attribut
    /// `exported` remis à faux, membres d'équipe compris).
    pub fn exported_players(&self, players: &PlayerStore, division: u32) -> Vec<(PlayerRef, u32)> {
        self.base
            .attendees
            .iter()
            .enumerate()
            .filter(|(_, r)| self.division_of(players, **r) == division)
            .map(|(i, r)| (*r, i as u32 + 1))
            .collect()
    }

    /// Tireurs restants (`GetRemainingList`), par rang de départ.
    pub fn remaining(&self, players: &PlayerStore) -> Vec<PlayerRef> {
        let mut list: Vec<PlayerRef> = self.base.attendees.iter().copied().filter(|r| self.division_of(players, *r) == 0).collect();
        let id = AttrId::local("stage_start_rank", self.owner());
        crate::sort::glib_sort_by(&mut list, |a, b| compare_players(players, *a, *b, &id, self.base.rand_seed));
        list
    }

    fn previous_rank(&self, players: &PlayerStore, r: PlayerRef) -> u32 {
        let id = common::previous_rank_id(self.previous, self.owner());
        players.get(r).and_then(|p| p.get(&id)).map(|v| v.as_uint()).unwrap_or(0)
    }

    /// Nom à imprimer (`GetPrintName`).
    pub fn print_name(&self) -> String {
        format!("{} - {}", tr("Split"), self.base.name)
    }

    fn clear_exported(&mut self, ctx: &mut StageContext) {
        self.assignments.clear();
        for r in &self.base.attendees {
            if let Some(p) = ctx.players.get_mut(*r) {
                p.set_uint(&AttrId::global("exported"), 0);
            }
        }
    }
}

impl Stage for Splitting {
    fn base(&self) -> &StageBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut StageBase {
        &mut self.base
    }

    fn set_previous(&mut self, previous: Option<PreviousStage>) {
        self.previous = previous;
    }

    fn set_attendees(&mut self, ctx: &mut StageContext, attendees: Vec<PlayerRef>) {
        common::retrieve_attendees(&mut self.base, ctx, attendees);
        // Répartition préremplie à l'entrée dans la phase seulement : une
        // répartition déjà faite (relue d'un fichier, corrigée à la main) reste.
        if self.assignments.is_empty() {
            self.apply_rules(ctx);
        }
    }

    fn reset(&mut self, ctx: &mut StageContext) {
        self.clear_exported(ctx);
        self.result = None;
    }

    fn load(&mut self, ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), crate::Error> {
        common::load_configuration(&mut self.base, node);
        if let Some(c) = node.attribute("Divisions").and_then(|v| serde_json::from_str::<SplitConfig>(v).ok()) {
            self.config = c;
        }
        // « Repartition » : « ref:division,ref:division… ».
        self.assignments = node
            .attribute("Repartition")
            .unwrap_or("")
            .split(',')
            .filter_map(|pair| {
                let (r, d) = pair.split_once(':')?;
                Some((r.trim().parse().ok()?, d.trim().parse().ok()?))
            })
            .filter(|(_, d): &(PlayerRef, u32)| *d > 0)
            .collect();
        let attendees = self.base.attendees.clone();
        common::retrieve_attendees(&mut self.base, ctx, attendees);
        Ok(())
    }

    fn save(&self, _ctx: &StageContext, w: &mut XmlWriter) {
        w.start(StageKind::Splitting.xml_name());
        self.save_configuration(w);
        if !self.assignments.is_empty() {
            let list: Vec<String> = self.assignments.iter().map(|(r, d)| format!("{r}:{d}")).collect();
            w.attr("Repartition", &list.join(","));
        }
        w.end();
    }

    fn save_configuration(&self, w: &mut XmlWriter) {
        common::save_configuration(&self.base, w, false);
        if self.config != SplitConfig::default() {
            w.attr("Divisions", &serde_json::to_string(&self.config).unwrap_or_default());
        }
    }

    fn is_over(&self, _ctx: &StageContext) -> bool {
        true
    }

    fn classification(&self, ctx: &StageContext) -> Vec<RankEntry> {
        let result = self.result.clone().unwrap_or_else(|| self.remaining(ctx.players));
        let rank = |r| self.previous_rank(ctx.players, r);
        common::rank_entries(ctx.players, self.owner(), &result, &rank)
    }

    fn output(&self, ctx: &StageContext) -> Vec<PlayerRef> {
        self.result.clone().unwrap_or_else(|| self.remaining(ctx.players))
    }

    fn lock(&mut self, ctx: &mut StageContext) {
        self.base.locked = true;
        let ranks: Vec<(PlayerRef, u32)> =
            self.base.attendees.iter().map(|r| (*r, self.previous_rank(ctx.players, *r))).collect();
        common::write_ranks(self.owner(), ctx, &ranks);
        self.result = Some(self.remaining(ctx.players));
    }

    fn unlock(&mut self, ctx: &mut StageContext) {
        self.result = None;
        self.base.locked = false;
        self.clear_exported(ctx);
    }

    fn remap_players(&mut self, map: &std::collections::HashMap<PlayerRef, PlayerRef>) {
        let tr = |r: &mut PlayerRef| {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        };
        self.base.attendees.iter_mut().for_each(tr);
        self.result.iter_mut().flatten().for_each(tr);
        self.assignments = std::mem::take(&mut self.assignments)
            .into_iter()
            .map(|(r, d)| (map.get(&r).copied().unwrap_or(r), d))
            .collect();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Fabrique des phases de ce module (signature fixe, appelée par `rounds::create_stage`).
pub fn new_stage(kind: StageKind, id: u32) -> Box<dyn Stage> {
    match kind {
        StageKind::Splitting => Box::new(Splitting::new(id)),
        _ => Box::new(crate::stage::RawStage::new(kind, id)),
    }
}

/// Attribut global `exported` (absent = faux).
fn exported(players: &PlayerStore, r: PlayerRef) -> bool {
    players.get(r).and_then(|p| p.attr("exported")).map(|v| v.as_bool()).unwrap_or(false)
}
