//! Suivi des niveaux de restriction d'eau : mémorise le dernier niveau vu par
//! ville et type de ressource, pour signaler un changement jusqu'à ce qu'il soit lu.

use rusqlite::params;
use serde::Serialize;

use crate::db::Db;
use crate::domain::vigieau::Restriction;

/// Niveau stocké quand VigiEau ne renvoie aucune zone.
const NO_RESTRICTION: &str = "aucune";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LevelChange {
    pub kind: String,
    pub from: String,
    pub to: String,
    pub changed_at: String,
}

pub fn migrate(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS water_level (
    city TEXT NOT NULL,
    kind TEXT NOT NULL,
    level TEXT NOT NULL,
    previous TEXT,
    changed_at TEXT,
    seen INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (city, kind)
);
"#,
    )
    .map_err(|e| format!("migration water_level : {e}"))
}

/// Compare les niveaux reçus aux derniers vus. Première observation = référence,
/// sans changement signalé.
pub fn record_levels(db: &Db, city: &str, restrictions: &[Restriction]) -> Result<(), String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    for r in restrictions {
        let level = r.level.as_deref().unwrap_or(NO_RESTRICTION);
        let known: Option<String> = conn
            .query_row(
                "SELECT level FROM water_level WHERE city = ? AND kind = ?",
                params![city, r.kind],
                |row| row.get(0),
            )
            .ok();
        match known {
            None => {
                conn.execute(
                    "INSERT INTO water_level (city, kind, level) VALUES (?, ?, ?)",
                    params![city, r.kind, level],
                )
                .map_err(|e| e.to_string())?;
            }
            Some(old) if old != level => {
                conn.execute(
                    "UPDATE water_level SET level = ?, previous = ?, changed_at = ?, seen = 0 WHERE city = ? AND kind = ?",
                    params![level, old, now, city, r.kind],
                )
                .map_err(|e| e.to_string())?;
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Changements pas encore lus pour une ville.
pub fn pending_changes(db: &Db, city: &str) -> Result<Vec<LevelChange>, String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let mut stmt = conn
        .prepare("SELECT kind, previous, level, changed_at FROM water_level WHERE city = ? AND seen = 0 ORDER BY kind")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![city], |row| {
            Ok(LevelChange {
                kind: row.get(0)?,
                from: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                to: row.get(2)?,
                changed_at: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Marque les changements de la ville comme lus (bandeau fermé).
pub fn mark_seen(db: &Db, city: &str) -> Result<(), String> {
    let conn = db.lock().expect("DB mutex poisoned");
    conn.execute("UPDATE water_level SET seen = 1 WHERE city = ?", params![city])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn mem_db() -> Db {
        let conn = Connection::open_in_memory().expect("mem db");
        migrate(&conn).expect("migrate");
        Arc::new(Mutex::new(conn))
    }

    fn restriction(kind: &str, level: Option<&str>) -> Restriction {
        Restriction {
            kind: kind.into(),
            zone: level.map(|_| "ZONE 1".into()),
            level: level.map(Into::into),
            potager: None,
            valid_until: None,
            arrete_url: None,
        }
    }

    #[test]
    fn first_observation_is_not_a_change() {
        let db = mem_db();
        record_levels(&db, "x", &[restriction("SUP", Some("crise"))]).expect("record");
        assert!(pending_changes(&db, "x").expect("pending").is_empty());
    }

    #[test]
    fn level_change_is_pending_until_seen() {
        let db = mem_db();
        record_levels(&db, "x", &[restriction("SUP", Some("alerte"))]).expect("record");
        record_levels(&db, "x", &[restriction("SUP", Some("crise"))]).expect("record");
        let changes = pending_changes(&db, "x").expect("pending");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].from, "alerte");
        assert_eq!(changes[0].to, "crise");

        mark_seen(&db, "x").expect("seen");
        assert!(pending_changes(&db, "x").expect("pending").is_empty());
    }

    #[test]
    fn lifted_restriction_is_a_change() {
        let db = mem_db();
        record_levels(&db, "x", &[restriction("AEP", Some("crise"))]).expect("record");
        record_levels(&db, "x", &[restriction("AEP", None)]).expect("record");
        let changes = pending_changes(&db, "x").expect("pending");
        assert_eq!(changes[0].to, NO_RESTRICTION);
    }

    #[test]
    fn same_level_keeps_no_change() {
        let db = mem_db();
        record_levels(&db, "x", &[restriction("SUP", Some("crise"))]).expect("record");
        record_levels(&db, "x", &[restriction("SUP", Some("crise"))]).expect("record");
        assert!(pending_changes(&db, "x").expect("pending").is_empty());
    }

    #[test]
    fn cities_are_independent() {
        let db = mem_db();
        record_levels(&db, "a", &[restriction("SUP", Some("alerte"))]).expect("record");
        record_levels(&db, "a", &[restriction("SUP", Some("crise"))]).expect("record");
        assert!(pending_changes(&db, "b").expect("pending").is_empty());
    }
}
