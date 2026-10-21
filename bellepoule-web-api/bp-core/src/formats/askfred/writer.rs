//! Écriture des résultats au format AskFred (port de `askfred/writer.cpp`
//! et `Contest::DumpToFRD`).
//!
//! Le schéma AskFred reçoit la sauvegarde BellePoule habituelle (noms
//! français : `Tireur`, `Poule`, `Match`…) et :
//! 1. **collecte** tout le document dans un arbre d'éléments, en notant les
//!    identifiants (tireurs, clubs) et les forfaits par tour ;
//! 2. masque les éléments inutiles (tireurs abandonnant dans les poules…) ;
//! 3. **réécrit** l'arbre en traduisant noms et valeurs selon les tables de
//!    `Scheme::Scheme` (ex. `Tireur` → `Fencer`, `Statut="Q"` → `P`), avec
//!    numérotation des séquences et ajout des `Bye`.

use std::collections::{HashMap, HashSet};

use crate::formats::fie_xml::{write_people_with_referees, write_team, XmlPeople};
use crate::formats::header::{weapon_msgid, ContestHeader};
use crate::formats::scheme::{replay_children, XmlScheme};
use crate::player::PlayerStore;
use crate::xml::XmlWriter;

/// Tables de traduction (`Scheme::_element_base`), par chemin d'élément.
fn scheme_table(key: &str) -> &'static [(&'static str, &'static str)] {
    match key {
        "FencingData" => &[
            ("Tireurs", "FencerDatabase"),
            ("Tireur", "Fencer"),
            ("Arbitres", "RefereeDatabase"),
            ("Arbitre", "Referee"),
            ("Equipes", "TeamDatabase"),
            ("Equipe", "Team"),
            ("Nom", "LastName"),
            ("Prenom", "FirstName"),
            ("DateNaissance", "BirthYear"),
            ("Sexe", "Gender"),
            ("Club", "ClubID"),
            ("Classement", "Place"),
            ("Phases", "RoundResults"),
            ("Poule", "Pool"),
            ("TourDePoules", "Round"),
            ("PhaseDeTableaux", "Round"),
            ("REF", "CompetitorID"),
            ("RangInitial", "Seed"),
            ("Statut", "Status"),
            ("Q", "P"),
            ("A", "W"),
            ("NoDansLaPoule", "CompetitorNum"),
            ("NbMatches", "Bouts"),
            ("NbVictoires", "Victories"),
            ("TD", "TS"),
            ("RangPoule", "Rank"),
            ("Tableau", "Table"),
            ("Taille", "Of"),
            ("Match", "Bout"),
        ],
        "Tournament" => &[
            ("Tireurs", "FinalResults"),
            ("Equipes", "FinalResults"),
            ("Tireur", "Result"),
            ("Equipe", "Result"),
            ("M", "Men"),
            ("F", "Female"),
            ("FM", "Mixed"),
            ("Sabre", "Saber"),
            ("Epée", "Epee"),
        ],
        "Phases" => &[("Tireur", "Competitor"), ("Equipe", "Competitor")],
        "Tableau/Match/Tireur" => &[("A", "D"), ("E", "D")],
        "Event/Tireurs" | "Event/Equipes" => &[("PluginID", "CompetitorID"), ("ID", "CompetitorID")],
        "FencingData/Tireurs" => &[("PluginID", "FencerID"), ("ID", "FencerID"), ("Club", "clubID1")],
        "Equipe/Tireur" => &[("PluginID", "FencerID"), ("ID", "FencerID"), ("Nom", "LastName")],
        "FencingData/Equipes" => &[("PluginID", "TeamID"), ("ID", "TeamID"), ("Nom", "Name")],
        "Arbitre" => &[("ID", "RefereeID")],
        "Club" => &[("Club", "Club")],
        _ => &[],
    }
}

fn has_scheme(key: &str) -> bool {
    !scheme_table(key).is_empty()
}

/// `IdFixer` : identifiants AskFred des tireurs (négatifs s'ils ne viennent
/// pas d'AskFred) et des clubs (numérotés `-1`, `-2`…).
#[derive(Debug, Default)]
struct IdFixer {
    primary_title: &'static str,
    generate_primary_id: bool,
    population_count: u32,
    populations: Vec<(String, String)>,
}

impl IdFixer {
    fn new(primary_title: &'static str, generate_primary_id: bool) -> Self {
        IdFixer { primary_title, generate_primary_id, ..Default::default() }
    }

    fn knows(&self, owner: &str, name: &str) -> bool {
        (owner == "Tireur" || owner == "Equipe") && (name == self.primary_title || name == "PluginID")
    }

    fn push_id(&mut self, name: &str, value: &str) {
        if self.get_id(value).is_some() {
            return;
        }
        if name == self.primary_title {
            let id = if self.generate_primary_id {
                self.population_count += 1;
                format!("-{}", self.population_count)
            } else {
                format!("-{value}")
            };
            self.populations.push((value.to_string(), id));
        } else if name == "PluginID" {
            self.populations.push((value.to_string(), value.to_string()));
        }
    }

    fn get_id(&self, of: &str) -> Option<&str> {
        self.populations.iter().find(|(k, _)| k == of).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug)]
struct Element {
    name: String,
    scheme: Option<&'static str>,
    round: Option<usize>,
    seq: i32,
    visible: bool,
    outline: Vec<String>,
    reference: Option<String>,
    parent: Option<usize>,
    children: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Collecting,
    PostCollecting,
    Flushing,
}

/// Schéma d'écriture AskFred (`AskFred::Writer::Scheme`).
pub struct AskFredScheme {
    writer: XmlWriter,
    mode: Mode,
    nodes: Vec<Element>,
    current: Option<usize>,
    root: Option<usize>,
    rounds: Vec<HashSet<String>>,
    fencer_ids: IdFixer,
    club_ids: IdFixer,
    aliases: HashMap<String, String>,
}

impl Default for AskFredScheme {
    fn default() -> Self {
        Self::new()
    }
}

const SCHEME_KEYS: [&str; 12] = [
    "FencingData",
    "Tournament",
    "Phases",
    "Tableau/Match/Tireur",
    "Event/Tireurs",
    "Event/Equipes",
    "FencingData/Tireurs",
    "Equipe/Tireur",
    "FencingData/Equipes",
    "Arbitre",
    "Club",
    "",
];

fn static_key(path: &str) -> Option<&'static str> {
    SCHEME_KEYS.iter().find(|k| !k.is_empty() && **k == path).copied()
}

impl AskFredScheme {
    pub fn new() -> Self {
        AskFredScheme {
            writer: XmlWriter::document(),
            mode: Mode::Collecting,
            nodes: Vec::new(),
            current: None,
            root: None,
            rounds: Vec::new(),
            fencer_ids: IdFixer::new("ID", false),
            club_ids: IdFixer::new("Club", true),
            aliases: HashMap::new(),
        }
    }

    /// Document produit (complet une fois l'élément racine refermé).
    pub fn finish(self) -> String {
        self.writer.finish()
    }

    // --- Outline ----------------------------------------------------------

    fn add_to_outline(&mut self, node: usize, name: &str, value: &str) {
        let e = &mut self.nodes[node];
        match e.outline.iter().position(|s| s == name) {
            Some(i) if i + 1 < e.outline.len() => e.outline[i + 1] = value.to_string(),
            Some(_) => e.outline.push(value.to_string()),
            None => {
                e.outline.push(name.to_string());
                e.outline.push(value.to_string());
            }
        }
        if name == "REF" {
            e.reference = Some(value.to_string());
        }
    }

    fn get_value(&self, node: usize, of: &str) -> Option<String> {
        let o = &self.nodes[node].outline;
        o.iter().position(|s| s == of).and_then(|i| o.get(i + 1)).cloned()
    }

    fn outline(&self, node: usize) -> Option<&[String]> {
        let e = &self.nodes[node];
        e.visible.then_some(e.outline.as_slice())
    }

    fn is_withdrawn(&self, node: usize) -> bool {
        let e = &self.nodes[node];
        match (e.round, &e.reference) {
            (Some(r), Some(reference)) => self.rounds[r].contains(reference),
            _ => false,
        }
    }

    // --- Traduction ---------------------------------------------------------

    fn translate(&self, term: &str) -> String {
        let mut cur = self.current;
        while let Some(n) = cur {
            if let Some(key) = self.nodes[n].scheme {
                if let Some((_, t)) = scheme_table(key).iter().find(|(k, _)| *k == term) {
                    return t.to_string();
                }
            }
            cur = self.nodes[n].parent;
        }
        term.to_string()
    }

    fn translate_value(&mut self, term: &str, value: &str) -> Option<String> {
        let owner = self.current?;
        let parent = self.nodes[owner].parent;

        if self.mode == Mode::Collecting {
            self.add_to_outline(owner, term, value);
            if let Some(p) = parent {
                let parent_name = self.nodes[p].name.clone();
                if self.translate(&parent_name) == "FinalResults" && term == "Statut" {
                    self.add_to_outline(owner, "Excluded", if value == "E" { "True" } else { "False" });
                    self.add_to_outline(owner, "DNF", if value == "A" { "True" } else { "False" });
                } else if parent_name == "RoundSeeding" && term == "Statut" && (value == "A" || value == "E") {
                    // Element::Withdraw
                    if let (Some(r), Some(reference)) = (self.nodes[owner].round, self.nodes[owner].reference.clone()) {
                        self.rounds[r].insert(reference);
                    }
                }
                let owner_name = self.nodes[owner].name.clone();
                if self.fencer_ids.knows(&owner_name, term) {
                    self.fencer_ids.push_id(term, value);
                } else if self.club_ids.knows(&owner_name, term) {
                    self.club_ids.push_id(term, value);
                }
            }
            return None;
        }

        if self.nodes[owner].name == "Tireur" {
            if term == "ID" {
                if let Some(alias) = self.get_value(owner, "PluginID") {
                    self.aliases.insert(value.to_string(), alias);
                    return None;
                }
            } else if term == "REF" {
                return Some(self.aliases.get(value).cloned().unwrap_or_else(|| value.to_string()));
            }
        }

        if term == "DateNaissance" {
            if let Some(pos) = value.rfind('.') {
                return Some(value[pos + 1..].to_string());
            }
        } else if let Some(p) = parent {
            if self.nodes[p].name == "RoundSeeding" && term == "Statut" && self.is_withdrawn(owner) {
                return Some("P".to_string());
            }
        }

        if term == "Statut" || term == "Gender" || term == "Weapon" {
            return Some(self.translate(value));
        }

        // RemoveNonBreakingSpaces (sans effet sur les chaînes de 2 octets ou moins).
        if value.len() > 2 {
            Some(value.replace('\u{a0}', " "))
        } else {
            Some(value.to_string())
        }
    }

    // --- Écriture de base (XmlScheme) ---------------------------------------

    fn base_start(&mut self, name: &str) {
        let t = self.translate(name);
        self.writer.start(&t);
    }

    fn base_end(&mut self) {
        self.writer.end();
    }

    fn base_write_attribute(&mut self, term: &str, value: &str) {
        if let Some(v) = self.translate_value(term, value) {
            let t = self.translate(term);
            self.writer.attr(&t, &v);
        }
    }

    fn base_write_format_attribute(&mut self, term: &str, value: &str) {
        let t = self.translate(term);
        self.writer.attr(&t, value);
    }

    // --- Collecte -------------------------------------------------------------

    fn push_element(&mut self, name: &str) -> usize {
        // Chemin le plus long d'abord : « A/B/nom », puis « B/nom », puis « nom ».
        let mut chain = Vec::new();
        let mut cur = self.current;
        while let Some(n) = cur {
            chain.insert(0, n);
            cur = self.nodes[n].parent;
        }
        let mut scheme = None;
        for from in 0..chain.len() {
            let mut path = String::new();
            for n in &chain[from..] {
                path.push_str(&self.nodes[*n].name);
                path.push('/');
            }
            path.push_str(name);
            if has_scheme(&path) {
                scheme = static_key(&path);
                break;
            }
        }
        if scheme.is_none() && has_scheme(name) {
            scheme = static_key(name);
        }

        let id = self.nodes.len();
        self.nodes.push(Element {
            name: name.to_string(),
            scheme,
            round: if self.rounds.is_empty() { None } else { Some(self.rounds.len() - 1) },
            seq: 0,
            visible: true,
            outline: Vec::new(),
            reference: None,
            parent: self.current,
            children: Vec::new(),
        });
        match self.current {
            Some(p) => self.nodes[p].children.push(id),
            None => self.root = Some(id),
        }
        self.current = Some(id);

        if matches!(name, "SuiteDeTableaux" | "Pointage" | "ClassementGeneral") {
            self.nodes[id].visible = false;
        } else if let Some(p) = self.nodes[id].parent {
            if let Some(gp) = self.nodes[p].parent {
                let gp_name = self.nodes[gp].name.clone();
                if self.nodes[p].name == "Equipe" && self.translate(&gp_name) == "FinalResults" && name == "Tireur" {
                    self.nodes[id].visible = false;
                }
            }
        }
        id
    }

    fn end_element_named(&mut self, name: &str) {
        if let Some(c) = self.current {
            if self.nodes[c].name == name {
                self.end_element();
            }
        }
    }

    // --- Réécriture -----------------------------------------------------------

    fn preorder(&self, from: usize, out: &mut Vec<usize>) {
        out.push(from);
        for c in &self.nodes[from].children {
            self.preorder(*c, out);
        }
    }

    fn flush(&mut self, root: usize) {
        let mut order = Vec::new();
        self.preorder(root, &mut order);

        self.mode = Mode::PostCollecting;
        for &n in &order {
            if self.nodes[n].children.is_empty() {
                self.check_visibility(n);
            }
        }

        self.mode = Mode::Flushing;
        for &n in &order {
            self.flush_node(n);
        }
    }

    /// `CheckVisibilityNodeFunc` : un tireur abandonnant disparaît de sa
    /// poule, ainsi que ses matchs de poule.
    fn check_visibility(&mut self, n: usize) {
        if !self.is_withdrawn(n) {
            return;
        }
        let Some(p) = self.nodes[n].parent else { return };
        match self.nodes[p].name.as_str() {
            "Poule" => self.nodes[n].visible = false,
            "Match" => {
                if let Some(gp) = self.nodes[p].parent {
                    if self.nodes[gp].name == "Poule" {
                        let opponents = self.nodes[p].children.clone();
                        for o in opponents {
                            self.nodes[o].visible = false;
                        }
                        self.nodes[p].visible = false;
                    }
                }
            }
            _ => {}
        }
    }

    fn flush_element(&mut self, node: usize, parent: Option<usize>) -> bool {
        let Some(outline) = self.outline(node).map(|o| o.to_vec()) else { return false };
        self.base_start(&outline[0]);
        let mut i = 1;
        while i + 1 < outline.len() {
            let term = &outline[i];
            let value = &outline[i + 1];
            i += 2;
            let fixed = match term.as_str() {
                "REF" | "ID" => self.fencer_ids.get_id(value).map(str::to_string),
                "Club" => self.club_ids.get_id(value).map(str::to_string),
                "NoDansLaPoule" => parent.map(|p| self.nodes[p].seq.to_string()),
                _ => None,
            };
            let v = fixed.unwrap_or_else(|| value.clone());
            self.base_write_attribute(term, &v);
        }
        true
    }

    fn next_sibling(&self, n: usize) -> Option<usize> {
        let p = self.nodes[n].parent?;
        let siblings = &self.nodes[p].children;
        let i = siblings.iter().position(|c| *c == n)?;
        siblings.get(i + 1).copied()
    }

    fn flush_node(&mut self, node: usize) {
        self.current = Some(node);
        let parent = self.nodes[node].parent;
        let name = self.nodes[node].name.clone();

        if let Some(p) = parent {
            if self.outline(node).is_some()
                && matches!(name.as_str(), "Tireur" | "Equipe" | "TourDePoules" | "Poule" | "PhaseDeTableaux" | "Tableau")
            {
                self.nodes[p].seq += 1;
            }
        }

        if self.flush_element(node, parent) {
            if let Some(p) = parent {
                let seq = self.nodes[p].seq.to_string();
                let parent_name = self.nodes[p].name.clone();
                match name.as_str() {
                    "TourDePoules" => {
                        self.base_write_attribute("Type", "Pool");
                        self.base_write_attribute("Desc", "Pools");
                        self.base_write_format_attribute("Seq", &seq);
                    }
                    "Poule" => self.base_write_format_attribute("Seq", &seq),
                    "PhaseDeTableaux" => {
                        self.base_write_attribute("Type", "DE");
                        self.base_write_attribute("Desc", "Tables");
                        self.base_write_format_attribute("Seq", &seq);
                    }
                    "Tableau" => self.base_write_format_attribute("Seq", &seq),
                    _ if parent_name == "Match" => self.base_write_format_attribute("Called", &seq),
                    "Equipe" if parent_name == "Equipes" => self.base_write_attribute("DivisionID", "69"),
                    _ => {}
                }
            }
        }

        if self.nodes[node].children.is_empty() {
            while let Some(c) = self.current {
                if self.outline(c).is_some() {
                    if self.nodes[c].name == "Match" && self.nodes[c].seq < 2 {
                        self.base_start("Bye");
                        self.base_end();
                    }
                    self.base_end();
                }
                if self.next_sibling(c).is_none() {
                    self.current = self.nodes[c].parent;
                } else {
                    break;
                }
            }
        }
    }
}

impl XmlScheme for AskFredScheme {
    fn start_element(&mut self, name: &str) {
        let parent = self.current;
        if let Some(p) = parent {
            let parent_name = self.nodes[p].name.clone();
            // Début implicite de la composition d'un tour.
            if (name == "Tireur" || name == "Equipe") && self.translate(&parent_name) == "Round" {
                self.rounds.push(HashSet::new());
                self.start_element("RoundSeeding");
            }
            // Fin implicite.
            if name == "Poule" || name == "SuiteDeTableaux" {
                self.end_element_named("RoundSeeding");
            }
        }

        let owner = self.push_element(name);
        self.nodes[owner].outline.push(name.to_string());
        if let Some(p) = parent {
            if self.nodes[p].name == "Match" {
                self.add_to_outline(owner, "Score", "0");
                self.add_to_outline(owner, "Statut", "V");
            }
        }
    }

    fn end_element(&mut self) {
        if self.mode == Mode::Collecting {
            if let Some(c) = self.current {
                self.current = self.nodes[c].parent;
                if self.current.is_none() {
                    if let Some(root) = self.root {
                        self.flush(root);
                    }
                }
            }
        }
    }

    fn write_attribute(&mut self, name: &str, value: &str) {
        if let Some(v) = self.translate_value(name, value) {
            let t = self.translate(name);
            self.writer.attr(&t, &v);
        }
    }

    fn write_format_attribute(&mut self, name: &str, value: &str) {
        if self.mode == Mode::Collecting {
            if let Some(c) = self.current {
                self.add_to_outline(c, name, value);
            }
            return;
        }
        self.base_write_format_attribute(name, value);
    }

    /// Base des clubs rencontrés (`Scheme::WriteCustom`).
    fn write_custom(&mut self, _what: &str) {
        self.start_element("ClubDatabase");
        let clubs = self.club_ids.populations.clone();
        for (name, id) in clubs {
            self.start_element("Club");
            self.write_attribute("ClubID", &id);
            self.write_attribute("Name", &name);
            self.write_attribute("Abbreviation", &name);
            self.end_element();
        }
        self.end_element();
    }

    fn save_fencers_and_teams_separately(&self) -> bool {
        true
    }
}

/// Données nécessaires à `Contest::DumpToFRD`.
pub struct FrdInput<'a> {
    pub header: &'a ContestHeader,
    pub store: &'a PlayerStore,
    /// Liste d'appel : tireurs, équipes, arbitres (dans l'ordre d'affichage).
    pub people: &'a XmlPeople,
    /// Élément `<Phases>` sérialisé par le module `contest` (`Schedule::Save`).
    pub phases_xml: Option<&'a str>,
}

fn replay_fragment(xml: &str, scheme: &mut dyn XmlScheme) {
    let wrapped = format!("<R>{xml}</R>");
    if let Ok(doc) = crate::xml::parse(&wrapped) {
        replay_children(doc.root_element(), scheme);
    }
}

/// `Contest::DumpToFRD` : document AskFred complet.
///
/// Écart assumé : l'attribut `Weapon` reprend le libellé anglais de l'arme
/// (le C++ utilise le libellé traduit, que la table AskFred ne sait
/// convertir qu'en anglais).
pub fn dump_to_frd(input: &FrdInput) -> String {
    dump_to_frd_with_referees(input, input.store)
}

/// Export avec magasins distincts, sans fusionner des références qui peuvent
/// être identiques pour un tireur et un arbitre.
pub fn dump_to_frd_with_referees(input: &FrdInput, referees: &PlayerStore) -> String {
    let h = input.header;
    let mut s = AskFredScheme::new();
    s.start_element("FencingData");
    s.write_attribute("Version", "5.0");
    s.write_attribute("Context", "Results");
    s.write_attribute("Source", "BellePouleV5.0");
    s.write_attribute("xmlns", super::NAMESPACE);

    s.start_element("Tournament");
    if let Some(id) = h.source.as_deref().and_then(|src| src.rfind(':').map(|i| &src[i + 1..])) {
        s.write_attribute("TournamentID", id);
    }
    s.write_attribute("Name", &h.name);
    s.write_attribute("Location", &h.location);

    s.start_element("Event");
    if let Some(id) = &h.fie_id {
        s.write_attribute("EventID", id);
    }
    s.write_attribute("Weapon", weapon_msgid(&h.weapon));
    s.write_attribute("Gender", &h.gender);
    s.write_attribute("IsTeam", if h.team_event { "True" } else { "False" });

    // Schedule::SaveFinalResults
    {
        let mut w = XmlWriter::new().without_indent();
        if h.team_event {
            w.start("Equipes");
            for r in &input.people.teams {
                if let Some(t) = input.store.get(*r) {
                    write_team(&mut w, input.store, t, true);
                }
            }
        } else {
            w.start("Tireurs");
            for r in &input.people.fencers {
                if let Some(p) = input.store.get(*r) {
                    p.save(&mut w, false);
                }
            }
        }
        w.end();
        replay_fragment(&w.finish(), &mut s);
    }
    // Schedule::Save
    if let Some(phases) = input.phases_xml {
        replay_fragment(phases, &mut s);
    }
    s.end_element(); // Event
    s.end_element(); // Tournament

    // Schedule::SavePeoples
    {
        let mut w = XmlWriter::new().without_indent();
        write_people_with_referees(&mut w, h.team_event, input.store, referees, input.people, true);
        replay_fragment(&w.finish(), &mut s);
    }
    s.write_custom("ClubDatabase");
    s.end_element(); // FencingData
    s.finish()
}
