//! Données de dessin d'une suite de tableaux (remplace la construction des
//! tables GooCanvas de `TableSet::Display`, `FillInNode`, `RefreshNode` et
//! `DrawConnector`).
//!
//! L'interface dessine, pour les tableaux affichés (`from_table` à
//! `to_table`, au plus 4 colonnes) :
//! - une ligne d'en-têtes (nom du tableau, icônes impression et état) ;
//! - une case par nœud : le vainqueur de la case, et à sa droite le score
//!   qu'il a obtenu dans le match suivant (saisissable) ;
//! - à gauche de chaque case (colonne précédente, même ligne) le nom du
//!   match qui la remplit et son affectation (piste, heure, arbitres) ;
//! - les connecteurs (polylignes) entre cases.
//!
//! Les coordonnées sont calculées sur une grille régulière
//! (`LayoutMetrics`) : la ligne `r` d'un nœud est donnée par `Table::row`.

use crate::bout::Match;
use crate::i18n::tr;
use crate::player::{PlayerRef, PlayerStore};

use super::common::undivadable;
use super::table_set::{NodeId, TableSet};

/// Dimensions de la grille de dessin (unités du canevas).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutMetrics {
    pub cell_width: f64,
    pub row_height: f64,
    pub header_height: f64,
    /// Espace entre colonnes (`_table_spacing` = 10).
    pub column_spacing: f64,
}

impl Default for LayoutMetrics {
    fn default() -> Self {
        LayoutMetrics { cell_width: 220.0, row_height: 24.0, header_height: 40.0, column_spacing: 10.0 }
    }
}

/// Rectangle `x1,y1` (haut gauche) – `x2,y2` (bas droit).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bounds {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Icône d'état d'un tableau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableStatus {
    /// Terminé (`GTK_STOCK_APPLY`).
    Over,
    /// En cours (`GTK_STOCK_EXECUTE`).
    Running,
    /// Un match sans vainqueur (`GTK_STOCK_DIALOG_WARNING`).
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeaderCell {
    pub table: usize,
    pub column: u32,
    /// Nom du tableau (« Tableau de 16 »…).
    pub image: String,
    pub bounds: Bounds,
    /// Icône d'impression des feuilles de match (tableaux de taille > 1).
    pub print_icon: bool,
    /// Icône d'état (tableaux de taille > 1).
    pub status: Option<TableStatus>,
}

/// Affectation affichée à côté du nom de match (`TableZone::PutRoadmapInTable`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoadmapText {
    /// « Piste 3 @ 14:20 » (espaces insécables).
    pub text: String,
    /// Arbitres : référence et « NOM Prénom ».
    pub referees: Vec<(PlayerRef, String)>,
}

/// Libellé du match qui remplit une case, dessiné dans la colonne de gauche.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchLabel {
    pub bounds: Bounds,
    /// Nom du match (vide pour un match avec exempt).
    pub name: String,
    pub roadmap: Option<RoadmapText>,
}

/// Case de score : score du vainqueur de la case dans le match suivant.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreSlot {
    pub bounds: Bounds,
    /// Nœud du match suivant (celui dont on saisit le score).
    pub bout_node: NodeId,
    pub fencer: Option<PlayerRef>,
    /// Texte (`V`, `V4`, `3`, vide).
    pub text: String,
    /// Abandon (`A`) ou exclusion (`E`) : icône de statut.
    pub drop_reason: Option<char>,
    /// Faux tant que le match suivant n'a pas ses deux tireurs.
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeCell {
    pub node: NodeId,
    pub table: usize,
    pub column: u32,
    /// Ligne de grille (0 = première ligne sous les en-têtes).
    pub row: u32,
    pub bounds: Bounds,
    /// Tireur qui occupe la case.
    pub winner: Option<PlayerRef>,
    /// Vrai si le tableau de gauche est affiché : n'afficher que nom et prénom.
    pub compact: bool,
    /// Icône d'impression de la feuille de match (match prêt, non terminé).
    pub print_icon: bool,
    pub match_label: Option<MatchLabel>,
    pub score: Option<ScoreSlot>,
}

/// Trait de liaison entre cases.
#[derive(Debug, Clone, PartialEq)]
pub struct Connector {
    pub node: NodeId,
    pub points: Vec<(f64, f64)>,
    pub line_width: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableSetDrawing {
    pub headers: Vec<HeaderCell>,
    pub cells: Vec<NodeCell>,
    pub connectors: Vec<Connector>,
    pub width: f64,
    pub height: f64,
}

impl TableSetDrawing {
    pub fn cell(&self, node: NodeId) -> Option<&NodeCell> {
        self.cells.iter().find(|c| c.node == node)
    }
}

/// Texte d'affectation d'un match (`Piste N @ hh:mm`), si piste et heure connues.
pub fn roadmap_text(m: &Match, players: &PlayerStore) -> Option<RoadmapText> {
    let start = m.start_time?;
    if m.piste == 0 {
        return None;
    }
    let text = format!("{} {}\u{a0}@\u{a0}{}", tr("Piste"), m.piste, start.0.format("%k:%M"));
    let referees = m
        .referees
        .iter()
        .map(|r| {
            let name = players
                .get(*r)
                .map(|p| {
                    let first = p.first_name();
                    if first.is_empty() { p.name() } else { format!("{} {}", p.name(), first) }
                })
                .unwrap_or_default();
            (*r, undivadable(&name))
        })
        .collect();
    Some(RoadmapText { text: undivadable(&text), referees })
}

impl TableSet {
    /// Variante pour l'application : les arbitres vivent dans un magasin
    /// distinct, dont les références peuvent coïncider avec celles des tireurs.
    pub fn drawing_with_referees(&self, players: &PlayerStore, referees: &PlayerStore, metrics: &LayoutMetrics) -> TableSetDrawing {
        let mut drawing = self.drawing(players, metrics);
        for cell in &mut drawing.cells {
            if let Some(label) = &mut cell.match_label {
                label.roadmap = self.bout(cell.node).and_then(|m| roadmap_text(m, referees));
            }
        }
        drawing
    }

    /// Rectangle de la case d'un nœud affiché.
    fn cell_bounds(&self, node: NodeId, metrics: &LayoutMetrics) -> Option<Bounds> {
        let n = &self.nodes[node];
        let table = &self.tables[n.table];
        if !table.displayed || n.bout.is_none() {
            return None;
        }
        Some(grid_bounds(table.column, table.row(n.table_index), metrics))
    }

    /// Construit toutes les données de dessin des tableaux affichés.
    pub fn drawing(&self, players: &PlayerStore, metrics: &LayoutMetrics) -> TableSetDrawing {
        let mut d = TableSetDrawing::default();
        if self.root.is_none() || self.tables.is_empty() {
            return d;
        }
        let pitch = metrics.cell_width + metrics.column_spacing;

        for (t, table) in self.tables.iter().enumerate() {
            if table.header_displayed {
                let status = (table.size > 1).then(|| {
                    if table.first_error.is_some() {
                        TableStatus::Error
                    } else if !table.is_over {
                        TableStatus::Running
                    } else {
                        TableStatus::Over
                    }
                });
                d.headers.push(HeaderCell {
                    table: t,
                    column: table.column,
                    image: table.image(),
                    bounds: Bounds {
                        x1: table.column as f64 * pitch,
                        y1: 0.0,
                        x2: table.column as f64 * pitch + metrics.cell_width,
                        y2: metrics.header_height,
                    },
                    print_icon: table.size > 1,
                    status,
                });
            }
        }

        for node in self.post_order() {
            let n = &self.nodes[node];
            let Some(bounds) = self.cell_bounds(node, metrics) else { continue };
            let m = n.bout.as_ref().unwrap();
            let table = &self.tables[n.table];
            let winner = m.winner();
            let left_displayed = self.tables.get(n.table + 1).map(|t| t.displayed).unwrap_or(false);

            let match_label = (table.column > 0).then(|| {
                let name = if m.number == 0 || m.exempted() { String::new() } else { m.name.clone() };
                MatchLabel {
                    bounds: Bounds { x1: bounds.x1 - pitch, y1: bounds.y1, x2: bounds.x1 - metrics.column_spacing, y2: bounds.y2 },
                    name,
                    roadmap: roadmap_text(m, players),
                }
            });

            let score = match n.parent {
                Some(parent) if table.header_displayed => {
                    let pm = self.nodes[parent].bout.as_ref();
                    let mut slot = ScoreSlot {
                        bounds: Bounds { x1: bounds.x2 - 45.0, y1: bounds.y1, x2: bounds.x2, y2: bounds.y1 + 20.0 },
                        bout_node: parent,
                        fencer: winner,
                        text: String::new(),
                        drop_reason: None,
                        visible: false,
                    };
                    if let Some(pm) = pm {
                        if m.is_over() && pm.opponent(0).is_some() && pm.opponent(1).is_some() {
                            slot.visible = true;
                            if let Some(score) = winner.and_then(|w| pm.score(w)) {
                                slot.text = score.image(pm.max_score);
                                slot.drop_reason = score.drop_reason();
                            }
                        }
                    }
                    Some(slot)
                }
                _ => None,
            };

            d.cells.push(NodeCell {
                node,
                table: n.table,
                column: table.column,
                row: table.row(n.table_index),
                bounds,
                winner,
                compact: left_displayed,
                print_icon: !m.is_over() && m.opponent(0).is_some() && m.opponent(1).is_some(),
                match_label,
                score,
            });
        }

        // Connecteurs (`DrawConnector`)
        let half = metrics.column_spacing / 2.0;
        for node in self.post_order() {
            let n = &self.nodes[node];
            let Some(b) = self.cell_bounds(node, metrics) else { continue };
            let m = n.bout.as_ref().unwrap();
            let table = &self.tables[n.table];
            if n.parent.is_none() || !table.header_displayed {
                d.connectors.push(Connector { node, points: vec![(b.x1 - half, b.y2), (b.x2, b.y2)], line_width: 1.7 });
            } else if !n.children.is_empty() || m.is_over() {
                let parent = n.parent.unwrap();
                if let Some(pb) = self.cell_bounds(parent, metrics) {
                    d.connectors.push(Connector {
                        node,
                        points: vec![(b.x1 - half, b.y2), (pb.x1 - half, b.y2), (pb.x1 - half, pb.y2)],
                        line_width: 1.7,
                    });
                } else {
                    let siblings = &self.nodes[parent].children;
                    let pos = siblings.iter().position(|c| *c == node).unwrap_or(0);
                    let sibling = siblings.get(pos + 1).or_else(|| pos.checked_sub(1).and_then(|p| siblings.get(p)));
                    if let Some(sb) = sibling.and_then(|s| self.cell_bounds(*s, metrics)) {
                        d.connectors.push(Connector {
                            node,
                            points: vec![(b.x1 - half, b.y2), (b.x2, b.y2), (b.x2, sb.y2)],
                            line_width: 2.0,
                        });
                    }
                }
            }
        }

        let columns = self.tables.iter().filter(|t| t.displayed).count() as f64;
        let rows = self.tables[self.from_table].size as f64 * 2.0 - 1.0;
        d.width = (columns * pitch - metrics.column_spacing).max(0.0);
        d.height = metrics.header_height + rows.max(1.0) * metrics.row_height;
        d
    }

    /// Rectangle d'un nœud affiché (pour l'impression et le défilement).
    pub fn node_bounds(&self, node: NodeId, metrics: &LayoutMetrics) -> Option<Bounds> {
        self.cell_bounds(node, metrics)
    }
}

fn grid_bounds(column: u32, row: u32, metrics: &LayoutMetrics) -> Bounds {
    let x1 = column as f64 * (metrics.cell_width + metrics.column_spacing);
    let y1 = metrics.header_height + row as f64 * metrics.row_height;
    Bounds { x1, y1, x2: x1 + metrics.cell_width, y2: y1 + metrics.row_height }
}
