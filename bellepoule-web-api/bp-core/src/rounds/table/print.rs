//! Impression des tableaux et des feuilles de match (port de
//! `sheet_compositor.cpp`, `table_print_session.cpp` et de la partie
//! calcul de `TableSet::PreparePrint`/`DrawPage`).
//!
//! Le dessin lui-même est fait par l'interface ; on fournit ici la
//! pagination, les zones de canevas à imprimer et le contenu des feuilles.

use std::cmp::Ordering;

use crate::bout::Match;
use crate::i18n::tr;
use crate::player::{PlayerRef, PlayerStore};

use super::layout::{Bounds, LayoutMetrics};
use super::table_set::{compare_matches, NodeId, TableSet};

// ------------------------------------------------------------------ feuilles

/// Une page de feuilles de match (au plus 4 emplacements).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    /// Matchs de la page (`None` = emplacement vide).
    pub slots: Vec<Option<NodeId>>,
    /// Taille des groupes ajoutés (0 pour une page remplie d'un bloc).
    pub accepted_size: usize,
}

impl Page {
    /// `FillWith` : prend jusqu'à 4 matchs, complète avec des vides.
    fn fill_with(&mut self, what: &mut Vec<NodeId>) {
        if self.slots.is_empty() {
            for _ in 0..4 {
                if what.is_empty() {
                    self.slots.push(None);
                } else {
                    self.slots.push(Some(what.remove(0)));
                }
            }
        }
    }

    /// `Append` : ajoute un groupe de même taille si la place le permet.
    fn append(&mut self, what: &[NodeId]) -> bool {
        if (self.accepted_size == 0 || self.accepted_size == what.len()) && self.slots.len() + what.len() <= 4 {
            self.slots.extend(what.iter().map(|n| Some(*n)));
            self.accepted_size = what.len();
            return true;
        }
        false
    }

    /// Matchs à dessiner, dans l'ordre (s'arrête au premier emplacement vide).
    pub fn matches(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.slots.iter().map_while(|s| *s)
    }

    /// Trait de découpe à dessiner après l'emplacement `i` (`CutterMarkReached`).
    pub fn cutter_after(&self, i: usize) -> bool {
        self.accepted_size != 0 && i + 1 < self.slots.len() && (i + 1) % self.accepted_size == 0
    }
}

/// Regroupe les feuilles de match par arbitre (`SheetCompositor`).
#[derive(Debug, Clone, Default)]
pub struct SheetCompositor {
    by_referee: Vec<Vec<NodeId>>,
    pages: Vec<Page>,
    match_per_sheet: usize,
}

impl SheetCompositor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.by_referee.clear();
        self.pages.clear();
    }

    /// `AddMatch` : range le match dans le groupe de son premier arbitre.
    pub fn add_match(&mut self, set: &TableSet, node: NodeId) {
        let Some(m) = set.bout(node) else { return };
        let referee = m.referees.first().copied();
        let mut list = Vec::new();
        if let Some(i) = self
            .by_referee
            .iter()
            .position(|g| set.bout(g[0]).and_then(|f| f.referees.first().copied()) == referee)
        {
            list = self.by_referee.remove(i);
        }
        // g_list_insert_sorted : avant le premier élément >= au nouveau
        let pos = list
            .iter()
            .position(|x| compare_matches(m, set.bout(*x).unwrap()) != Ordering::Greater)
            .unwrap_or(list.len());
        list.insert(pos, node);
        self.by_referee.insert(0, list);
    }

    /// `GetPageCount` : 2 feuilles par page en paysage, 4 en portrait.
    pub fn compose(&mut self, landscape: bool) -> usize {
        self.match_per_sheet = if landscape { 2 } else { 4 };
        self.pages.clear();
        self.by_referee.sort_by(|a, b| b.len().cmp(&a.len()));
        for group in &self.by_referee {
            let mut matchs = group.clone();
            if matchs.len() >= 4 {
                while !matchs.is_empty() {
                    let mut page = Page::default();
                    page.fill_with(&mut matchs);
                    self.pages.push(page);
                }
            } else {
                if self.pages.iter_mut().any(|p| p.append(&matchs)) {
                    continue;
                }
                let mut page = Page::default();
                page.append(&matchs);
                self.pages.push(page);
            }
        }
        self.pages.len()
    }

    pub fn match_per_sheet(&self) -> usize {
        self.match_per_sheet
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }
}

/// Contenu d'une feuille de match (`TableSet::DrawPage`, cas feuilles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreSheet {
    pub node: NodeId,
    /// Nom du match (`1-16.4`).
    pub name: String,
    /// Référence du code QR.
    pub flash_ref: Option<String>,
    /// Libellés « Arbitre », « Piste », « Signature ».
    pub referee_label: String,
    pub piste_label: String,
    pub signature_label: String,
    /// Premier arbitre : nom et prénom.
    pub referee: Option<(String, String)>,
    /// « 03 @ 14:20 » si la piste est connue.
    pub piste: Option<String>,
    pub fencers: [Option<PlayerRef>; 2],
    /// Nombre de cases de touches (si score max ≤ 15, `GetScoreTable`).
    pub score_boxes: Option<u32>,
}

impl TableSet {
    /// Variante avec le magasin d'arbitres séparé de celui des tireurs.
    pub fn score_sheet_with_referees(&self, node: NodeId, players: &PlayerStore, referees: &PlayerStore, contest_net_id: u32, stage_net_id: u32) -> Option<ScoreSheet> {
        let mut sheet = self.score_sheet(node, players, contest_net_id, stage_net_id)?;
        sheet.referee = self.bout(node)?.referees.first().and_then(|r| referees.get(*r)).map(|p| (p.name(), p.first_name()));
        Some(sheet)
    }
    /// `LookForMatchToPrint` : matchs prêts (deux tireurs) d'un tableau (ou
    /// de tous), éventuellement seulement ceux pas encore imprimés.
    pub fn matches_to_print(&self, table: Option<usize>, all_sheets: bool) -> Vec<NodeId> {
        let mut out = Vec::new();
        for t in (1..self.tables.len()).rev() {
            if table.is_some_and(|x| x != t) {
                continue;
            }
            let mut nodes: Vec<NodeId> = self.tables[t].match_list.clone();
            nodes.sort_by_key(|n| self.nodes[*n].bout.as_ref().map(|m| m.number));
            for n in nodes {
                let m = self.nodes[n].bout.as_ref().unwrap();
                if m.opponent(0).is_some() && m.opponent(1).is_some() && (all_sheets || !self.nodes[n].printed) {
                    out.push(n);
                }
            }
        }
        out
    }

    /// Marque une feuille comme imprimée.
    pub fn mark_printed(&mut self, node: NodeId) {
        if let Some(n) = self.nodes.get_mut(node) {
            n.printed = true;
        }
    }

    /// Contenu d'une feuille de match.
    pub fn score_sheet(&self, node: NodeId, players: &PlayerStore, contest_net_id: u32, stage_net_id: u32) -> Option<ScoreSheet> {
        let m: &Match = self.bout(node)?;
        let referee = m.referees.first().and_then(|r| players.get(*r)).map(|p| (p.name(), p.first_name()));
        let piste = (m.piste != 0).then(|| {
            let time = m.start_time.map(|t| t.0.format("%k:%M").to_string()).unwrap_or_default();
            format!("{:02}\u{a0}@\u{a0}{}", m.piste, time)
        });
        Some(ScoreSheet {
            node,
            name: m.name.clone(),
            flash_ref: self.flash_ref(node, contest_net_id, stage_net_id),
            referee_label: tr("Referee"),
            piste_label: tr("Piste"),
            signature_label: tr("Signature"),
            referee,
            piste,
            fencers: [m.opponent(0), m.opponent(1)],
            score_boxes: (m.max_score <= 15).then_some(m.max_score),
        })
    }
}

// ---------------------------------------------------------------- tableaux

/// Type d'impression (`PrintSession::Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintType {
    ScoreSheets,
    DisplayedTables,
    AllTables,
}

/// Session d'impression des tableaux : découpe du canevas en pages et
/// échelle (`Table::PrintSession`).
#[derive(Debug, Clone, PartialEq)]
pub struct PrintSession {
    pub kind: PrintType,
    /// Premier tableau imprimé.
    pub from_table: Option<usize>,
    /// Titre (balisage Pango) de la section.
    pub title: String,
    bounds: Vec<Bounds>,
    cutting_w: f64,
    cutting_h: f64,
    scale: f64,
    page_w: f64,
    page_h: f64,
    mini_header: Bounds,
    canvas_bounds: Bounds,
    current_page: usize,
    source_resolution: f64,
    target_resolution: f64,
}

impl PrintSession {
    /// `stage_full_name` : `GetFullName` de la phase. Le titre cite le
    /// premier et le dernier (au plus 2 plus loin) tableau imprimé.
    pub fn new(kind: PrintType, stage_full_name: &str, set: Option<&TableSet>, from_table: Option<usize>) -> Self {
        let mut title = String::new();
        if let (Some(set), Some(from)) = (set, from_table) {
            let to = from.saturating_sub(2);
            title = format!(
                "{}\n\n<span size=\"xx-small\">{}\n...\n{}</span>",
                stage_full_name,
                set.tables[from].image(),
                set.tables[to].image()
            );
        }
        PrintSession {
            kind,
            from_table,
            title,
            bounds: Vec::new(),
            cutting_w: 0.0,
            cutting_h: 0.0,
            scale: 1.0,
            page_w: 0.0,
            page_h: 0.0,
            mini_header: Bounds::default(),
            canvas_bounds: Bounds::default(),
            current_page: 0,
            source_resolution: 1.0,
            target_resolution: 1.0,
        }
    }

    pub fn begin(&mut self, nb_pages: usize) {
        self.cutting_w = 0.0;
        self.cutting_h = 0.0;
        self.bounds = vec![Bounds::default(); nb_pages];
    }

    pub fn nb_pages(&self) -> usize {
        self.bounds.len()
    }

    pub fn set_resolutions(&mut self, source: f64, target: f64) {
        self.source_resolution = source;
        self.target_resolution = target;
    }

    /// Adaptation de résolution (cible / source).
    pub fn dpi_adaptation(&self) -> f64 {
        self.target_resolution / self.source_resolution
    }

    pub fn set_page_bounds(&mut self, page: usize, b: Bounds) {
        if page < self.bounds.len() {
            self.bounds[page] = b;
            self.cutting_w = self.cutting_w.max(b.x2 - b.x1);
            self.cutting_h = self.cutting_h.max(b.y2 - b.y1);
        }
    }

    pub fn page_bounds(&self, page: usize) -> Option<Bounds> {
        self.bounds.get(page).copied()
    }

    /// `SetPaperSize` : échelle (en-tête réduit limité à 3 % de la hauteur).
    pub fn set_paper_size(&mut self, paper_w: f64, paper_h: f64, header_h_on_paper: f64) {
        let mini_header_h = self.bounds.first().map(|b| b.y1).unwrap_or(0.0);
        let x_scale = paper_w * 0.95 / self.cutting_w;
        let y_scale = (paper_h - header_h_on_paper) / (mini_header_h + self.cutting_h);
        let max_scale = (3.0 * paper_h) / (100.0 * mini_header_h);
        self.scale = x_scale.min(y_scale).min(max_scale);
        self.page_w = paper_w / self.scale;
        self.page_h = paper_h / self.scale;
        self.mini_header = Bounds { x1: 0.0, x2: self.page_w, y1: 0.0, y2: mini_header_h };
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn process_current_page(&mut self, page: usize) {
        self.current_page = page;
        if let Some(b) = self.bounds.get(page) {
            self.canvas_bounds = Bounds { x1: 0.0, x2: self.page_w, y1: b.y1, y2: b.y2 };
        }
    }

    /// Zone de canevas de l'en-tête (noms des tableaux), répétée sur chaque page.
    pub fn mini_header_bounds(&self) -> Bounds {
        self.mini_header
    }

    /// Zone de canevas de la page courante.
    pub fn canvas_bounds(&self) -> Bounds {
        self.canvas_bounds
    }

    /// Décalage vertical à appliquer pour la page courante.
    pub fn paper_y_shift(&self) -> f64 {
        -self.bounds.get(self.current_page).map(|b| b.y1).unwrap_or(0.0)
    }
}

impl TableSet {
    /// Sections du livret de résultats : une par groupe de 3 tableaux
    /// (`GetBookSections`), du plus grand au plus petit.
    pub fn book_sections(&self, stage_full_name: &str) -> Vec<PrintSession> {
        let nb = self.tables.len();
        let nb_section = nb / 3 + usize::from(nb % 3 != 0);
        (0..nb_section)
            .map(|i| PrintSession::new(PrintType::AllTables, stage_full_name, Some(self), Some((nb - 1) - i * 3)))
            .collect()
    }

    /// `PreparePrint` (impression des tableaux) : affiche la section
    /// demandée, découpe le canevas en pages de `nb_row` cases du premier
    /// tableau (32 par défaut, 8 ou 16 selon le choix « 4 »/« 8 » matchs) et
    /// calcule l'échelle. Renvoie le nombre de pages.
    pub fn prepare_print(
        &mut self,
        session: &mut PrintSession,
        nb_row: u32,
        metrics: &LayoutMetrics,
        paper_w: f64,
        paper_h: f64,
        header_h_on_paper: f64,
    ) -> usize {
        if session.kind == PrintType::ScoreSheets || self.tables.is_empty() {
            return 0;
        }
        if session.kind == PrintType::AllTables {
            if let Some(from) = session.from_table {
                self.change_from_table(from);
            }
        }
        let from = &self.tables[self.from_table];
        if from.size <= 1 {
            return 0;
        }
        let nb_page = (from.size / nb_row + u32::from(from.size % nb_row != 0)) as usize;
        session.begin(nb_page);
        if self.tables[self.to_table].size > from.size {
            return 0;
        }
        let e_node = self.tables[self.to_table].nodes.first().copied();
        let e_bounds = self.get_bounds(e_node, e_node, metrics);
        for i in 0..nb_page {
            let mut nw = None;
            let mut sw = None;
            for n in i as u32 * nb_row..(i as u32 + 1) * nb_row {
                if let Some(node) = from.nodes.get(n as usize).copied() {
                    if self.node_bounds(node, metrics).is_some() {
                        if nw.is_none() {
                            nw = Some(node);
                        }
                        sw = Some(node);
                    }
                }
            }
            let w = self.get_bounds(nw, sw, metrics);
            let b = Bounds { x1: w.x1, y1: w.y1, x2: e_bounds.x2, y2: w.y2 };
            session.set_page_bounds(i, b);
        }
        session.set_paper_size(paper_w, paper_h, header_h_on_paper);
        nb_page
    }

    fn get_bounds(&self, top: Option<NodeId>, bottom: Option<NodeId>, metrics: &LayoutMetrics) -> Bounds {
        let mut b = Bounds::default();
        if let Some(top) = top {
            let x = self.node_bounds(top, metrics).or_else(|| bottom.and_then(|n| self.node_bounds(n, metrics)));
            if let Some(x) = x {
                b = x;
            }
        }
        if let Some(bottom) = bottom {
            let x = self.node_bounds(bottom, metrics).or_else(|| top.and_then(|n| self.node_bounds(n, metrics)));
            if let Some(x) = x {
                b.x1 = x.x1;
                b.x2 = x.x2;
                b.y2 = x.y2;
            }
        }
        b
    }
}
