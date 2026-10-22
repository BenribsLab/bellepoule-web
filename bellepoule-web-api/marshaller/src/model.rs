//! Entités du Marshaller : matchs (`Job`), lots (`Batch`), compétitions,
//! tireurs, arbitres engagés, horloge, frise chronologique et lasso.
//!
//! Les comportements qui touchent plusieurs entités sont des méthodes de
//! `Hall` (voir `hall.rs`, `schedule.rs`, `assign.rs`, `referees.rs`).

use bp_core::Player;
use bp_net::Message;

use crate::affinities::Affinities;
use crate::color::Rgb;
use crate::piste::Bounds;
use crate::slot::{JobKey, SlotKey};
use crate::time::{self, add, diff, Span, Time, HOUR, MINUTE, SECOND};
use crate::weapon::Weapon;

// ------------------------------------------------------------------------------------
// Réseau

/// Ordre réseau produit par le modèle et exécuté par une implémentation de
/// `network::Network`.
#[derive(Debug, Clone, PartialEq)]
pub enum NetCommand {
    /// `Message::Spread` : diffusion (fitness 1), mémorisée pour les
    /// partenaires qui rejoindront le réseau plus tard.
    Spread(Message),
    /// `Message::Recall` : retrait (fitness 0) d'un message diffusé.
    Recall(Message),
    /// `Ring::ChangePassphrase`.
    ChangePassphrase(String),
    /// `Ring::AnnounceAvailability`.
    AnnounceAvailability,
    /// Clé secrète d'un arbitre (QR code smartphone), utilisée pour
    /// authentifier les requêtes `…/…/<ref>` (`Marshaller::GetSecretKey`).
    RegisterSecretKey { referee: u32, key: String },
}

/// `Object::Spread` d'un colis.
pub fn spread(outbox: &mut Vec<NetCommand>, parcel: &mut Message) {
    parcel.set_fitness(1);
    parcel.mark_spread(true);
    outbox.push(NetCommand::Spread(parcel.clone()));
}

/// `Object::Recall` d'un colis (seulement s'il est « vivant »).
pub fn recall(outbox: &mut Vec<NetCommand>, parcel: &mut Message) {
    parcel.mark_spread(false);
    if parcel.fitness() != 0 {
        parcel.set_fitness(0);
        outbox.push(NetCommand::Recall(parcel.clone()));
    }
}

/// Notifications à destination de l'interface.
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// Afficher la liste d'arbitres d'une arme (`OnExposeWeapon`).
    ExposeWeapon(String),
    /// Déplacement d'un match en cours : griser les panneaux (`OnBlur`).
    Blur,
    UnBlur,
    CompetitionAdded(u32),
    CompetitionRemoved(u32),
    BatchAdded { competition: u32, batch: u32 },
    BatchRemoved { competition: u32, batch: u32 },
    /// Identifiants de connexion reçus du smartphone : fermer le dialogue
    /// de code d'accès.
    PartnerCredentials,
    /// Le tableau des matchs ouvert doit être fermé (match supprimé).
    CloseJobBoard,
}

// ------------------------------------------------------------------------------------
// Tireurs

/// Tireur ou équipe reçu de BellePoule (`BellePoule::Fencer`).
#[derive(Debug, Clone)]
pub struct Fencer {
    pub player: Player,
    /// `netid` du message qui l'a apporté.
    pub netid: u32,
    pub affinities: Affinities,
    /// Membres d'une équipe.
    pub members: Vec<(Player, Affinities)>,
}

// ------------------------------------------------------------------------------------
// Matchs

/// Icône d'état d'un match dans la liste du lot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    None,
    /// `GTK_STOCK_DIALOG_QUESTION` : non planifié.
    Question,
    /// `GTK_STOCK_DIALOG_ERROR`.
    Error,
}

/// Poule ou match à placer (port de `marshaller/job.cpp`).
#[derive(Debug, Clone)]
pub struct Job {
    pub is_pool: bool,
    pub key: JobKey,
    pub netid: u32,
    pub competition: u32,
    pub batch: u32,
    pub name: String,
    pub position: u32,
    pub color: Rgb,
    pub slot: Option<SlotKey>,
    pub fencers: Vec<Fencer>,
    pub kinship: u32,
    pub workload_units: u32,
    pub regular_duration: Span,
    pub duration_span: u32,
    pub over: bool,
    /// Groupe de la poule (entraînement des jeunes).
    pub group: Option<u32>,
    /// Feuille de route `BellePoule2D::Roadmap` (absente pendant la destruction).
    pub parcel: Option<Message>,
}

impl Job {
    /// `Job::Job`.
    pub fn new(key: JobKey, competition: u32, batch: u32, message: &Message, color: Rgb) -> Self {
        let duration_span = match message.get_uint("duration_span") {
            0 => 1,
            n => n,
        };
        let netid = message.net_id();
        let mut parcel = Message::new("BellePoule2D::Roadmap");
        parcel.set_uint("competition", message.get_uint("competition"));
        parcel.set_uint("stage", message.get_uint("stage"));
        parcel.set_uint("batch", message.get_uint("batch"));
        parcel.set_uint("source", netid);
        parcel.set_uint("duration_span", duration_span);
        parcel.set_fitness(1);
        Job {
            is_pool: message.get_str("kind").as_deref() == Some("pool"),
            key,
            netid,
            competition,
            batch,
            name: String::new(),
            position: message.get_uint("display_position"),
            color,
            slot: None,
            fencers: Vec::new(),
            kinship: 0,
            workload_units: message.get_uint("workload_units"),
            regular_duration: 0,
            duration_span,
            over: false,
            group: Some(message.get_uint("group")).filter(|g| *g > 0),
            parcel: Some(parcel),
        }
    }

    /// `SetWeapon` : durée réglementaire arrondie au pas de la frise
    /// (le C++ ajoute toujours un pas, même si la durée est déjà un multiple).
    pub fn set_weapon(&mut self, weapon: &Weapon) {
        let mut d = self.workload_units as Span * weapon.standard_duration_sec as Span * SECOND;
        d += STEP - (d % STEP);
        self.regular_duration = d;
    }

    /// `GetRegularDuration`.
    pub fn duration(&self) -> Span {
        self.regular_duration * self.duration_span as Span
    }

    /// `ExtendDuration`.
    pub fn extend_duration(&mut self, span: u32) {
        self.duration_span = span;
        if let Some(p) = self.parcel.as_mut() {
            p.set_uint("duration_span", span);
        }
    }

    /// `ComparePosition`.
    pub fn compare_position(a: &Job, b: &Job) -> std::cmp::Ordering {
        a.position.cmp(&b.position)
    }
}

// ------------------------------------------------------------------------------------
// Lots

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchStatus {
    Incomplete,
    Complete,
}

/// Ligne de la liste des matchs d'un lot.
#[derive(Debug, Clone, PartialEq)]
pub struct JobRow {
    pub job: JobKey,
    pub status: JobStatus,
    pub warning: Option<Rgb>,
}

/// Fin de rafale (`end_of_burst.cpp`).
#[derive(Debug, Clone)]
pub struct EndOfBurst {
    pub parcel: Message,
    pub suspended: bool,
}

impl EndOfBurst {
    pub fn new(competition: u32, stage: u32, batch: &str) -> Self {
        let mut parcel = Message::new("BellePoule2D::EndOfBurst");
        parcel.set_uint("competition", competition);
        parcel.set_uint("stage", stage);
        parcel.set_str("batch", batch);
        EndOfBurst { parcel, suspended: false }
    }

    pub fn spread(&mut self, outbox: &mut Vec<NetCommand>) {
        if !self.suspended {
            spread(outbox, &mut self.parcel);
        }
    }
}

/// Panneau d'un lot dans l'onglet de la compétition (`batch_panel.cpp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchPanel {
    pub visible: bool,
    pub expected: u32,
    pub ready: u32,
}

/// Lot de matchs reçu du superviseur (`batch.cpp`).
#[derive(Debug, Clone)]
pub struct Batch {
    pub id: u32,
    pub competition: u32,
    pub stage: u32,
    pub batch_ref: String,
    pub name: String,
    pub extension1: Option<String>,
    pub extension2: Option<String>,
    /// Bouton radio d'extension actif : 0 (non), 1, 2.
    pub extension_choice: u8,
    pub rows: Vec<JobRow>,
    pub pending: Vec<JobKey>,
    pub partly: Vec<JobKey>,
    pub scheduled: Vec<JobKey>,
    pub status: BatchStatus,
    pub eob: EndOfBurst,
    pub assign_sensitive: bool,
    pub cancel_sensitive: bool,
    pub panel: BatchPanel,
}

impl Batch {
    pub fn new(message: &Message, competition: u32) -> Self {
        let stage = message.get_uint("stage");
        let batch_ref = message.get_str("batch").unwrap_or_default();
        Batch {
            id: message.net_id(),
            competition,
            stage,
            eob: EndOfBurst::new(competition, stage, &batch_ref),
            batch_ref,
            name: message.get_str("name").unwrap_or_default(),
            extension1: message.get_str("extension1"),
            extension2: message.get_str("extension2"),
            extension_choice: 0,
            rows: Vec::new(),
            pending: Vec::new(),
            partly: Vec::new(),
            scheduled: Vec::new(),
            status: BatchStatus::Incomplete,
            assign_sensitive: true,
            cancel_sensitive: true,
            panel: BatchPanel { visible: true, expected: 0, ready: 0 },
        }
    }

    pub fn row(&self, job: JobKey) -> Option<&JobRow> {
        self.rows.iter().find(|r| r.job == job)
    }
}

// ------------------------------------------------------------------------------------
// Compétitions

/// Compétition annoncée par BellePoule (`competition.cpp`).
#[derive(Debug, Clone)]
pub struct Competition {
    pub id: u32,
    pub color: Rgb,
    /// Libellés utilisateur : `gender`, `weapon`, `category`.
    pub gender: String,
    pub weapon_label: String,
    pub category: String,
    /// Code XML de l'arme.
    pub weapon: String,
    /// `g_list_prepend` : le dernier reçu en tête.
    pub fencers: Vec<Fencer>,
    pub batches: Vec<Batch>,
    pub current_batch: Option<u32>,
    pub frozen: bool,
    /// Icône « lot incomplet » de l'onglet.
    pub batch_image: bool,
}

impl Competition {
    pub fn new(id: u32) -> Self {
        Competition {
            id,
            color: Rgb::default(),
            gender: String::new(),
            weapon_label: String::new(),
            category: String::new(),
            weapon: String::new(),
            fencers: Vec::new(),
            batches: Vec::new(),
            current_batch: None,
            frozen: false,
            batch_image: false,
        }
    }

    pub fn batch(&self, id: u32) -> Option<&Batch> {
        self.batches.iter().find(|b| b.id == id)
    }

    pub fn batch_mut(&mut self, id: u32) -> Option<&mut Batch> {
        self.batches.iter_mut().find(|b| b.id == id)
    }

    /// `GetFencer (ref)`.
    pub fn fencer(&self, reference: u32) -> Option<&Fencer> {
        self.fencers.iter().find(|f| f.player.reference == reference)
    }

    /// Titre court « gender-weapon-category ».
    pub fn title(&self) -> String {
        format!("{}-{}-{}", self.gender, self.weapon_label, self.category)
    }
}

// ------------------------------------------------------------------------------------
// Arbitres

/// Arbitre engagé (`enlisted_referee.cpp`).
#[derive(Debug, Clone)]
pub struct EnlistedReferee {
    pub player: Player,
    /// Créneaux réservés, triés par heure de début.
    pub slots: Vec<SlotKey>,
    /// Charge en minutes.
    pub workload: i32,
    pub affinities: Affinities,
    /// Couleur d'affichage (occupé à l'heure du curseur).
    pub cell_color: Option<Rgb>,
    /// Colis `BellePoule2D::Referee` (après `Disclose`).
    pub parcel: Option<Message>,
    /// Clé du QR code smartphone (`WifiCode::GetKey`).
    pub wifi_key: Option<String>,
}

/// Liste d'arbitres d'une arme (`RefereesList`).
#[derive(Debug, Clone)]
pub struct RefereeList {
    pub weapon: &'static Weapon,
    pub members: Vec<u32>,
    /// Vue réduite (filtre `collapsed`) ou appel (filtre `expanded`).
    pub collapsed: bool,
}

// ------------------------------------------------------------------------------------
// Horloge

/// Horloge décalable (`clock.cpp`).
#[derive(Debug, Clone)]
pub struct Clock {
    pub offset: Span,
    /// Dernière lecture de l'heure système (`_absolute`).
    pub absolute: Time,
    /// Heure figée (tests).
    pub fixed: Option<Time>,
    pub parcel: Option<Message>,
}

impl Clock {
    pub fn new(fixed: Option<Time>) -> Self {
        Clock { offset: 0, absolute: fixed.unwrap_or_else(time::now_local), fixed, parcel: None }
    }

    fn system_now(&self) -> Time {
        self.fixed.unwrap_or_else(time::now_local)
    }

    /// `RetreiveNow`.
    pub fn now(&self) -> Time {
        add(self.system_now(), self.offset)
    }

    /// `OnTimeout` : relit l'heure et renvoie l'affichage `HH:MM`.
    pub fn tick(&mut self) -> String {
        self.absolute = self.system_now();
        time::hh_mm(add(self.absolute, self.offset))
    }

    /// `Set` : décale l'horloge et diffuse `BellePoule2D::ClockOffset`.
    pub fn set(&mut self, to: Time, outbox: &mut Vec<NetCommand>) {
        self.offset = diff(to, self.absolute);
        self.tick();
        let mut parcel = self.parcel.take().unwrap_or_else(|| Message::new("BellePoule2D::ClockOffset"));
        parcel.set_int("offset", (self.offset / SECOND) as i32);
        spread(outbox, &mut parcel);
        self.parcel = Some(parcel);
    }
}

// ------------------------------------------------------------------------------------
// Frise chronologique

/// Pas de la frise (`Timeline::STEP`).
pub const STEP: Span = 15 * MINUTE;
/// Heures visibles (`HOURS_MONITORED`).
pub const HOURS_MONITORED: i64 = 10;
/// Marge avant l'origine (`START_MARGING`).
pub const START_MARGIN: Span = STEP;

/// État de la frise (`timeline.cpp`, sans le dessin).
#[derive(Debug, Clone)]
pub struct Timeline {
    pub origin: Time,
    /// Position du curseur depuis l'origine.
    pub cursor: Span,
    pub width: f64,
    pub height: f64,
    /// Ordre des lignes de compétition (`g_list_append`).
    pub competitions: Vec<u32>,
    pub button_pressed: bool,
    pub slideout_origin: f64,
    pub button_press_origin: f64,
    /// Glissement du curseur en cours (dernière abscisse).
    pub cursor_drag: Option<f64>,
}

impl Timeline {
    pub fn new(now: Time) -> Self {
        use chrono::Timelike;
        let origin = now
            .with_nanosecond(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_minute(0))
            .map(|t| add(t, -START_MARGIN))
            .unwrap_or(now);
        let mut cursor = diff(now, origin);
        cursor += STEP;
        cursor -= cursor % STEP;
        Timeline {
            origin,
            cursor,
            width: 1000.0,
            height: 50.0,
            competitions: Vec::new(),
            button_pressed: false,
            slideout_origin: 0.0,
            button_press_origin: 0.0,
            cursor_drag: None,
        }
    }

    /// Pixels par microseconde.
    pub fn time_scale(&self) -> f64 {
        self.width.max(1.0) / (HOURS_MONITORED * HOUR + START_MARGIN) as f64
    }

    /// `GetTimeRectification`.
    pub fn rectification(time: Span) -> Span {
        let round = time % STEP;
        if round > STEP / 2 {
            STEP - round
        } else {
            -round
        }
    }

    /// `RetreiveCursorTime`.
    pub fn cursor_time(&self, rounded: bool) -> Time {
        let mut c = self.cursor;
        if rounded {
            c += Self::rectification(self.cursor);
        }
        add(self.origin, c)
    }

    /// `SlideOut` (sans les notifications).
    pub fn slide_out(&mut self, position: f64, one_shot: bool) {
        let delta_us = (self.slideout_origin - position) / self.time_scale();
        self.origin = add(self.origin, delta_us as Span);
        self.slideout_origin = if one_shot { 0.0 } else { position };
    }

    /// `SetCursorTime` (sans les notifications).
    pub fn set_cursor_time(&mut self, t: Time) {
        self.cursor = diff(t, self.origin);
        let max = HOURS_MONITORED * HOUR - START_MARGIN;
        if self.cursor < 0 {
            self.slide_out((-self.cursor + START_MARGIN) as f64 * self.time_scale(), true);
            self.cursor = START_MARGIN;
        } else if self.cursor > max {
            self.slide_out((max - self.cursor) as f64 * self.time_scale(), true);
            self.cursor = max;
        }
    }

    /// Abscisse du curseur.
    pub fn cursor_x(&self) -> f64 {
        self.cursor as f64 * self.time_scale()
    }

    /// `TranslateCursor` : renvoie vrai si le curseur a bougé.
    pub fn translate_cursor(&mut self, mut delta: f64) -> bool {
        if delta == 0.0 {
            return false;
        }
        let x1 = self.cursor_x();
        let x2 = x1 + 2.0;
        if delta < 0.0 {
            delta = delta.max(-x1);
        } else {
            delta = delta.min(self.width - x2 * 1.04);
        }
        let new_x = x1 + delta;
        self.cursor = (new_x / self.time_scale()) as Span;
        true
    }

    /// Arrondi de l'origine au pas (relâchement après glissement).
    pub fn round_origin(&mut self) {
        use chrono::Timelike;
        let span = self.origin.minute() as Span * MINUTE
            + self.origin.second() as Span * SECOND
            + (self.origin.nanosecond() / 1000) as Span;
        self.origin = add(self.origin, Self::rectification(span));
    }
}

// ------------------------------------------------------------------------------------
// Lasso

/// Rectangle de sélection (`lasso.cpp`).
#[derive(Debug, Clone, Default)]
pub struct Lasso {
    pub origin: Option<(f64, f64)>,
    pub bounds: Option<Bounds>,
}

impl Lasso {
    pub fn throw(&mut self, x: f64, y: f64) {
        self.origin = Some((x, y));
        self.bounds = Some(Bounds { x1: x, y1: y, x2: x, y2: y });
    }

    pub fn pull(&mut self) {
        self.origin = None;
    }

    pub fn is_active(&self) -> bool {
        self.origin.is_some()
    }

    /// `OnCursorMotion` : renvoie vrai si le lasso est actif.
    pub fn on_cursor_motion(&mut self, x: f64, y: f64) -> bool {
        match self.origin {
            Some((ox, oy)) => {
                self.bounds = Some(Bounds::from_corners(ox, oy, x, y));
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::parse_fie;

    #[test]
    fn job_duration_rounding() {
        let mut m = Message::new("BellePoule::Job");
        m.set_uint("workload_units", 30);
        let mut j = Job::new(1, 1, 1, &m, Rgb::default());
        j.set_weapon(&crate::weapon::WEAPONS[1]); // épée : 30 s
        // 30 × 30 s = 15 min, multiple du pas : un pas de plus.
        assert_eq!(j.duration(), 30 * MINUTE);
        j.extend_duration(2);
        assert_eq!(j.duration(), 60 * MINUTE);
    }

    #[test]
    fn timeline_initial_cursor_on_quarter() {
        let now = parse_fie("01.01.2024", "10:07").unwrap();
        let tl = Timeline::new(now);
        assert_eq!(tl.origin, parse_fie("01.01.2024", "09:45").unwrap());
        assert_eq!(tl.cursor_time(false), parse_fie("01.01.2024", "10:15").unwrap());
    }

    #[test]
    fn timeline_cursor_slides_origin() {
        let now = parse_fie("01.01.2024", "10:00").unwrap();
        let mut tl = Timeline::new(now);
        let target = parse_fie("01.01.2024", "08:00").unwrap();
        tl.set_cursor_time(target);
        assert_eq!(tl.cursor, START_MARGIN);
        assert_eq!(tl.cursor_time(true), target);
        let late = parse_fie("01.01.2024", "23:00").unwrap();
        tl.set_cursor_time(late);
        assert_eq!(tl.cursor_time(true), late);
    }

    #[test]
    fn clock_offset_message() {
        let now = parse_fie("01.01.2024", "10:00").unwrap();
        let mut c = Clock::new(Some(now));
        let mut out = Vec::new();
        c.set(parse_fie("01.01.2024", "10:30").unwrap(), &mut out);
        assert_eq!(c.now(), parse_fie("01.01.2024", "10:30").unwrap());
        match &out[0] {
            NetCommand::Spread(m) => {
                assert!(m.is("BellePoule2D::ClockOffset"));
                assert_eq!(m.get_int("offset"), 1800);
            }
            _ => panic!(),
        }
    }
}
