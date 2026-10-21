//! Affinités tireurs / arbitres (port de `marshaller/affinities.cpp`).
//!
//! Une affinité est une parenté (même club, même ligue, même pays) entre un
//! arbitre et un tireur. Le résultat de `kinship_with` est un masque de bits :
//! le bit `i` correspond au titre `TITLES[i]` (ordre `_titles` du C++, construit
//! par `g_list_prepend` à partir de `country`, `league`, `club`).

use bp_core::Player;
use sha1::{Digest, Sha1};

use crate::color::Rgb;

/// Titres gérés, dans l'ordre des bits.
pub const TITLES: [&str; 3] = ["club", "league", "country"];

/// Règles globales : couleurs et validité de chaque affinité
/// (`Affinities::Manage`, `SetValidity`).
#[derive(Debug, Clone)]
pub struct AffinityRules {
    pub colors: [Rgb; 3],
    pub validity: [bool; 3],
}

impl Default for AffinityRules {
    fn default() -> Self {
        // Marshaller::Marshaller : country #fff36c, league #ffb245, club #ffa191.
        AffinityRules {
            colors: [Rgb::parse("#ffa191").unwrap(), Rgb::parse("#ffb245").unwrap(), Rgb::parse("#fff36c").unwrap()],
            validity: [true; 3],
        }
    }
}

impl AffinityRules {
    pub fn index_of(title: &str) -> Option<usize> {
        TITLES.iter().position(|t| *t == title)
    }

    /// `GetColor (const gchar *of)`.
    pub fn color_of(&self, title: &str) -> Option<Rgb> {
        Self::index_of(title).map(|i| self.colors[i])
    }

    /// `GetColor (guint of)` : couleur du bit de poids le plus faible.
    pub fn color_of_kinship(&self, kinship: u32) -> Option<Rgb> {
        (0..TITLES.len()).find(|i| kinship & (1 << i) != 0).map(|i| self.colors[i])
    }

    pub fn set_validity(&mut self, title: &str, valid: bool) {
        if let Some(i) = Self::index_of(title) {
            self.validity[i] = valid;
        }
    }
}

/// Empreinte des affinités d'un joueur.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Affinities {
    checksums: [u64; 3],
    /// Membres d'une équipe (`_shareholders`).
    shareholders: Vec<Affinities>,
}

impl Affinities {
    /// `Affinities (Player *)`.
    pub fn new(player: &Player) -> Self {
        let mut checksums = [0u64; 3];
        for (i, title) in TITLES.iter().enumerate() {
            checksums[i] = digest(player.attr(title).map(|v| v.xml_image()).as_deref());
        }
        Affinities { checksums, shareholders: Vec::new() }
    }

    /// `ShareWith` vu du côté de l'équipe : ajoute un membre (en tête).
    pub fn add_shareholder(&mut self, member: Affinities) {
        self.shareholders.insert(0, member);
    }

    /// `KinshipWith`.
    pub fn kinship_with(&self, with: Option<&Affinities>, rules: &AffinityRules) -> u32 {
        if let Some(w) = with {
            if !w.shareholders.is_empty() {
                return w.simple_kinship_with(Some(self), rules);
            }
        }
        self.simple_kinship_with(with, rules)
    }

    fn simple_kinship_with(&self, with: Option<&Affinities>, rules: &AffinityRules) -> u32 {
        let Some(with) = with else { return 0 };
        if !self.shareholders.is_empty() {
            return self.shareholders.iter().fold(0, |k, s| k | s.simple_kinship_with(Some(with), rules));
        }
        let mut kinship = 0;
        for i in 0..TITLES.len() {
            if self.checksums[i] != 0 && self.checksums[i] == with.checksums[i] && rules.validity[i] {
                kinship |= 1 << i;
            }
        }
        kinship
    }
}

/// 8 premiers chiffres hexadécimaux du SHA1 de la valeur (0 si vide).
fn digest(value: Option<&str>) -> u64 {
    match value {
        Some(v) if !v.is_empty() => {
            let h = Sha1::digest(v.as_bytes());
            u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bp_core::player::AttrId;
    use bp_core::PlayerClass;

    fn player(club: &str, league: &str, country: &str) -> Player {
        let mut p = Player::new(PlayerClass::Fencer);
        p.set_str(&AttrId::global("club"), club);
        p.set_str(&AttrId::global("league"), league);
        p.set_str(&AttrId::global("country"), country);
        p
    }

    #[test]
    fn kinship_bits_and_validity() {
        let mut rules = AffinityRules::default();
        let a = Affinities::new(&player("PARIS", "IDF", "FRA"));
        let b = Affinities::new(&player("LYON", "IDF", "FRA"));
        assert_eq!(a.kinship_with(Some(&b), &rules), 0b110);
        rules.set_validity("country", false);
        assert_eq!(a.kinship_with(Some(&b), &rules), 0b010);
        assert_eq!(rules.color_of_kinship(0b110), rules.color_of("league"));
        assert_eq!(a.kinship_with(None, &rules), 0);
        // Valeurs vides : aucune parenté.
        let e = Affinities::new(&player("", "", ""));
        assert_eq!(e.kinship_with(Some(&e), &rules), 0);
    }

    #[test]
    fn team_kinship_is_union_of_members() {
        let rules = AffinityRules::default();
        let mut team = Affinities::new(&player("", "", ""));
        team.add_shareholder(Affinities::new(&player("PARIS", "", "FRA")));
        team.add_shareholder(Affinities::new(&player("", "IDF", "")));
        let referee = Affinities::new(&player("PARIS", "IDF", "GER"));
        assert_eq!(referee.kinship_with(Some(&team), &rules), 0b011);
        assert_eq!(team.kinship_with(Some(&referee), &rules), 0b011);
    }
}
