//! Import d'un classement (port de `rounds/checkin/rank_importer.cpp`).
//!
//! Formats acceptés : `.cotcot`/`.xml` (`/CompetitionIndividuelle/Tireurs/
//! Tireur` avec `Classement`, `Nom`, `Prenom`) ou texte FFE (ISO-8859-1,
//! lignes `NOM/Prénom/…/rang` après le mot `TIREURS`). Un tireur absent du
//! classement reçoit le pire rang + 1.

use std::collections::HashMap;
use std::path::Path;

use crate::contest::players::divisable;
use crate::player::{AttrId, Player};

#[derive(Debug, Clone, Default)]
pub struct RankImporter {
    table: HashMap<String, u32>,
    worst_rank: u32,
}

fn key(name: &str, first_name: &str) -> String {
    format!("{}:{}", divisable(name).trim_end(), divisable(first_name).trim_end())
}

impl RankImporter {
    /// Charge un fichier (format choisi selon l'extension).
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let name = path.to_string_lossy();
        let bytes = std::fs::read(path)?;
        let mut importer = RankImporter::default();
        if name.ends_with(".cotcot") || name.ends_with(".xml") || name.ends_with(".XML") {
            let text = crate::contest::decode_xml_bytes(&bytes);
            importer.load_xml(&text);
        } else {
            importer.load_txt(&bytes.iter().map(|b| *b as char).collect::<String>());
        }
        Ok(importer)
    }

    fn update_worst_rank(&mut self, rank: u32) {
        if self.worst_rank == 0 || self.worst_rank < rank {
            self.worst_rank = rank;
        }
    }

    /// `LoadTxt` (contenu déjà décodé).
    pub fn load_txt(&mut self, text: &str) {
        let Some(pos) = text.find("TIREURS") else { return };
        let fencers = text.get(pos + "TIREURS".len() + 1..).unwrap_or("");
        for line in fencers.split('\n') {
            let tokens: Vec<&str> = line.split('/').collect();
            if tokens.is_empty() {
                continue;
            }
            let name = tokens[0];
            let first_name = tokens.get(1).copied().unwrap_or("");
            let rank = crate::attribute::parse_leading_int(tokens[tokens.len() - 1]).max(0) as u32;
            self.table.insert(key(name, first_name), rank);
            self.update_worst_rank(rank);
        }
    }

    /// `LoadXml`.
    pub fn load_xml(&mut self, text: &str) {
        let Ok(doc) = crate::xml::parse(text) else { return };
        let root = doc.root_element();
        if root.tag_name().name() != "CompetitionIndividuelle" {
            return;
        }
        let Some(list) = crate::xml::child(root, "Tireurs") else { return };
        for n in list.children().filter(|n| n.is_element()) {
            if n.tag_name().name() != "Tireur" {
                return;
            }
            if let (Some(rank), Some(name), Some(first)) =
                (n.attribute("Classement"), n.attribute("Nom"), n.attribute("Prenom"))
            {
                let rank = crate::attribute::parse_leading_int(rank).max(0) as u32;
                self.table.insert(key(name, first), rank);
                self.update_worst_rank(rank);
            }
        }
    }

    /// Nombre de rangs lus.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// `ModifyRank`.
    pub fn modify_rank(&self, fencer: &mut Player) {
        let (Some(name), Some(first)) = (fencer.str_attr("name"), fencer.str_attr("first_name")) else { return };
        let rank = self.table.get(&key(name, first)).copied().filter(|r| *r != 0).unwrap_or(self.worst_rank + 1);
        fencer.set_uint(&AttrId::global("ranking"), rank);
    }
}
