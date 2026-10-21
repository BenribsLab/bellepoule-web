//! Dates et durées (remplace `GDateTime` / `GTimeSpan` et `FieTime`).
//!
//! Les instants sont des heures locales « murales » (`NaiveDateTime`), les
//! durées sont exprimées en microsecondes comme les `GTimeSpan` de GLib.

use chrono::{Duration, NaiveDateTime, Timelike};

/// Instant local.
pub type Time = NaiveDateTime;

/// Durée en microsecondes (`GTimeSpan`).
pub type Span = i64;

pub const SECOND: Span = 1_000_000;
pub const MINUTE: Span = 60 * SECOND;
pub const HOUR: Span = 60 * MINUTE;

/// `g_date_time_add`.
pub fn add(t: Time, span: Span) -> Time {
    t + Duration::microseconds(span)
}

/// `g_date_time_difference (a, b)` : a - b.
pub fn diff(a: Time, b: Time) -> Span {
    (a - b).num_microseconds().unwrap_or(0)
}

/// Heure locale courante.
pub fn now_local() -> Time {
    chrono::Local::now().naive_local()
}

/// `%d.%m.%Y` (date XML FIE).
pub fn xml_date(t: Time) -> String {
    t.format("%d.%m.%Y").to_string()
}

/// `%R` (heure XML FIE).
pub fn xml_time(t: Time) -> String {
    t.format("%H:%M").to_string()
}

/// `HH:MM`.
pub fn hh_mm(t: Time) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

/// Lecture `JJ.MM.AAAA` + `HH:MI` (constructeur `FieTime (date, time)`).
pub fn parse_fie(date: &str, time: &str) -> Option<Time> {
    let d: Vec<&str> = date.split('.').collect();
    let h: Vec<&str> = time.split(':').collect();
    if d.len() != 3 || h.len() != 2 {
        return None;
    }
    let num = |s: &str| bp_core::attribute::parse_leading_int(s.trim());
    let date = chrono::NaiveDate::from_ymd_opt(num(d[2]) as i32, num(d[1]) as u32, num(d[0]) as u32)?;
    date.and_hms_opt(num(h[0]) as u32, num(h[1]) as u32, 0)
}

/// Insertion triée façon `g_list_insert_sorted` : l'élément est placé avant
/// le premier élément pour lequel `cmp (item, elem) <= 0`.
pub fn insert_sorted<T: Copy>(list: &mut Vec<T>, item: T, mut cmp: impl FnMut(&T, &T) -> std::cmp::Ordering) {
    let pos = list.iter().position(|e| cmp(&item, e) != std::cmp::Ordering::Greater).unwrap_or(list.len());
    list.insert(pos, item);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fie_roundtrip() {
        let t = parse_fie("03.02.2024", "9:05").unwrap();
        assert_eq!(xml_date(t), "03.02.2024");
        assert_eq!(xml_time(t), "09:05");
        assert!(parse_fie("2024", "9:05").is_none());
    }

    #[test]
    fn sorted_insert_before_equals() {
        let mut v = vec![1, 3, 3, 5];
        insert_sorted(&mut v, 3, |a, b| a.cmp(b));
        assert_eq!(v, vec![1, 3, 3, 3, 5]);
        insert_sorted(&mut v, 9, |a, b| a.cmp(b));
        assert_eq!(*v.last().unwrap(), 9);
    }
}
