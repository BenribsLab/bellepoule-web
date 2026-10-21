//! Outils communs aux phases portées ici (tableaux, barrage, scission) :
//! reprise des participants (`Stage::RetrieveAttendees`), calcul de la liste
//! des qualifiés avec quota et ex-aequo (`Stage::SetOutputShortlist`),
//! filtres d'affichage (`Affichage…`) et comparaisons de joueurs.

use std::cmp::Ordering;

use crate::attribute::{self, Value};
use crate::player::{AttrId, Owner, PlayerRef, PlayerStore};
use crate::stage::{PreviousStage, RankEntry, StageBase, StageContext};
use crate::xml::XmlWriter;

/// `Stage::RetrieveAttendees` : rang de départ (`stage_start_rank` = index
/// + 1) et statuts `Q` (local et global) pour chaque participant.
pub fn retrieve_attendees(base: &mut StageBase, ctx: &mut StageContext, attendees: Vec<PlayerRef>) {
    let owner = base.owner();
    for (i, r) in attendees.iter().enumerate() {
        if let Some(p) = ctx.players.get_mut(*r) {
            p.set_uint(&AttrId::local("stage_start_rank", owner), i as u32 + 1);
            p.set_str(&AttrId::local("status", owner), "Q");
            p.set_str(&AttrId::global("global_status"), "Q");
        }
    }
    base.attendees = attendees;
}

/// `Stage::Reset` : statuts remis à `Q`, place finale effacée.
pub fn reset_statuses(base: &StageBase, ctx: &mut StageContext) {
    let owner = base.owner();
    for r in &base.attendees {
        if let Some(p) = ctx.players.get_mut(*r) {
            p.set_str(&AttrId::local("status", owner), "Q");
            p.set_str(&AttrId::global("global_status"), "Q");
            p.remove(&AttrId::global("final_rank"));
        }
    }
}

/// Premier caractère du statut local d'un joueur, s'il existe.
pub fn status_of(players: &PlayerStore, r: PlayerRef, owner: Owner) -> Option<char> {
    players
        .get(r)?
        .get(&AttrId::local("status", owner))
        .map(|v| v.xml_image().chars().next().unwrap_or(' '))
}

/// Identifiant de l'attribut « rang de la phase précédente ». Sans phase
/// précédente connue, on se rabat sur le rang de départ dans cette phase
/// (qui suit l'ordre de sortie de la précédente).
pub fn previous_rank_id(previous: Option<PreviousStage>, own: Owner) -> AttrId {
    match previous {
        Some(p) => AttrId::local("rank", Owner::Stage(p.id)),
        None => AttrId::local("stage_start_rank", own),
    }
}

/// `Player::Compare` (C++) : comparaison selon l'attribut puis, en cas
/// d'égalité et si `seed` est non nul, tirage reproductible.
pub fn compare_players(players: &PlayerStore, a: PlayerRef, b: PlayerRef, id: &AttrId, seed: u32) -> Ordering {
    let (Some(pa), Some(pb)) = (players.get(a), players.get(b)) else {
        return Ordering::Equal;
    };
    let o = match attribute::registry().by_code(&id.name) {
        Some(desc) => attribute::compare_values(desc, pa.get(id), pb.get(id)),
        None => match (pa.get(id), pb.get(id)) {
            (Some(x), Some(y)) => x.compare(y),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => Ordering::Equal,
        },
    };
    if o == Ordering::Equal && seed != 0 {
        crate::player::random_compare(pa, pb, seed)
    } else {
        o
    }
}

/// Résultat de `Stage::SetOutputShortlist`, calculé sans modifier les joueurs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputComputation {
    /// Qualifiés pour la phase suivante, dans l'ordre du classement.
    pub output: Vec<PlayerRef>,
    /// Statuts locaux à écrire (`N` pour les non-qualifiés).
    pub local_status: Vec<(PlayerRef, char)>,
    /// Statuts globaux à écrire (abandons, exclusions, `N`).
    pub global_status: Vec<(PlayerRef, char)>,
    /// Nombre d'ex-aequo qui font dépasser le quota.
    pub quota_exceedance: u32,
}

/// Port de `Stage::SetOutputShortlist`. `result` est le classement complet,
/// `ranks` le rang (attribut `rank` de la phase) de chacun. Sans objet
/// classement (`has_classification` faux, cas de la scission), la sortie est
/// le classement tel quel.
pub fn compute_output(
    base: &StageBase,
    players: &PlayerStore,
    result: &[PlayerRef],
    rank_of: &dyn Fn(PlayerRef) -> u32,
    has_classification: bool,
) -> OutputComputation {
    let owner = base.owner();
    let mut c = OutputComputation { output: result.to_vec(), ..Default::default() };
    if c.output.is_empty() || !has_classification {
        return c;
    }

    // Abandons et exclusions retirés
    let mut kept = Vec::new();
    for r in &c.output {
        let Some(p) = players.get(*r) else { continue };
        if let Some(v) = p.get(&AttrId::local("status", owner)) {
            let s = v.xml_image();
            let first = s.chars().next();
            if first.is_some() && first != Some('Q') && first != Some('N') {
                c.global_status.push((*r, first.unwrap()));
            } else {
                kept.push(*r);
            }
        }
    }
    c.output = kept;

    // Non promus du barrage retirés (en fin de liste)
    let promoted_id = AttrId::local("promoted", owner);
    while let Some(last) = c.output.last().copied() {
        let promoted = players.get(last).and_then(|p| p.get(&promoted_id)).map(Value::as_uint);
        if promoted == Some(0) {
            c.local_status.push((last, 'N'));
            c.global_status.push((last, 'N'));
            c.output.pop();
        } else {
            break;
        }
    }

    // Quota
    if let Some(nq) = base.nb_qualified.filter(|n| *n > 0) {
        if let Some(last_qualified) = c.output.get(nq as usize - 1).copied() {
            let last_rank = rank_of(last_qualified);
            while let Some(last) = c.output.last().copied() {
                if rank_of(last) <= last_rank {
                    break;
                }
                if status_of(players, last, owner) == Some('Q') {
                    c.local_status.push((last, 'N'));
                    c.global_status.push((last, 'N'));
                }
                c.output.pop();
            }
        }

        // Dépassement du quota (ex-aequo)
        if c.output.len() > nq as usize {
            c.quota_exceedance = c.output.len() as u32;
            let mut last_rank = None;
            for (i, r) in c.output.iter().rev().enumerate() {
                let rank = rank_of(*r);
                match last_rank {
                    None => last_rank = Some(rank),
                    Some(l) if rank < l => {
                        c.quota_exceedance = i as u32;
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    c
}

/// Applique les statuts calculés par `compute_output`.
pub fn apply_output(base: &mut StageBase, ctx: &mut StageContext, c: &OutputComputation) {
    let owner = base.owner();
    for (r, s) in &c.local_status {
        if let Some(p) = ctx.players.get_mut(*r) {
            p.set_str(&AttrId::local("status", owner), &s.to_string());
        }
    }
    for (r, s) in &c.global_status {
        if let Some(p) = ctx.players.get_mut(*r) {
            p.set_str(&AttrId::global("global_status"), &s.to_string());
        }
    }
    base.quota_exceedance = c.quota_exceedance;
}

/// Début de `Stage::SetResult` : les `N` de la sortie précédente redeviennent `Q`.
pub fn restore_not_qualified(owner: Owner, ctx: &mut StageContext, previous_output: &[PlayerRef]) {
    for r in previous_output {
        if let Some(p) = ctx.players.get_mut(*r) {
            for id in [AttrId::local("status", owner), AttrId::global("global_status")] {
                if p.get(&id).map(|v| v.xml_image().starts_with('N')).unwrap_or(false) {
                    p.set_str(&id, "Q");
                }
            }
        }
    }
}

/// Écrit les rangs calculés (attribut local `rank`).
pub fn write_ranks(owner: Owner, ctx: &mut StageContext, ranks: &[(PlayerRef, u32)]) {
    for (r, rank) in ranks {
        if let Some(p) = ctx.players.get_mut(*r) {
            p.set_uint(&AttrId::local("rank", owner), *rank);
        }
    }
}

/// Lignes de classement à partir d'un résultat ordonné et des rangs.
pub fn rank_entries(players: &PlayerStore, owner: Owner, result: &[PlayerRef], rank_of: &dyn Fn(PlayerRef) -> u32) -> Vec<RankEntry> {
    result
        .iter()
        .map(|r| RankEntry { player: *r, rank: rank_of(*r), status: status_of(players, *r, owner).unwrap_or('Q') })
        .collect()
}

/// `Filter::Save` : `Affichage<as>="Nom,Prenom,…"` (noms XML).
pub fn save_filter(w: &mut XmlWriter, suffix: &str, codes: &[String]) {
    let names: Vec<&str> = codes
        .iter()
        .filter_map(|c| attribute::registry().by_code(c).map(|d| d.xml_name))
        .collect();
    w.attr(&format!("Affichage{suffix}"), &names.join(","));
}

/// `Filter::Load` : relit la liste (le premier attribut déclaré portant le
/// nom XML est retenu, comme en C++). `None` si l'attribut est absent.
pub fn load_filter(node: roxmltree::Node, suffix: &str) -> Option<Vec<String>> {
    let raw = node.attribute(format!("Affichage{suffix}").as_str())?;
    Some(
        raw.split(',')
            .filter_map(|x| attribute::registry().all().iter().find(|d| d.xml_name == x).map(|d| d.code_name.to_string()))
            .collect(),
    )
}

/// `Stage::SaveConfiguration` tel que l'écrit la v5.7 : `PhaseID`, `ID`,
/// `NetID`, `ScoreMax`, `NbQualifiesParIndice`, puis les filtres.
pub fn save_configuration(base: &StageBase, w: &mut XmlWriter, with_classification_filter: bool) {
    w.attr_fmt("PhaseID", base.id);
    w.attr("ID", &base.name);
    if base.net_id != 0 {
        w.attr("NetID", &format!("{:x}", base.net_id));
    }
    if let Some(m) = base.max_score {
        w.attr_fmt("ScoreMax", m);
    }
    if let Some(n) = base.nb_qualified {
        w.attr_fmt("NbQualifiesParIndice", n);
    }
    save_filter(w, "", &base.filter);
    if with_classification_filter {
        save_filter(w, "Classement", &base.classification_filter);
    }
}

/// `Stage::LoadConfiguration` : attributs communs (dont `NbQualifiesParIndice`
/// de la v5.7, en plus de `NbQualifies` lu par la fondation) et filtres.
pub fn load_configuration(base: &mut StageBase, node: roxmltree::Node) {
    base.load_configuration(node);
    if let Some(v) = node.attribute("NbQualifiesParIndice") {
        base.nb_qualified = Some(attribute::parse_leading_int(v).max(0) as u32);
    }
    if let Some(f) = load_filter(node, "") {
        base.filter = f;
    }
    if let Some(f) = load_filter(node, "Classement") {
        base.classification_filter = f;
    }
}

/// Nouvel identifiant réseau aléatoire non nul (`Net::Message`).
pub fn new_net_id() -> u32 {
    loop {
        let v: u32 = rand::random();
        if v != 0 {
            return v;
        }
    }
}

/// `Object::GetUndivadableText` : espaces remplacées par des espaces insécables.
pub fn undivadable(text: &str) -> String {
    text.replace(' ', "\u{a0}")
}

/// Convertit une liste de noms de code en `Vec<String>`.
pub fn codes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}
