//! Armes connues du Marshaller (port local de `application/weapon.cpp`).
//!
//! L'ordre est celui de `Weapon::GetList ()` (liste construite par
//! `g_list_prepend`) : Sabre, Épée, Fleuret, Éducatif.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weapon {
    /// Libellé (msgid gettext).
    pub image: &'static str,
    /// Code XML (`F`, `E`, `S`, `X`).
    pub xml: &'static str,
    /// Durée standard d'une touche, en secondes.
    pub standard_duration_sec: u32,
}

pub const WEAPONS: [Weapon; 4] = [
    Weapon { image: "Sabre", xml: "S", standard_duration_sec: 20 },
    Weapon { image: "Epée", xml: "E", standard_duration_sec: 30 },
    Weapon { image: "Foil", xml: "F", standard_duration_sec: 30 },
    Weapon { image: "Educ", xml: "X", standard_duration_sec: 30 },
];

/// `Weapon::GetFromXml` (comparaison insensible à la casse).
pub fn from_xml(code: &str) -> Option<&'static Weapon> {
    WEAPONS.iter().find(|w| w.xml.eq_ignore_ascii_case(code))
}

/// Index de l'arme dans `WEAPONS`.
pub fn index_of(code: &str) -> Option<usize> {
    WEAPONS.iter().position(|w| w.xml.eq_ignore_ascii_case(code))
}
