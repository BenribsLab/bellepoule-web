//! Résultat commun des imports : liste de joueurs à enregistrer, avec leur
//! éventuel « propriétaire » (équipe), comme `Checkin::OnPlayerLoaded
//! (player, owner)` en C++.

use std::cmp::Ordering;

use crate::attribute::{self, Persistency, Uniqueness, Value};
use crate::formats::encoding::same_text;
use crate::player::{AttrId, Player, PlayerClass, PlayerRef, PlayerStore};

/// Un joueur lu, et l'index (dans la même liste) de l'équipe qui le contient.
#[derive(Debug, Clone)]
pub struct LoadedPlayer {
    pub player: Player,
    pub owner: Option<usize>,
}

/// Joueurs importés, dans l'ordre de lecture.
#[derive(Debug, Clone, Default)]
pub struct PlayerList {
    pub entries: Vec<LoadedPlayer>,
}

impl PlayerList {
    pub fn push(&mut self, player: Player, owner: Option<usize>) -> usize {
        self.entries.push(LoadedPlayer { player, owner });
        self.entries.len() - 1
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.entries.iter().map(|e| &e.player)
    }

    pub fn of_class(&self, class: PlayerClass) -> impl Iterator<Item = &Player> {
        self.players().filter(move |p| p.class == class)
    }

    /// Ajoute les joueurs à un `PlayerStore` :
    /// - un tireur dont la lecture a fourni une équipe propriétaire y est rattaché ;
    /// - sinon, un tireur portant l'attribut `team` (nom d'équipe, ex. fichiers
    ///   FIE par équipes) est rattaché à l'équipe de ce nom, créée au besoin
    ///   (`CheckinSupervisor::OnAttrTeamChanged`) ;
    /// - les attributs communs des équipes sont recalculés
    ///   (`Team::SetAttributesFromMembers`).
    ///
    /// Renvoie les références attribuées, dans l'ordre de la liste (les
    /// équipes créées à la volée sont ajoutées à la fin).
    pub fn add_to_store(self, store: &mut PlayerStore) -> Vec<PlayerRef> {
        let mut refs = Vec::with_capacity(self.entries.len());
        let mut owners = Vec::with_capacity(self.entries.len());
        for e in self.entries {
            owners.push(e.owner);
            refs.push(store.add(e.player));
        }
        let mut touched: Vec<PlayerRef> = Vec::new();
        for i in 0..refs.len() {
            let r = refs[i];
            let Some(p) = store.get(r) else { continue };
            if p.class != PlayerClass::Fencer {
                continue;
            }
            let team = match owners[i] {
                Some(o) => Some(refs[o]),
                None => {
                    let name = p.str_attr("team").unwrap_or("").to_string();
                    if name.is_empty() {
                        None
                    } else {
                        let existing = store
                            .of_class(PlayerClass::Team)
                            .find(|t| t.str_attr("name").map(|n| same_text(n, &name)).unwrap_or(false))
                            .map(|t| t.reference);
                        Some(existing.unwrap_or_else(|| {
                            let mut t = Player::new(PlayerClass::Team);
                            t.set_str(&AttrId::global("name"), &name);
                            let tr = store.add(t);
                            refs.push(tr);
                            tr
                        }))
                    }
                }
            };
            if let Some(t) = team {
                store.set_team(r, Some(t));
                if !touched.contains(&t) {
                    touched.push(t);
                }
            }
        }
        for t in touched {
            set_attributes_from_members_in_store(store, t);
        }
        refs
    }
}

/// `Team::SetAttributesFromMembers` sur une équipe du store.
pub fn set_attributes_from_members_in_store(store: &mut PlayerStore, team: PlayerRef) {
    let Some(t) = store.get(team) else { return };
    let members: Vec<Player> = t.members.iter().filter_map(|m| store.get(*m).cloned()).collect();
    let refs: Vec<&Player> = members.iter().collect();
    if let Some(t) = store.get_mut(team) {
        set_attributes_from_members(t, &refs);
    }
}

/// `Team::SetAttributesFromMembers` : chaque attribut persistant non
/// singulier (sauf `team`) commun à tous les membres est recopié sur
/// l'équipe ; il est supprimé sinon. Reproduit le parcours C++ (un premier
/// membre sans l'attribut n'interrompt pas la comparaison).
pub fn set_attributes_from_members(team: &mut Player, members: &[&Player]) {
    for desc in attribute::registry().all() {
        if desc.uniqueness != Uniqueness::NotSingular || desc.persistency != Persistency::Persistent || desc.code_name == "team" {
            continue;
        }
        let id = AttrId::global(desc.code_name);
        team.remove(&id);
        let mut team_attr: Option<&Value> = None;
        for m in members {
            let attr = m.attr(desc.code_name);
            if let Some(t) = team_attr {
                let equal = match attr {
                    Some(a) => attribute::compare_values(desc, Some(t), Some(a)) == Ordering::Equal,
                    None => false,
                };
                if !equal {
                    team_attr = None;
                    break;
                }
            }
            team_attr = attr;
        }
        if let Some(v) = team_attr {
            team.set(&id, v.clone());
        }
    }
}
