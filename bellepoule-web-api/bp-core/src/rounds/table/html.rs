//! Export HTML d'une suite de tableaux (port de `html_table.cpp` et de
//! `TableSet::DumpToHTML`). La grille reprend les tableaux affichés.

use std::fmt::Write;

use crate::attribute::{self, Look};
use crate::player::{AttrId, PlayerRef, PlayerStore};

use super::table_set::TableSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HtmlCell {
    Empty,
    Connector,
    Bout(usize),
}

/// Grille `HtmlTable` : une colonne par tableau, `2^n - 1` lignes.
struct HtmlTable {
    width: usize,
    height: usize,
    cells: Vec<HtmlCell>,
}

impl HtmlTable {
    fn new(column_count: usize) -> Self {
        let height = (1usize << column_count) - 1;
        HtmlTable { width: column_count, height, cells: vec![HtmlCell::Empty; height * column_count] }
    }

    fn put(&mut self, node: usize, row: usize, column: usize) {
        if row < self.height && column < self.width {
            self.cells[row * self.width + column] = HtmlCell::Bout(node);
        }
    }

    fn connect(&mut self, row1: usize, row2: usize, col2: usize) {
        let (a, b) = if row1 > row2 { (row2, row1) } else { (row1, row2) };
        for r in a + 1..b {
            if r < self.height && col2 < self.width {
                self.cells[r * self.width + col2] = HtmlCell::Connector;
            }
        }
    }
}

fn span(out: &mut String, players: &PlayerStore, fencer: PlayerRef, code: &str, look: Look) {
    let Some(p) = players.get(fencer) else { return };
    let Some(v) = p.get(&AttrId::global(code)) else { return };
    let xml = v.xml_image();
    let image = match attribute::registry().by_code(code) {
        Some(desc) if desc.has_discrete_value() && look == Look::ShortText => xml,
        Some(desc) if !desc.discrete.is_empty() => desc.user_image(&xml, look),
        _ => xml.replace(' ', "\u{a0}"),
    };
    let _ = write!(out, "<span class=\"{code}\">{image} </span>");
}

impl TableSet {
    /// Fragment HTML de la suite (`TableSet::DumpToHTML`).
    pub fn dump_to_html(&self, players: &PlayerStore) -> String {
        let mut out = String::new();
        out.push_str("        <div>\n");
        out.push_str("          <table class=\"TableTable\">\n");
        out.push_str("            <tr class=\"TableName\">\n");
        for t in (0..self.tables.len()).rev() {
            let _ = writeln!(out, "              <th>{}</th>", self.tables[t].image());
        }
        out.push_str("            </tr>\n");
        out.push_str("            <tr>\n");
        out.push_str("              <th></th>\n");
        out.push_str("            </tr>\n");

        if !self.tables.is_empty() {
            let mut grid = HtmlTable::new(self.tables.len());
            let mut position = std::collections::HashMap::new();
            for n in self.post_order() {
                let node = &self.nodes[n];
                let table = &self.tables[node.table];
                if node.bout.is_some() && table.displayed {
                    let row = table.row(node.table_index) as usize;
                    grid.put(n, row, table.column as usize);
                    position.insert(n, (row, table.column as usize));
                }
            }
            for n in self.post_order() {
                let node = &self.nodes[n];
                let Some(&(row, _)) = position.get(&n) else { continue };
                let table = &self.tables[node.table];
                let Some(parent) = node.parent else { continue };
                if !table.header_displayed {
                    continue;
                }
                let over = node.bout.as_ref().map(|m| m.is_over()).unwrap_or(false);
                if !node.children.is_empty() || over {
                    if let Some(&(prow, pcol)) = position.get(&parent) {
                        grid.connect(row, prow, pcol);
                    }
                }
            }
            for r in 0..grid.height {
                out.push_str("            <tr>\n");
                for c in 0..grid.width {
                    match grid.cells[r * grid.width + c] {
                        HtmlCell::Connector => out.push_str("              <td class=\"TableConnector\">|</td>\n"),
                        HtmlCell::Empty => out.push_str("              <td class=\"EmptyTableCell\"></td>\n"),
                        HtmlCell::Bout(n) => {
                            let m = self.nodes[n].bout.as_ref().unwrap();
                            let class = if c == 0 {
                                "TableCellFirstCol"
                            } else if c == grid.width - 1 {
                                "TableCellLastCol"
                            } else {
                                "TableCell"
                            };
                            let _ = write!(out, "              <td class=\"{class}\">");
                            if m.number != 0 {
                                let _ = write!(
                                    out,
                                    "<span class=\"TableScore\">{}-{}</span>",
                                    m.score_at(0).image(m.max_score),
                                    m.score_at(1).image(m.max_score)
                                );
                            }
                            if let Some(w) = m.winner() {
                                span(&mut out, players, w, "name", Look::LongText);
                                span(&mut out, players, w, "first_name", Look::LongText);
                                span(&mut out, players, w, "country", Look::ShortText);
                            }
                            out.push_str("</td>\n");
                        }
                    }
                }
                out.push_str("            </tr>\n");
            }
        }

        out.push_str("          </table>\n");
        out.push_str("        </div>\n");
        out
    }
}
