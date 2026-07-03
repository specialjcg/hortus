//! Fiches problèmes : suivi quasi scientifique des soucis au jardin.
//! Une fiche = espèce + catégorie + statut, avec entrées datées
//! (observation / traitement / résultat) et conclusion à la clôture.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::Db;

#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    pub id: i64,
    pub species_id: Option<String>,
    pub action_id: Option<i64>,
    pub title: String,
    pub category: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub created_at: String,
    pub entries: Vec<ProblemEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProblemEntry {
    pub id: i64,
    pub problem_id: i64,
    pub date: String,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct ProblemInput {
    pub species_id: Option<String>,
    pub action_id: Option<i64>,
    pub title: String,
    pub category: String,
}

#[derive(Debug, Deserialize)]
pub struct ProblemUpdate {
    pub title: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub conclusion: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EntryInput {
    pub date: String,
    pub kind: String,
    pub text: String,
}

pub const PROBLEM_CATEGORIES: &[&str] = &[
    "maladie",
    "ravageur",
    "carence",
    "climat",
    "croissance",
    "germination",
    "autre",
];

pub const ENTRY_KINDS: &[&str] = &["observation", "traitement", "resultat"];

pub const PROBLEM_STATUSES: &[&str] = &["open", "resolved"];

pub fn migrate(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS problem (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            species_id TEXT,
            action_id INTEGER,
            title TEXT NOT NULL,
            category TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'open',
            conclusion TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS problem_entry (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            problem_id INTEGER NOT NULL,
            date TEXT NOT NULL,
            kind TEXT NOT NULL,
            text TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (problem_id) REFERENCES problem(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_problem_entry_pb ON problem_entry(problem_id);
        "#,
    )
    .map_err(|e| format!("migration problems : {e}"))
}

pub fn list_problems(db: &Db) -> Result<Vec<Problem>, String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let mut stmt = conn
        .prepare("SELECT id, species_id, action_id, title, category, status, conclusion, created_at FROM problem ORDER BY status ASC, id DESC")
        .map_err(|e| format!("prep : {e}"))?;
    let mut problems: Vec<Problem> = stmt
        .query_map([], |r| {
            Ok(Problem {
                id: r.get(0)?,
                species_id: r.get(1)?,
                action_id: r.get(2)?,
                title: r.get(3)?,
                category: r.get(4)?,
                status: r.get(5)?,
                conclusion: r.get(6)?,
                created_at: r.get(7)?,
                entries: Vec::new(),
            })
        })
        .map_err(|e| format!("query : {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("collect : {e}"))?;

    let mut estmt = conn
        .prepare("SELECT id, problem_id, date, kind, text FROM problem_entry ORDER BY date ASC, id ASC")
        .map_err(|e| format!("prep entries : {e}"))?;
    let entries: Vec<ProblemEntry> = estmt
        .query_map([], |r| {
            Ok(ProblemEntry {
                id: r.get(0)?,
                problem_id: r.get(1)?,
                date: r.get(2)?,
                kind: r.get(3)?,
                text: r.get(4)?,
            })
        })
        .map_err(|e| format!("query entries : {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("collect entries : {e}"))?;

    for e in entries {
        if let Some(p) = problems.iter_mut().find(|p| p.id == e.problem_id) {
            p.entries.push(e);
        }
    }
    Ok(problems)
}

pub fn insert_problem(db: &Db, input: &ProblemInput) -> Result<i64, String> {
    if !PROBLEM_CATEGORIES.contains(&input.category.as_str()) {
        return Err(format!("catégorie invalide : {}", input.category));
    }
    if input.title.trim().is_empty() {
        return Err("titre vide".into());
    }
    let conn = db.lock().expect("DB mutex poisoned");
    conn.execute(
        "INSERT INTO problem (species_id, action_id, title, category) VALUES (?, ?, ?, ?)",
        params![input.species_id, input.action_id, input.title.trim(), input.category],
    )
    .map_err(|e| format!("insert : {e}"))?;
    Ok(conn.last_insert_rowid())
}

pub fn update_problem(db: &Db, id: i64, upd: &ProblemUpdate) -> Result<(), String> {
    if let Some(c) = &upd.category {
        if !PROBLEM_CATEGORIES.contains(&c.as_str()) {
            return Err(format!("catégorie invalide : {c}"));
        }
    }
    if let Some(s) = &upd.status {
        if !PROBLEM_STATUSES.contains(&s.as_str()) {
            return Err(format!("statut invalide : {s}"));
        }
    }
    let conn = db.lock().expect("DB mutex poisoned");
    let n = conn
        .execute(
            "UPDATE problem SET
                title = COALESCE(?, title),
                category = COALESCE(?, category),
                status = COALESCE(?, status),
                conclusion = COALESCE(?, conclusion)
             WHERE id = ?",
            params![upd.title, upd.category, upd.status, upd.conclusion, id],
        )
        .map_err(|e| format!("update : {e}"))?;
    if n == 0 {
        return Err("fiche inconnue".into());
    }
    Ok(())
}

pub fn delete_problem(db: &Db, id: i64) -> Result<(), String> {
    let conn = db.lock().expect("DB mutex poisoned");
    let n = conn
        .execute("DELETE FROM problem WHERE id = ?", params![id])
        .map_err(|e| format!("delete : {e}"))?;
    if n == 0 {
        return Err("fiche inconnue".into());
    }
    Ok(())
}

pub fn insert_entry(db: &Db, problem_id: i64, input: &EntryInput) -> Result<i64, String> {
    if !ENTRY_KINDS.contains(&input.kind.as_str()) {
        return Err(format!("type d'entrée invalide : {}", input.kind));
    }
    if input.text.trim().is_empty() {
        return Err("texte vide".into());
    }
    let conn = db.lock().expect("DB mutex poisoned");
    let exists: bool = conn
        .query_row("SELECT 1 FROM problem WHERE id = ?", params![problem_id], |_| Ok(true))
        .unwrap_or(false);
    if !exists {
        return Err("fiche inconnue".into());
    }
    conn.execute(
        "INSERT INTO problem_entry (problem_id, date, kind, text) VALUES (?, ?, ?, ?)",
        params![problem_id, input.date, input.kind, input.text.trim()],
    )
    .map_err(|e| format!("insert entrée : {e}"))?;
    Ok(conn.last_insert_rowid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn test_db() -> Db {
        let conn = Connection::open_in_memory().expect("mem db");
        migrate(&conn).expect("migrate");
        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn cycle_complet_fiche() {
        let db = test_db();
        let pid = insert_problem(
            &db,
            &ProblemInput {
                species_id: Some("tomato".into()),
                action_id: None,
                title: "Feuilles jaunes".into(),
                category: "maladie".into(),
            },
        )
        .expect("insert");

        insert_entry(&db, pid, &EntryInput {
            date: "2026-06-12".into(),
            kind: "observation".into(),
            text: "jaunissement bas du plant".into(),
        })
        .expect("obs");
        insert_entry(&db, pid, &EntryInput {
            date: "2026-06-19".into(),
            kind: "traitement".into(),
            text: "purin d'ortie 10%".into(),
        })
        .expect("ttt");

        update_problem(&db, pid, &ProblemUpdate {
            title: None,
            category: None,
            status: Some("resolved".into()),
            conclusion: Some("purin efficace en 1 semaine".into()),
        })
        .expect("close");

        let all = list_problems(&db).expect("list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, "resolved");
        assert_eq!(all[0].entries.len(), 2);
        assert_eq!(all[0].entries[0].kind, "observation");
    }

    #[test]
    fn rejette_categorie_et_kind_invalides() {
        let db = test_db();
        assert!(insert_problem(&db, &ProblemInput {
            species_id: None,
            action_id: None,
            title: "x".into(),
            category: "pas_une_cat".into(),
        })
        .is_err());

        let pid = insert_problem(&db, &ProblemInput {
            species_id: None,
            action_id: None,
            title: "x".into(),
            category: "autre".into(),
        })
        .expect("insert");
        assert!(insert_entry(&db, pid, &EntryInput {
            date: "2026-01-01".into(),
            kind: "invalide".into(),
            text: "y".into(),
        })
        .is_err());
    }

    #[test]
    fn cascade_suppression() {
        let db = test_db();
        {
            let conn = db.lock().expect("lock");
            conn.execute_batch("PRAGMA foreign_keys = ON;").expect("fk");
        }
        let pid = insert_problem(&db, &ProblemInput {
            species_id: None,
            action_id: None,
            title: "x".into(),
            category: "autre".into(),
        })
        .expect("insert");
        insert_entry(&db, pid, &EntryInput {
            date: "2026-01-01".into(),
            kind: "observation".into(),
            text: "y".into(),
        })
        .expect("entry");
        delete_problem(&db, pid).expect("delete");
        assert!(list_problems(&db).expect("list").is_empty());
    }
}
