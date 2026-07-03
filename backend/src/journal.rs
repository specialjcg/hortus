//! Carnet de jardin : parcelles et actions journalisées.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parcel {
    pub id: i64,
    pub name: String,
    pub surface_m2: Option<f64>,
    pub exposition: Option<String>,
    pub soil_notes: Option<String>,
    pub grid_x: i64,
    pub grid_y: i64,
    pub grid_w: i64,
    pub grid_h: i64,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ParcelInput {
    pub name: String,
    pub surface_m2: Option<f64>,
    pub exposition: Option<String>,
    pub soil_notes: Option<String>,
    #[serde(default)]
    pub grid_x: Option<i64>,
    #[serde(default)]
    pub grid_y: Option<i64>,
    #[serde(default)]
    pub grid_w: Option<i64>,
    #[serde(default)]
    pub grid_h: Option<i64>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: i64,
    pub date: String,
    pub parcel_id: Option<i64>,
    pub species_id: Option<String>,
    pub kind: String,
    pub quantity_g: Option<f64>,
    pub notes: Option<String>,
    pub grid_x: Option<i64>,
    pub grid_y: Option<i64>,
    pub solution: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ActionInput {
    pub date: String,
    pub parcel_id: Option<i64>,
    pub species_id: Option<String>,
    pub kind: String,
    pub quantity_g: Option<f64>,
    pub notes: Option<String>,
    pub grid_x: Option<i64>,
    pub grid_y: Option<i64>,
    #[serde(default)]
    pub solution: Option<String>,
}

/// Types d'action acceptés.
pub const ACTION_KINDS: &[&str] = &[
    "semis_direct",
    "semis_abri",
    "repiquage",
    "arrosage",
    "paillage",
    "compost",
    "recolte",
    "traitement",
    "arrachage",
    "note",
];

pub fn list_parcels(db: &Db) -> Result<Vec<Parcel>, String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let mut stmt = conn
        .prepare("SELECT id, name, surface_m2, exposition, soil_notes, grid_x, grid_y, grid_w, grid_h, color, created_at FROM parcel ORDER BY name")
        .map_err(|e| format!("prep : {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Parcel {
                id: r.get(0)?,
                name: r.get(1)?,
                surface_m2: r.get(2)?,
                exposition: r.get(3)?,
                soil_notes: r.get(4)?,
                grid_x: r.get(5)?,
                grid_y: r.get(6)?,
                grid_w: r.get(7)?,
                grid_h: r.get(8)?,
                color: r.get(9)?,
                created_at: r.get(10)?,
            })
        })
        .map_err(|e| format!("query : {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("collect : {e}"))
}

pub fn insert_parcel(db: &Db, input: &ParcelInput) -> Result<Parcel, String> {
    let conn = db.lock().expect("DB mutex poisoned");
    conn.execute(
        "INSERT INTO parcel (name, surface_m2, exposition, soil_notes, grid_x, grid_y, grid_w, grid_h, color) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            input.name,
            input.surface_m2,
            input.exposition,
            input.soil_notes,
            input.grid_x.unwrap_or(0),
            input.grid_y.unwrap_or(0),
            input.grid_w.unwrap_or(2),
            input.grid_h.unwrap_or(2),
            input.color.clone().unwrap_or_else(|| "#8fbc4a".into()),
        ],
    )
    .map_err(|e| format!("insert parcel : {e}"))?;
    let id = conn.last_insert_rowid();
    let parcel = conn
        .query_row(
            "SELECT id, name, surface_m2, exposition, soil_notes, grid_x, grid_y, grid_w, grid_h, color, created_at FROM parcel WHERE id = ?",
            params![id],
            |r| {
                Ok(Parcel {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    surface_m2: r.get(2)?,
                    exposition: r.get(3)?,
                    soil_notes: r.get(4)?,
                    grid_x: r.get(5)?,
                    grid_y: r.get(6)?,
                    grid_w: r.get(7)?,
                    grid_h: r.get(8)?,
                    color: r.get(9)?,
                    created_at: r.get(10)?,
                })
            },
        )
        .map_err(|e| format!("fetch parcel : {e}"))?;
    Ok(parcel)
}

pub fn update_parcel(db: &Db, id: i64, input: &ParcelInput) -> Result<Parcel, String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let n = conn
        .execute(
            "UPDATE parcel SET name = ?, surface_m2 = ?, exposition = ?, soil_notes = ?, grid_x = COALESCE(?, grid_x), grid_y = COALESCE(?, grid_y), grid_w = COALESCE(?, grid_w), grid_h = COALESCE(?, grid_h), color = COALESCE(?, color) WHERE id = ?",
            params![
                input.name,
                input.surface_m2,
                input.exposition,
                input.soil_notes,
                input.grid_x,
                input.grid_y,
                input.grid_w,
                input.grid_h,
                input.color,
                id
            ],
        )
        .map_err(|e| format!("update : {e}"))?;
    if n == 0 {
        return Err("parcelle inconnue".into());
    }
    let parcel = conn
        .query_row(
            "SELECT id, name, surface_m2, exposition, soil_notes, grid_x, grid_y, grid_w, grid_h, color, created_at FROM parcel WHERE id = ?",
            params![id],
            |r| {
                Ok(Parcel {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    surface_m2: r.get(2)?,
                    exposition: r.get(3)?,
                    soil_notes: r.get(4)?,
                    grid_x: r.get(5)?,
                    grid_y: r.get(6)?,
                    grid_w: r.get(7)?,
                    grid_h: r.get(8)?,
                    color: r.get(9)?,
                    created_at: r.get(10)?,
                })
            },
        )
        .map_err(|e| format!("fetch : {e}"))?;
    Ok(parcel)
}

pub fn delete_parcel(db: &Db, id: i64) -> Result<(), String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let n = conn
        .execute("DELETE FROM parcel WHERE id = ?", params![id])
        .map_err(|e| format!("delete : {e}"))?;
    if n == 0 {
        return Err("parcelle inconnue".into());
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
pub struct ActionFilter {
    pub parcel_id: Option<i64>,
    pub species_id: Option<String>,
    pub kind: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub limit: Option<u32>,
}

pub fn list_actions(db: &Db, f: &ActionFilter) -> Result<Vec<Action>, String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let mut sql = String::from(
        "SELECT id, date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y, solution, created_at FROM action WHERE 1=1",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(p) = f.parcel_id {
        sql.push_str(" AND parcel_id = ?");
        args.push(Box::new(p));
    }
    if let Some(s) = &f.species_id {
        sql.push_str(" AND species_id = ?");
        args.push(Box::new(s.clone()));
    }
    if let Some(k) = &f.kind {
        sql.push_str(" AND kind = ?");
        args.push(Box::new(k.clone()));
    }
    if let Some(fd) = &f.from_date {
        sql.push_str(" AND date >= ?");
        args.push(Box::new(fd.clone()));
    }
    if let Some(td) = &f.to_date {
        sql.push_str(" AND date <= ?");
        args.push(Box::new(td.clone()));
    }
    sql.push_str(" ORDER BY date DESC, id DESC");
    if let Some(lim) = f.limit {
        sql.push_str(&format!(" LIMIT {}", lim));
    }
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("prep : {e}"))?;
    let args_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(rusqlite::params_from_iter(args_ref), |r| {
            Ok(Action {
                id: r.get(0)?,
                date: r.get(1)?,
                parcel_id: r.get(2)?,
                species_id: r.get(3)?,
                kind: r.get(4)?,
                quantity_g: r.get(5)?,
                notes: r.get(6)?,
                grid_x: r.get(7)?,
                grid_y: r.get(8)?,
                solution: r.get(9)?,
                created_at: r.get(10)?,
            })
        })
        .map_err(|e| format!("query : {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("collect : {e}"))
}

pub fn insert_action(db: &Db, input: &ActionInput) -> Result<Action, String> {
    if !ACTION_KINDS.contains(&input.kind.as_str()) {
        return Err(format!("type d'action invalide : {}", input.kind));
    }
    let conn = db.lock().expect("DB mutex poisoned");
    conn.execute(
        "INSERT INTO action (date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y, solution) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![input.date, input.parcel_id, input.species_id, input.kind, input.quantity_g, input.notes, input.grid_x, input.grid_y, input.solution],
    )
    .map_err(|e| format!("insert : {e}"))?;
    let id = conn.last_insert_rowid();
    conn.query_row(
        "SELECT id, date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y, solution, created_at FROM action WHERE id = ?",
        params![id],
        |r| {
            Ok(Action {
                id: r.get(0)?,
                date: r.get(1)?,
                parcel_id: r.get(2)?,
                species_id: r.get(3)?,
                kind: r.get(4)?,
                quantity_g: r.get(5)?,
                notes: r.get(6)?,
                grid_x: r.get(7)?,
                grid_y: r.get(8)?,
                solution: r.get(9)?,
                created_at: r.get(10)?,
            })
        },
    )
    .map_err(|e| format!("fetch : {e}"))
}

pub fn insert_actions_bulk(db: &Db, inputs: &[ActionInput]) -> Result<Vec<Action>, String> {
    for input in inputs {
        if !ACTION_KINDS.contains(&input.kind.as_str()) {
            return Err(format!("type d'action invalide : {}", input.kind));
        }
    }
    let mut conn = db.lock().expect("DB mutex poisoned");
    let tx = conn.transaction().map_err(|e| format!("tx : {e}"))?;
    let mut results = Vec::with_capacity(inputs.len());
    for input in inputs {
        tx.execute(
            "INSERT INTO action (date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y, solution) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![input.date, input.parcel_id, input.species_id, input.kind, input.quantity_g, input.notes, input.grid_x, input.grid_y, input.solution],
        )
        .map_err(|e| format!("insert : {e}"))?;
        let id = tx.last_insert_rowid();
        let action = tx
            .query_row(
                "SELECT id, date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y, solution, created_at FROM action WHERE id = ?",
                params![id],
                |r| {
                    Ok(Action {
                        id: r.get(0)?,
                        date: r.get(1)?,
                        parcel_id: r.get(2)?,
                        species_id: r.get(3)?,
                        kind: r.get(4)?,
                        quantity_g: r.get(5)?,
                        notes: r.get(6)?,
                        grid_x: r.get(7)?,
                        grid_y: r.get(8)?,
                        solution: r.get(9)?,
                        created_at: r.get(10)?,
                    })
                },
            )
            .map_err(|e| format!("fetch : {e}"))?;
        results.push(action);
    }
    tx.commit().map_err(|e| format!("commit : {e}"))?;
    Ok(results)
}

pub fn update_action(db: &Db, id: i64, input: &ActionInput) -> Result<Action, String> {
    if !ACTION_KINDS.contains(&input.kind.as_str()) {
        return Err(format!("type d'action invalide : {}", input.kind));
    }
    let conn = db.lock().expect("DB mutex poisoned");
    let n = conn
        .execute(
            "UPDATE action SET date = ?, parcel_id = ?, species_id = ?, kind = ?, quantity_g = ?, notes = ?, grid_x = ?, grid_y = ?, solution = COALESCE(?, solution) WHERE id = ?",
            params![input.date, input.parcel_id, input.species_id, input.kind, input.quantity_g, input.notes, input.grid_x, input.grid_y, input.solution, id],
        )
        .map_err(|e| format!("update : {e}"))?;
    if n == 0 {
        return Err("action inconnue".into());
    }
    conn.query_row(
        "SELECT id, date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y, solution, created_at FROM action WHERE id = ?",
        params![id],
        |r| {
            Ok(Action {
                id: r.get(0)?,
                date: r.get(1)?,
                parcel_id: r.get(2)?,
                species_id: r.get(3)?,
                kind: r.get(4)?,
                quantity_g: r.get(5)?,
                notes: r.get(6)?,
                grid_x: r.get(7)?,
                grid_y: r.get(8)?,
                solution: r.get(9)?,
                created_at: r.get(10)?,
            })
        },
    )
    .map_err(|e| format!("fetch : {e}"))
}

pub fn delete_action(db: &Db, id: i64) -> Result<(), String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let n = conn
        .execute("DELETE FROM action WHERE id = ?", params![id])
        .map_err(|e| format!("delete : {e}"))?;
    if n == 0 {
        return Err("action inconnue".into());
    }
    Ok(())
}

pub fn delete_all_actions(db: &Db) -> Result<(), String> {
    let conn = db.lock().expect("DB mutex poisoned");
    conn.execute("DELETE FROM action", [])
        .map_err(|e| format!("delete all : {e}"))?;
    Ok(())
}

/// Petit helper pour détecter si une parcelle existe.
pub fn parcel_exists(db: &Db, id: i64) -> Result<bool, String> {
    let conn = db.lock().expect("DB mutex poisoned");
    conn.query_row("SELECT 1 FROM parcel WHERE id = ?", params![id], |_| Ok(()))
        .optional()
        .map(|o| o.is_some())
        .map_err(|e| format!("exists : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn mk_db() -> Db {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            r#"
            CREATE TABLE parcel (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                surface_m2 REAL,
                exposition TEXT,
                soil_notes TEXT,
                grid_x INTEGER NOT NULL DEFAULT 0,
                grid_y INTEGER NOT NULL DEFAULT 0,
                grid_w INTEGER NOT NULL DEFAULT 2,
                grid_h INTEGER NOT NULL DEFAULT 2,
                color TEXT NOT NULL DEFAULT '#8fbc4a',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE action (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                parcel_id INTEGER,
                species_id TEXT,
                kind TEXT NOT NULL,
                quantity_g REAL,
                notes TEXT,
                grid_x INTEGER,
                grid_y INTEGER,
                solution TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (parcel_id) REFERENCES parcel(id) ON DELETE SET NULL
            );
            "#,
        )
        .expect("schema");
        Arc::new(Mutex::new(conn))
    }

    fn mk_action(date: &str, kind: &str, sid: Option<&str>, x: Option<i64>, y: Option<i64>) -> ActionInput {
        ActionInput {
            date: date.into(),
            parcel_id: None,
            species_id: sid.map(String::from),
            kind: kind.into(),
            quantity_g: None,
            notes: None,
            grid_x: x,
            grid_y: y,
            solution: None,
        }
    }

    /// Régression : un client qui demande un limit > total ne doit pas crasher
    /// et doit recevoir TOUTES les actions, y compris les plus anciennes.
    #[test]
    fn list_actions_high_limit_returns_all() {
        let db = mk_db();
        // 10 arrosages récents + 5 semis_abri anciens
        for i in 0..10 {
            insert_action(&db, &mk_action(&format!("2026-05-{:02}", i + 1), "arrosage", Some("tomato"), Some(10), Some(20))).unwrap();
        }
        for i in 0..5 {
            insert_action(&db, &mk_action(&format!("2026-04-{:02}", i + 1), "semis_abri", Some("pumpkin"), Some(100), Some(50))).unwrap();
        }

        let f = ActionFilter { limit: Some(5000), ..Default::default() };
        let got = list_actions(&db, &f).expect("list ok");
        assert_eq!(got.len(), 15, "doit retourner les 15 actions, pas seulement 10");
        assert_eq!(got.iter().filter(|a| a.kind == "semis_abri").count(), 5);
    }

    /// Régression historique : limit=200 + 200 arrosages récents écrasaient les
    /// semis_abri plus anciens (cause du bug "abri vide").
    #[test]
    fn list_actions_low_limit_drops_old_semis_abri() {
        let db = mk_db();
        // 10 semis_abri anciens (avril)
        for i in 0..10 {
            insert_action(&db, &mk_action(&format!("2026-04-{:02}", i + 1), "semis_abri", Some("pumpkin"), Some(100), Some(50))).unwrap();
        }
        // 200 arrosages récents (mai) - plus récents donc en tête du tri DESC
        for i in 0..200 {
            insert_action(&db, &mk_action(&format!("2026-05-{:02}", (i % 28) + 1), "arrosage", Some("tomato"), Some(10), Some(20))).unwrap();
        }

        let f = ActionFilter { limit: Some(200), ..Default::default() };
        let got = list_actions(&db, &f).expect("list ok");
        assert_eq!(got.len(), 200);
        let abri_count = got.iter().filter(|a| a.kind == "semis_abri").count();
        assert_eq!(abri_count, 0, "limit=200 mange tous les semis_abri anciens — c'est attendu, le frontend doit utiliser un limit plus large");
    }

    /// Le tri par date DESC doit être stable et placer les plus récentes en tête.
    #[test]
    fn list_actions_orders_date_desc() {
        let db = mk_db();
        insert_action(&db, &mk_action("2026-01-15", "semis_abri", Some("a"), Some(0), Some(0))).unwrap();
        insert_action(&db, &mk_action("2026-05-01", "semis_abri", Some("b"), Some(0), Some(0))).unwrap();
        insert_action(&db, &mk_action("2026-03-10", "semis_abri", Some("c"), Some(0), Some(0))).unwrap();

        let got = list_actions(&db, &ActionFilter::default()).unwrap();
        let dates: Vec<&str> = got.iter().map(|a| a.date.as_str()).collect();
        assert_eq!(dates, vec!["2026-05-01", "2026-03-10", "2026-01-15"]);
    }

    /// Filtre par kind doit isoler exclusivement les semis_abri.
    #[test]
    fn list_actions_filter_by_kind() {
        let db = mk_db();
        insert_action(&db, &mk_action("2026-05-01", "arrosage", Some("a"), Some(0), Some(0))).unwrap();
        insert_action(&db, &mk_action("2026-05-01", "semis_abri", Some("b"), Some(10), Some(20))).unwrap();
        insert_action(&db, &mk_action("2026-05-01", "repiquage", Some("c"), Some(30), Some(40))).unwrap();

        let f = ActionFilter { kind: Some("semis_abri".into()), ..Default::default() };
        let got = list_actions(&db, &f).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].species_id.as_deref(), Some("b"));
    }

    /// Régression cross-zone : un repiquage venu d'un drag conserve grid_x/y
    /// (pour rendu sur le terrain) et n'écrase pas les autres actions.
    #[test]
    fn insert_repiquage_keeps_grid_coords() {
        let db = mk_db();
        let inserted = insert_action(
            &db,
            &mk_action("2026-05-09", "repiquage", Some("basil"), Some(231), Some(89)),
        )
        .unwrap();
        assert_eq!(inserted.grid_x, Some(231));
        assert_eq!(inserted.grid_y, Some(89));
        assert_eq!(inserted.kind, "repiquage");
    }

    #[test]
    fn insert_action_rejects_unknown_kind() {
        let db = mk_db();
        let err = insert_action(
            &db,
            &mk_action("2026-05-09", "kind_inconnu", Some("basil"), Some(10), Some(20)),
        );
        assert!(err.is_err(), "kind inconnu doit être rejeté");
    }

    #[test]
    fn delete_action_removes_only_target() {
        let db = mk_db();
        let a = insert_action(&db, &mk_action("2026-05-01", "semis_abri", Some("a"), Some(0), Some(0))).unwrap();
        let b = insert_action(&db, &mk_action("2026-05-02", "semis_abri", Some("b"), Some(0), Some(0))).unwrap();
        delete_action(&db, a.id).unwrap();
        let remaining = list_actions(&db, &ActionFilter::default()).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, b.id);
    }
}
