//! Lecture des pré-inscriptions AskFred (port de `askfred/reader.cpp`).
//!
//! Le fichier (`Context="PreReg"`) contient un tournoi, ses épreuves, et des
//! bases de tireurs, d'équipes et de clubs. Chaque épreuve devient une
//! compétition ; ses compétiteurs sont convertis en `Player` par
//! [`AskFredFile::event_players`].
//!
//! Les listes sont restituées dans l'ordre du C++ (qui les construit par
//! `g_list_prepend`) : épreuves et compétiteurs d'une épreuve en ordre
//! inverse du fichier.

use std::collections::HashMap;

use chrono::Datelike;

use super::NAMESPACE;
use crate::formats::encoding::{atoi, decode_xml, parse_xml, truncate_bytes};
use crate::formats::team::{set_attributes_from_members, PlayerList};
use crate::player::{AttrId, Player, PlayerClass};
use crate::Result;

/// Divisions de l'USFA et du Canada (`Parser::_divisions`).
pub const DIVISIONS: &[(u32, &str)] = &[
    (1, "Western WA"),
    (2, "Inland Emp"),
    (3, "Central CA"),
    (4, "Mt. Valley"),
    (5, "North Coast"),
    (6, "Northern CA"),
    (7, "Orange Coast"),
    (8, "San Bern'do"),
    (9, "San Diego"),
    (10, "Southern CA"),
    (11, "Oregon"),
    (12, "Utah-S.Idaho"),
    (13, "Alaska"),
    (14, "Nevada"),
    (15, "Hawaii"),
    (18, "Columbus OH"),
    (19, "Indiana"),
    (20, "Michigan"),
    (21, "Northern OH"),
    (22, "Kentucky"),
    (23, "Southwest OH"),
    (24, "Metro NYC"),
    (25, "Capitol"),
    (26, "Central PA"),
    (27, "Harrisburg"),
    (28, "Maryland"),
    (29, "New Jersey"),
    (30, "Philadelphia"),
    (31, "S. Jersey"),
    (32, "Western PA"),
    (33, "Illinois"),
    (34, "Iowa"),
    (35, "Minnesota"),
    (36, "St. Louis"),
    (37, "Wisconsin"),
    (39, "Connecticut"),
    (40, "Huds-Berks"),
    (41, "Long Island"),
    (42, "New England"),
    (43, "Northeast"),
    (44, "West-Rock"),
    (45, "Western NY"),
    (46, "Arizona"),
    (47, "Border TX"),
    (48, "Colorado"),
    (49, "Kansas"),
    (50, "Nebr-S.Dak"),
    (51, "New Mexico"),
    (52, "Plains TX"),
    (53, "Wyoming"),
    (54, "Alabama"),
    (55, "Central FL"),
    (56, "Gateway FL"),
    (57, "Georgia"),
    (58, "Gold Coast"),
    (59, "N. Carolina"),
    (60, "S. Carolina"),
    (61, "Tennessee"),
    (62, "Virginia"),
    (63, "Ark-La-Miss"),
    (64, "Gulf Coast"),
    (65, "Louisiana"),
    (66, "North TX"),
    (67, "Oklahoma"),
    (68, "South TX"),
    (69, "National"),
    (82, "Green Mt."),
    (294, "Northeast PA"),
    (70, "British Columbia"),
    (71, "Alberta"),
    (72, "Saskatchewan"),
    (73, "Manitoba"),
    (74, "Yukon"),
    (75, "Ontario"),
    (76, "New Brunswick"),
    (77, "Prince Edward Island"),
    (78, "Nunavut"),
    (79, "Québec"),
    (80, "Nova Scotia"),
    (81, "Newfoundland"),
];

pub fn division_name(id: u32) -> Option<&'static str> {
    DIVISIONS.iter().find(|(i, _)| *i == id).map(|(_, n)| *n)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Club {
    /// Abréviation du club.
    pub name: Option<String>,
    /// Nom de la division (ligue).
    pub division: Option<String>,
}

/// Classement USFA par arme : (lettre+année, rang calculé).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rating {
    pub rating: Option<String>,
    pub ranking: u32,
}

impl Default for Rating {
    fn default() -> Self {
        Rating { rating: None, ranking: 9999 }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Competitor {
    pub competitor_id: Option<String>,
    pub last_name: Option<String>,
    pub first_name: Option<String>,
    pub birth: Option<String>,
    pub gender: Option<String>,
    pub licence: Option<String>,
    /// Index dans [`AskFredFile::clubs`].
    pub club: Option<usize>,
    pub saber: Rating,
    pub epee: Rating,
    pub foil: Rating,
    /// Membres (équipes), index dans [`AskFredFile::competitors`], dans
    /// l'ordre inverse du fichier (comme le C++).
    pub members: Vec<usize>,
}

impl Competitor {
    /// `Competitor::SetRanking` : `A13` → 1000×(1 + rang de la lettre) +
    /// 100 × ancienneté de l'année.
    pub fn set_ranking(&mut self, weapon: &str, usfa_rating: &str, current_year: i32) {
        if usfa_rating.len() != 5 {
            return;
        }
        let letter = usfa_rating.as_bytes()[0];
        if !letter.is_ascii_alphabetic() {
            return;
        }
        let rating_year = atoi(&usfa_rating[1..]) as i32;
        let ranking = (1000 * (1 + letter as i32 - b'A' as i32) + 100 * (current_year - rating_year)) as u32;
        let target = match weapon {
            "Foil" => &mut self.foil,
            "Epee" => &mut self.epee,
            "Saber" => &mut self.saber,
            _ => return,
        };
        target.rating = Some(usfa_rating.to_string());
        target.ranking = ranking;
    }

    fn rating_for(&self, weapon: &str) -> Option<&Rating> {
        match weapon.chars().next() {
            Some('F') => Some(&self.foil),
            Some('E') => Some(&self.epee),
            Some('S') => Some(&self.saber),
            _ => None,
        }
    }

    /// `GetRating` : « U » (non classé) par défaut.
    pub fn rating(&self, weapon: &str) -> String {
        self.rating_for(weapon).and_then(|r| r.rating.clone()).unwrap_or_else(|| "U".to_string())
    }

    pub fn ranking(&self, weapon: &str) -> u32 {
        self.rating_for(weapon).map(|r| r.ranking).unwrap_or(9999)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Event {
    pub name: Option<String>,
    pub location: Option<String>,
    pub organizer: Option<String>,
    pub tournament_id: Option<String>,
    pub event_id: Option<String>,
    /// `M`, `F` ou `FM`.
    pub gender: Option<String>,
    /// `E`, `F` ou `S`.
    pub weapon: Option<String>,
    /// Catégorie BellePoule (`M17`, `S`, `V1`…).
    pub category: Option<String>,
    pub team_event: bool,
    pub hour: u32,
    pub minute: u32,
    pub day: u32,
    pub month: u32,
    pub year: u32,
    /// Index dans [`AskFredFile::competitors`].
    pub competitors: Vec<usize>,
}

impl Event {
    /// `Contest::LoadAskFred` : source de la compétition.
    pub fn source(&self) -> String {
        format!("AskFred:{}", self.tournament_id.as_deref().unwrap_or("(null)"))
    }

    /// Heures d'appel, de scratch (+15 min) et de début (+30 min).
    pub fn times(&self) -> [(u32, u32); 3] {
        let base = self.hour * 60 + self.minute;
        let at = |m: u32| (((base + m) / 60) % 24, (base + m) % 60);
        [at(0), at(15), at(30)]
    }
}

#[derive(Debug, Clone, Default)]
pub struct AskFredFile {
    pub name: Option<String>,
    pub location: Option<String>,
    pub organizer: Option<String>,
    pub tournament_id: Option<String>,
    pub clubs: Vec<Club>,
    pub competitors: Vec<Competitor>,
    pub events: Vec<Event>,
}

fn is_fred(n: &roxmltree::Node, name: &str) -> bool {
    n.is_element() && n.tag_name().name() == name && n.tag_name().namespace() == Some(NAMESPACE)
}

fn fred_children<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> {
    n.children().filter(move |c| is_fred(c, name))
}

fn text_of(n: roxmltree::Node) -> Option<String> {
    n.first_child().filter(|c| c.is_text()).and_then(|c| c.text()).map(str::to_string)
}

impl AskFredFile {
    pub fn parse_file(path: &std::path::Path) -> Result<AskFredFile> {
        AskFredFile::parse(&std::fs::read(path)?)
    }

    pub fn parse(bytes: &[u8]) -> Result<AskFredFile> {
        AskFredFile::parse_with_year(bytes, chrono::Local::now().year())
    }

    /// `Parser::Parser` ; un fichier qui n'est pas `Context="PreReg"` donne
    /// un résultat vide (`CheckData`).
    pub fn parse_with_year(bytes: &[u8], current_year: i32) -> Result<AskFredFile> {
        let text = decode_xml(bytes)?;
        let doc = parse_xml(&text)?;
        let mut file = AskFredFile::default();
        let root = doc.root_element();
        if !is_fred(&root, "FencingData") || root.attribute("Context") != Some("PreReg") {
            return Ok(file);
        }

        // Tournoi
        for t in fred_children(root, "Tournament") {
            if let Some(name) = t.attribute("Name") {
                let mut n = truncate_bytes(name, 30).to_string();
                if name.len() > 30 {
                    n = format!("{}...", truncate_bytes(&n, 27));
                }
                file.name = Some(n);
            }
            if let Some(v) = t.attribute("TournamentID") {
                file.tournament_id = Some(v.to_string());
            }
            if let Some(v) = t.attribute("Org") {
                file.organizer = Some(v.to_string());
            }
            if let Some(v) = t.attribute("Location") {
                file.location = Some(v.to_string());
            }
        }

        // Clubs
        let mut clubs_by_id: HashMap<i64, usize> = HashMap::new();
        for db in fred_children(root, "ClubDatabase") {
            for c in fred_children(db, "Club") {
                let club = Club {
                    name: c.attribute("Abbreviation").map(str::to_string),
                    division: c.attribute("DivisionID").and_then(|d| division_name(atoi(d) as u32)).map(str::to_string),
                };
                file.clubs.push(club);
                if let Some(id) = c.attribute("ClubID") {
                    clubs_by_id.insert(atoi(id), file.clubs.len() - 1);
                }
            }
        }

        // Tireurs
        let mut competitors_by_id: HashMap<i64, usize> = HashMap::new();
        for db in fred_children(root, "FencerDatabase") {
            for f in fred_children(db, "Fencer") {
                let mut c = Competitor::default();
                let index = file.competitors.len();
                if let Some(id) = f.attribute("FencerID") {
                    c.competitor_id = Some(id.to_string());
                    competitors_by_id.insert(atoi(id), index);
                }
                c.last_name = f.attribute("LastName").map(str::to_string);
                c.first_name = f.attribute("FirstName").map(str::to_string);
                c.gender = f.attribute("Gender").map(str::to_string);
                c.birth = f.attribute("BirthYear").map(str::to_string);
                if let Some(id) = f.attribute("ClubID1") {
                    c.club = clubs_by_id.get(&atoi(id)).copied();
                }
                for n in f.children().filter(|n| n.is_element()) {
                    match n.tag_name().name() {
                        "Membership" => {
                            if let Some(t) = text_of(n) {
                                c.licence = Some(t);
                            }
                        }
                        "Rating" => {
                            if let (Some(weapon), Some(t)) = (n.attribute("Weapon"), text_of(n)) {
                                c.set_ranking(weapon, &t, current_year);
                            }
                        }
                        _ => {}
                    }
                }
                file.competitors.push(c);
            }
        }

        // Équipes
        for db in fred_children(root, "TeamDatabase") {
            for t in fred_children(db, "Team") {
                let mut c = Competitor::default();
                let index = file.competitors.len();
                if let Some(id) = t.attribute("TeamID") {
                    c.competitor_id = Some(id.to_string());
                    competitors_by_id.insert(atoi(id), index);
                }
                c.last_name = t.attribute("Name").map(str::to_string);
                if let Some(id) = t.attribute("ClubID") {
                    c.club = clubs_by_id.get(&atoi(id)).copied();
                }
                for n in t.children().filter(|n| n.is_element() && n.tag_name().name() == "Fencer") {
                    if let Some(id) = n.attribute("FencerID") {
                        if let Some(m) = competitors_by_id.get(&atoi(id)) {
                            c.members.insert(0, *m);
                        }
                    }
                }
                file.competitors.push(c);
            }
        }

        // Épreuves
        for tournament in fred_children(root, "Tournament") {
            for e in fred_children(tournament, "Event") {
                let mut ev = Event {
                    name: file.name.clone(),
                    location: file.location.clone(),
                    organizer: file.organizer.clone(),
                    tournament_id: file.tournament_id.clone(),
                    ..Default::default()
                };
                ev.event_id = e.attribute("EventID").map(str::to_string);
                ev.gender = e.attribute("Gender").map(|g| match g {
                    "Women" => "F".to_string(),
                    "Men" => "M".to_string(),
                    _ => "FM".to_string(),
                });
                ev.weapon = e.attribute("Weapon").map(|w| match w {
                    "Epee" => "E".to_string(),
                    "Foil" => "F".to_string(),
                    _ => "S".to_string(),
                });
                if let Some(t) = e.attribute("IsTeam") {
                    ev.team_event = t == "True";
                }
                ev.category = e.attribute("AgeLimitMax").map(|a| {
                    match a {
                        "Y8" => "M9",
                        "Y10" => "M11",
                        "Y12" | "Y14" => "M14",
                        "Cadet" => "M17",
                        "Junior" => "M20",
                        "Senior" => "S",
                        "Vet40" => "V1",
                        "Vet50" => "V2",
                        "Vet60" => "V3",
                        "Vet70" => "V4",
                        "VetCombined" => "VET",
                        _ => "S",
                    }
                    .to_string()
                });
                if let Some(dt) = e.attribute("EventDateTime") {
                    let parts: Vec<&str> = dt.splitn(6, [' ', '-', ':']).collect();
                    if parts.len() >= 5 {
                        ev.year = atoi(parts[0]) as u32;
                        ev.month = atoi(parts[1]) as u32;
                        ev.day = atoi(parts[2]) as u32;
                        ev.hour = atoi(parts[3]) as u32;
                        ev.minute = atoi(parts[4]) as u32;
                    }
                }
                for n in e.children().filter(|n| n.is_element()) {
                    if let Some(id) = n.attribute("CompetitorID") {
                        if let Some(c) = competitors_by_id.get(&atoi(id)) {
                            ev.competitors.insert(0, *c);
                        }
                    }
                }
                file.events.insert(0, ev);
            }
        }
        Ok(file)
    }

    /// `Competitor::CreatePlayer` : tireur, ou équipe avec ses membres.
    /// Renvoie le joueur et ses membres (ordre de `Team::GetMemberList`).
    pub fn create_player(&self, competitor: usize, weapon: &str) -> (Player, Vec<Player>) {
        let c = &self.competitors[competitor];
        let mut members = Vec::new();
        let mut player = if c.members.is_empty() {
            Player::new(PlayerClass::Fencer)
        } else {
            // Les membres sont ajoutés en tête (`Team::AddMember`), ce qui
            // rétablit l'ordre du fichier.
            for m in &c.members {
                let (p, _) = self.create_player(*m, weapon);
                members.insert(0, p);
            }
            let mut team = Player::new(PlayerClass::Team);
            let refs: Vec<&Player> = members.iter().collect();
            set_attributes_from_members(&mut team, &refs);
            team
        };
        let set = |p: &mut Player, name: &str, v: &Option<String>| {
            if let Some(v) = v {
                p.set_str(&AttrId::global(name), v);
            }
        };
        set(&mut player, "name", &c.last_name);
        set(&mut player, "first_name", &c.first_name);
        set(&mut player, "birth_date", &c.birth);
        set(&mut player, "gender", &c.gender);
        if let Some(club) = c.club.and_then(|i| self.clubs.get(i)) {
            set(&mut player, "club", &club.name);
            set(&mut player, "league", &club.division);
        }
        set(&mut player, "plugin_ID", &c.competitor_id);
        set(&mut player, "licence", &c.licence);
        player.set_uint(&AttrId::global("ranking"), c.ranking(weapon));
        player.set_str(&AttrId::global("rating"), &c.rating(weapon));
        (player, members)
    }

    /// Joueurs d'une épreuve, prêts pour la liste d'appel (équipe suivie de
    /// ses membres, comme `Contest::LoadAskFred`).
    pub fn event_players(&self, event: &Event) -> PlayerList {
        let weapon = event.weapon.as_deref().unwrap_or("");
        let mut list = PlayerList::default();
        for c in &event.competitors {
            let (player, members) = self.create_player(*c, weapon);
            let owner = list.push(player, None);
            for m in members {
                list.push(m, Some(owner));
            }
        }
        list
    }
}
