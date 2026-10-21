//! Fichiers de classement appliqués à la liste d'appel
//! (`People::RankImporter`).
//!
//! - `.cotcot` / `.xml` : liste `Tireurs/Tireur` (`Nom`, `Prenom`,
//!   `Classement`) ;
//! - autres (`.txt`, export Engarde « CLASSEMENT ») : texte ISO-8859-1, la
//!   section commençant après `TIREURS` contient une ligne par tireur
//!   `NOM/Prénom/…/rang` (le rang est le dernier champ).
//!
//! Les tireurs absents du fichier reçoivent le pire rang + 1.
//!
//! Écart assumé : le C++ ne lit que la racine `CompetitionIndividuelle` ;
//! les fichiers FFE de préparation (`BaseCompetitionIndividuelle`, comme
//! ceux de `tests-data/Ranking-Classements`) sont aussi acceptés ici.

use std::collections::HashMap;
use std::path::Path;

use crate::formats::encoding::{atoi, decode_xml, latin1_decode, parse_xml, undivadable};
use crate::player::{AttrId, Player};
use crate::Result;

#[derive(Debug, Clone, Default)]
pub struct RankTable {
    ranks: HashMap<String, u32>,
    worst_rank: u32,
}

fn key(name: &str, first_name: &str) -> String {
    format!("{}:{}", undivadable(name), undivadable(first_name))
}

impl RankTable {
    /// Choix du lecteur selon l'extension (`DisplayChooser`).
    pub fn load_file(path: &Path) -> Result<RankTable> {
        let bytes = std::fs::read(path)?;
        let name = path.to_string_lossy();
        if name.ends_with(".cotcot") || name.ends_with(".xml") || name.ends_with(".XML") {
            RankTable::from_xml(&bytes)
        } else {
            Ok(RankTable::from_txt(&bytes))
        }
    }

    pub fn len(&self) -> usize {
        self.ranks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranks.is_empty()
    }

    pub fn worst_rank(&self) -> u32 {
        self.worst_rank
    }

    fn insert(&mut self, name: &str, first_name: &str, rank: u32) {
        self.ranks.insert(key(name, first_name), rank);
        if self.worst_rank == 0 || self.worst_rank < rank {
            self.worst_rank = rank;
        }
    }

    /// `RankImporter::LoadTxt`.
    pub fn from_txt(bytes: &[u8]) -> RankTable {
        let mut table = RankTable::default();
        let text = latin1_decode(bytes);
        let Some(pos) = text.find("TIREURS") else { return table };
        let start = (pos + "TIREURS".len() + 1).min(text.len());
        for line in text[start..].split('\n') {
            let tokens: Vec<&str> = line.split('/').collect();
            if line.is_empty() {
                continue;
            }
            // Le C++ formate un prénom absent en « (null) ».
            let name = tokens[0];
            let first = tokens.get(1).copied().unwrap_or("(null)");
            let rank = atoi(tokens[tokens.len() - 1]).max(0) as u32;
            table.insert(name, first, rank);
        }
        table
    }

    /// `RankImporter::LoadXml`.
    pub fn from_xml(bytes: &[u8]) -> Result<RankTable> {
        let text = decode_xml(bytes)?;
        let doc = parse_xml(&text)?;
        let mut table = RankTable::default();
        let root = doc.root_element();
        if !matches!(root.tag_name().name(), "CompetitionIndividuelle" | "BaseCompetitionIndividuelle") {
            return Ok(table);
        }
        if let Some(list) = crate::xml::child(root, "Tireurs") {
            table.load_list(list);
        }
        Ok(table)
    }

    fn load_list(&mut self, node: roxmltree::Node) {
        let mut current = Some(node);
        while let Some(n) = current {
            if n.is_element() {
                match n.tag_name().name() {
                    "Tireurs" => {}
                    "Tireur" => {
                        if let (Some(rank), Some(name), Some(first)) = (n.attribute("Classement"), n.attribute("Nom"), n.attribute("Prenom")) {
                            self.insert(name, first, atoi(rank).max(0) as u32);
                        }
                    }
                    _ => return,
                }
            }
            if let Some(c) = n.first_child() {
                self.load_list(c);
            }
            current = n.next_sibling();
        }
    }

    /// Rang d'un tireur, ou pire rang + 1.
    pub fn rank_of(&self, name: &str, first_name: &str) -> u32 {
        match self.ranks.get(&key(name, first_name)) {
            Some(r) if *r != 0 => *r,
            _ => self.worst_rank + 1,
        }
    }

    /// `RankImporter::ModifyRank` : met à jour `ranking` si le tireur a un nom
    /// et un prénom.
    pub fn modify_rank(&self, fencer: &mut Player) {
        let (Some(name), Some(first)) = (fencer.attr("name"), fencer.attr("first_name")) else { return };
        let rank = self.rank_of(&name.xml_image(), &first.xml_image());
        fencer.set_uint(&AttrId::global("ranking"), rank);
    }
}
