//! Feuille de route « jardin autonome » : étapes datées (installation unique ou
//! récurrentes chaque année), statut calculé d'après la date du jour, le climat
//! local et le carnet. Catalogue d'étapes : `data/roadmap.json`.

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::domain::soil::Soil;
use crate::domain::species::CalendarWindow;

/// Une étape en retard reste signalée ce nombre de jours après sa fenêtre.
const LATE_GRACE_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// Installation : faite une fois pour toutes.
    Setup,
    /// Geste de saison, à refaire chaque année.
    Yearly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub theme: String,
    pub kind: StepKind,
    pub title: String,
    pub why: String,
    pub how: Vec<String>,
    /// Fenêtre nominale (référence Paris).
    pub window: CalendarWindow,
    /// Décaler la fenêtre selon l'avance/le retard de printemps local.
    pub climate_shift: bool,
    pub requires: Vec<String>,
    /// Type d'action du carnet qui valide l'étape automatiquement.
    pub done_by: Option<String>,
    /// Validée dès que le sol est renseigné dans les réglages (sans date).
    #[serde(default)]
    pub done_when_soil_known: bool,
    /// Sols concernés ; vide = tous les sols.
    #[serde(default)]
    pub soils: Vec<Soil>,
    /// Conseils ajoutés à `how` selon le sol du jardin (fusionnés à la lecture).
    #[serde(default, skip_serializing)]
    pub how_by_soil: HashMap<Soil, Vec<String>>,
}

impl Step {
    fn applies_to(&self, soil: Soil) -> bool {
        self.soils.is_empty() || self.soils.contains(&soil)
    }

    /// L'étape vue pour ce sol : conseils spécifiques ajoutés à la fin de `how`.
    fn for_soil(&self, soil: Soil) -> Step {
        let mut step = self.clone();
        step.how.extend(self.how_by_soil.get(&soil).into_iter().flatten().cloned());
        step
    }

    pub fn load_from_json(path: &str) -> Result<Vec<Step>, String> {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("lecture {path} : {e}"))?;
        serde_json::from_str(&raw).map_err(|e| format!("parse {path} : {e}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Now,
    Late,
    Upcoming,
    Done,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepStatus {
    #[serde(flatten)]
    pub step: Step,
    pub status: Status,
    /// Fenêtre décalée au climat local.
    pub window_local: CalendarWindow,
    /// Jours avant la prochaine ouverture de la fenêtre (0 si ouverte).
    pub starts_in_days: i64,
    /// Date de réalisation retenue (cette année pour une étape récurrente).
    pub done_on: Option<NaiveDate>,
    /// Étapes préalables pas encore faites.
    pub waiting_for: Vec<String>,
    /// Faite à la main (annulable), pas déduite du carnet.
    pub manual: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Roadmap {
    pub soil: Soil,
    pub setup_done: usize,
    pub setup_total: usize,
    pub yearly_done: usize,
    pub yearly_total: usize,
    pub steps: Vec<StepStatus>,
}

/// Réalisation d'une étape : cochée à la main ou déduite d'une action du carnet.
#[derive(Debug, Clone)]
pub struct Completion {
    pub step_id: String,
    pub date: NaiveDate,
}

/// Jour de l'année ramené à 1..=365 (le 31/12 bissextile compte comme le 365).
fn doy(d: NaiveDate) -> u16 {
    d.ordinal().min(365) as u16
}

fn date_of_doy(year: i32, doy: u16) -> NaiveDate {
    NaiveDate::from_yo_opt(year, doy as u32)
        .or_else(|| NaiveDate::from_yo_opt(year, 365))
        .expect("jour 1..=365 toujours valide")
}

/// Début de l'occurrence la plus récente de la fenêtre (≤ today), et sa fin.
fn last_occurrence(w: CalendarWindow, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let mut start = date_of_doy(today.year(), w.doy_start);
    if start > today {
        start = date_of_doy(today.year() - 1, w.doy_start);
    }
    let len = (w.doy_end as i64 - w.doy_start as i64).rem_euclid(365);
    (start, start + Duration::days(len))
}

fn days_until_start(w: CalendarWindow, today: NaiveDate) -> i64 {
    (w.doy_start as i64 - doy(today) as i64).rem_euclid(365)
}

/// Contexte du jardin pour évaluer la feuille de route.
#[derive(Debug, Clone, Copy)]
pub struct Garden {
    pub today: NaiveDate,
    /// Avance (-) ou retard (+) de printemps local, en jours.
    pub shift: i16,
    pub soil: Soil,
}

/// Statut de chaque étape applicable au sol du jardin, à la date `garden.today`.
/// `manual` : étapes cochées à la main ; `journal` : (type d'action, date) du carnet.
pub fn roadmap_status(
    catalog: &[Step],
    garden: Garden,
    manual: &[Completion],
    journal: &[(String, NaiveDate)],
) -> Roadmap {
    let Garden { today, shift, soil } = garden;
    let steps: Vec<Step> = catalog.iter().filter(|s| s.applies_to(soil)).map(|s| s.for_soil(soil)).collect();
    let steps = steps.as_slice();
    let windows: Vec<CalendarWindow> = steps
        .iter()
        .map(|s| if s.climate_shift { s.window.shifted(shift) } else { s.window })
        .collect();

    // Dernière réalisation valable pour l'étape (ever pour Setup, cette occurrence pour Yearly).
    let done_on = |i: usize| -> Option<(NaiveDate, bool)> {
        let step = &steps[i];
        let since = match step.kind {
            StepKind::Setup => NaiveDate::MIN,
            StepKind::Yearly => {
                let (start, _) = last_occurrence(windows[i], today);
                // Fait un peu avant l'ouverture : compte quand même pour cette saison.
                start - Duration::days(LATE_GRACE_DAYS)
            }
        };
        let by_hand = manual
            .iter()
            .filter(|c| c.step_id == step.id && c.date >= since && c.date <= today)
            .map(|c| (c.date, true));
        let by_journal = journal
            .iter()
            .filter(|(kind, d)| step.done_by.as_deref() == Some(kind.as_str()) && *d >= since && *d <= today)
            .map(|(_, d)| (*d, false));
        by_hand.chain(by_journal).max_by_key(|(d, _)| *d)
    };
    let soil_known = soil != Soil::Unknown;
    let done: Vec<Option<(Option<NaiveDate>, bool)>> = (0..steps.len())
        .map(|i| {
            done_on(i)
                .map(|(date, by_hand)| (Some(date), by_hand))
                .or_else(|| (steps[i].done_when_soil_known && soil_known).then_some((None, false)))
        })
        .collect();
    let is_done = |id: &str| steps.iter().position(|s| s.id == id).is_some_and(|i| done[i].is_some());

    let mut out: Vec<StepStatus> = steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let w = windows[i];
            let (_, end) = last_occurrence(w, today);
            let status = if done[i].is_some() {
                Status::Done
            } else if w.contains(doy(today)) {
                Status::Now
            } else if (today - end).num_days() <= LATE_GRACE_DAYS {
                Status::Late
            } else {
                Status::Upcoming
            };
            StepStatus {
                step: step.clone(),
                status,
                window_local: w,
                starts_in_days: if w.contains(doy(today)) { 0 } else { days_until_start(w, today) },
                done_on: done[i].and_then(|(date, _)| date),
                waiting_for: step.requires.iter().filter(|r| !is_done(r)).cloned().collect(),
                manual: done[i].is_some_and(|(_, by_hand)| by_hand),
            }
        })
        .collect();

    let count = |kind: StepKind, only_done: bool| {
        out.iter().filter(|s| s.step.kind == kind && (!only_done || s.status == Status::Done)).count()
    };
    let (setup_done, setup_total) = (count(StepKind::Setup, true), count(StepKind::Setup, false));
    let (yearly_done, yearly_total) = (count(StepKind::Yearly, true), count(StepKind::Yearly, false));

    // À faire d'abord (prêtes avant bloquées), puis à venir par date, puis faites.
    let rank = |s: &StepStatus| match s.status {
        Status::Late => 0,
        Status::Now => 1,
        Status::Upcoming => 2,
        Status::Done => 3,
    };
    out.sort_by_key(|s| (rank(s), !s.waiting_for.is_empty(), s.starts_in_days));
    Roadmap { soil, setup_done, setup_total, yearly_done, yearly_total, steps: out }
}

// ---------------------------------------------------------------- SQLite

pub fn migrate(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS roadmap_done (
    step_id TEXT NOT NULL,
    date TEXT NOT NULL,
    PRIMARY KEY (step_id, date)
);
"#,
    )
    .map_err(|e| format!("migration roadmap_done : {e}"))
}

pub fn list_done(db: &Db) -> Result<Vec<Completion>, String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let mut stmt = conn
        .prepare("SELECT step_id, date FROM roadmap_done")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (step_id, date) = row.map_err(|e| e.to_string())?;
        if let Ok(date) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") {
            out.push(Completion { step_id, date });
        }
    }
    Ok(out)
}

pub fn mark_done(db: &Db, step_id: &str, date: NaiveDate) -> Result<(), String> {
    let conn = db.lock().expect("DB mutex poisoned");
    conn.execute(
        "INSERT OR IGNORE INTO roadmap_done (step_id, date) VALUES (?, ?)",
        params![step_id, date.format("%Y-%m-%d").to_string()],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Annule la dernière réalisation cochée à la main.
pub fn unmark_done(db: &Db, step_id: &str) -> Result<(), String> {
    let conn = db.lock().expect("DB mutex poisoned");
    conn.execute(
        "DELETE FROM roadmap_done WHERE step_id = ?1 AND date = (SELECT MAX(date) FROM roadmap_done WHERE step_id = ?1)",
        params![step_id],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("date valide")
    }

    fn step(id: &str, kind: StepKind, start: u16, end: u16) -> Step {
        Step {
            id: id.into(),
            theme: "sol".into(),
            kind,
            title: id.into(),
            why: String::new(),
            how: vec![],
            window: CalendarWindow { doy_start: start, doy_end: end },
            climate_shift: false,
            requires: vec![],
            done_by: None,
            done_when_soil_known: false,
            soils: vec![],
            how_by_soil: HashMap::new(),
        }
    }

    fn garden(today: NaiveDate, shift: i16) -> Garden {
        Garden { today, shift, soil: Soil::Unknown }
    }

    fn status_of(r: &Roadmap, id: &str) -> Status {
        r.steps.iter().find(|s| s.step.id == id).expect("étape présente").status
    }

    #[test]
    fn catalog_parses_and_requirements_exist() {
        let steps = Step::load_from_json("data/roadmap.json").expect("roadmap.json valide");
        assert!(steps.len() >= 10);
        for s in &steps {
            for r in &s.requires {
                assert!(steps.iter().any(|o| &o.id == r), "{} requiert {r} inconnu", s.id);
            }
            assert!((1..=365).contains(&s.window.doy_start) && (1..=365).contains(&s.window.doy_end));
        }
    }

    #[test]
    fn status_follows_window() {
        // 12 novembre = jour 316.
        let steps = vec![
            step("now", StepKind::Yearly, 300, 330),
            step("late", StepKind::Yearly, 250, 300),
            step("later", StepKind::Yearly, 60, 120),
            step("wraps", StepKind::Setup, 305, 45),
        ];
        let r = roadmap_status(&steps, garden(d(2026, 11, 12), 0), &[], &[]);
        assert_eq!(status_of(&r, "now"), Status::Now);
        assert_eq!(status_of(&r, "late"), Status::Late);
        assert_eq!(status_of(&r, "later"), Status::Upcoming);
        assert_eq!(status_of(&r, "wraps"), Status::Now);
        assert_eq!(r.steps[0].step.id, "late");
    }

    #[test]
    fn setup_done_once_yearly_done_per_season() {
        let steps = vec![step("install", StepKind::Setup, 60, 120), step("mulch", StepKind::Yearly, 290, 340)];
        let manual = vec![
            Completion { step_id: "install".into(), date: d(2024, 4, 1) },
            Completion { step_id: "mulch".into(), date: d(2025, 11, 1) },
        ];
        let r = roadmap_status(&steps, garden(d(2026, 11, 12), 0), &manual, &[]);
        assert_eq!(status_of(&r, "install"), Status::Done);
        assert_eq!(status_of(&r, "mulch"), Status::Now, "le paillage 2025 ne vaut pas pour 2026");
        assert_eq!((r.setup_done, r.yearly_done), (1, 0));
    }

    #[test]
    fn journal_action_completes_step() {
        let mut mulch = step("mulch", StepKind::Yearly, 290, 340);
        mulch.done_by = Some("paillage".into());
        let journal = vec![("paillage".to_string(), d(2026, 11, 2))];
        let r = roadmap_status(&[mulch], garden(d(2026, 11, 12), 0), &[], &journal);
        assert_eq!(r.steps[0].status, Status::Done);
        assert!(!r.steps[0].manual);
        assert_eq!(r.steps[0].done_on, Some(d(2026, 11, 2)));
    }

    #[test]
    fn requirements_and_climate_shift() {
        let mut drip = step("drip", StepKind::Setup, 300, 320);
        drip.requires = vec!["survey".into()];
        drip.climate_shift = true;
        let steps = vec![step("survey", StepKind::Setup, 1, 40), drip];
        // Climat plus froid de 20 jours : la fenêtre s'ouvre au jour 320.
        let r = roadmap_status(&steps, garden(d(2026, 11, 12), 20), &[], &[]);
        let drip = r.steps.iter().find(|s| s.step.id == "drip").expect("présente");
        assert_eq!(drip.window_local.doy_start, 320);
        assert_eq!(drip.status, Status::Upcoming);
        assert_eq!(drip.starts_in_days, 4);
        assert_eq!(drip.waiting_for, vec!["survey".to_string()]);
    }

    #[test]
    fn soil_test_is_done_once_the_soil_is_known() {
        let mut soil_test = step("soil_test", StepKind::Setup, 1, 365);
        soil_test.done_when_soil_known = true;
        let steps = vec![soil_test];
        let today = d(2026, 11, 20);

        let unknown = roadmap_status(&steps, Garden { today, shift: 0, soil: Soil::Unknown }, &[], &[]);
        let clay = roadmap_status(&steps, Garden { today, shift: 0, soil: Soil::Clay }, &[], &[]);

        assert_eq!(status_of(&unknown, "soil_test"), Status::Now);
        assert_eq!(status_of(&clay, "soil_test"), Status::Done);
        assert_eq!(clay.steps[0].done_on, None);
        assert!(!clay.steps[0].manual);
    }

    #[test]
    fn soil_filters_steps_and_adds_advice() {
        let mut clay_only = step("no_dig", StepKind::Setup, 300, 330);
        clay_only.soils = vec![Soil::Clay];
        let mut mulch = step("mulch", StepKind::Yearly, 300, 330);
        mulch.how = vec!["5 cm".into()];
        mulch.how_by_soil = HashMap::from([(Soil::Clay, vec!["10 cm en argile".to_string()])]);
        let steps = vec![clay_only, mulch];
        let today = d(2026, 11, 12);

        let loam = roadmap_status(&steps, Garden { today, shift: 0, soil: Soil::Loam }, &[], &[]);
        let clay = roadmap_status(&steps, Garden { today, shift: 0, soil: Soil::Clay }, &[], &[]);

        assert_eq!(loam.steps.len(), 1);
        assert_eq!(loam.steps[0].step.how, vec!["5 cm".to_string()]);
        assert_eq!(clay.steps.len(), 2);
        let clay_mulch = clay.steps.iter().find(|s| s.step.id == "mulch").expect("présente");
        assert_eq!(clay_mulch.step.how, vec!["5 cm".to_string(), "10 cm en argile".to_string()]);
    }
}
