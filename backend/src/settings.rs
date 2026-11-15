//! Réglages du jardin (clé/valeur SQLite) : pour l'instant le type de sol.

use rusqlite::{params, OptionalExtension};

use crate::db::Db;
use crate::domain::soil::Soil;

const SOIL_KEY: &str = "soil";

pub fn migrate(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#,
    )
    .map_err(|e| format!("migration settings : {e}"))
}

/// Sol enregistré ; `Unknown` si jamais réglé (ou valeur obsolète en base).
pub fn soil(db: &Db) -> Result<Soil, String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?", params![SOIL_KEY], |row| row.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw.as_deref().and_then(Soil::parse).unwrap_or_default())
}

pub fn set_soil(db: &Db, soil: Soil) -> Result<(), String> {
    let conn = db.lock().expect("DB mutex poisoned");
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![SOIL_KEY, soil.as_str()],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn memory_db() -> Db {
        let conn = rusqlite::Connection::open_in_memory().expect("SQLite mémoire");
        migrate(&conn).expect("migration");
        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn soil_defaults_to_unknown_then_persists() {
        let db = memory_db();

        assert_eq!(soil(&db), Ok(Soil::Unknown));
        set_soil(&db, Soil::Clay).expect("écriture");
        set_soil(&db, Soil::Loam).expect("mise à jour");

        assert_eq!(soil(&db), Ok(Soil::Loam));
    }
}
