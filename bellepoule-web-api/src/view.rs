//! Représentation des données BellePoule pour le client : textes affichés,
//! listes de valeurs (mêmes listes que l'application de bureau).

use bp_core::attribute::{self, AttrType, AttributeDesc, Look, Rights, Uniqueness, Value};
use bp_core::formats::discrete;
use bp_core::i18n::tr;
use bp_core::player::{AttrId, Owner, Player};
use serde::Serialize;

/// Valeur d'un attribut du joueur (`owner` pour les attributs locaux à une phase).
pub fn value_of<'a>(player: &'a Player, code: &str, owner: Owner) -> Option<&'a Value> {
    let local = attribute::registry().by_code(code).map(|d| d.scope == attribute::Scope::Local).unwrap_or(false);
    if local {
        player.get(&AttrId::local(code, owner))
    } else {
        player.get(&AttrId::global(code))
    }
}

/// Texte affiché d'une valeur (`GetUserImage`).
pub fn user_image(desc: &AttributeDesc, value: &Value, look: Look) -> String {
    match value {
        Value::Str(s) => {
            let look = if look == Look::Graphical { Look::LongText } else { look };
            if desc.has_discrete_value() {
                discrete::attribute_user_image(desc, value, look)
            } else if look == Look::ShortText {
                s.chars().take(desc.short_length as usize).collect()
            } else {
                s.clone()
            }
        }
        Value::UInt(v) => v.to_string(),
        Value::Int(v) => v.to_string(),
        Value::Bool(b) => if *b { "✓".into() } else { String::new() },
    }
}

/// Texte affiché d'un attribut d'un joueur.
pub fn cell(player: &Player, code: &str, owner: Owner) -> String {
    let Some(desc) = attribute::registry().by_code(code) else { return String::new() };
    value_of(player, code, owner).map(|v| user_image(desc, v, Look::LongText)).unwrap_or_default()
}

/// Colonne d'une liste.
#[derive(Serialize, Clone)]
pub struct Column {
    pub code: String,
    pub title: String,
}

pub fn columns(codes: &[String]) -> Vec<Column> {
    codes
        .iter()
        .filter_map(|c| attribute::registry().by_code(c).map(|d| Column { code: c.clone(), title: tr(d.user_name) }))
        .collect()
}

/// Une valeur proposée dans une liste.
#[derive(Serialize, Clone)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

/// Champ du formulaire d'un tireur.
#[derive(Serialize)]
pub struct FormField {
    pub code: String,
    pub title: String,
    /// `text`, `number`, `bool`, `select` (liste fermée), `combo` (liste
    /// avec saisie libre), `date`.
    pub kind: &'static str,
    pub choices: Vec<Choice>,
    /// Valeur unique pour chaque tireur (le formulaire se vide après ajout).
    pub singular: bool,
}

/// Valeurs proposées pour un attribut à valeurs discrètes.
pub fn discrete_choices(code: &str) -> Vec<Choice> {
    if let Some(cat) = discrete::catalog() {
        let localized = attribute::registry()
            .by_code(code)
            .map(|d| matches!(d.discrete_source, Some(attribute::DiscreteSource::Localized(_))))
            .unwrap_or(false);
        let items = if localized { Vec::new() } else { cat.values(code) };
        if !items.is_empty() {
            return items.into_iter().map(|i| Choice { value: i.xml, label: i.long_text }).collect();
        }
    }
    attribute::registry()
        .by_code(code)
        .map(|d| d.discrete.iter().map(|v| Choice { value: v.xml.clone(), label: tr(&v.user) }).collect())
        .unwrap_or_default()
}

/// Valeurs d'un attribut propres à un pays (ligues, régions, clubs).
pub fn country_choices(code: &str, country: &str) -> Vec<Choice> {
    discrete::catalog()
        .map(|cat| cat.values_for_country(code, country).into_iter().map(|i| Choice { value: i.xml, label: i.long_text }).collect())
        .unwrap_or_default()
}

pub fn form_fields(codes: &[String]) -> Vec<FormField> {
    codes
        .iter()
        .filter_map(|code| {
            let desc = attribute::registry().by_code(code)?;
            if desc.rights != Rights::Public {
                return None;
            }
            let (kind, choices) = if desc.ty == AttrType::Bool {
                ("bool", Vec::new())
            } else if desc.has_discrete_value() {
                (if desc.free_value_allowed { "combo" } else { "select" }, discrete_choices(code))
            } else if code == "birth_date" {
                ("date", Vec::new())
            } else if matches!(desc.ty, AttrType::UInt | AttrType::Int) {
                ("number", Vec::new())
            } else {
                ("text", Vec::new())
            };
            Some(FormField {
                code: code.clone(),
                title: tr(desc.user_name),
                kind,
                choices,
                singular: desc.uniqueness == Uniqueness::Singular,
            })
        })
        .collect()
}

/// Valeur pour le formulaire : code XML pour une liste fermée, texte affiché
/// pour une liste avec saisie libre, sinon valeur brute.
pub fn form_value(player: &Player, code: &str) -> Option<String> {
    let desc = attribute::registry().by_code(code)?;
    let value = player.get(&AttrId::global(code))?;
    Some(match value {
        Value::Str(s) if desc.has_discrete_value() && desc.free_value_allowed => user_image(desc, value, Look::LongText),
        Value::Str(s) => s.clone(),
        Value::Bool(b) => if *b { "1".into() } else { "0".into() },
        other => other.xml_image(),
    })
}

/// Valeur saisie → valeur enregistrée (code XML d'une liste avec saisie libre).
pub fn stored_value(code: &str, text: &str) -> String {
    let Some(desc) = attribute::registry().by_code(code) else { return text.to_string() };
    if desc.has_discrete_value() && desc.free_value_allowed {
        discrete::catalog().map(|c| c.xml_image_from_user(code, text)).unwrap_or_else(|| text.to_string())
    } else {
        text.to_string()
    }
}
