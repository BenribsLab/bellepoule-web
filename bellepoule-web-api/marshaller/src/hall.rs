//! La salle : pistes, sélection, lasso, glisser des pistes, persistance
//! (port de `marshaller/hall.cpp`, partie modèle).
//!
//! `Hall` est le modèle complet du Marshaller : il possède les pistes, les
//! créneaux, les compétitions, les matchs, les arbitres, l'horloge et la frise.
//! L'interface GTK le relit après chaque action et vide `outbox` (ordres
//! réseau) et `events` (notifications d'interface).

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::affinities::AffinityRules;
use crate::model::{Clock, Competition, Job, Lasso, NetCommand, Timeline, UiEvent};
use crate::piste::{Bounds, Piste, H, W};
use crate::referees::RefereePool;
use crate::slot::{JobKey, PisteKey, Slot, SlotKey};
use crate::time::Time;

/// Variables de la console de mise au point (`ConsoleServer::ExposeVariable`).
#[derive(Debug, Clone)]
pub struct ConsoleVars {
    pub fix_affinities: bool,
    pub preserve_piste: bool,
    pub clock_alterable: bool,
}

impl Default for ConsoleVars {
    fn default() -> Self {
        ConsoleVars {
            fix_affinities: true,
            preserve_piste: true,
            clock_alterable: std::env::var("BELLEPOULE_CLOCK_ALTERABLE").map(|v| v == "1").unwrap_or(false),
        }
    }
}

/// Curseur de souris demandé par la salle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Default,
    Fleur,
    Plus,
    Cross,
}

/// Avertissement de chevauchement (liste `overlap_liststore`).
#[derive(Debug, Clone, PartialEq)]
pub struct OverlapWarning {
    pub competition: u32,
    pub batch: u32,
    pub message: String,
    pub color: crate::color::Rgb,
}

/// État de la barre d'outils de la salle (`SetToolBarSensitivity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarState {
    pub remove: bool,
    pub rotate: bool,
    pub cone_sensitive: bool,
    pub cone_active: bool,
    /// Toute la barre (désactivée pendant le déplacement d'un match).
    pub enabled: bool,
}

/// Match en cours de déplacement (`_floating_job`).
#[derive(Debug, Clone, PartialEq)]
pub struct FloatingJob {
    pub job: JobKey,
    pub referees: Vec<u32>,
    pub additional_pistes: Vec<PisteKey>,
}

pub struct Hall {
    /// Pistes triées par numéro (`CompareId`).
    pub pistes: Vec<Piste>,
    /// Pistes sélectionnées, triées par numéro.
    pub selected: Vec<PisteKey>,
    pub slots: BTreeMap<SlotKey, Slot>,
    pub(crate) next_slot: SlotKey,
    pub(crate) next_piste: PisteKey,
    /// Compétitions (`g_list_prepend` : la plus récente en tête).
    pub competitions: Vec<Competition>,
    pub jobs: BTreeMap<JobKey, Job>,
    pub(crate) next_job: JobKey,
    pub referees: RefereePool,
    pub timeline: Timeline,
    pub clock: Clock,
    pub lasso: Lasso,
    pub affinities: AffinityRules,
    pub console: ConsoleVars,
    pub overlaps: Vec<OverlapWarning>,
    pub floating: Option<FloatingJob>,
    /// Glisser de pistes en cours (dernière position dans la salle).
    pub dragging: Option<(f64, f64)>,
    pub cursor: CursorKind,
    pub toolbar_enabled: bool,
    /// Rafraîchissement périodique actif (`ScheduleRedraw` / `StopRedraw`).
    pub redraw_scheduled: bool,
    /// `hall.json` à réenregistrer (`JsonFile::MakeDirty`).
    pub dirty: bool,
    pub outbox: Vec<NetCommand>,
    pub events: Vec<UiEvent>,
}

impl Hall {
    /// Salle vide. `fixed_now` fige l'horloge (tests).
    pub fn new(fixed_now: Option<Time>) -> Self {
        let clock = Clock::new(fixed_now);
        let now = clock.now();
        Hall {
            pistes: Vec::new(),
            selected: Vec::new(),
            slots: BTreeMap::new(),
            next_slot: 1,
            next_piste: 1,
            competitions: Vec::new(),
            jobs: BTreeMap::new(),
            next_job: 1,
            referees: RefereePool::new(),
            timeline: Timeline::new(now),
            clock,
            lasso: Lasso::default(),
            affinities: AffinityRules::default(),
            console: ConsoleVars::default(),
            overlaps: Vec::new(),
            floating: None,
            dragging: None,
            cursor: CursorKind::Default,
            toolbar_enabled: true,
            redraw_scheduled: true,
            dirty: false,
            outbox: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Disposition par défaut quand `hall.json` est absent (`OnPlugged`).
    pub fn default_layout(&mut self) {
        self.add_piste(0.0, 0.0);
        self.add_piste(0.0, 0.0);
        self.add_piste(W + H, 0.0);
        self.add_piste(0.0, 0.0);
        self.add_piste((W + H) / 2.0, 4.0 * H);
        self.add_piste(0.0, 6.0 * H);
        self.add_piste(0.0, 0.0);
        self.add_piste(W + H, 6.0 * H);
        self.add_piste(0.0, 0.0);
    }

    pub fn take_outbox(&mut self) -> Vec<NetCommand> {
        std::mem::take(&mut self.outbox)
    }

    pub fn take_events(&mut self) -> Vec<UiEvent> {
        std::mem::take(&mut self.events)
    }

    // --------------------------------------------------------------------------------
    // Accès

    pub fn piste(&self, key: PisteKey) -> Option<&Piste> {
        self.pistes.iter().find(|p| p.key == key)
    }

    pub fn piste_mut(&mut self, key: PisteKey) -> Option<&mut Piste> {
        self.pistes.iter_mut().find(|p| p.key == key)
    }

    /// `GetPiste (id)`.
    pub fn piste_by_id(&self, id: u32) -> Option<PisteKey> {
        self.pistes.iter().find(|p| p.id == id).map(|p| p.key)
    }

    pub fn competition(&self, id: u32) -> Option<&Competition> {
        self.competitions.iter().find(|c| c.id == id)
    }

    pub fn competition_mut(&mut self, id: u32) -> Option<&mut Competition> {
        self.competitions.iter_mut().find(|c| c.id == id)
    }

    pub fn job(&self, key: JobKey) -> Option<&Job> {
        self.jobs.get(&key)
    }

    // --------------------------------------------------------------------------------
    // Pistes

    /// `AddPiste` : numérote la piste avec le premier numéro libre.
    pub fn add_piste(&mut self, tx: f64, ty: f64) -> PisteKey {
        let anchor = self.pistes.last().map(|p| p.bounds());
        let key = self.next_piste;
        self.next_piste += 1;
        let cursor = self.timeline.cursor_time(true);
        let mut piste = Piste::new(key, cursor);

        let mut pos = self.pistes.len();
        for (i, p) in self.pistes.iter().enumerate() {
            if p.id != i as u32 + 1 {
                pos = i;
                break;
            }
            piste.id = i as u32 + 2;
        }
        if tx != 0.0 || ty != 0.0 {
            piste.translate(tx, ty);
        } else if let Some(a) = anchor {
            piste.anchor_to(&a);
        }
        self.pistes.insert(pos, piste);
        self.dirty = true;
        key
    }

    /// `SetId` (la liste est retriée pour garder l'ordre des numéros).
    pub fn set_piste_id(&mut self, key: PisteKey, id: u32) {
        if let Some(p) = self.piste_mut(key) {
            p.id = id;
        }
        self.pistes.sort_by_key(|p| p.id);
        self.selected.sort_by_key(|k| self.pistes.iter().position(|p| p.key == *k));
        self.dirty = true;
    }

    /// Renumérotation depuis l'interface, sans ambiguïté pour les feuilles de route.
    pub fn renumber_piste(&mut self, key: PisteKey, id: u32) -> Result<(), String> {
        let Some(piste) = self.piste(key) else {
            return Err(bp_core::i18n::tr("Piste not found."));
        };
        if id == 0 {
            return Err(bp_core::i18n::tr("The piste number must be greater than zero."));
        }
        if piste.id == id {
            return Ok(());
        }
        if self.pistes.iter().any(|p| p.key != key && p.id == id) {
            return Err(bp_core::i18n::tr("This piste number is already used."));
        }
        let jobs = self.piste_jobs(key);
        self.set_piste_id(key, id);
        for job in jobs {
            self.job_make_piste_parcel(job);
            self.job_spread(job);
        }
        Ok(())
    }

    /// `RemovePiste` : annule les créneaux puis retire la piste.
    pub fn remove_piste(&mut self, key: PisteKey) {
        let slots = self.piste(key).map(|p| p.slots.clone()).unwrap_or_default();
        for s in slots {
            self.slot_cancel(s);
        }
        self.pistes.retain(|p| p.key != key);
        self.selected.retain(|k| *k != key);
        self.gc_slots();
        self.dirty = true;
    }

    /// `RemoveSelected` : seules les pistes sans créneau sont retirées.
    pub fn remove_selected(&mut self) {
        let removable: Vec<PisteKey> = self
            .selected
            .iter()
            .copied()
            .filter(|k| self.piste(*k).map(|p| p.slots.is_empty()).unwrap_or(false))
            .collect();
        for k in removable {
            self.remove_piste(k);
        }
        self.redraw();
    }

    pub fn translate_selected(&mut self, dx: f64, dy: f64) {
        for k in self.selected.clone() {
            if let Some(p) = self.piste_mut(k) {
                p.translate(dx, dy);
            }
        }
        self.dirty = true;
    }

    pub fn rotate_selected(&mut self) {
        for k in self.selected.clone() {
            if let Some(p) = self.piste_mut(k) {
                p.rotate();
            }
        }
        self.dirty = true;
    }

    pub fn align_selected_on_grid(&mut self) {
        for k in self.selected.clone() {
            if let Some(p) = self.piste_mut(k) {
                p.align_on_grid();
            }
        }
        self.dirty = true;
    }

    /// `OnConeToggled` : bloque ou débloque les pistes sélectionnées.
    pub fn block_selected(&mut self, block: bool) {
        for k in self.selected.clone() {
            if let Some(p) = self.piste_mut(k) {
                p.blocked = block;
            }
        }
        self.dirty = true;
    }

    /// `SetToolBarSensitivity`.
    pub fn toolbar_state(&self) -> ToolbarState {
        let any = !self.selected.is_empty();
        let mut state = ToolbarState {
            remove: any,
            rotate: any,
            cone_sensitive: false,
            cone_active: false,
            enabled: self.toolbar_enabled,
        };
        if any {
            let blocked: Vec<bool> = self.selected.iter().filter_map(|k| self.piste(*k)).map(|p| p.blocked).collect();
            if let Some(first) = blocked.first() {
                if blocked.iter().all(|b| b == first) {
                    state.cone_sensitive = true;
                    state.cone_active = *first;
                }
            }
        }
        state
    }

    /// `CancelSelection`.
    pub fn cancel_selection(&mut self) {
        for p in self.pistes.iter_mut() {
            p.selected = false;
        }
        self.selected.clear();
    }

    /// `SelectPiste` : place aussi le match flottant s'il y en a un.
    pub fn select_piste(&mut self, key: PisteKey) {
        let Some(piste) = self.piste(key) else { return };
        let blurred = piste.blurred;
        if !self.selected.contains(&key) {
            let id = piste.id;
            let pos = self
                .selected
                .iter()
                .position(|k| self.piste(*k).map(|p| p.id >= id).unwrap_or(true))
                .unwrap_or(self.selected.len());
            self.selected.insert(pos, key);
        }

        if let Some(floating) = self.floating.clone() {
            if !blurred {
                let duration = self.jobs.get(&floating.job).map(|j| j.duration()).unwrap_or(0);
                let cursor = self.timeline.cursor_time(false);
                if let Some(mut free) = self.piste_free_slot(key, cursor, duration) {
                    if floating.additional_pistes.contains(&key)
                        || floating.additional_pistes.iter().any(|p| self.piste_free_slot(*p, cursor, duration).is_none())
                        || floating.referees.iter().any(|r| !self.referee_is_available(*r, &free, duration)) { return; }
                    free.additional_pistes = floating.additional_pistes.clone();
                    let sk = self.slot_insert(free);
                    self.slot_add_job(sk, floating.job);
                    for r in floating.referees.iter().rev() {
                        self.slot_add_referee(sk, *r);
                    }
                }
                self.stop_moving_job();
            }
        }

        if let Some(p) = self.piste_mut(key) {
            p.selected = true;
        }
        self.gc_slots();
    }

    /// `UnSelectPiste`.
    pub fn unselect_piste(&mut self, key: PisteKey) {
        self.selected.retain(|k| *k != key);
        if let Some(p) = self.piste_mut(key) {
            p.selected = false;
        }
    }

    // --------------------------------------------------------------------------------
    // Souris

    /// Piste sous un point de la salle (la dernière dessinée l'emporte).
    pub fn piste_at(&self, x: f64, y: f64) -> Option<PisteKey> {
        self.pistes.iter().rev().find(|p| p.bounds().contains(x, y)).map(|p| p.key)
    }

    /// `OnButtonPress` sur le fond de la salle (bouton 1).
    pub fn background_press(&mut self, x: f64, y: f64) {
        self.stop_moving_job();
        self.cancel_selection();
        self.lasso.throw(x, y);
    }

    /// `OnButtonReleased` sur le fond.
    pub fn background_release(&mut self) {
        self.lasso.pull();
    }

    /// `OnPisteButtonEvent` (appui).
    pub fn piste_press(&mut self, key: PisteKey, ctrl: bool, x: f64, y: f64) {
        if self.selected.contains(&key) {
            if ctrl {
                self.unselect_piste(key);
            } else {
                self.cursor = CursorKind::Fleur;
            }
        } else {
            if !ctrl {
                self.cancel_selection();
            }
            self.select_piste(key);
            self.cursor = CursorKind::Fleur;
        }
        self.dragging = Some((x, y));
    }

    /// `OnPisteButtonEvent` (relâchement ou double-clic).
    pub fn piste_release(&mut self) {
        self.cursor = CursorKind::Default;
        self.lasso.pull();
        if self.dragging.take().is_some() {
            self.align_selected_on_grid();
        }
    }

    /// Double-clic sur une piste (`OnDoubleClick`) : matchs de la piste.
    pub fn piste_jobs(&self, key: PisteKey) -> Vec<JobKey> {
        let mut jobs = Vec::new();
        if let Some(p) = self.piste(key) {
            for s in &p.slots {
                if let Some(slot) = self.slots.get(s) {
                    jobs.extend(slot.jobs.iter().copied());
                }
            }
        }
        jobs
    }

    /// Mouvement de la souris dans la salle (`OnPisteMotionEvent` /
    /// `OnCursorMotion`). `button1` et `ctrl` décrivent l'état des boutons.
    pub fn motion(&mut self, x: f64, y: f64, button1: bool, ctrl: bool) {
        if let Some((lx, ly)) = self.dragging {
            self.cursor = CursorKind::Fleur;
            if button1 && !ctrl {
                self.translate_selected(x - lx, y - ly);
                self.dragging = Some((x, y));
            }
            return;
        }

        self.cursor = CursorKind::Default;
        if self.lasso.on_cursor_motion(x, y) {
            let bounds = self.lasso.bounds.unwrap_or(Bounds { x1: x, y1: y, x2: x, y2: y });
            let keys: Vec<PisteKey> = self.pistes.iter().map(|p| p.key).collect();
            for k in keys {
                self.unselect_piste(k);
                if self.piste(k).map(|p| p.overlaps(&bounds)).unwrap_or(false) {
                    self.select_piste(k);
                }
            }
        } else if self.floating.is_some() {
            self.cursor = match self.piste_at(x, y).and_then(|k| self.piste(k)) {
                Some(p) if p.blurred => CursorKind::Cross,
                _ => CursorKind::Plus,
            };
        }
    }

    // --------------------------------------------------------------------------------
    // Persistance (`hall.json`)

    /// `FeedJsonBuilder`.
    pub fn to_json(&self) -> Value {
        json!({ "pistes": self.pistes.iter().map(Piste::to_json).collect::<Vec<_>>() })
    }

    /// `ReadJson` : renvoie faux si le document n'a pas de membre `pistes`.
    pub fn read_json(&mut self, v: &Value) -> bool {
        let Some(list) = v.get("pistes").and_then(Value::as_array) else { return false };
        let cursor = self.timeline.cursor_time(true);
        for item in list {
            let key = self.next_piste;
            self.next_piste += 1;
            let mut p = Piste::new(key, cursor);
            p.read_json(item);
            let pos = self.pistes.iter().position(|q| q.id >= p.id).unwrap_or(self.pistes.len());
            self.pistes.insert(pos, p);
        }
        true
    }

    /// Chemin de `hall.json` (`<config utilisateur>/BellePoule2D/hall.json`).
    pub fn json_path() -> std::path::PathBuf {
        crate::config::config_dir().join("hall.json")
    }

    /// `JsonFile::Load`.
    pub fn load_file(&mut self, path: &std::path::Path) -> bool {
        match std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
            Some(v) => self.read_json(&v),
            None => false,
        }
    }

    /// `JsonFile::Save`.
    pub fn save_file(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&self.to_json()).unwrap_or_default();
        std::fs::write(path, text)?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::parse_fie;

    fn hall() -> Hall {
        Hall::new(Some(parse_fie("01.01.2024", "10:00").unwrap()))
    }

    #[test]
    fn default_layout_has_nine_numbered_pistes() {
        let mut h = hall();
        h.default_layout();
        let ids: Vec<u32> = h.pistes.iter().map(|p| p.id).collect();
        assert_eq!(ids, (1..=9).collect::<Vec<_>>());
        // La deuxième piste est ancrée sous la première.
        assert_eq!(h.pistes[1].bounds().y1, h.pistes[0].bounds().y1 + H * 1.5);
    }

    #[test]
    fn add_piste_fills_gaps() {
        let mut h = hall();
        let a = h.add_piste(0.0, 0.0);
        let b = h.add_piste(0.0, 0.0);
        h.add_piste(0.0, 0.0);
        h.selected = vec![b];
        h.remove_selected();
        let _ = a;
        let k = h.add_piste(0.0, 0.0);
        assert_eq!(h.piste(k).unwrap().id, 2);
        assert_eq!(h.pistes.iter().map(|p| p.id).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn json_roundtrip() {
        let mut h = hall();
        h.default_layout();
        h.selected = vec![h.pistes[2].key];
        h.rotate_selected();
        h.block_selected(true);
        let j = h.to_json();
        let mut g = hall();
        assert!(g.read_json(&j));
        assert_eq!(g.pistes.len(), 9);
        for (p, q) in h.pistes.iter().zip(g.pistes.iter()) {
            assert_eq!(p.bounds(), q.bounds());
            assert_eq!(p.blocked, q.blocked);
        }
        assert!(!g.read_json(&serde_json::json!({})));
    }

    #[test]
    fn renumbering_rejects_duplicates_and_preserves_piste_geometry() {
        let mut h = hall();
        h.default_layout();
        let key = h.pistes[0].key;
        let bounds = h.piste(key).unwrap().bounds();
        h.dirty = false;
        assert!(h.renumber_piste(key, 0).is_err());
        assert!(h.renumber_piste(key, 2).is_err());
        assert_eq!(h.piste(key).unwrap().id, 1);
        assert!(!h.dirty);
        h.renumber_piste(key, 12).unwrap();
        assert_eq!(h.piste(key).unwrap().bounds(), bounds);
        assert_eq!(h.piste_by_id(12), Some(key));
        let json = h.to_json();
        let mut loaded = hall();
        assert!(loaded.read_json(&json));
        assert!(loaded.piste_by_id(12).is_some());
        assert!(loaded.piste_by_id(1).is_none());
    }

    #[test]
    fn lasso_selects_and_drag_moves() {
        let mut h = hall();
        h.default_layout();
        h.background_press(0.0, 0.0);
        h.motion(300.0, 40.0, true, false);
        h.background_release();
        assert_eq!(h.selected.len(), 2); // pistes 1 et 3 (même rangée)
        let toolbar = h.toolbar_state();
        assert!(toolbar.remove && toolbar.cone_sensitive && !toolbar.cone_active);

        let k = h.pistes[0].key;
        let before = h.piste(k).unwrap().bounds();
        h.piste_press(k, false, 20.0, 20.0);
        h.motion(52.0, 20.0, true, false);
        h.piste_release();
        let after = h.piste(k).unwrap().bounds();
        assert_eq!(after.x1, before.x1 + 30.0);
        assert_eq!(after.y1, before.y1);
    }
}
