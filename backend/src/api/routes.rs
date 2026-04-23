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
        .route("/sim/{id}/water", post(water))
        .route("/sim/{id}/mulch", post(mulch))
        .route("/sim/{id}/compost", post(compost))
        .route("/sim/{id}/uproot", post(uproot))
        .route("/sim/{id}/transform", post(transform))
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

// ---- actions joueur ----

#[derive(Debug, Deserialize)]
pub struct CellAction {
    pub col: u16,
    pub row: u16,
}

#[derive(Debug, Deserialize)]
pub struct WaterReq {
    pub col: u16,
    pub row: u16,
    pub mm: f64,
}

#[derive(Debug, Deserialize)]
pub struct CompostReq {
    pub col: u16,
    pub row: u16,
    pub kg_per_m2: f64,
}

#[derive(Debug, Deserialize)]
pub struct TransformReq {
    pub species_id: String,
    pub from: String,
    pub to: String,
    pub mass_g: f64,
}

#[derive(Debug, Serialize)]
pub struct ActionResponse {
    pub ok: bool,
    pub message: String,
    pub state: SimSnapshot,
}

fn with_sim_mut<F, T>(
    state: &AppState,
    id: &str,
    f: F,
) -> Result<(T, SimSnapshot), (StatusCode, String)>
where
    F: FnOnce(&mut Simulation) -> Result<T, String>,
{
    let mut store = state.sims.lock().unwrap();
    let sim = store
        .get_mut(id)
        .ok_or((StatusCode::NOT_FOUND, format!("sim {id} introuvable")))?;
    let result = f(sim).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let snap = snapshot(sim, id);
    Ok((result, snap))
}

async fn water(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<WaterReq>,
) -> Result<Json<ActionResponse>, (StatusCode, String)> {
    let (runoff, snap) = with_sim_mut(&state, &id, |sim| {
        sim.water(CellCoord::new(req.col, req.row), req.mm)
    })?;
    Ok(Json(ActionResponse {
        ok: true,
        message: format!(
            "{} mm arrosés en ({},{}), runoff {:.1} mm",
            req.mm, req.col, req.row, runoff
        ),
        state: snap,
    }))
}

async fn mulch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CellAction>,
) -> Result<Json<ActionResponse>, (StatusCode, String)> {
    let (_, snap) = with_sim_mut(&state, &id, |sim| {
        sim.mulch(CellCoord::new(req.col, req.row))
    })?;
    Ok(Json(ActionResponse {
        ok: true,
        message: format!("paillage en ({},{})", req.col, req.row),
        state: snap,
    }))
}

async fn compost(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CompostReq>,
) -> Result<Json<ActionResponse>, (StatusCode, String)> {
    let (_, snap) = with_sim_mut(&state, &id, |sim| {
        sim.add_compost(CellCoord::new(req.col, req.row), req.kg_per_m2)
    })?;
    Ok(Json(ActionResponse {
        ok: true,
        message: format!(
            "compost {} kg/m² en ({},{})",
            req.kg_per_m2, req.col, req.row
        ),
        state: snap,
    }))
}

async fn uproot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CellAction>,
) -> Result<Json<ActionResponse>, (StatusCode, String)> {
    let (mass, snap) = with_sim_mut(&state, &id, |sim| {
        sim.uproot(CellCoord::new(req.col, req.row))
    })?;
    Ok(Json(ActionResponse {
        ok: true,
        message: format!("plante arrachée en ({},{}) — {:.0} g de biomasse", req.col, req.row, mass),
        state: snap,
    }))
}

async fn transform(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<TransformReq>,
) -> Result<Json<ActionResponse>, (StatusCode, String)> {
    use crate::domain::pantry::StorageCompartment;
    let from = parse_compartment(&req.from)
        .ok_or((StatusCode::BAD_REQUEST, format!("compartiment inconnu : {}", req.from)))?;
    let to = parse_compartment(&req.to)
        .ok_or((StatusCode::BAD_REQUEST, format!("compartiment inconnu : {}", req.to)))?;
    let species_id = req.species_id.clone();
    let mass = req.mass_g;
    let (kept, snap) = with_sim_mut(&state, &id, move |sim| {
        sim.transform(&species_id, from, to, mass)
    })?;
    Ok(Json(ActionResponse {
        ok: true,
        message: format!(
            "transformé {:.0} g {} → {} ({:.0} g conservés)",
            req.mass_g, req.from, req.to, kept
        ),
        state: snap,
    }))
}

fn parse_compartment(s: &str) -> Option<crate::domain::pantry::StorageCompartment> {
    use crate::domain::pantry::StorageCompartment as C;
    match s.to_ascii_lowercase().as_str() {
        "fresh" | "frais" => Some(C::Fresh),
        "cellar" | "cellier" => Some(C::Cellar),
        "dry" | "sec" => Some(C::Dry),
        "frozen" | "congel" | "congelé" => Some(C::Frozen),
        "canned" | "conserve" => Some(C::Canned),
        "lacto" | "lactofermenté" => Some(C::Lacto),
        _ => None,
    }
}
