//! En-tête de compétition minimal partagé par les imports et exports.
//!
//! Sous-ensemble des champs de `Contest` (C++) nécessaires aux formats ; le
//! module `contest` le remplira (ou le lira) lors de l'intégration.

use crate::attribute;
use crate::i18n::tr;

/// Sexes de compétition (`Contest::gender_xml_image`).
pub const GENDER_XML_IMAGES: [&str; 3] = ["M", "F", "X"];
/// Alias acceptés à la lecture (`Contest::gender_xml_alias`).
pub const GENDER_XML_ALIASES: [&str; 3] = ["M", "D", "FM"];
/// Libellés (msgid) des sexes (`Contest::gender_image`).
pub const GENDER_IMAGES: [&str; 3] = ["Men", "Women", "Mixed"];
/// Niveaux (`Contest::level_xml_image`).
pub const LEVEL_XML_IMAGES: [&str; 7] = ["L", "Z", "CN", "A", "F", "IE", "X"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContestHeader {
    pub team_event: bool,
    /// `Couleur` (`#rrggbb`).
    pub color: Option<String>,
    /// `Championnat`.
    pub authority: Option<String>,
    /// `ID` (identifiant fédéral de l'épreuve).
    pub fie_id: Option<String>,
    /// `NetID` (hexadécimal).
    pub net_id: Option<u64>,
    /// `TitreLong`.
    pub name: String,
    pub day: u32,
    pub month: u32,
    pub year: u32,
    /// `Appel`, `Scratch`, `Debut` (`hh:mm`).
    pub checkin_time: Option<String>,
    pub scratch_time: Option<String>,
    pub start_time: Option<String>,
    /// Code XML de l'arme (`F`, `E`, `S`, `X`).
    pub weapon: String,
    /// Code XML du sexe (`M`, `F`, `X`).
    pub gender: String,
    /// Image XML de la catégorie (`Categorie`).
    pub category: String,
    /// Image affichée de la catégorie (`Category::GetDisplayImage`) ; à
    /// défaut, `category` est utilisée.
    pub category_display: Option<String>,
    /// `Niveau`.
    pub level: Option<String>,
    pub organizer: String,
    pub source: Option<String>,
    /// `URLorganisateur`.
    pub web_site: String,
    /// `Lieu`.
    pub location: String,
    pub label: String,
    /// `ScoreAleatoire`.
    pub score_stuffing: bool,
    /// `TailleMinimaleEquipe`, `ClassementParDefaut`, `ClassementManuel`.
    pub minimum_team_size: Option<u32>,
    pub default_classification: Option<u32>,
    pub manual_classification: Option<u32>,
    /// Tous les attributs bruts de l'élément racine lu (ordre du fichier).
    pub raw_attributes: Vec<(String, String)>,
}

impl Default for ContestHeader {
    fn default() -> Self {
        ContestHeader {
            team_event: false,
            color: None,
            authority: None,
            fie_id: None,
            net_id: None,
            name: String::new(),
            day: 1,
            month: 1,
            year: 2000,
            checkin_time: None,
            scratch_time: None,
            start_time: None,
            weapon: "E".into(),
            gender: "M".into(),
            category: String::new(),
            category_display: None,
            level: None,
            organizer: String::new(),
            source: None,
            web_site: String::new(),
            location: String::new(),
            label: String::new(),
            score_stuffing: false,
            minimum_team_size: None,
            default_classification: None,
            manual_classification: None,
            raw_attributes: Vec::new(),
        }
    }
}

impl ContestHeader {
    /// `Contest::GetDate` (`%x`, format local français `jj/mm/aaaa`).
    pub fn date_image(&self) -> String {
        format!("{:02}/{:02}/{}", self.day, self.month, self.year)
    }

    /// `Weapon::GetImage` : libellé traduit de l'arme.
    pub fn weapon_image(&self) -> String {
        weapon_image(&self.weapon)
    }

    /// `Contest::GetGender` : libellé traduit du sexe.
    pub fn gender_label(&self) -> String {
        let i = GENDER_XML_IMAGES.iter().position(|g| *g == self.gender).unwrap_or(0);
        tr(GENDER_IMAGES[i])
    }

    pub fn category_image(&self) -> &str {
        self.category_display.as_deref().unwrap_or(&self.category)
    }

    /// Lit le code XML du sexe, alias compris (`Contest::LoadXmlDoc`).
    pub fn parse_gender(xml: &str) -> Option<&'static str> {
        GENDER_XML_IMAGES
            .iter()
            .position(|g| *g == xml)
            .or_else(|| GENDER_XML_ALIASES.iter().position(|g| *g == xml))
            .map(|i| GENDER_XML_IMAGES[i])
    }
}

/// Libellé (msgid) d'une arme à partir de son code XML.
pub fn weapon_msgid(xml: &str) -> &'static str {
    match xml {
        "X" => "Educ",
        "F" => "Foil",
        "S" => "Sabre",
        _ => "Epée",
    }
}

pub fn weapon_image(xml: &str) -> String {
    let desc = attribute::desc("weapon");
    match desc.discrete_from_xml(xml) {
        Some(d) => tr(&d.user),
        None => tr(weapon_msgid(xml)),
    }
}
