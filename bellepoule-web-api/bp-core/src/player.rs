//! Joueurs : tireurs, arbitres, équipes (port de `common/util/player.cpp` et
//! `BellePoule/actors/{fencer,referee,team}.cpp`).
//!
//! Différences de conception avec le C++ :
//! - les joueurs vivent dans un `PlayerStore` (arène indexée par `PlayerRef`)
//!   et les phases ne manipulent que des références numériques ;
//! - les attributs « locaux » (portée `Scope::Local`) sont rattachés à un
//!   `Owner` (en général l'identifiant d'une phase) au lieu d'un `Object *` ;
//! - pas de callbacks de changement : l'interface relit le modèle.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use crate::attribute::{self, compare_values, AttrType, Scope, Value};
use crate::xml::XmlWriter;

/// Identifiant d'un joueur (`ID` dans les fichiers XML).
pub type PlayerRef = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerClass {
    Fencer,
    Referee,
    Team,
}

impl PlayerClass {
    pub fn xml_tag(self) -> &'static str {
        match self {
            PlayerClass::Fencer => "Tireur",
            PlayerClass::Referee => "Arbitre",
            PlayerClass::Team => "Equipe",
        }
    }

    pub fn from_xml_tag(tag: &str) -> Option<PlayerClass> {
        match tag {
            "Tireur" => Some(PlayerClass::Fencer),
            "Arbitre" => Some(PlayerClass::Referee),
            "Equipe" => Some(PlayerClass::Team),
            _ => None,
        }
    }

    pub fn class_name(self) -> &'static str {
        match self {
            PlayerClass::Fencer => "Fencer",
            PlayerClass::Referee => "Referee",
            PlayerClass::Team => "Team",
        }
    }
}

/// Propriétaire d'un attribut (`AttributeId::_owner`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Owner {
    Global,
    /// Attribut local à une phase (identifiant de phase).
    Stage(u32),
    /// Autre propriétaire (pool, tableau…) : identifiant libre.
    Other(u64),
}

/// Identifiant d'attribut complet (`Player::AttributeId`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttrId {
    pub name: String,
    pub owner: Owner,
    /// Graine pour un tri aléatoire stable (`MakeRandomReady`).
    pub rand_seed: u32,
}

impl AttrId {
    pub fn global(name: &str) -> Self {
        AttrId { name: name.to_string(), owner: Owner::Global, rand_seed: 0 }
    }

    pub fn local(name: &str, owner: Owner) -> Self {
        AttrId { name: name.to_string(), owner, rand_seed: 0 }
    }

    /// Équivalent de `AttributeId::Create` : le propriétaire n'est retenu que
    /// si l'attribut est de portée locale.
    pub fn for_desc(code_name: &str, owner: Owner) -> Self {
        let local = attribute::registry().by_code(code_name).map(|d| d.scope == Scope::Local).unwrap_or(false);
        AttrId { name: code_name.to_string(), owner: if local { owner } else { Owner::Global }, rand_seed: 0 }
    }

    pub fn random_ready(mut self, seed: u32) -> Self {
        self.rand_seed = seed;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Player {
    pub class: PlayerClass,
    pub reference: PlayerRef,
    attrs: HashMap<(Owner, String), Value>,
    /// Équipe d'un tireur.
    pub team: Option<PlayerRef>,
    /// Membres d'une équipe (ordre d'inscription).
    pub members: Vec<PlayerRef>,
    /// Arme (code XML : `F`, `E`, `S`, `X`).
    pub weapon: Option<String>,
}

impl Player {
    pub fn new(class: PlayerClass) -> Self {
        Player { class, reference: 0, attrs: HashMap::new(), team: None, members: Vec::new(), weapon: None }
    }

    pub fn is(&self, class: PlayerClass) -> bool {
        self.class == class
    }

    pub fn get(&self, id: &AttrId) -> Option<&Value> {
        self.attrs.get(&(id.owner, id.name.clone()))
    }

    /// Raccourci pour un attribut global.
    pub fn attr(&self, name: &str) -> Option<&Value> {
        self.attrs.get(&(Owner::Global, name.to_string()))
    }

    pub fn str_attr(&self, name: &str) -> Option<&str> {
        self.attr(name).and_then(Value::as_str)
    }

    pub fn uint_attr(&self, name: &str) -> u32 {
        self.attr(name).map(Value::as_uint).unwrap_or(0)
    }

    /// `SetAttributeValue (id, const gchar *)` : la chaîne est interprétée selon
    /// le type déclaré. Renvoie vrai si la valeur a changé.
    pub fn set_str(&mut self, id: &AttrId, value: &str) -> bool {
        let ty = attribute::registry().by_code(&id.name).map(|d| d.ty).unwrap_or(AttrType::Str);
        self.set(id, Value::parse(ty, value))
    }

    /// `SetAttributeValue (id, guint)`.
    pub fn set_uint(&mut self, id: &AttrId, value: u32) -> bool {
        let ty = attribute::registry().by_code(&id.name).map(|d| d.ty).unwrap_or(AttrType::UInt);
        self.set(id, Value::from_uint(ty, value))
    }

    pub fn set(&mut self, id: &AttrId, value: Value) -> bool {
        if id.name == "ref" {
            self.reference = value.as_uint();
        }
        let key = (id.owner, id.name.clone());
        if self.attrs.get(&key) == Some(&value) {
            return false;
        }
        self.attrs.insert(key, value);
        true
    }

    pub fn remove(&mut self, id: &AttrId) {
        self.attrs.remove(&(id.owner, id.name.clone()));
    }

    /// Supprime tous les attributs locaux d'un propriétaire (fin de phase).
    pub fn remove_owner(&mut self, owner: Owner) {
        self.attrs.retain(|(o, _), _| *o != owner);
    }

    pub fn set_ref(&mut self, reference: PlayerRef) {
        self.set_uint(&AttrId::global("ref"), reference);
    }

    pub fn name(&self) -> String {
        self.str_attr("name").unwrap_or_default().to_string()
    }

    pub fn first_name(&self) -> String {
        self.str_attr("first_name").unwrap_or_default().to_string()
    }

    /// « NOM Prénom », utilisé dans les feuilles de poule et tableaux.
    pub fn full_name(&self) -> String {
        let first = self.first_name();
        if first.is_empty() {
            self.name()
        } else {
            format!("{} {}", self.name(), first)
        }
    }

    pub fn attending(&self) -> bool {
        self.attr("attending").map(Value::as_bool).unwrap_or(false)
    }

    /// Copie profonde sans les attributs locaux (`Duplicate`).
    pub fn duplicate(&self) -> Player {
        let mut p = self.clone();
        p.attrs.retain(|(o, _), _| *o == Owner::Global);
        p
    }

    /// `UpdateFrom` : recopie les attributs globaux d'un autre joueur.
    pub fn update_from(&mut self, from: &Player) {
        for ((owner, name), v) in &from.attrs {
            if *owner == Owner::Global && name != "ref" {
                self.attrs.insert((*owner, name.clone()), v.clone());
            }
        }
    }

    /// Change le propriétaire des attributs locaux selon une table
    /// (renumérotation des phases). Ajout de `contest`/`schedule`.
    pub fn remap_owners(&mut self, map: &HashMap<Owner, Owner>) {
        if map.is_empty() {
            return;
        }
        let attrs = std::mem::take(&mut self.attrs);
        self.attrs = attrs
            .into_iter()
            .map(|((o, n), v)| ((map.get(&o).copied().unwrap_or(o), n), v))
            .collect();
    }

    /// Attributs locaux d'un propriétaire donné.
    pub fn local_attrs(&self, owner: Owner) -> impl Iterator<Item = (&str, &Value)> {
        self.attrs.iter().filter(move |((o, _), _)| *o == owner).map(|((_, n), v)| (n.as_str(), v))
    }

    pub fn global_attrs(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.attrs.iter().filter(|((o, _), _)| *o == Owner::Global).map(|((_, n), v)| (n.as_str(), v))
    }

    /// Écrit les attributs globaux (`Player::SaveAttributes`).
    /// `full_profile` inclut les attributs non persistants (échanges réseau).
    pub fn write_attributes(&self, w: &mut XmlWriter, full_profile: bool) {
        for desc in attribute::registry().all() {
            if desc.scope != Scope::Global {
                continue;
            }
            if !full_profile && !desc.is_persistent() {
                continue;
            }
            if desc.code_name == "final_rank" && self.attr("exported").map(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let Some(value) = self.attr(desc.code_name) else { continue };
            let mut image = value.xml_image();
            if desc.code_name == "birth_date" {
                image = birth_date_xml_image(&image);
            }
            w.attr(desc.xml_name, &image);
        }
    }

    /// `Player::Save` (sans les membres d'équipe : voir `contest`).
    pub fn save(&self, w: &mut XmlWriter, full_profile: bool) {
        w.start(self.class.xml_tag());
        self.write_attributes(w, full_profile);
        w.end();
    }

    /// `Player::Load` : lit les attributs globaux d'un nœud XML.
    pub fn load(&mut self, node: roxmltree::Node) {
        let mut attending_set = false;
        for desc in attribute::registry().all() {
            // Les attributs locaux (ex. `status`) sont lus par les phases.
            if desc.scope == Scope::Local {
                continue;
            }
            let Some(raw) = node.attribute(desc.xml_name) else { continue };
            let id = AttrId::global(desc.code_name);
            let mut value = raw.to_string();
            if desc.code_name == "birth_date" {
                value = birth_date_from_xml(&value);
            }
            self.set_str(&id, &value);
            if desc.code_name == "global_status" {
                attending_set = true;
                self.set(&AttrId::global("attending"), Value::Bool(!raw.starts_with('F')));
            }
        }
        if !attending_set && self.attr("attending").is_none() {
            self.set(&AttrId::global("attending"), Value::Bool(false));
        }
    }
}

/// Les fichiers stockent `jj.mm.aaaa` ; une année seule devient `00.00.aaaa`.
fn birth_date_xml_image(value: &str) -> String {
    if let Some(d) = attribute::parse_date(value) {
        return d.format("%d.%m.%Y").to_string();
    }
    let year = attribute::parse_leading_int(value);
    if (1000..=9999).contains(&year) && value.trim().len() == 4 {
        return format!("00.00.{year}");
    }
    value.to_string()
}

/// En mémoire on garde `jj.mm.aaaa` (le C++ utilise le format local `%x`) ;
/// `00.00.aaaa` (AskFred) devient l'année seule.
fn birth_date_from_xml(value: &str) -> String {
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() == 3 && (parts[0] == "00" || parts[1] == "00") {
        return parts[2].to_string();
    }
    value.to_string()
}

/// Comparaison selon un attribut (`Player::Compare`).
pub fn compare(a: &Player, b: &Player, id: &AttrId) -> Ordering {
    if id.rand_seed != 0 {
        return random_compare(a, b, id.rand_seed);
    }
    match attribute::registry().by_code(&id.name) {
        Some(desc) => compare_values(desc, a.get(id), b.get(id)),
        None => match (a.get(id), b.get(id)) {
            (Some(x), Some(y)) => x.compare(y),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => Ordering::Equal,
        },
    }
}

/// Tri multicritère (`Player::MultiCompare`).
pub fn multi_compare(a: &Player, b: &Player, ids: &[AttrId]) -> Ordering {
    for id in ids {
        let o = compare(a, b, id);
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// Tri aléatoire reproductible (`Player::RandomCompare`) : hachage
/// déterministe de la référence et de la graine.
pub fn random_compare(a: &Player, b: &Player, seed: u32) -> Ordering {
    random_key(a.reference, seed).cmp(&random_key(b.reference, seed))
}

pub fn random_key(reference: PlayerRef, seed: u32) -> u64 {
    // splitmix64 : stable entre plateformes et versions.
    let mut z = ((seed as u64) << 32 | reference as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Arène des joueurs d'une compétition.
#[derive(Debug, Clone, Default)]
pub struct PlayerStore {
    players: BTreeMap<PlayerRef, Player>,
    next_ref: PlayerRef,
}

impl PlayerStore {
    pub fn new() -> Self {
        PlayerStore { players: BTreeMap::new(), next_ref: 1 }
    }

    /// Ajoute un joueur ; une référence 0 ou déjà prise est remplacée.
    pub fn add(&mut self, mut player: Player) -> PlayerRef {
        if player.reference == 0 || self.players.contains_key(&player.reference) {
            while self.players.contains_key(&self.next_ref) {
                self.next_ref += 1;
            }
            player.set_ref(self.next_ref);
        }
        let r = player.reference;
        self.next_ref = self.next_ref.max(r + 1);
        self.players.insert(r, player);
        r
    }

    pub fn get(&self, r: PlayerRef) -> Option<&Player> {
        self.players.get(&r)
    }

    pub fn get_mut(&mut self, r: PlayerRef) -> Option<&mut Player> {
        self.players.get_mut(&r)
    }

    pub fn remove(&mut self, r: PlayerRef) -> Option<Player> {
        let p = self.players.remove(&r)?;
        if let Some(team) = p.team {
            if let Some(t) = self.players.get_mut(&team) {
                t.members.retain(|m| *m != r);
            }
        }
        Some(p)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Player> {
        self.players.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Player> {
        self.players.values_mut()
    }

    pub fn of_class(&self, class: PlayerClass) -> impl Iterator<Item = &Player> {
        self.players.values().filter(move |p| p.class == class)
    }

    pub fn len(&self) -> usize {
        self.players.len()
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    /// Rattache un tireur à une équipe (`Fencer::SetTeam`).
    pub fn set_team(&mut self, fencer: PlayerRef, team: Option<PlayerRef>) {
        let old = self.players.get(&fencer).and_then(|p| p.team);
        if let Some(old) = old {
            if let Some(t) = self.players.get_mut(&old) {
                t.members.retain(|m| *m != fencer);
            }
        }
        if let Some(team) = team {
            if let Some(t) = self.players.get_mut(&team) {
                if !t.members.contains(&fencer) {
                    t.members.push(fencer);
                }
            }
        }
        if let Some(f) = self.players.get_mut(&fencer) {
            f.team = team;
        }
    }

    /// Change le propriétaire des attributs locaux de tous les joueurs.
    pub fn remap_owners(&mut self, map: &HashMap<Owner, Owner>) {
        for p in self.players.values_mut() {
            p.remap_owners(map);
        }
    }

    /// Renumérote les joueurs (`SetRef`) : les clés, les références, les
    /// liens équipe/membres sont mis à jour. Les références absentes de la
    /// table sont conservées. La table doit produire des références uniques.
    pub fn renumber(&mut self, map: &HashMap<PlayerRef, PlayerRef>) {
        if map.iter().all(|(a, b)| a == b) {
            return;
        }
        let tr = |r: PlayerRef| map.get(&r).copied().unwrap_or(r);
        let old = std::mem::take(&mut self.players);
        for (r, mut p) in old {
            let new_ref = tr(r);
            if new_ref != r {
                p.set_ref(new_ref);
            }
            p.team = p.team.map(tr);
            for m in p.members.iter_mut() {
                *m = tr(*m);
            }
            self.players.insert(new_ref, p);
        }
        self.next_ref = self.players.keys().next_back().map(|r| r + 1).unwrap_or(1).max(1);
    }

    /// Vrai si la référence est utilisée.
    pub fn contains(&self, r: PlayerRef) -> bool {
        self.players.contains_key(&r)
    }

    /// Références de tous les joueurs (ordre croissant).
    pub fn refs(&self) -> Vec<PlayerRef> {
        self.players.keys().copied().collect()
    }

    /// Trie des références selon une liste de critères.
    pub fn sort_refs(&self, refs: &mut [PlayerRef], criteria: &[AttrId]) {
        refs.sort_by(|a, b| match (self.get(*a), self.get(*b)) {
            (Some(pa), Some(pb)) => multi_compare(pa, pb, criteria),
            _ => Ordering::Equal,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_attributes_are_isolated() {
        let mut p = Player::new(PlayerClass::Fencer);
        p.set_uint(&AttrId::local("rank", Owner::Stage(2)), 3);
        p.set_uint(&AttrId::local("rank", Owner::Stage(3)), 7);
        assert_eq!(p.get(&AttrId::local("rank", Owner::Stage(2))).unwrap().as_uint(), 3);
        p.remove_owner(Owner::Stage(2));
        assert!(p.get(&AttrId::local("rank", Owner::Stage(2))).is_none());
        assert_eq!(p.get(&AttrId::local("rank", Owner::Stage(3))).unwrap().as_uint(), 7);
    }

    #[test]
    fn load_save_roundtrip() {
        let xml = r#"<Tireur ID="51" Nom="SEITZ" Prenom="Tom" DateNaissance="31.05.1994" Sexe="M" Ranking="1010" Statut="Q"/>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let mut p = Player::new(PlayerClass::Fencer);
        p.load(doc.root_element());
        assert_eq!(p.reference, 51);
        assert!(p.attending());
        let mut w = XmlWriter::new();
        p.save(&mut w, false);
        let out = w.finish();
        assert!(out.contains(r#"ID="51""#));
        assert!(out.contains(r#"DateNaissance="31.05.1994""#));
        assert!(out.contains(r#"Statut="Q""#));
    }

    #[test]
    fn store_assigns_refs() {
        let mut s = PlayerStore::new();
        let a = s.add(Player::new(PlayerClass::Fencer));
        let b = s.add(Player::new(PlayerClass::Fencer));
        assert_ne!(a, b);
    }
}
