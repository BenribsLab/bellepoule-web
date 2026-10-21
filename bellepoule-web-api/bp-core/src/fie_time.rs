//! Horodatage au format FIE (port de `common/util/fie_time.cpp`).
//!
//! Les fichiers stockent la date `JJ.MM.AAAA` et l'heure `HH:MI`
//! séparément ; un `FieTime` les regroupe et fournit les images utilisées
//! à l'affichage (`%k:%M`) et à l'écriture (`%d.%m.%Y`, `%R`).

use std::cmp::Ordering;

use chrono::{NaiveDate, NaiveDateTime, Timelike};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieTime {
    date_time: Option<NaiveDateTime>,
}

impl FieTime {
    /// À partir d'une date et heure déjà connues.
    pub fn new(date_time: NaiveDateTime) -> Self {
        FieTime { date_time: Some(date_time) }
    }

    /// À partir des images XML (`JJ.MM.AAAA`, `HH:MI`). Si l'une est mal
    /// formée, l'horodatage est invalide (`date_time()` vaut `None`).
    pub fn parse(date: &str, time: &str) -> Self {
        let d: Vec<&str> = date.split('.').collect();
        let t: Vec<&str> = time.split(':').collect();
        let atoi = |s: &str| crate::attribute::parse_leading_int(s) as i32;
        let date_time = if d.len() == 3 && t.len() == 2 {
            NaiveDate::from_ymd_opt(atoi(d[2]), atoi(d[1]) as u32, atoi(d[0]) as u32)
                .and_then(|nd| nd.and_hms_opt(atoi(t[0]) as u32, atoi(t[1]) as u32, 0))
        } else {
            None
        };
        FieTime { date_time }
    }

    pub fn date_time(&self) -> Option<NaiveDateTime> {
        self.date_time
    }

    pub fn is_valid(&self) -> bool {
        self.date_time.is_some()
    }

    /// Image affichée (`%k:%M` : heure sur deux caractères complétée par une espace).
    pub fn image(&self) -> Option<String> {
        self.date_time.map(|d| format!("{:>2}:{:02}", d.hour(), d.minute()))
    }

    /// Date XML `JJ.MM.AAAA`.
    pub fn xml_date(&self) -> Option<String> {
        self.date_time.map(|d| d.format("%d.%m.%Y").to_string())
    }

    /// Heure XML `HH:MM`.
    pub fn xml_time(&self) -> Option<String> {
        self.date_time.map(|d| d.format("%H:%M").to_string())
    }

    /// `FieTime::Compare` : un horodatage invalide passe avant un valide.
    pub fn compare(a: Option<&FieTime>, b: Option<&FieTime>) -> Ordering {
        let a = a.and_then(|t| t.date_time);
        let b = b.and_then(|t| t.date_time);
        match (a, b) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_images() {
        let t = FieTime::parse("17.12.2011", "9:05");
        assert_eq!(t.xml_date().unwrap(), "17.12.2011");
        assert_eq!(t.xml_time().unwrap(), "09:05");
        assert_eq!(t.image().unwrap(), " 9:05");
        assert!(!FieTime::parse("17.12", "9:05").is_valid());
        assert_eq!(FieTime::compare(Some(&t), None), Ordering::Greater);
    }
}
