//! Matchs et scores (port de `supervisor/match.cpp` et `supervisor/score.cpp`),
//! partagés par les poules et les tableaux.

use std::sync::atomic::{AtomicI64, Ordering as AtomicOrdering};

use crate::player::{PlayerClass, PlayerRef, PlayerStore};

/// Décalage de l'horloge du Marshaller en secondes (`Match::_clock_offset`,
/// message `BellePoule2D::ClockOffset`).
static CLOCK_OFFSET: AtomicI64 = AtomicI64::new(0);

/// `Match::OnClockOffset`.
pub fn set_clock_offset(seconds: i64) {
    CLOCK_OFFSET.store(seconds, AtomicOrdering::Relaxed);
}

/// Heure locale décalée, secondes à zéro (`Match::GetDuration`).
pub fn now_rounded() -> chrono::NaiveDateTime {
    use chrono::Timelike;
    let now = chrono::Local::now().naive_local() + chrono::Duration::seconds(CLOCK_OFFSET.load(AtomicOrdering::Relaxed));
    now.with_second(0).unwrap_or(now).with_nanosecond(0).unwrap_or(now)
}
use crate::xml::XmlWriter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreStatus {
    Unknown,
    Victory,
    Defeat,
    Withdrawal,
    BlackCard,
    OpponentOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Score {
    score: u32,
    status: ScoreStatus,
    backup_status: ScoreStatus,
}

impl Default for Score {
    fn default() -> Self {
        Score { score: 0, status: ScoreStatus::Unknown, backup_status: ScoreStatus::Unknown }
    }
}

impl Score {
    pub fn status(&self) -> ScoreStatus {
        self.status
    }

    pub fn is_known(&self) -> bool {
        self.status != ScoreStatus::Unknown
    }

    pub fn is_the_best(&self) -> bool {
        self.status == ScoreStatus::Victory
    }

    /// Touches données (0 si abandon/exclusion).
    pub fn get(&self) -> u32 {
        match self.status {
            ScoreStatus::Victory | ScoreStatus::Defeat => self.score,
            _ => 0,
        }
    }

    /// Image affichée dans les feuilles (`V`, `V4`, `3` ou vide).
    pub fn image(&self, max_score: u32) -> String {
        match self.status {
            ScoreStatus::Victory if self.score == max_score => "V".into(),
            ScoreStatus::Victory => format!("V{}", self.score),
            ScoreStatus::Defeat => self.score.to_string(),
            _ => String::new(),
        }
    }

    /// Image XML du statut (`Statut`).
    pub fn status_image(&self) -> Option<&'static str> {
        match self.status {
            ScoreStatus::Defeat => Some("D"),
            ScoreStatus::Victory => Some("V"),
            ScoreStatus::Withdrawal => Some("A"),
            ScoreStatus::BlackCard => Some("E"),
            _ => None,
        }
    }

    pub fn clean(&mut self) {
        self.status = ScoreStatus::Unknown;
    }

    /// Abandon (`A` ou `F`) ou exclusion (`E`).
    pub fn drop_with(&mut self, reason: char) {
        let previous = self.status;
        self.status = match reason {
            'A' | 'F' => ScoreStatus::Withdrawal,
            'E' => ScoreStatus::BlackCard,
            _ => return,
        };
        if previous != ScoreStatus::Withdrawal && previous != ScoreStatus::BlackCard {
            self.backup_status = previous;
        }
    }

    pub fn drop_reason(&self) -> Option<char> {
        match self.status {
            ScoreStatus::Withdrawal => Some('A'),
            ScoreStatus::BlackCard => Some('E'),
            _ => None,
        }
    }

    pub fn restore(&mut self) {
        self.status = self.backup_status;
    }

    pub fn set(&mut self, score: u32, victory: bool, max_score: u32) {
        self.score = score;
        self.status = if score == max_score || victory { ScoreStatus::Victory } else { ScoreStatus::Defeat };
    }

    pub fn is_valid(&self, max_score: u32) -> bool {
        !(self.status != ScoreStatus::Unknown && self.score > max_score)
    }

    pub fn is_out(&self) -> bool {
        matches!(self.status, ScoreStatus::Withdrawal | ScoreStatus::BlackCard)
    }

    pub fn synchronize_with(&mut self, with: &Score) {
        if !self.is_out() {
            if with.is_out() {
                self.backup_status = self.status;
                self.status = ScoreStatus::OpponentOut;
            } else {
                self.status = self.backup_status;
            }
        }
    }

    pub fn is_consistent_with(&self, with: &Score, max_score: u32) -> bool {
        use ScoreStatus::*;
        if self.status == Unknown || with.status == Unknown {
            return true;
        }
        if self.status == OpponentOut || with.status == OpponentOut {
            return true;
        }
        if self.status == Victory && with.status == Victory {
            return false;
        }
        if self.status == Defeat && with.status == Defeat {
            return false;
        }
        if self.status == Victory && self.score < with.score {
            return false;
        }
        if with.status == Victory && with.score < self.score {
            return false;
        }
        if self.score >= max_score && with.score >= max_score {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opponent {
    pub fencer: Option<PlayerRef>,
    /// Vrai quand la place est déterminée (tireur connu ou exempt).
    pub is_known: bool,
    pub score: Score,
}

impl Default for Opponent {
    fn default() -> Self {
        Opponent { fencer: None, is_known: false, score: Score::default() }
    }
}

/// Date et heure d'un match (`FieTime` : `Date="jj.mm.aaaa"`, `Heure="hh:mm"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StartTime(pub chrono::NaiveDateTime);

impl StartTime {
    pub fn xml_date(&self) -> String {
        self.0.format("%d.%m.%Y").to_string()
    }

    pub fn xml_time(&self) -> String {
        self.0.format("%H:%M").to_string()
    }

    pub fn parse(date: &str, time: &str) -> Option<StartTime> {
        let d = crate::attribute::parse_date(date)?;
        let t = chrono::NaiveTime::parse_from_str(time.trim(), "%H:%M")
            .or_else(|_| chrono::NaiveTime::parse_from_str(time.trim(), "%Hh%M"))
            .ok()?;
        Some(StartTime(d.and_time(t)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub opponents: [Opponent; 2],
    pub max_score: u32,
    /// Numéro du match (`ID`) ; 0 = match non enregistré.
    pub number: u32,
    pub name: String,
    pub net_id: u32,
    pub referees: Vec<PlayerRef>,
    pub piste: u32,
    pub start_time: Option<StartTime>,
    pub duration_sec: u32,
    pub duration_span: u32,
    pub officiating: Option<crate::officiating::Officiating>,
    /// Rencontre par équipes : journal des relais (attribut `Relais`).
    pub relay: Option<crate::relay::RelayLog>,
    /// Match de poule : la double défaite est possible (temps de la poule
    /// écoulé : 0-0 si le match n'a pas commencé, X-X en cas d'égalité).
    pub allow_double_defeat: bool,
}

impl Match {
    pub fn new(max_score: u32) -> Self {
        Match {
            opponents: [Opponent::default(), Opponent::default()],
            max_score,
            number: 0,
            name: String::new(),
            net_id: 0,
            referees: Vec::new(),
            piste: 0,
            start_time: None,
            duration_sec: 0,
            duration_span: 1,
            officiating: None,
            relay: None,
            allow_double_defeat: false,
        }
    }

    pub fn with_opponents(a: PlayerRef, b: PlayerRef, max_score: u32) -> Self {
        let mut m = Match::new(max_score);
        m.set_opponent(0, Some(a));
        m.set_opponent(1, Some(b));
        m
    }

    pub fn opponent(&self, position: usize) -> Option<PlayerRef> {
        self.opponents[position].fencer
    }

    /// `None` = exempt (la place est connue mais vide).
    pub fn set_opponent(&mut self, position: usize, fencer: Option<PlayerRef>) {
        self.opponents[position].fencer = fencer;
        self.opponents[position].is_known = true;
        if fencer.is_none() {
            self.opponents[(position + 1) % 2].score.clean();
        }
    }

    pub fn remove_opponent(&mut self, position: usize) {
        self.opponents[position] = Opponent::default();
    }

    pub fn position_of(&self, fencer: PlayerRef) -> Option<usize> {
        (0..2).find(|i| self.opponents[*i].fencer == Some(fencer))
    }

    pub fn has_fencer(&self, fencer: PlayerRef) -> bool {
        self.position_of(fencer).is_some()
    }

    pub fn exempted(&self) -> bool {
        self.opponents.iter().any(|o| o.fencer.is_none() && o.is_known)
    }

    pub fn score(&self, fencer: PlayerRef) -> Option<&Score> {
        self.position_of(fencer).map(|i| &self.opponents[i].score)
    }

    pub fn score_at(&self, position: usize) -> &Score {
        &self.opponents[position].score
    }

    pub fn is_dropped(&self) -> bool {
        self.opponents.iter().any(|o| o.score.is_out())
    }

    pub fn drop_fencer(&mut self, fencer: PlayerRef, reason: char) {
        if let Some(i) = self.position_of(fencer) {
            self.opponents[i].score.drop_with(reason);
            let s = self.opponents[i].score;
            self.opponents[1 - i].score.synchronize_with(&s);
        }
    }

    pub fn restore_fencer(&mut self, fencer: PlayerRef) {
        if let Some(i) = self.position_of(fencer) {
            self.opponents[i].score.restore();
            let s = self.opponents[i].score;
            self.opponents[1 - i].score.synchronize_with(&s);
        }
    }

    pub fn has_error(&self) -> bool {
        let m = self.max_score;
        if self.opponents.iter().any(|o| !o.score.is_valid(m)) {
            return true;
        }
        if self.is_double_defeat() {
            return false;
        }
        !self.opponents[0].score.is_consistent_with(&self.opponents[1].score, m)
    }

    /// Double défaite : deux défaites au même score (match de poule
    /// seulement), sans vainqueur.
    pub fn is_double_defeat(&self) -> bool {
        let [a, b] = &self.opponents;
        self.allow_double_defeat
            && a.score.status() == ScoreStatus::Defeat
            && b.score.status() == ScoreStatus::Defeat
            && a.score.get() == b.score.get()
            && a.score.get() < self.max_score
    }

    /// Saisit une double défaite au score donné (0 si le match n'a pas commencé).
    pub fn set_double_defeat(&mut self, touches: u32) {
        if !self.allow_double_defeat {
            return;
        }
        let touches = touches.min(self.max_score.saturating_sub(1));
        for o in &mut self.opponents {
            o.score.set(touches, false, self.max_score);
        }
    }

    pub fn is_over(&self) -> bool {
        self.opponents.iter().all(|o| o.is_known && o.score.is_known()) && !self.has_error()
    }

    pub fn winner(&self) -> Option<PlayerRef> {
        for i in 0..2 {
            if self.opponents[i].fencer.is_none() && self.opponents[i].is_known {
                return self.opponents[1 - i].fencer;
            }
        }
        let [a, b] = &self.opponents;
        if a.score.is_known() && b.score.is_known() {
            if self.is_dropped() {
                for i in 0..2 {
                    if self.opponents[i].score.is_out() && !self.opponents[1 - i].score.is_out() {
                        return self.opponents[1 - i].fencer;
                    }
                }
                return None;
            } else if !self.has_error() {
                if a.score.get() > b.score.get() {
                    return a.fencer;
                } else if a.score.get() < b.score.get() {
                    return b.fencer;
                }
                for o in &self.opponents {
                    if o.score.is_the_best() {
                        return o.fencer;
                    }
                }
            }
        }
        None
    }

    pub fn looser(&self) -> Option<PlayerRef> {
        let w = self.winner()?;
        self.opponents.iter().filter_map(|o| o.fencer).find(|f| *f != w)
    }

    pub fn set_score(&mut self, fencer: PlayerRef, score: u32, is_the_best: bool) {
        if let Some(i) = self.position_of(fencer) {
            self.opponents[i].score.set(score, is_the_best, self.max_score);
        }
    }

    /// Saisie textuelle (`V`, `W3`, `4`, vide). Renvoie vrai si la saisie
    /// est complète (pour passer automatiquement à la case suivante).
    pub fn set_score_str(&mut self, fencer: PlayerRef, value: &str) -> bool {
        let Some(i) = self.position_of(fencer) else { return false };
        if !value.is_ascii() { return false; }
        let max = self.max_score;
        // Poule : « D » (0-0) ou « D3 » (3-3) = double défaite.
        if self.allow_double_defeat && value.as_bytes().first().is_some_and(|c| c.eq_ignore_ascii_case(&b'd')) {
            let rest = &value[1..];
            if rest.chars().all(|c| c.is_ascii_digit()) {
                self.set_double_defeat(rest.parse().unwrap_or(0));
                return true;
            }
        }
        if value.len() == 1 && value.eq_ignore_ascii_case("v") {
            self.opponents[i].score.set(max, true, max);
            return true;
        }
        let (digits, best) = if value.len() > 1 && value.as_bytes()[0].eq_ignore_ascii_case(&b'w') {
            (&value[1..], true)
        } else {
            (value, false)
        };
        if value.is_empty() {
            self.opponents[i].score.clean();
        }
        if !digits.is_empty() && !digits.chars().any(|c| c.is_ascii_alphabetic()) {
            let n = crate::attribute::parse_leading_int(digits).max(0) as u32;
            self.opponents[i].score.set(n, best, max);
            let max_str = max.to_string();
            let one_more = crate::attribute::parse_leading_int(&format!("{digits}0")).max(0) as u32;
            return digits.len() >= max_str.len() || one_more > max;
        }
        false
    }

    pub fn clean_score(&mut self) {
        for o in &mut self.opponents {
            o.score.clean();
        }
    }

    /// Enregistre `<Match ID=…>` avec arbitres et tireurs (`Match::Save`).
    pub fn save(&self, players: &PlayerStore, w: &mut XmlWriter) {
        if self.number == 0 {
            return;
        }
        w.start("Match");
        if let Some(state) = &self.officiating {
            if let Ok(value) = serde_json::to_string(state) { w.attr("Arbitrage", &value); }
        }
        if let Some(log) = &self.relay {
            if let Ok(value) = serde_json::to_string(log) { w.attr("Relais", &value); }
        }
        w.attr_fmt("ID", self.number);
        if self.net_id != 0 {
            w.attr("NetID", &format!("{:x}", self.net_id));
        }
        if self.piste != 0 {
            w.attr_fmt("Piste", self.piste);
        }
        if let Some(t) = &self.start_time {
            w.attr("Date", &t.xml_date());
            w.attr("Heure", &t.xml_time());
        }
        if self.duration_sec > 0 {
            w.attr_fmt("Duree", self.duration_sec);
        }
        if self.duration_sec == 0 && self.duration_span > 1 {
            w.attr_fmt("Portee", self.duration_span);
        }
        // Les arbitres ont leur propre liste de joueurs : leur référence peut
        // coïncider avec celle d'un tireur, on n'y cherche donc rien.
        for r in &self.referees {
            w.start(PlayerClass::Referee.xml_tag());
            w.attr_fmt("REF", *r);
            w.end();
        }
        for o in &self.opponents {
            let Some(f) = o.fencer else { continue };
            let tag = players.get(f).map(|p| p.class.xml_tag()).unwrap_or("Tireur");
            w.start(tag);
            w.attr_fmt("REF", f);
            if let Some(status) = o.score.status_image() {
                if status == "V" || status == "D" {
                    w.attr_fmt("Score", o.score.get());
                }
                w.attr("Statut", status);
            }
            w.end();
        }
        w.end();
    }

    /// Lit le score d'un tireur depuis son nœud `<Tireur REF Score Statut>`.
    pub fn load_fencer(&mut self, node: roxmltree::Node, fencer: PlayerRef) {
        let Some(i) = self.position_of(fencer) else { return };
        let mut victory = false;
        if let Some(st) = node.attribute("Statut") {
            match st.chars().next() {
                Some('V') => victory = true,
                Some('D') => victory = false,
                Some(c) => self.opponents[i].score.drop_with(c),
                None => {}
            }
        }
        if let Some(s) = node.attribute("Score") {
            let n = crate::attribute::parse_leading_int(s).max(0) as u32;
            self.opponents[i].score.set(n, victory, self.max_score);
        }
    }

    /// Lit les attributs communs d'un `<Match>` (hors tireurs).
    pub fn load_header(&mut self, node: roxmltree::Node) {
        self.officiating = node.attribute("Arbitrage").and_then(|v| serde_json::from_str(v).ok());
        self.relay = node.attribute("Relais").and_then(|v| serde_json::from_str(v).ok());
        self.number = crate::xml::attr_u32(node, "ID", self.number);
        if let Some(v) = node.attribute("NetID") {
            self.net_id = u32::from_str_radix(v, 16).unwrap_or(0);
        }
        self.piste = crate::xml::attr_u32(node, "Piste", 0);
        if let (Some(d), Some(t)) = (node.attribute("Date"), node.attribute("Heure")) {
            self.start_time = StartTime::parse(d, t);
        }
        self.duration_sec = crate::xml::attr_u32(node, "Duree", 0);
        self.duration_span = crate::xml::attr_u32(node, "Portee", 1);
        self.referees = crate::xml::children(node, "Arbitre")
            .filter_map(|n| n.attribute("REF"))
            .map(|r| crate::attribute::parse_leading_int(r) as PlayerRef)
            .collect();
    }

    /// Après chargement des deux tireurs : synchronise les abandons.
    pub fn after_load(&mut self) {
        for i in 0..2 {
            if self.opponents[i].score.is_out() {
                let s = self.opponents[i].score;
                self.opponents[1 - i].score.synchronize_with(&s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_victory() {
        let mut m = Match::with_opponents(1, 2, 5);
        assert!(m.set_score_str(1, "V"));
        m.set_score_str(2, "3");
        assert!(m.is_over());
        assert_eq!(m.winner(), Some(1));
        assert_eq!(m.looser(), Some(2));
    }

    #[test]
    fn double_defeat_only_in_pools() {
        let mut m = Match::with_opponents(1, 2, 5);
        m.set_score_str(1, "2");
        m.set_score_str(2, "2");
        assert!(m.has_error(), "tableau ou barrage : il faut un vainqueur");
        m.allow_double_defeat = true;
        assert!(m.is_over() && m.is_double_defeat());
        assert_eq!(m.winner(), None);
        assert_eq!(m.looser(), None);
        let mut m = Match::with_opponents(1, 2, 5);
        m.allow_double_defeat = true;
        assert!(m.set_score_str(2, "D"));
        assert!(m.is_double_defeat() && m.score_at(0).get() == 0);
        assert!(m.set_score_str(1, "d3"));
        assert_eq!((m.score_at(0).get(), m.score_at(1).get()), (3, 3));
        m.set_score_str(1, "5");
        m.set_score_str(2, "5");
        assert!(m.has_error(), "5-5 n'est pas une double défaite");
    }

    #[test]
    fn inconsistent_scores() {
        let mut m = Match::with_opponents(1, 2, 5);
        m.set_score(1, 5, false);
        m.set_score(2, 5, false);
        assert!(m.has_error());
        assert!(!m.is_over());
    }

    #[test]
    fn withdrawal_gives_victory() {
        let mut m = Match::with_opponents(1, 2, 15);
        m.set_score(1, 15, false);
        m.set_score(2, 2, false);
        m.drop_fencer(2, 'A');
        assert_eq!(m.winner(), Some(1));
        m.restore_fencer(2);
        assert_eq!(m.winner(), Some(1));
    }

    #[test]
    fn partial_input_not_complete() {
        let mut m = Match::with_opponents(1, 2, 15);
        assert!(!m.set_score_str(1, "1"));
        assert!(m.set_score_str(1, "12"));
        assert!(m.set_score_str(1, "9"));
    }

    #[test]
    fn unicode_score_input_does_not_panic() {
        let mut m = Match::with_opponents(1, 2, 5);
        assert!(!m.set_score_str(1, "é"));
        assert!(!m.score(1).unwrap().is_known());
    }

    #[test]
    fn exempted() {
        let mut m = Match::new(15);
        m.set_opponent(0, Some(7));
        m.set_opponent(1, None);
        assert!(m.exempted());
        assert_eq!(m.winner(), Some(7));
    }
}
