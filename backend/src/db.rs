//! Connexion SQLite + migration initiale.

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub type Db = Arc<Mutex<Connection>>;

pub fn open() -> Result<Db, String> {
    let candidates: Vec<PathBuf> = vec![
        PathBuf::from("data/hortus.db"),
        PathBuf::from("backend/data/hortus.db"),
    ];
    let path = candidates
        .into_iter()
        .find(|p| p.parent().map(|d| d.exists()).unwrap_or(false))
        .unwrap_or_else(|| PathBuf::from("data/hortus.db"));

    tracing::info!("DB : {}", path.display());
    let conn = Connection::open(&path).map_err(|e| format!("open DB : {e}"))?;
    conn.execute_batch(SCHEMA).map_err(|e| format!("migration : {e}"))?;
    run_additive_migrations(&conn)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("pragma : {e}"))?;
    Ok(Arc::new(Mutex::new(conn)))
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS parcel (
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

CREATE TABLE IF NOT EXISTS action (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    parcel_id INTEGER,
    species_id TEXT,
    kind TEXT NOT NULL,
    quantity_g REAL,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (parcel_id) REFERENCES parcel(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_action_date ON action(date DESC);
CREATE INDEX IF NOT EXISTS idx_action_parcel ON action(parcel_id);
CREATE INDEX IF NOT EXISTS idx_action_species ON action(species_id);
"#;

/// Migrations additives pour bases existantes.
/// `ALTER TABLE ADD COLUMN` avec fallback (SQLite ne supporte pas IF NOT EXISTS
/// sur les colonnes — on ignore les erreurs "duplicate column").
pub fn run_additive_migrations(conn: &rusqlite::Connection) -> Result<(), String> {
    let adds = [
        "ALTER TABLE parcel ADD COLUMN grid_x INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE parcel ADD COLUMN grid_y INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE parcel ADD COLUMN grid_w INTEGER NOT NULL DEFAULT 2",
        "ALTER TABLE parcel ADD COLUMN grid_h INTEGER NOT NULL DEFAULT 2",
        "ALTER TABLE parcel ADD COLUMN color TEXT NOT NULL DEFAULT '#8fbc4a'",
        "ALTER TABLE action ADD COLUMN grid_x INTEGER",
        "ALTER TABLE action ADD COLUMN grid_y INTEGER",
    ];
    for sql in &adds {
        if let Err(e) = conn.execute(sql, []) {
            let msg = e.to_string();
            if msg.contains("duplicate column") {
                continue;
            }
            return Err(format!("migration {sql} : {msg}"));
        }
    }
    Ok(())
}
