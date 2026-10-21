//! Équipes (port de `actors/team.cpp`, `fencer.cpp`, `null_team.cpp`).
//!
//! L'équipe « nulle » du C++ (« ** Without team ** ») est représentée par
//! `team = None` sur le tireur ; elle n'est jamais enregistrée.

use std::cmp::Ordering;

use crate::attribute::{registry, Persistency, Uniqueness, Value};
use crate::player::{AttrId, PlayerClass, PlayerRef, PlayerStore};
use crate::stage::ContestSettings;

use super::players::compare_values;

/// Libellé de l'équipe nulle (msgid).
pub const NULL_TEAM_NAME: &str = "** Without team **";

/// `Fencer::SetTeam` : retire le tireur de son ancienne équipe et l'ajoute
/// en tête de la nouvelle (`g_slist_prepend` dans `Team::AddMember`).
pub fn set_team(store: &mut PlayerStore, fencer: PlayerRef, team: Option<PlayerRef>) {
    let old = store.get(fencer).and_then(|p| p.team);
    if let Some(old) = old {
        if let Some(t) = store.get_mut(old) {
            t.members.retain(|m| *m != fencer);
        }
    }
    if let Some(team) = team {
        if let Some(t) = store.get_mut(team) {
            t.members.retain(|m| *m != fencer);
            t.members.insert(0, fencer);
        }
    }
    if let Some(f) = store.get_mut(fencer) {
        f.team = team;
    }
}

/// Équipe portant un nom donné (`CheckinSupervisor::GetTeam`), parmi `list`.
pub fn find_team(store: &PlayerStore, list: &[PlayerRef], name: &str) -> Option<PlayerRef> {
    list.iter().copied().find(|r| {
        store
            .get(*r)
            .map(|p| p.class == PlayerClass::Team && p.str_attr("name") == Some(name))
            .unwrap_or(false)
    })
}

/// Modifie `attending` et, comme `Checkin::OnAttrAttendingChanged`,
/// synchronise `global_status` (`Q` présent, `F` absent) si la valeur change.
pub fn set_attending(store: &mut PlayerStore, r: PlayerRef, attending: bool) -> bool {
    let Some(p) = store.get_mut(r) else { return false };
    let changed = p.set(&AttrId::global("attending"), Value::Bool(attending));
    if changed {
        p.set_str(&AttrId::global("global_status"), if attending { "Q" } else { "F" });
    }
    changed
}

/// `Team::SetAttendingFromMembers` : une équipe est présente si au moins
/// `minimum_team_size` membres le sont ; en classement automatique, son
/// classement est la somme des classements de ses premiers membres présents
/// (classement par défaut pour un membre non classé).
pub fn set_attending_from_members(store: &mut PlayerStore, team: PlayerRef, settings: &ContestSettings) {
    let ranking_id = AttrId::global("ranking");
    let Some(t) = store.get(team) else { return };
    let mut members = t.members.clone();
    if !settings.manual_classification {
        members.sort_by(|a, b| match (store.get(*a), store.get(*b)) {
            (Some(x), Some(y)) => compare_values("ranking", x.get(&ranking_id), y.get(&ranking_id)),
            _ => Ordering::Equal,
        });
        if let Some(t) = store.get_mut(team) {
            t.members = members.clone();
            t.set_uint(&ranking_id, 0);
        }
    }
    let mut present = 0u32;
    let mut team_rank = 0u32;
    for m in &members {
        let Some(p) = store.get(*m) else { continue };
        if p.attending() {
            present += 1;
            if present <= settings.minimum_team_size {
                let rank = p.get(&ranking_id).map(Value::as_uint).unwrap_or(0);
                team_rank += if rank != 0 { rank } else { settings.default_classification };
            }
        }
    }
    set_attending(store, team, present >= settings.minimum_team_size);
    if !settings.manual_classification {
        if let Some(t) = store.get_mut(team) {
            t.set_uint(&ranking_id, team_rank);
        }
    }
}

/// `Team::SetAttributesFromMembers` : une équipe hérite des attributs
/// multiples et persistants (nation, club…) communs à tous ses membres.
pub fn set_attributes_from_members(store: &mut PlayerStore, team: PlayerRef) {
    let Some(t) = store.get(team) else { return };
    let members = t.members.clone();
    let mut updates: Vec<(&'static str, Option<Value>)> = Vec::new();
    for desc in registry().all() {
        if desc.uniqueness != Uniqueness::NotSingular
            || desc.persistency != Persistency::Persistent
            || desc.code_name == "team"
        {
            continue;
        }
        let id = AttrId::global(desc.code_name);
        let mut team_attr: Option<Value> = None;
        for m in &members {
            let attr = store.get(*m).and_then(|p| p.get(&id)).cloned();
            if let Some(ta) = &team_attr {
                if attr.as_ref() != Some(ta) {
                    team_attr = None;
                    break;
                }
            }
            team_attr = attr;
        }
        updates.push((desc.code_name, team_attr));
    }
    if let Some(t) = store.get_mut(team) {
        for (code, v) in updates {
            let id = AttrId::global(code);
            t.remove(&id);
            if let Some(v) = v {
                t.set(&id, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contest::players::new_player;

    #[test]
    fn team_attending_and_ranking() {
        let mut store = PlayerStore::new();
        let team = store.add(new_player(PlayerClass::Team));
        let settings = ContestSettings { minimum_team_size: 2, default_classification: 9999, ..Default::default() };
        for (rank, present) in [(3, true), (0, true), (1, false)] {
            let mut f = new_player(PlayerClass::Fencer);
            f.set_uint(&AttrId::global("ranking"), rank);
            f.set_str(&AttrId::global("country"), "FRA");
            f.set(&AttrId::global("attending"), Value::Bool(present));
            let r = store.add(f);
            set_team(&mut store, r, Some(team));
        }
        set_attending_from_members(&mut store, team, &settings);
        let t = store.get(team).unwrap();
        assert!(t.attending());
        assert_eq!(t.uint_attr("ranking"), 3 + 9999);
        set_attributes_from_members(&mut store, team);
        assert_eq!(store.get(team).unwrap().str_attr("country"), Some("FRA"));
    }
}
