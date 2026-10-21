//! Pistes de la salle (port de `marshaller/piste.cpp`, partie modèle).
//!
//! La géométrie reproduit celle de l'élément GooCanvas du C++ : un groupe
//! translaté en (`tx`, `ty`) puis éventuellement tourné de 90° autour de son
//! origine. Les coordonnées publiques (`bounds`, fichier `hall.json`) sont
//! celles du coin supérieur gauche du rectangle englobant.

use serde_json::{json, Value};

use crate::slot::{PisteKey, SlotKey};
use crate::time::Time;

/// Longueur d'une piste (`Piste::_W`).
pub const W: f64 = 240.0;
/// Largeur d'une piste (`Piste::_H`).
pub const H: f64 = 30.0;
/// Épaisseur du contour (`_BORDER_W`).
pub const BORDER_W: f64 = 0.75;
/// Pas de la grille d'alignement (`_RESOLUTION`).
pub const RESOLUTION: f64 = 15.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl Bounds {
    /// Normalise deux coins quelconques.
    pub fn from_corners(ax: f64, ay: f64, bx: f64, by: f64) -> Self {
        Bounds { x1: ax.min(bx), y1: ay.min(by), x2: ax.max(bx), y2: ay.max(by) }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x1 && x <= self.x2 && y >= self.y1 && y <= self.y2
    }
}

#[derive(Debug, Clone)]
pub struct Piste {
    pub key: PisteKey,
    /// Numéro affiché.
    pub id: u32,
    pub tx: f64,
    pub ty: f64,
    pub horizontal: bool,
    pub blocked: bool,
    pub selected: bool,
    /// Volet gris (`Blur`) : la piste ne peut pas recevoir l'objet en cours.
    pub blurred: bool,
    /// Survol pendant un glisser-déposer (`Focus`).
    pub focused: bool,
    /// Créneaux réservés, triés par heure de début.
    pub slots: Vec<SlotKey>,
    /// Heure affichée (`_display_time`).
    pub display_time: Time,
}

impl Piste {
    /// Nouvelle piste n° 1 en (15, 15) (constructeur C++).
    pub fn new(key: PisteKey, display_time: Time) -> Self {
        Piste {
            key,
            id: 1,
            tx: RESOLUTION,
            ty: RESOLUTION,
            horizontal: true,
            blocked: false,
            selected: false,
            blurred: false,
            focused: false,
            slots: Vec::new(),
            display_time,
        }
    }

    /// Rectangle englobant dans l'espace de la salle.
    pub fn bounds(&self) -> Bounds {
        if self.horizontal {
            Bounds { x1: self.tx, y1: self.ty, x2: self.tx + W, y2: self.ty + H }
        } else {
            Bounds { x1: self.tx - H, y1: self.ty, x2: self.tx, y2: self.ty + W }
        }
    }

    /// `Translate` : déplacement dans l'espace de la salle.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.tx += dx;
        self.ty += dy;
    }

    /// `Rotate` : rotation de ±90° autour de l'origine de l'élément.
    pub fn rotate(&mut self) {
        self.horizontal = !self.horizontal;
    }

    /// `GetGridAdjustment`.
    pub fn grid_adjustment(coordinate: f64) -> f64 {
        let modulo = coordinate % RESOLUTION;
        if modulo > RESOLUTION / 2.0 {
            RESOLUTION - modulo
        } else {
            -modulo
        }
    }

    /// `AlignOnGrid`.
    pub fn align_on_grid(&mut self) {
        let b = self.bounds();
        self.translate(Self::grid_adjustment(b.x1), Self::grid_adjustment(b.y1));
    }

    /// `AnchorTo` : se place une piste et demie sous `other`.
    pub fn anchor_to(&mut self, other: &Bounds) {
        let b = self.bounds();
        self.translate(other.x1 - b.x1, (other.y1 + H * 1.5) - b.y1);
    }

    /// `Overlaps`.
    pub fn overlaps(&self, r: &Bounds) -> bool {
        let b = self.bounds();
        !(r.y2 < b.y1 || r.y1 > b.y2 || r.x2 < b.x1 || r.x1 > b.x2)
    }

    /// Point de la salle → espace de la piste (avant rotation).
    pub fn to_piste_space(&self, x: f64, y: f64) -> (f64, f64) {
        if self.horizontal {
            (x - self.tx, y - self.ty)
        } else {
            (y - self.ty, self.tx - x)
        }
    }

    /// `FeedJsonBuilder`.
    pub fn to_json(&self) -> Value {
        let b = self.bounds();
        json!({
            "id": self.id,
            "blocked": self.blocked as i32,
            "x": b.x1 as i64,
            "y": b.y1 as i64,
            "rotation": if self.horizontal { 0 } else { 90 },
        })
    }

    /// `ReadJson` (appliqué à une piste neuve).
    pub fn read_json(&mut self, v: &Value) {
        let int = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
        self.id = int("id").max(0) as u32;
        self.blocked = int("blocked") != 0;
        let b = self.bounds();
        self.translate(int("x") as f64 - b.x1, int("y") as f64 - b.y1);
        if int("rotation") == 90 {
            self.rotate();
            self.translate(H, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piste() -> Piste {
        Piste::new(1, crate::time::parse_fie("01.01.2024", "10:00").unwrap())
    }

    #[test]
    fn json_roundtrip_with_rotation() {
        let mut p = piste();
        p.translate(100.0, 45.0);
        p.rotate();
        p.id = 7;
        p.blocked = true;
        let j = p.to_json();
        let mut q = piste();
        q.read_json(&j);
        assert_eq!(q.bounds(), p.bounds());
        assert!(!q.horizontal);
        assert!(q.blocked);
        assert_eq!(q.id, 7);
    }

    #[test]
    fn grid_and_anchor() {
        let mut p = piste();
        p.translate(8.0, 4.0);
        p.align_on_grid();
        assert_eq!((p.bounds().x1, p.bounds().y1), (30.0, 15.0));
        let mut q = piste();
        q.anchor_to(&p.bounds());
        assert_eq!(q.bounds().y1, 15.0 + H * 1.5);
        assert!(p.overlaps(&Bounds::from_corners(0.0, 0.0, 40.0, 20.0)));
        assert!(!p.overlaps(&Bounds::from_corners(0.0, 100.0, 10.0, 110.0)));
    }

    #[test]
    fn piste_space() {
        let mut p = piste();
        p.rotate();
        let (px, py) = p.to_piste_space(p.tx - 10.0, p.ty + 100.0);
        assert_eq!((px, py), (100.0, 10.0));
    }
}
