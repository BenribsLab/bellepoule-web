//! SmartPoule : données pour l'application des arbitres (téléphone).
//!
//! L'application d'origine n'existe plus ; celle-ci dialogue en JSON avec
//! BellePoule (voir `smartpoule-android/` et `bellepoule/src/network.rs`).
//!
//! Un match à arbitrer est désigné par un jeton :
//! - poule : `p~<compétition>~<tour>~<n° de poule>` ;
//! - tableau : `t~<compétition>~<phase>~<suite>~<chemin du match>`.
//!
//! Un arbitre voit les poules et les matchs auxquels il est affecté (pour une
//! poule sur plusieurs pistes : les matchs de sa piste) ; l'administrateur
//! (`referee = None`) voit tout.

use serde_json::{json, Value};

use crate::bout::Match;
use crate::contest::Contest;
use crate::player::{PlayerRef, PlayerStore};
use crate::rounds::pool::PoolAllocatorStage;
use crate::rounds::table::TableSupervisor;

fn identity(players: &PlayerStore, r: PlayerRef) -> (String, String) {
    players
        .get(r)
        .map(|p| {
            let name = format!("{} {}", p.name(), p.str_attr("first_name").unwrap_or_default()).trim().to_string();
            let club = p.str_attr("club").unwrap_or_default().to_string();
            (name, club)
        })
        .unwrap_or_default()
}

/// `<Match><Tireur REF Score Statut/>…</Match>` à partir des saisies
/// (`V`, `W3`/`V3`, `4`, vide), comme le message `SmartPoule::Score`.
fn match_xml(m: &Match, values: &[String; 2]) -> Option<String> {
    let mut xml = String::from("<Match>");
    for (k, value) in values.iter().enumerate() {
        let fencer = m.opponent(k)?;
        let v = value.trim().to_ascii_uppercase();
        let (score, victory) = if v == "V" {
            (m.max_score, true)
        } else if let Some(n) = v.strip_prefix('V').or_else(|| v.strip_prefix('W')) {
            (n.parse::<u32>().ok()?, true)
        } else if v.is_empty() {
            return None;
        } else {
            (v.parse::<u32>().ok()?, false)
        };
        xml.push_str(&format!(
            "<Tireur REF=\"{fencer}\" Score=\"{score}\" Statut=\"{}\"/>",
            if victory { "V" } else { "D" }
        ));
    }
    xml.push_str("</Match>");
    Some(xml)
}

impl Contest {
    fn title(&self) -> String {
        format!("{} · {} · {} · {}", self.name, self.weapon.label(), self.gender_label(), self.category.display_image())
    }

    /// Poules et matchs à arbitrer par `referee` (tous pour l'administrateur).
    pub fn smartpoule_jobs(&self, referee: Option<u32>) -> Vec<Value> {
        let mut out = Vec::new();
        let current = self.schedule.current_index();
        for i in 0..self.schedule.len().min(current + 1) {
            let Some(stage) = self.schedule.stage(i) else { continue };
            // Poules du tour en cours (superviseur non verrouillé)
            if let Some(a) = stage.as_any().downcast_ref::<PoolAllocatorStage>() {
                if self.schedule.stage(i + 1).map(|s| s.base().locked).unwrap_or(true) {
                    continue;
                }
                let round = a.round();
                for pool in round.pools() {
                    let mine = match referee {
                        None => true,
                        Some(r) => {
                            pool.referees().contains(&r) || pool.match_assignments().iter().any(|m| m.referee == Some(r))
                        }
                    };
                    if !mine || pool.nb_players() == 0 {
                        continue;
                    }
                    out.push(json!({
                        "token": format!("p~{}~{}~{}", self.net_id, round.net_id(), pool.number()),
                        "kind": "pool",
                        "competition": self.title(),
                        "name": pool.name(),
                        "piste": pool.piste_label().unwrap_or_default(),
                        "time": pool.start_time_image().unwrap_or_default(),
                        "over": pool.is_over(),
                        "fencers": pool.nb_players(),
                        "referees": pool.referees().iter().copied().chain(pool.match_assignments().iter().filter_map(|m| m.referee)).collect::<Vec<_>>(),
                    }));
                }
            }
            // Matchs de tableau prêts
            if let Some(sup) = stage.as_any().downcast_ref::<TableSupervisor>() {
                if stage.base().locked {
                    continue;
                }
                for set in sup.table_sets().iter().filter(|s| s.is_visible()) {
                    for (node, n) in set.nodes().iter().enumerate() {
                        let Some(m) = n.bout.as_ref() else { continue };
                        if m.opponent(0).is_none() || m.opponent(1).is_none() || m.number == 0 {
                            continue;
                        }
                        if let Some(r) = referee {
                            if !m.referees.contains(&r) {
                                continue;
                            }
                        }
                        let Some(path) = set.match_path(node) else { continue };
                        let (a, _) = identity(&self.players, m.opponent(0).unwrap());
                        let (b, _) = identity(&self.players, m.opponent(1).unwrap());
                        out.push(json!({
                            "token": format!("t~{}~{}~{}~{}", self.net_id, stage.base().net_id, set.id(), path),
                            "kind": "match",
                            "competition": self.title(),
                            "name": format!("{} — {}", set.name(), m.name),
                            "piste": if m.piste != 0 { format!("{:02}", m.piste) } else { String::new() },
                            "time": m.start_time.map(|t| t.0.format("%H:%M").to_string()).unwrap_or_default(),
                            "over": m.is_over(),
                            "fencers": 2,
                            "summary": format!("{a} / {b}"),
                            "referees": m.referees.clone(),
                        }));
                    }
                }
            }
        }
        out
    }

    /// Feuille d'un match à arbitrer (poule complète, ou match de tableau).
    pub fn smartpoule_sheet(&self, token: &str, referee: Option<u32>) -> Option<Value> {
        let parts: Vec<&str> = token.split('~').collect();
        if parts.get(1)?.parse::<u32>().ok()? != self.net_id {
            return None;
        }
        let stage_id: u32 = parts.get(2)?.parse().ok()?;
        match *parts.first()? {
            "p" => {
                let number: u32 = parts.get(3)?.parse().ok()?;
                for stage in self.schedule.stages() {
                    let Some(a) = stage.as_any().downcast_ref::<PoolAllocatorStage>() else { continue };
                    let round = a.round();
                    if round.net_id() != stage_id {
                        continue;
                    }
                    let pool = round.pools().iter().find(|p| p.number() == number)?;
                    let sheet = pool.sheet(&self.players, self.referees.store(), false);
                    let bouts: Vec<Value> = sheet
                        .matches
                        .iter()
                        .filter(|m| match (referee, &m.assignment) {
                            (Some(r), Some(a)) if pool.strips().len() > 1 => a.referee == Some(r),
                            _ => true,
                        })
                        .map(|m| {
                            json!({
                                "index": m.match_index,
                                "name": m.name,
                                "referee": m.assignment.as_ref().and_then(|a| a.referee),
                                "resource": m.resource,
                                "fencers": m.opponents.iter().flatten().map(|f| json!({
                                    "reference": f.player,
                                    "position": f.position,
                                    "name": format!("{} {}", f.identity.name, f.identity.first_name).trim().to_string(),
                                    "club": f.identity.club,
                                    "score": f.score,
                                })).collect::<Vec<_>>(),
                                "over": pool.matches().get(m.match_index).map(|x| x.bout.is_over()).unwrap_or(false),
                            })
                        })
                        .collect();
                    return Some(json!({
                        "token": token,
                        "kind": "pool",
                        "competition": self.title(),
                        "name": pool.name(),
                        "piste": pool.piste_label().unwrap_or_default(),
                        "max_score": pool.max_score(),
                        "fencers": sheet.rows.iter().map(|r| json!({
                            "position": r.position,
                            "name": format!("{} {}", r.identity.name, r.identity.first_name).trim().to_string(),
                            "club": r.identity.club,
                            "cells": r.cells.iter().map(|c| if c.grey { Value::Null } else { json!(c.text) }).collect::<Vec<_>>(),
                            "victories": r.dashboard.as_ref().map(|d| d.victories.clone()),
                            "indice": r.dashboard.as_ref().map(|d| d.indice.clone()),
                            "rank": r.dashboard.as_ref().map(|d| d.rank.clone()),
                        })).collect::<Vec<_>>(),
                        "bouts": bouts,
                        "over": pool.is_over(),
                    }));
                }
                None
            }
            "t" => {
                let (set_id, path) = (parts.get(3)?, parts.get(4)?);
                for stage in self.schedule.stages() {
                    let Some(sup) = stage.as_any().downcast_ref::<TableSupervisor>() else { continue };
                    if stage.base().net_id != stage_id {
                        continue;
                    }
                    let set = sup.table_set(sup.table_set_by_id(set_id)?);
                    let node = set.node_from_path(path)?;
                    let m = set.bout(node)?;
                    let fencers: Vec<Value> = (0..2)
                        .filter_map(|k| {
                            let f = m.opponent(k)?;
                            let (name, club) = identity(&self.players, f);
                            Some(json!({ "reference": f, "position": k + 1, "name": name, "club": club, "score": m.score_at(k).image(m.max_score) }))
                        })
                        .collect();
                    return Some(json!({
                        "token": token,
                        "kind": "match",
                        "competition": self.title(),
                        "name": format!("{} — {}", set.name(), m.name),
                        "piste": if m.piste != 0 { format!("{:02}", m.piste) } else { String::new() },
                        "max_score": m.max_score,
                        "bouts": [{ "index": 0, "name": m.name, "fencers": fencers, "over": m.is_over() }],
                        "over": m.is_over(),
                    }));
                }
                None
            }
            _ => None,
        }
    }

    /// Score d'un match saisi sur le téléphone (`values` : tireur du haut,
    /// tireur du bas). Renvoie une erreur lisible si la saisie est refusée.
    pub fn smartpoule_apply_score(&mut self, token: &str, bout: usize, values: [String; 2]) -> Result<(), String> {
        let parts: Vec<&str> = token.split('~').collect();
        let stage_id: u32 = parts.get(2).and_then(|s| s.parse().ok()).ok_or("jeton invalide")?;
        match parts.first().copied() {
            Some("p") => {
                let number: u32 = parts.get(3).and_then(|s| s.parse().ok()).ok_or("jeton invalide")?;
                for i in 0..self.schedule.len() {
                    let Some(stage) = self.schedule.stage(i) else { continue };
                    let Some(a) = stage.as_any().downcast_ref::<PoolAllocatorStage>() else { continue };
                    let shared = a.shared_round();
                    if shared.lock().map(|r| r.net_id()).unwrap_or(0) != stage_id {
                        continue;
                    }
                    if self.schedule.stage(i + 1).map(|s| s.base().locked).unwrap_or(true) {
                        return Err("le tour de poules est terminé".into());
                    }
                    let mut round = shared.lock().unwrap_or_else(|e| e.into_inner());
                    let players = &mut self.players;
                    let pool = round.pools_mut().iter_mut().find(|p| p.number() == number).ok_or("poule introuvable")?;
                    let m = pool.matches().get(bout).ok_or("match introuvable")?;
                    let fencers = [m.bout.opponent(0).ok_or("match incomplet")?, m.bout.opponent(1).ok_or("match incomplet")?];
                    match_xml(&m.bout, &values).ok_or("score incomplet ou invalide")?;
                    for (f, v) in fencers.iter().zip(values.iter()) {
                        pool.set_score_str(players, bout, *f, v.trim());
                    }
                    if pool.matches().get(bout).map(|m| m.bout.has_error()).unwrap_or(false) {
                        return Err("scores incohérents (un seul vainqueur, au plus le score maximum)".into());
                    }
                    drop(round);
                    self.dirty = true;
                    return Ok(());
                }
                Err("poule introuvable".into())
            }
            Some("t") => {
                let (set_id, path) = (parts.get(3).ok_or("jeton invalide")?.to_string(), parts.get(4).ok_or("jeton invalide")?.to_string());
                for i in 0..self.schedule.len() {
                    let Some(stage) = self.schedule.stage(i) else { continue };
                    let Some(sup) = stage.as_any().downcast_ref::<TableSupervisor>() else { continue };
                    if stage.base().net_id != stage_id {
                        continue;
                    }
                    if stage.base().locked {
                        return Err("le tableau est terminé".into());
                    }
                    let set = sup.table_set_by_id(&set_id).ok_or("tableau introuvable")?;
                    let node = sup.table_set(set).node_from_path(&path).ok_or("match introuvable")?;
                    let m = sup.table_set(set).bout(node).ok_or("match introuvable")?;
                    let xml = match_xml(m, &values).ok_or("score incomplet ou invalide")?;
                    let applied = self.with_schedule(|s, ctx| {
                        let Some(sup) = s.stage_mut(i).and_then(|x| x.as_any_mut().downcast_mut::<TableSupervisor>()) else { return false };
                        sup.load_remote_score(ctx, set, &path, &xml)
                    });
                    if !applied {
                        return Err("score refusé".into());
                    }
                    self.dirty = true;
                    return Ok(());
                }
                Err("tableau introuvable".into())
            }
            _ => Err("jeton invalide".into()),
        }
    }
}
