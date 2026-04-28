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
        "SELECT id, date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y, created_at FROM action WHERE 1=1",
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
                created_at: r.get(9)?,
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
        "INSERT INTO action (date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![input.date, input.parcel_id, input.species_id, input.kind, input.quantity_g, input.notes, input.grid_x, input.grid_y],
    )
    .map_err(|e| format!("insert : {e}"))?;
    let id = conn.last_insert_rowid();
    conn.query_row(
        "SELECT id, date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y, created_at FROM action WHERE id = ?",
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
                created_at: r.get(9)?,
            })
        },
    )
    .map_err(|e| format!("fetch : {e}"))
}

pub fn update_action(db: &Db, id: i64, input: &ActionInput) -> Result<Action, String> {
    if !ACTION_KINDS.contains(&input.kind.as_str()) {
        return Err(format!("type d'action invalide : {}", input.kind));
    }
    let conn = db.lock().expect("DB mutex poisoned");
    let n = conn
        .execute(
            "UPDATE action SET date = ?, parcel_id = ?, species_id = ?, kind = ?, quantity_g = ?, notes = ?, grid_x = ?, grid_y = ? WHERE id = ?",
            params![input.date, input.parcel_id, input.species_id, input.kind, input.quantity_g, input.notes, input.grid_x, input.grid_y, id],
        )
        .map_err(|e| format!("update : {e}"))?;
    if n == 0 {
        return Err("action inconnue".into());
    }
    conn.query_row(
        "SELECT id, date, parcel_id, species_id, kind, quantity_g, notes, grid_x, grid_y, created_at FROM action WHERE id = ?",
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
                created_at: r.get(9)?,
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
