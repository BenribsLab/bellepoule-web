//! Liste des arbitres d'une compétition (port de `actors/referees_list.cpp`
//! et `actors/referee.cpp`, parties sans interface).
//!
//! Les arbitres vivent dans leur propre `PlayerStore` : leurs références
//! (condensat SHA-1 du nom, sur 16 bits) peuvent coïncider avec celles des
//! tireurs.

use sha1::{Digest, Sha1};

use crate::attribute::Value;
use crate::player::{AttrId, Player, PlayerClass, PlayerRef, PlayerStore};
use crate::weapon::Weapon;
use crate::xml::XmlWriter;

use super::players::{guess_player_organization, load_player, new_player, write_player_attributes};

#[derive(Debug, Clone, Default)]
pub struct RefereesList {
    store: PlayerStore,
    /// Ordre d'affichage (ordre du fichier).
    order: Vec<PlayerRef>,
    weapon: Option<&'static Weapon>,
}

impl RefereesList {
    pub fn new() -> Self {
        RefereesList { store: PlayerStore::new(), order: Vec::new(), weapon: None }
    }

    /// `SetWeapon` : arme de la compétition ; elle est ajoutée aux armes de
    /// chaque arbitre inscrit ensuite.
    pub fn set_weapon(&mut self, weapon: &'static Weapon) {
        self.weapon = Some(weapon);
    }

    pub fn weapon_code(&self) -> Option<&'static str> {
        self.weapon.map(|w| w.xml_image)
    }

    pub fn store(&self) -> &PlayerStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut PlayerStore {
        &mut self.store
    }

    /// Références dans l'ordre de la liste.
    pub fn refs(&self) -> &[PlayerRef] {
        &self.order
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn get(&self, r: PlayerRef) -> Option<&Player> {
        self.store.get(r)
    }

    pub fn get_mut(&mut self, r: PlayerRef) -> Option<&mut Player> {
        self.store.get_mut(r)
    }

    /// `Referee::GiveAnId` : sans identifiant, il est dérivé du nom et du
    /// prénom (quatre premiers chiffres hexadécimaux du SHA-1).
    pub fn give_an_id(p: &mut Player) {
        if p.attr("ref").is_some() {
            return;
        }
        let mut sha = Sha1::new();
        sha.update(p.name().as_bytes());
        sha.update(p.first_name().as_bytes());
        let digest = sha.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let value = u32::from_str_radix(&hex[..4], 16).unwrap_or(0);
        p.set_ref(value);
    }

    /// `RefereesList::Add` : l'arme de la compétition est ajoutée aux armes
    /// de l'arbitre.
    pub fn add(&mut self, mut referee: Player) -> PlayerRef {
        if let Some(w) = self.weapon {
            let id = AttrId::global("weapon");
            let code = w.xml_image;
            let digest = match referee.str_attr("weapon") {
                Some(current) if current.to_ascii_uppercase().contains(code) => None,
                Some(current) => Some(format!("{current}{code}")),
                None => Some(code.to_string()),
            };
            if let Some(d) = digest {
                referee.set(&id, Value::Str(d));
            }
        }
        Self::give_an_id(&mut referee);
        let r = self.store.add(referee);
        self.order.push(r);
        r
    }

    /// Nouvel arbitre avec les valeurs par défaut du constructeur C++.
    pub fn new_referee() -> Player {
        new_player(PlayerClass::Referee)
    }

    pub fn remove(&mut self, r: PlayerRef) {
        self.order.retain(|x| *x != r);
        self.store.remove(r);
    }

    /// `LoadList` : `<Arbitres><Arbitre …/></Arbitres>` sous la racine.
    pub fn load(&mut self, root: roxmltree::Node, extra: &[String]) {
        let Some(list) = crate::xml::child(root, "Arbitres") else { return };
        for n in list.children().filter(|n| n.is_element()) {
            if n.tag_name().name() != PlayerClass::Referee.xml_tag() {
                break;
            }
            let mut p = load_player(PlayerClass::Referee, n, extra);
            guess_player_organization(&mut p, "league");
            guess_player_organization(&mut p, "region");
            self.add(p);
        }
    }

    /// `SaveList` : `<Arbitres>` (toujours écrit, même vide).
    pub fn save(&self, w: &mut XmlWriter, extra: &[String]) {
        w.start("Arbitres");
        for r in &self.order {
            if let Some(p) = self.store.get(*r) {
                w.start(p.class.xml_tag());
                write_player_attributes(p, w, extra, false);
                w.end();
            }
        }
        w.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_is_appended() {
        let mut l = RefereesList::new();
        l.set_weapon(Weapon::from_xml("E").unwrap());
        let mut p = RefereesList::new_referee();
        p.set(&AttrId::global("weapon"), Value::Str("F".into()));
        let r = l.add(p);
        assert_eq!(l.get(r).unwrap().str_attr("weapon"), Some("FE"));
        let mut q = RefereesList::new_referee();
        q.set(&AttrId::global("weapon"), Value::Str("SE".into()));
        let r = l.add(q);
        assert_eq!(l.get(r).unwrap().str_attr("weapon"), Some("SE"));
    }

    #[test]
    fn id_is_derived_from_name() {
        let mut a = RefereesList::new_referee();
        a.set(&AttrId::global("name"), Value::Str("DUPONT".into()));
        a.set(&AttrId::global("first_name"), Value::Str("Jean".into()));
        let mut l = RefereesList::new();
        let r = l.add(a);
        assert!(r > 0 && r <= 0xffff);
    }
}
