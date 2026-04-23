//! Routes HTTP.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::api::snapshot::{snapshot, species_cards, SimSnapshot, SpeciesCard};
use crate::api::state::{fresh_id, AppState};
use crate::domain::garden::CellCoord;
use crate::domain::sim::{Simulation, TickReport};

pub fn router() -> Router {
    let state = AppState::new();
    Router::new()
        .route("/health", get(health))
        .route("/sim/new", post(create_sim))
        .route("/sim/{id}/state", get(get_state))
        .route("/sim/{id}/sow", post(sow))
        .route("/sim/{id}/advance", post(advance))
        .route("/sim/{id}/catalog", get(get_catalog))
        .route("/sim/{id}", axum::routing::delete(delete_sim))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn health() -> &'static str { "ok" }

#[derive(Debug, Deserialize, Default)]
pub struct NewSimReq {
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub plant_pilot_plan: bool,
}

fn default_seed() -> u64 { 42 }

async fn create_sim(
    State(state): State<AppState>,
    Json(req): Json<NewSimReq>,
) -> (StatusCode, Json<SimSnapshot>) {
    let mut sim = Simulation::new_default(req.seed);
    if req.plant_pilot_plan {
        let _ = sim.plant_established_tree(CellCoord::new(0, 0), "apple_tree", 5);
    }
    let id = fresh_id();
    let snap = snapshot(&sim, &id);
    state.sims.lock().unwrap().insert(id.clone(), sim);
    (StatusCode::CREATED, Json(snap))
}

async fn get_state(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SimSnapshot>, StatusCode> {
    let store = state.sims.lock().unwrap();
    let sim = store.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(snapshot(sim, &id)))
}

#[derive(Debug, Deserialize)]
pub struct SowReq {
    pub col: u16,
    pub row: u16,
    pub species_id: String,
}

#[derive(Debug, Serialize)]
pub struct SowResponse {
    pub ok: bool,
    pub message: String,
    pub state: SimSnapshot,
}

async fn sow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SowReq>,
) -> Result<Json<SowResponse>, (StatusCode, String)> {
    let mut store = state.sims.lock().unwrap();
    let sim = store
        .get_mut(&id)
        .ok_or((StatusCode::NOT_FOUND, format!("sim {id} introuvable")))?;
    match sim.sow(CellCoord::new(req.col, req.row), &req.species_id) {
        Ok(()) => Ok(Json(SowResponse {
            ok: true,
            message: format!("{} semé en ({},{})", req.species_id, req.col, req.row),
            state: snapshot(sim, &id),
        })),
        Err(e) => Err((StatusCode::BAD_REQUEST, e)),
    }
}

#[derive(Debug, Deserialize)]
pub struct AdvanceReq {
    pub days: u32,
}

#[derive(Debug, Serialize)]
pub struct AdvanceResponse {
    pub state: SimSnapshot,
    /// Récapitulatif compact des ticks joués (pour ne pas saturer la réponse).
    pub ticks: Vec<TickSummary>,
}

#[derive(Debug, Serialize)]
pub struct TickSummary {
    pub date: crate::domain::time::SimDate,
    pub harvests_g: f64,
    pub fully_covered: bool,
    pub n_events: usize,
}

impl From<&TickReport> for TickSummary {
    fn from(r: &TickReport) -> Self {
        Self {
            date: r.date,
            harvests_g: r.harvests_g,
            fully_covered: r.consumption.balance.fully_covered,
            n_events: r.events.len(),
        }
    }
}

async fn advance(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AdvanceReq>,
) -> Result<Json<AdvanceResponse>, (StatusCode, String)> {
    if req.days == 0 || req.days > 3650 {
        return Err((StatusCode::BAD_REQUEST, "days doit être 1..=3650".into()));
    }
    let mut store = state.sims.lock().unwrap();
    let sim = store
        .get_mut(&id)
        .ok_or((StatusCode::NOT_FOUND, format!("sim {id} introuvable")))?;
    let before = sim.history.len();
    for _ in 0..req.days { sim.tick(); }
    let ticks: Vec<TickSummary> = sim.history[before..]
        .iter()
        .map(TickSummary::from)
        .collect();
    Ok(Json(AdvanceResponse {
        state: snapshot(sim, &id),
        ticks,
    }))
}

async fn get_catalog(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<SpeciesCard>>, StatusCode> {
    let store = state.sims.lock().unwrap();
    let sim = store.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(species_cards(sim)))
}

async fn delete_sim(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state.sims.lock().unwrap().remove(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(StatusCode::NO_CONTENT)
}
