//! Fichiers XML FIE/FFE (`BaseCompetitionIndividuelle`,
//! `BaseCompetitionParEquipes`, et formats résultats
//! `CompetitionIndividuelle` / `CompetitionParEquipes`, qui sont aussi la
//! base des `.cotcot`).
//!
//! Lecture : en-tête (`Contest::LoadXmlDoc`) et listes de personnes
//! (`Checkin::LoadList` pour les tireurs, équipes et arbitres). Les phases
//! sont laissées au module `contest`.
//!
//! Particularité des fichiers « Base » (`ConvertFromBaseToResult`) : leur
//! attribut `Classement` est un classement d'entrée, pas une place ; il est
//! donc converti en `ranking`.

use crate::formats::discrete;
use crate::formats::encoding::{decode_xml, parse_xml};
use crate::formats::header::ContestHeader;
use crate::formats::team::PlayerList;
use crate::player::{AttrId, Player, PlayerClass, PlayerRef, PlayerStore};
use crate::xml::{child, XmlWriter};
use crate::{Error, Result};

/// Racines reconnues, dans l'ordre de recherche du C++ :
/// (balise, épreuve par équipes, format « Base »).
pub const ROOTS: [(&str, bool, bool); 4] = [
    ("CompetitionIndividuelle", false, false),
    ("BaseCompetitionIndividuelle", false, true),
    ("CompetitionParEquipes", true, false),
    ("BaseCompetitionParEquipes", true, true),
];

#[derive(Debug, Clone)]
pub struct XmlContestFile {
    /// Balise racine.
    pub root: String,
    /// Fichier « Base » (préparation) : `Classement` = classement d'entrée.
    pub base: bool,
    pub header: ContestHeader,
    /// Tireurs et équipes (liste d'appel), avec rattachement des membres.
    pub players: PlayerList,
    pub referees: PlayerList,
}

pub fn read_file(path: &std::path::Path) -> Result<XmlContestFile> {
    read(&std::fs::read(path)?)
}

/// Lecture d'un fichier XML de compétition (encodage selon la déclaration).
pub fn read(bytes: &[u8]) -> Result<XmlContestFile> {
    let text = decode_xml(bytes)?;
    read_str(&text)
}

pub fn read_str(text: &str) -> Result<XmlContestFile> {
    let doc = parse_xml(text)?;
    let root = doc.root_element();
    let name = root.tag_name().name();
    let Some(&(_, team_event, base)) = ROOTS.iter().find(|(n, _, _)| *n == name) else {
        return Err(Error::Format(format!("racine XML inattendue : {name}")));
    };
    let header = read_header(root, team_event);

    let mut players = PlayerList::default();
    if let Some(list) = child(root, "Tireurs") {
        load_list(list, PlayerClass::Fencer, None, &mut players, None);
    }
    if team_event {
        if let Some(list) = child(root, "Equipes") {
            load_list(list, PlayerClass::Team, Some(PlayerClass::Fencer), &mut players, None);
        }
    }
    let mut referees = PlayerList::default();
    if let Some(list) = child(root, "Arbitres") {
        load_list(list, PlayerClass::Referee, None, &mut referees, None);
    }

    if base {
        convert_from_base_to_result(&mut players);
    }
    Ok(XmlContestFile { root: name.to_string(), base, header, players, referees })
}

/// Lecture des attributs de l'élément racine (`Contest::LoadXmlDoc`).
pub fn read_header(root: roxmltree::Node, team_event: bool) -> ContestHeader {
    let mut h = ContestHeader { team_event, ..Default::default() };
    h.raw_attributes = root.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect();
    let get = |n: &str| root.attribute(n).map(str::to_string);
    h.color = get("Couleur");
    h.authority = get("Championnat");
    h.fie_id = get("ID");
    h.net_id = root.attribute("NetID").and_then(|s| u64::from_str_radix(s.trim(), 16).ok());
    if let Some(n) = get("TitreLong") {
        h.name = n;
    }
    if let Some(date) = root.attribute("Date").or_else(|| root.attribute("DateDebut")) {
        let parts: Vec<&str> = date.splitn(3, '.').collect();
        if parts.len() == 3 {
            h.day = crate::formats::encoding::atoi(parts[0]) as u32;
            h.month = crate::formats::encoding::atoi(parts[1]) as u32;
            h.year = crate::formats::encoding::atoi(parts[2]) as u32;
        }
    }
    h.checkin_time = get("Appel");
    h.scratch_time = get("Scratch");
    h.start_time = get("Debut");
    if let Some(w) = get("Arme") {
        h.weapon = w;
    }
    if let Some(g) = root.attribute("Sexe").and_then(ContestHeader::parse_gender) {
        h.gender = g.to_string();
    }
    if let Some(c) = get("Categorie") {
        h.category = c;
    }
    h.level = get("Niveau");
    if let Some(o) = get("Organisateur") {
        h.organizer = o;
    }
    h.source = get("Source");
    if let Some(w) = get("URLorganisateur") {
        h.web_site = w;
    }
    if let Some(l) = get("Lieu") {
        h.location = l;
    }
    if let Some(l) = get("Label") {
        h.label = l;
    }
    h.score_stuffing = root.attribute("ScoreAleatoire").map(|s| crate::formats::encoding::atoi(s) != 0).unwrap_or(false);
    let num = |n: &str| root.attribute(n).map(|s| crate::formats::encoding::atoi(s).max(0) as u32);
    h.minimum_team_size = num("TailleMinimaleEquipe");
    h.default_classification = num("ClassementParDefaut");
    h.manual_classification = num("ClassementManuel");
    h
}

/// `Checkin::LoadList (xmlNode *, …)` : parcourt le nœud de liste, ses
/// enfants et ses frères ; s'arrête au premier élément inattendu.
fn load_list(
    node: roxmltree::Node,
    class: PlayerClass,
    gathered: Option<PlayerClass>,
    out: &mut PlayerList,
    mut owner: Option<usize>,
) {
    let tag = class.xml_tag();
    let list_tag = format!("{tag}s");
    let mut current = Some(node);
    while let Some(n) = current {
        if n.is_element() {
            let name = n.tag_name().name();
            if name == tag {
                owner = Some(load_player(n, class, None, out));
            } else if gathered.map(|g| g.xml_tag() == name).unwrap_or(false) {
                load_player(n, gathered.unwrap(), owner, out);
            } else if name != list_tag {
                return;
            }
        }
        // L'arrêt d'un niveau ne remonte pas au niveau appelant (comme en C++).
        if let Some(first) = n.first_child() {
            load_list(first, class, gathered, out, owner);
        }
        current = n.next_sibling();
    }
}

/// `Checkin::LoadPlayer` + `RegisterPlayer`.
fn load_player(node: roxmltree::Node, class: PlayerClass, owner: Option<usize>, out: &mut PlayerList) -> usize {
    let mut player = Player::new(class);
    player.load(node);
    if class == PlayerClass::Team {
        // Team::Load : sans nom (ou « ??? »), le nom est l'identifiant brut.
        let missing = player.str_attr("name").map(|n| n == "???").unwrap_or(true);
        if missing {
            if let Some(id) = node.attribute("ID") {
                player.set_str(&AttrId::global("name"), id);
            }
        }
    }
    if let Some(cat) = discrete::catalog() {
        cat.guess_player_organization(&mut player, "league");
        cat.guess_player_organization(&mut player, "region");
    }
    out.push(player, owner)
}

/// `CheckinSupervisor::ConvertFromBaseToResult` (partie « classement ») :
/// `final_rank` devient `ranking`. Le calcul de présence des équipes
/// (`Team::SetAttendingFromMembers`) dépend de la configuration de
/// l'épreuve et revient au module `contest`.
pub fn convert_from_base_to_result(players: &mut PlayerList) {
    for e in &mut players.entries {
        let place = AttrId::global("final_rank");
        if let Some(v) = e.player.get(&place).map(|v| v.as_uint()) {
            e.player.set_uint(&AttrId::global("ranking"), v);
            e.player.remove(&place);
        }
    }
}

/// Personnes à écrire dans un fichier XML de compétition.
#[derive(Debug, Clone, Default)]
pub struct XmlPeople {
    /// Liste d'appel des tireurs (ordre d'écriture).
    pub fencers: Vec<PlayerRef>,
    pub teams: Vec<PlayerRef>,
    pub referees: Vec<PlayerRef>,
}

/// Écriture d'un fichier XML de compétition au format résultat
/// (`Contest::SaveHeader` + `Schedule::SavePeoples`). `phases_xml` est
/// l'élément `<Phases>` déjà sérialisé par le module `contest`, s'il y en a.
pub fn write(header: &ContestHeader, store: &PlayerStore, people: &XmlPeople, phases_xml: Option<&str>, version: &str) -> String {
    let mut w = XmlWriter::document();
    let root = if header.team_event { "CompetitionParEquipes" } else { "CompetitionIndividuelle" };
    w.dtd(root);
    w.comment(&format!("\n           By BellePoule (V{version})\n\n   http://betton.escrime.free.fr/index.php/bellepoule\n"));
    w.start(root);
    write_header_attributes(&mut w, header);
    write_people(&mut w, header.team_event, store, people, false);
    if let Some(p) = phases_xml {
        w.raw("\n ");
        w.raw(p.trim());
    }
    w.end();
    w.finish()
}

/// Attributs de l'en-tête, dans l'ordre de `Contest::SaveHeader`.
pub fn write_header_attributes(w: &mut XmlWriter, h: &ContestHeader) {
    if let Some(c) = &h.color {
        w.attr("Couleur", c);
    }
    if let Some(a) = &h.authority {
        w.attr("Championnat", a);
    }
    if let Some(id) = &h.fie_id {
        w.attr("ID", id);
    }
    if let Some(n) = h.net_id {
        w.attr("NetID", &format!("{n:x}"));
    }
    w.attr("Annee", &h.year.to_string());
    w.attr("Arme", &h.weapon);
    w.attr("Sexe", &h.gender);
    w.attr("Organisateur", &h.organizer);
    if let Some(s) = &h.source {
        w.attr("Source", s);
    }
    w.attr("Categorie", &h.category);
    if let Some(l) = &h.level {
        w.attr("Niveau", l);
    }
    w.attr("Date", &format!("{:02}.{:02}.{}", h.day, h.month, h.year));
    for (name, t) in [("Appel", &h.checkin_time), ("Scratch", &h.scratch_time), ("Debut", &h.start_time)] {
        if let Some(t) = t {
            w.attr(name, t);
        }
    }
    w.attr("TitreLong", &h.name);
    w.attr("URLorganisateur", &h.web_site);
    w.attr("ScoreAleatoire", if h.score_stuffing { "1" } else { "0" });
    for (name, v) in [
        ("TailleMinimaleEquipe", h.minimum_team_size),
        ("ClassementParDefaut", h.default_classification),
        ("ClassementManuel", h.manual_classification),
    ] {
        if let Some(v) = v {
            w.attr(name, &v.to_string());
        }
    }
    w.attr("Lieu", &h.location);
    w.attr("Label", &h.label);
}

/// `Schedule::SavePeoples` : `<Tireurs>`, `<Equipes>`, `<Arbitres>`.
/// Sauf si `separately` (AskFred), les tireurs membres d'une équipe ne sont
/// écrits qu'à l'intérieur de leur équipe pour une épreuve par équipes.
pub fn write_people(w: &mut XmlWriter, team_event: bool, store: &PlayerStore, people: &XmlPeople, separately: bool) {
    write_people_with_referees(w, team_event, store, store, people, separately);
}

/// Variante pour les compétitions dont les arbitres ont leur propre magasin.
pub fn write_people_with_referees(w: &mut XmlWriter, team_event: bool, store: &PlayerStore, referees: &PlayerStore, people: &XmlPeople, separately: bool) {
    w.start("Tireurs");
    for r in &people.fencers {
        let Some(p) = store.get(*r) else { continue };
        if team_event && !separately && p.team.is_some() {
            continue;
        }
        p.save(w, false);
    }
    w.end();
    w.start("Equipes");
    for r in &people.teams {
        let Some(t) = store.get(*r) else { continue };
        write_team(w, store, t, team_event);
    }
    w.end();
    w.start("Arbitres");
    for r in &people.referees {
        if let Some(p) = referees.get(*r) {
            p.save(w, false);
        }
    }
    w.end();
}

/// `Team::Save` : attributs puis membres (si l'épreuve est par équipes).
pub fn write_team(w: &mut XmlWriter, store: &PlayerStore, team: &Player, with_members: bool) {
    w.start(team.class.xml_tag());
    team.write_attributes(w, false);
    if with_members {
        for m in &team.members {
            if let Some(p) = store.get(*m) {
                p.save(w, false);
            }
        }
    }
    w.end();
}
