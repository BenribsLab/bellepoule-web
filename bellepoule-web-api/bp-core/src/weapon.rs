//! Armes (port de `application/weapon.cpp`).
//!
//! En C++ les armes sont créées dans `Application::Prepare` par
//! `g_list_prepend` : l'ordre de la liste est donc l'inverse de l'ordre de
//! déclaration (sabre, épée, fleuret, éducatif). Cet ordre fixe l'index
//! affiché dans la liste déroulante des propriétés et l'arme par défaut.

/// Une arme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weapon {
    /// Libellé non traduit (msgid) : « Sabre », « Epée »…
    pub image: &'static str,
    /// Code XML (`Arme`) : `S`, `E`, `F`, `X`.
    pub xml_image: &'static str,
    /// Code utilisé par la publication GREG (escrime-info).
    pub greg_image: &'static str,
    /// Durée standard d'un match (secondes).
    pub standard_duration_sec: u32,
}

/// Liste des armes, dans l'ordre de `Weapon::GetList` (déclaration inversée).
pub const WEAPONS: [Weapon; 4] = [
    Weapon { image: "Sabre", xml_image: "S", greg_image: "S", standard_duration_sec: 20 },
    Weapon { image: "Epée", xml_image: "E", greg_image: "E", standard_duration_sec: 30 },
    Weapon { image: "Foil", xml_image: "F", greg_image: "F", standard_duration_sec: 30 },
    Weapon { image: "Educ", xml_image: "X", greg_image: "S", standard_duration_sec: 30 },
];

impl Weapon {
    /// Toutes les armes (`GetList`).
    pub fn list() -> &'static [Weapon] {
        &WEAPONS
    }

    /// Arme par défaut (première de la liste : le sabre).
    pub fn default_weapon() -> &'static Weapon {
        &WEAPONS[0]
    }

    /// `GetFromXml` : comparaison exacte du code.
    pub fn from_xml(xml: &str) -> Option<&'static Weapon> {
        WEAPONS.iter().find(|w| w.xml_image == xml)
    }

    /// `GetFromIndex`.
    pub fn from_index(index: usize) -> Option<&'static Weapon> {
        WEAPONS.get(index)
    }

    /// `GetIndex` : position dans la liste.
    pub fn index(&self) -> usize {
        WEAPONS.iter().position(|w| w.xml_image == self.xml_image).unwrap_or(0)
    }

    /// Libellé traduit.
    pub fn label(&self) -> String {
        crate::i18n::tr(self.image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_and_lookup() {
        assert_eq!(Weapon::default_weapon().xml_image, "S");
        assert_eq!(Weapon::from_xml("E").unwrap().index(), 1);
        assert_eq!(Weapon::from_xml("X").unwrap().greg_image, "S");
        assert!(Weapon::from_xml("e").is_none());
    }
}
