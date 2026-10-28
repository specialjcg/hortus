//! Routes HTTP — calendrier des plantations (stateless).

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::db::{self, Db};
use crate::domain::geo::{ClimateProfile, Location};
use crate::domain::geometry::{self, Polygon};
use crate::domain::species::{CalendarWindow, Species};
use crate::journal::{
    self, Action, ActionFilter, ActionInput, Parcel, ParcelInput, ACTION_KINDS,
};
use crate::placement;
use crate::planner;
use crate::problems::{self, EntryInput, Problem, ProblemInput, ProblemUpdate};
use crate::domain::vigieau::{self, Restriction};
use crate::restrictions::{self, LevelChange};

/// État partagé.
#[derive(Clone)]
pub struct AppState {
    pub species: Arc<Vec<Species>>,
    pub db: Db,
    /// Cache VigiEau par ville : (instant du fetch, restrictions).
    pub water_cache: Arc<Mutex<HashMap<String, (Instant, Vec<Restriction>)>>>,
    pub plan_job: Arc<Mutex<Option<PlanJob>>>,
}

/// Durée de vie du cache VigiEau (les arrêtés changent au plus quelques fois par semaine).
const WATER_CACHE_TTL: Duration = Duration::from_secs(6 * 3600);

/// Répertoire des photos du journal (jamais touché par le déploiement,
/// comme la DB — il vit à côté dans data/).
const PHOTOS_DIR: &str = "data/photos";

/// Associe une photo (corps brut jpeg/png/webp) à une action. Remplace
/// l'ancienne photo le cas échéant.
async fn upload_action_photo(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let ext = match headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
    {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        other => {
            return Err((
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                format!("type non supporté : {other} (jpeg/png/webp)"),
            ))
        }
    };
    if body.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "corps vide".into()));
    }
    // L'ancienne photo est remplacée (nom horodaté = cache-bust naturel).
    let old = journal::get_action_photo(&s.db, id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    std::fs::create_dir_all(PHOTOS_DIR)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("mkdir : {e}")))?;
    let name = format!("{}-{}.{}", id, chrono::Local::now().format("%Y%m%d%H%M%S"), ext);
    std::fs::write(std::path::Path::new(PHOTOS_DIR).join(&name), &body)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("write : {e}")))?;
    journal::set_action_photo(&s.db, id, Some(&name)).map_err(|e| {
        let _ = std::fs::remove_file(std::path::Path::new(PHOTOS_DIR).join(&name));
        if e == "action inconnue" {
            (StatusCode::NOT_FOUND, e)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        }
    })?;
    if let Some(old_name) = old {
        let _ = std::fs::remove_file(std::path::Path::new(PHOTOS_DIR).join(old_name));
    }
    Ok(Json(serde_json::json!({ "photo_path": name })))
}

/// Sert une photo du journal. Nom strictement alphanumérique + [._-] :
/// pas de traversée de répertoire possible.
async fn serve_photo(Path(name): Path<String>) -> impl IntoResponse {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return (StatusCode::BAD_REQUEST, "nom invalide").into_response();
    }
    let content_type = if name.ends_with(".png") {
        "image/png"
    } else if name.ends_with(".webp") {
        "image/webp"
    } else {
        "image/jpeg"
    };
    match tokio::fs::read(std::path::Path::new(PHOTOS_DIR).join(&name)).await {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, content_type.to_string()),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "photo introuvable").into_response(),
    }
}

pub fn router() -> Router {
    let candidates = [
        "data/species.json",
        "backend/data/species.json",
        "../data/species.json",
        "../../data/species.json",
    ];
    let species = candidates
        .iter()
        .find_map(|path| {
            Species::load_from_json(path).ok().map(|s| {
                tracing::info!("catalogue chargé depuis {} ({} espèces)", path, s.len());
                s
            })
        })
        .unwrap_or_else(|| {
            eprintln!("[warn] data/species.json introuvable — catalogue vide");
            Vec::new()
        });
    let db = db::open().unwrap_or_else(|e| {
        eprintln!("[fatal] DB non ouverte : {e}");
        std::process::exit(1);
    });
    let state = AppState {
        species: Arc::new(species),
        db,
        water_cache: Arc::new(Mutex::new(HashMap::new())),
        plan_job: Arc::new(Mutex::new(None)),
    };

    Router::new()
        .route("/health", get(health))
        .route("/species", get(list_species))
        .route("/cities", get(list_cities))
        .route("/calendar", get(calendar))
        .route("/forecast", get(forecast))
        .route("/water-restrictions", get(water_restrictions))
        .route("/water-restrictions/seen", post(water_restrictions_seen))
        .route("/historical-year", get(historical_year))
        .route("/action-kinds", get(list_action_kinds))
        .route("/parcels", get(get_parcels).post(create_parcel))
        .route("/parcels/propose", post(propose_beds))
        .route("/parcels/{id}", put(put_parcel).delete(del_parcel))
        .route("/actions", get(get_actions).post(create_action).delete(del_all_actions))
        .route("/actions/bulk", post(create_actions_bulk))
        .route("/actions/plan/{year}", delete(del_plan_actions))
        .route("/actions/{id}", put(put_action).delete(del_action))
        .route("/actions/{id}/photo", post(upload_action_photo))
        .route("/photos/{name}", get(serve_photo))
        .route("/plan", get(get_plan))
        .route("/plan/jobs", post(start_plan_job))
        .route("/plan/jobs/{id}", get(get_plan_job))
        .route("/problems", get(get_problems).post(create_problem))
        .route("/problems/{id}", put(put_problem).delete(del_problem))
        .route("/problems/{id}/entries", post(create_problem_entry))
        .with_state(state)
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn list_action_kinds() -> Json<Vec<&'static str>> {
    Json(ACTION_KINDS.to_vec())
}

#[derive(Debug, Serialize)]
pub struct HistoricalDay {
    pub doy: u16,
    pub temp_min_c: f64,
    pub temp_max_c: f64,
    pub precipitation_mm: f64,
    pub samples: u16,
}

async fn historical_year(Query(q): Query<ForecastQuery>) -> Json<Vec<HistoricalDay>> {
    use crate::domain::weather_live;
    use std::collections::HashMap;

    let slug = q.city.as_deref().unwrap_or("le_bois_doingt");
    let loc = Location::by_slug(slug);
    let today = chrono::Local::now().date_naive();
    let years_back = 5;
    let start = match chrono::NaiveDate::from_ymd_opt(today.year() - years_back, 1, 1) {
        Some(d) => d,
        None => return Json(Vec::new()),
    };
    let end = today - chrono::Duration::days(1);
    let map =
        weather_live::fetch_live_weather(loc.latitude_deg, loc.longitude_deg, start, end).await;

    // Agréger par DOY (1..365)
    let mut by_doy: HashMap<u16, (f64, f64, f64, u16)> = HashMap::new();
    for w in map.into_values() {
        let doy = w.date.day_of_year.min(365);
        let entry = by_doy.entry(doy).or_insert((0.0, 0.0, 0.0, 0));
        entry.0 += w.temp_min_c;
        entry.1 += w.temp_max_c;
        entry.2 += w.precipitation_mm;
        entry.3 += 1;
    }

    let mut out: Vec<HistoricalDay> = Vec::new();
    for doy in 1u16..=365 {
        if let Some(&(sum_min, sum_max, sum_pr, n)) = by_doy.get(&doy) {
            if n > 0 {
                let nf = n as f64;
                out.push(HistoricalDay {
                    doy,
                    temp_min_c: sum_min / nf,
                    temp_max_c: sum_max / nf,
                    precipitation_mm: sum_pr / nf,
                    samples: n,
                });
            }
        }
    }
    Json(out)
}

#[derive(Debug, Deserialize)]
pub struct ForecastQuery {
    pub city: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ForecastDay {
    pub date: String,
    pub temp_min_c: f64,
    pub temp_max_c: f64,
    pub precipitation_mm: f64,
    pub wind_kmh: f64,
    pub kind: String,
}

async fn forecast(Query(q): Query<ForecastQuery>) -> Json<Vec<ForecastDay>> {
    use crate::domain::weather_live;
    let slug = q.city.as_deref().unwrap_or("le_bois_doingt");
    let loc = Location::by_slug(slug);
    let today = chrono::Local::now().date_naive();
    let end = today + chrono::Duration::days(7);
    let map = weather_live::fetch_live_weather(loc.latitude_deg, loc.longitude_deg, today, end).await;
    let mut out: Vec<ForecastDay> = map
        .into_values()
        .filter_map(|w| {
            let nd = chrono::NaiveDate::from_yo_opt(w.date.year as i32, w.date.day_of_year as u32)?;
            Some(ForecastDay {
                date: nd.format("%Y-%m-%d").to_string(),
                temp_min_c: w.temp_min_c,
                temp_max_c: w.temp_max_c,
                precipitation_mm: w.precipitation_mm,
                wind_kmh: w.wind_kmh,
                kind: format!("{:?}", w.kind),
            })
        })
        .collect();
    out.sort_by(|a, b| a.date.cmp(&b.date));
    Json(out)
}

#[derive(Debug, Serialize)]
pub struct WaterRestrictions {
    /// false = VigiEau injoignable et rien en cache.
    pub available: bool,
    pub resources: Vec<Restriction>,
    /// Changements de niveau pas encore lus (bandeau).
    pub changes: Vec<LevelChange>,
}

/// Restrictions d'eau VigiEau à la position de la ville. Cache 6 h ; le suivi
/// des niveaux n'est mis à jour qu'à chaque fetch frais.
async fn water_restrictions(
    State(s): State<AppState>,
    Query(q): Query<ForecastQuery>,
) -> Result<Json<WaterRestrictions>, (StatusCode, String)> {
    let slug = q.city.as_deref().unwrap_or("le_bois_doingt").to_string();
    let cached = s
        .water_cache
        .lock()
        .expect("cache mutex poisoned")
        .get(&slug)
        .filter(|(at, _)| at.elapsed() < WATER_CACHE_TTL)
        .map(|(_, r)| r.clone());
    let resources = match cached {
        Some(r) => Some(r),
        None => {
            let loc = Location::by_slug(&slug);
            match vigieau::fetch_restrictions(loc.latitude_deg, loc.longitude_deg).await {
                Ok(r) => {
                    restrictions::record_levels(&s.db, &slug, &r)
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
                    s.water_cache
                        .lock()
                        .expect("cache mutex poisoned")
                        .insert(slug.clone(), (Instant::now(), r.clone()));
                    Some(r)
                }
                Err(e) => {
                    tracing::warn!("{e}");
                    // Cache périmé plutôt que rien.
                    s.water_cache
                        .lock()
                        .expect("cache mutex poisoned")
                        .get(&slug)
                        .map(|(_, r)| r.clone())
                }
            }
        }
    };
    let changes = restrictions::pending_changes(&s.db, &slug)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(WaterRestrictions {
        available: resources.is_some(),
        resources: resources.unwrap_or_default(),
        changes,
    }))
}

/// Ferme le bandeau de changement de niveau.
async fn water_restrictions_seen(
    State(s): State<AppState>,
    Query(q): Query<ForecastQuery>,
) -> Result<StatusCode, (StatusCode, String)> {
    let slug = q.city.as_deref().unwrap_or("le_bois_doingt");
    restrictions::mark_seen(&s.db, slug).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_parcels(State(s): State<AppState>) -> Result<Json<Vec<Parcel>>, (StatusCode, String)> {
    journal::list_parcels(&s.db)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn create_parcel(
    State(s): State<AppState>,
    Json(input): Json<ParcelInput>,
) -> Result<(StatusCode, Json<Parcel>), (StatusCode, String)> {
    if input.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "name obligatoire".into()));
    }
    journal::insert_parcel(&s.db, &input)
        .map(|p| (StatusCode::CREATED, Json(p)))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Debug, Deserialize)]
pub struct ProposeInput {
    /// Zone cultivable dessinée (validée à la désérialisation).
    pub zone: Polygon,
    pub bed_cm: Option<i64>,
    pub path_cm: Option<i64>,
}

/// Propose des planches en bandes dans la zone, sans les enregistrer.
async fn propose_beds(
    State(s): State<AppState>,
    Json(input): Json<ProposeInput>,
) -> Result<Json<Vec<Polygon>>, (StatusCode, String)> {
    let bed_cm = input.bed_cm.unwrap_or(120);
    let path_cm = input.path_cm.unwrap_or(40);
    if !(60..=300).contains(&bed_cm) || !(20..=150).contains(&path_cm) {
        return Err((StatusCode::BAD_REQUEST, "largeur 60..=300 cm, allée 20..=150 cm".into()));
    }
    let taken: Vec<Polygon> = journal::list_parcels(&s.db)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .into_iter()
        .filter_map(|p| p.shape)
        .collect();
    Ok(Json(geometry::strip_beds(&input.zone, &taken, bed_cm, path_cm)))
}

async fn put_parcel(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<ParcelInput>,
) -> Result<Json<Parcel>, (StatusCode, String)> {
    journal::update_parcel(&s.db, id, &input)
        .map(Json)
        .map_err(|e| {
            if e == "parcelle inconnue" {
                (StatusCode::NOT_FOUND, e)
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, e)
            }
        })
}

async fn del_parcel(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    journal::delete_parcel(&s.db, id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            if e == "parcelle inconnue" {
                (StatusCode::NOT_FOUND, e)
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, e)
            }
        })
}

async fn get_actions(
    State(s): State<AppState>,
    Query(f): Query<ActionFilter>,
) -> Result<Json<Vec<Action>>, (StatusCode, String)> {
    journal::list_actions(&s.db, &f)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn create_action(
    State(s): State<AppState>,
    Json(input): Json<ActionInput>,
) -> Result<(StatusCode, Json<Action>), (StatusCode, String)> {
    if let Some(pid) = input.parcel_id {
        match journal::parcel_exists(&s.db, pid) {
            Ok(false) => return Err((StatusCode::BAD_REQUEST, format!("parcel_id {pid} inconnu"))),
            Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
            _ => {}
        }
    }
    journal::insert_action(&s.db, &input)
        .map(|a| (StatusCode::CREATED, Json(a)))
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn create_actions_bulk(
    State(s): State<AppState>,
    Json(inputs): Json<Vec<ActionInput>>,
) -> Result<(StatusCode, Json<Vec<Action>>), (StatusCode, String)> {
    let results = journal::insert_actions_bulk(&s.db, &inputs)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok((StatusCode::CREATED, Json(results)))
}

async fn get_problems(
    State(s): State<AppState>,
) -> Result<Json<Vec<Problem>>, (StatusCode, String)> {
    problems::list_problems(&s.db)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn create_problem(
    State(s): State<AppState>,
    Json(input): Json<ProblemInput>,
) -> Result<(StatusCode, Json<i64>), (StatusCode, String)> {
    problems::insert_problem(&s.db, &input)
        .map(|id| (StatusCode::CREATED, Json(id)))
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn put_problem(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(upd): Json<ProblemUpdate>,
) -> Result<StatusCode, (StatusCode, String)> {
    problems::update_problem(&s.db, id, &upd)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn del_problem(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    problems::delete_problem(&s.db, id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn create_problem_entry(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<EntryInput>,
) -> Result<(StatusCode, Json<i64>), (StatusCode, String)> {
    problems::insert_entry(&s.db, id, &input)
        .map(|eid| (StatusCode::CREATED, Json(eid)))
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn put_action(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<ActionInput>,
) -> Result<Json<Action>, (StatusCode, String)> {
    journal::update_action(&s.db, id, &input)
        .map(Json)
        .map_err(|e| {
            if e == "action inconnue" {
                (StatusCode::NOT_FOUND, e)
            } else {
                (StatusCode::BAD_REQUEST, e)
            }
        })
}

async fn del_action(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    // La photo associée part avec l'action.
    if let Ok(Some(name)) = journal::get_action_photo(&s.db, id) {
        let _ = std::fs::remove_file(std::path::Path::new(PHOTOS_DIR).join(name));
    }
    journal::delete_action(&s.db, id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            if e == "action inconnue" {
                (StatusCode::NOT_FOUND, e)
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, e)
            }
        })
}

async fn del_all_actions(State(s): State<AppState>) -> Result<StatusCode, (StatusCode, String)> {
    journal::delete_all_actions(&s.db)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn del_plan_actions(
    State(s): State<AppState>,
    Path(year): Path<i32>,
) -> Result<Json<usize>, (StatusCode, String)> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    journal::delete_plan_actions(&s.db, year, &today)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn health() -> &'static str {
    "ok"
}

async fn list_species(State(state): State<AppState>) -> Json<Vec<Species>> {
    Json((*state.species).clone())
}

#[derive(Debug, Serialize)]
pub struct CityDto {
    pub slug: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

async fn list_cities() -> Json<Vec<CityDto>> {
    Json(
        Location::available()
            .iter()
            .map(|(slug, name)| {
                let loc = Location::by_slug(slug);
                CityDto {
                    slug: (*slug).into(),
                    name: (*name).into(),
                    latitude: loc.latitude_deg,
                    longitude: loc.longitude_deg,
                }
            })
            .collect(),
    )
}

#[derive(Debug, Deserialize)]
pub struct CalendarQuery {
    /// Slug de ville (défaut : le_bois_doingt).
    pub city: Option<String>,
    /// Si true, tente d'actualiser le climat via Open-Meteo (5 dernières années).
    #[serde(default)]
    pub refresh_climate: bool,
}

#[derive(Debug, Serialize)]
pub struct CalendarResponse {
    pub location: LocationDto,
    pub climate: ClimateProfile,
    /// Origine du profil climat : "defaults" (statique) ou "open_meteo_5y".
    pub climate_source: String,
    pub species: Vec<SpeciesLocal>,
}

#[derive(Debug, Serialize)]
pub struct LocationDto {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: f64,
}

/// Espèce avec fenêtres ajustées au climat local.
#[derive(Debug, Serialize)]
pub struct SpeciesLocal {
    #[serde(flatten)]
    pub species: Species,
    /// Décalage appliqué (jours) entre la fenêtre nominale et la fenêtre locale.
    /// Positif = plus tard (climat plus froid), négatif = plus tôt (plus chaud).
    pub shift_days: i16,
    pub indoor_sow_local: Option<CalendarWindow>,
    pub direct_sow_local: Option<CalendarWindow>,
    pub transplant_local: Option<CalendarWindow>,
    pub harvest_local: CalendarWindow,
}

async fn calendar(
    State(state): State<AppState>,
    Query(q): Query<CalendarQuery>,
) -> Result<Json<CalendarResponse>, (StatusCode, String)> {
    let city_slug = q.city.as_deref().unwrap_or("le_bois_doingt");
    let loc = Location::by_slug(city_slug);

    // Profil climatique : par défaut statique, sinon calculé depuis Open-Meteo 5y.
    let (climate, climate_source) = if q.refresh_climate {
        match fetch_5y_climate(loc.latitude_deg, loc.longitude_deg).await {
            Some(c) => (c, "open_meteo_5y".to_string()),
            None => (loc.climate.clone(), "defaults (open-meteo indisponible)".to_string()),
        }
    } else {
        (loc.climate.clone(), "defaults".to_string())
    };

    // Référentiel : Paris. Les fenêtres nominales des espèces sont exprimées
    // pour ce climat-là ; on calcule un décalage pour la ville cible.
    let reference = ClimateProfile::paris();
    let shift = spring_shift_days(&reference, &climate);

    let species_local: Vec<SpeciesLocal> = state
        .species
        .iter()
        .map(|s| {
            SpeciesLocal {
                indoor_sow_local: s.indoor_sow.map(|w| shift_window(w, shift)),
                direct_sow_local: s.direct_sow.map(|w| shift_window(w, shift)),
                transplant_local: s.transplant.map(|w| shift_window(w, shift)),
                harvest_local: shift_window(s.harvest, shift),
                species: s.clone(),
                shift_days: shift,
            }
        })
        .collect();

    Ok(Json(CalendarResponse {
        location: LocationDto {
            name: loc.name.clone(),
            latitude: loc.latitude_deg,
            longitude: loc.longitude_deg,
            altitude_m: loc.altitude_m,
        },
        climate,
        climate_source,
        species: species_local,
    }))
}

#[derive(Debug, Deserialize)]
pub struct PlanQuery {
    pub city: Option<String>,
    pub year: Option<i32>,
    /// Part max de la surface par espèce (0.05..=1, défaut 0.25).
    pub share: Option<f64>,
    /// Envies « espece:m2,espece:m2 », ex. `tomato:2,basil:0.5`.
    pub wish: Option<String>,
    /// Espèces exclues « beet,leek » : jamais proposées par le plan.
    pub exclude: Option<String>,
    /// Taille du foyer (1..=12) : plafonne chaque espèce à sa consommation.
    pub persons: Option<u32>,
}

/// Parse `tomato:2,basil:0.5` → [(id, m²)], ids vérifiés dans le catalogue.
fn parse_wishes(raw: &str, species: &[Species]) -> Result<Vec<(String, f64)>, String> {
    raw.split(',')
        .filter(|w| !w.trim().is_empty())
        .map(|w| {
            let (id, m2) = w.trim().split_once(':').ok_or(format!("envie mal formée : {w}"))?;
            let m2: f64 = m2.parse().map_err(|_| format!("surface invalide : {w}"))?;
            if !(m2 > 0.0 && m2 <= 1000.0) {
                return Err(format!("surface hors bornes : {w}"));
            }
            if !species.iter().any(|s| s.id == id) {
                return Err(format!("espèce inconnue : {id}"));
            }
            Ok((id.to_string(), m2))
        })
        .collect()
}

/// Parse `beet,leek` → ids vérifiés dans le catalogue.
fn parse_excludes(raw: &str, species: &[Species]) -> Result<Vec<String>, String> {
    raw.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| {
            if species.iter().any(|s| s.id == id) {
                Ok(id.to_string())
            } else {
                Err(format!("espèce inconnue : {id}"))
            }
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct PlanResponse {
    pub year: i32,
    #[serde(flatten)]
    pub plan: planner::Plan,
    /// Positions des plants (tirage de Poisson), pour l'aperçu sur le terrain.
    pub placements: Vec<placement::Placement>,
}

/// Entrées validées d'un calcul de plan, prêtes à partir sur un thread bloquant.
struct PlanInput {
    year: i32,
    share: f64,
    wishes: Vec<(String, f64)>,
    excludes: Vec<String>,
    persons: Option<u32>,
    shift: i16,
    parcels: Vec<Parcel>,
    actions: Vec<Action>,
    species: Arc<Vec<Species>>,
}

type HttpError = (StatusCode, String);

fn plan_input(s: &AppState, q: &PlanQuery) -> Result<PlanInput, HttpError> {
    let year = q.year.unwrap_or_else(|| chrono::Local::now().year() + 1);
    let share = q.share.unwrap_or(0.25);
    if !(0.05..=1.0).contains(&share) {
        return Err((StatusCode::BAD_REQUEST, "share hors de 0.05..=1".into()));
    }
    let persons = q.persons;
    if persons.is_some_and(|n| !(1..=12).contains(&n)) {
        return Err((StatusCode::BAD_REQUEST, "persons hors de 1..=12".into()));
    }
    let wishes = parse_wishes(q.wish.as_deref().unwrap_or(""), &s.species)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let excludes = parse_excludes(q.exclude.as_deref().unwrap_or(""), &s.species)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    if let Some((id, _)) = wishes.iter().find(|(id, _)| excludes.contains(id)) {
        return Err((StatusCode::BAD_REQUEST, format!("espèce à la fois voulue et exclue : {id}")));
    }
    let loc = Location::by_slug(q.city.as_deref().unwrap_or("le_bois_doingt"));
    let shift = spring_shift_days(&ClimateProfile::paris(), &loc.climate);
    let parcels = journal::list_parcels(&s.db).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let actions = journal::list_actions(&s.db, &ActionFilter::default())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(PlanInput { year, share, wishes, excludes, persons, shift, parcels, actions, species: s.species.clone() })
}

/// MILP + placement des plants : calcul bloquant, à lancer hors du runtime async.
fn compute_plan(input: PlanInput, time_limit_s: f64) -> Result<PlanResponse, HttpError> {
    let PlanInput { year, share, wishes, excludes, persons, shift, parcels, actions, species } = input;
    let beds = planner::beds_from_parcels(&parcels, &actions, &species);
    let history = planner::history_from_actions(&parcels, &actions, &species);
    let crops: Vec<planner::Crop> = species
        .iter()
        .filter(|sp| !sp.is_perennial() && (sp.yield_kg_m2.is_some() || sp.green_manure))
        .filter(|sp| !excludes.contains(&sp.id))
        .map(|sp| planner::Crop {
            species: sp,
            direct: sp.direct_sow.map(|w| shift_window(w, shift)),
            transplant: sp.transplant.map(|w| shift_window(w, shift)),
            harvest: shift_window(sp.harvest, shift),
        })
        .collect();
    let params = planner::PlanParams { year, max_species_share: share, time_limit_s, wishes, persons };
    let plan = planner::plan(&beds, &crops, &history, &params).map_err(|e| match e {
        planner::PlanError::NoBeds => (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
        planner::PlanError::Solver(_) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;
    let shapes: Vec<(i64, &Polygon)> = parcels.iter().filter_map(|p| Some((p.id, p.shape.as_ref()?))).collect();
    let placements = placement::place(&plan.items, &shapes, &species, year as u64);
    Ok(PlanResponse { year, plan, placements })
}

/// Plan de saison optimisé sur les planches dessinées (MILP HiGHS), en synchrone.
async fn get_plan(
    State(s): State<AppState>,
    Query(q): Query<PlanQuery>,
) -> Result<Json<PlanResponse>, HttpError> {
    let input = plan_input(&s, &q)?;
    tokio::task::spawn_blocking(move || compute_plan(input, 20.0))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("tâche plan : {e}")))?
        .map(Json)
}

/// Le calcul en tâche de fond échappe au délai du proxy : le solveur peut chercher plus longtemps.
const PLAN_JOB_TIME_LIMIT_S: f64 = 60.0;

/// Calcul de plan en tâche de fond. Un seul à la fois (le solveur occupe un cœur) ;
/// le dernier résultat reste lisible jusqu'au calcul suivant.
pub struct PlanJob {
    id: u64,
    /// Paramètres de la requête : relancer le même calcul rejoint celui en cours.
    key: String,
    started: Instant,
    /// `None` tant que le calcul tourne ; sinon durée et plan (JSON) ou erreur.
    outcome: Option<(Duration, Result<serde_json::Value, HttpError>)>,
}

/// Lance un calcul de plan ; le client interroge ensuite `GET /plan/jobs/{id}`.
async fn start_plan_job(
    State(s): State<AppState>,
    Query(q): Query<PlanQuery>,
) -> Result<(StatusCode, Json<serde_json::Value>), HttpError> {
    let key = format!("{q:?}");
    let input = plan_input(&s, &q)?;
    let id = {
        let mut slot = s.plan_job.lock().expect("plan_job mutex poisoned");
        match slot.as_ref() {
            Some(job) if job.outcome.is_none() && job.key == key => {
                return Ok((StatusCode::ACCEPTED, Json(serde_json::json!({ "id": job.id }))));
            }
            Some(job) if job.outcome.is_none() => {
                return Err((StatusCode::CONFLICT, "un calcul de plan est déjà en cours".into()));
            }
            _ => {}
        }
        let id = slot.as_ref().map_or(1, |job| job.id + 1);
        *slot = Some(PlanJob { id, key, started: Instant::now(), outcome: None });
        id
    };
    let jobs = s.plan_job.clone();
    tokio::spawn(async move {
        let started = Instant::now();
        let result = tokio::task::spawn_blocking(move || compute_plan(input, PLAN_JOB_TIME_LIMIT_S))
            .await
            .unwrap_or_else(|e| Err((StatusCode::INTERNAL_SERVER_ERROR, format!("tâche plan : {e}"))))
            .and_then(|r| serde_json::to_value(r).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())));
        let mut slot = jobs.lock().expect("plan_job mutex poisoned");
        if let Some(job) = slot.as_mut().filter(|job| job.id == id) {
            job.outcome = Some((started.elapsed(), result));
        }
    });
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({ "id": id }))))
}

/// État d'un calcul : `running` (+ secondes écoulées), `done` (+ plan) ou `error`.
async fn get_plan_job(
    State(s): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let slot = s.plan_job.lock().expect("plan_job mutex poisoned");
    let job = slot
        .as_ref()
        .filter(|job| job.id == id)
        .ok_or((StatusCode::NOT_FOUND, "calcul inconnu (remplacé ou serveur redémarré)".into()))?;
    Ok(Json(match &job.outcome {
        None => serde_json::json!({ "status": "running", "elapsed_s": job.started.elapsed().as_secs_f64() }),
        Some((took, Ok(plan))) => serde_json::json!({ "status": "done", "elapsed_s": took.as_secs_f64(), "result": plan }),
        Some((took, Err((_, msg)))) => serde_json::json!({ "status": "error", "elapsed_s": took.as_secs_f64(), "message": msg }),
    }))
}

/// Décalage de printemps en jours entre climat cible et référence.
/// Règle simple : comparer les Tmin moyens de mars-avril. Chaque °C d'avance
/// déplace de 4 jours.
fn spring_shift_days(reference: &ClimateProfile, target: &ClimateProfile) -> i16 {
    let ref_spring = (reference.temp_min_c[2] + reference.temp_min_c[3]) / 2.0;
    let tgt_spring = (target.temp_min_c[2] + target.temp_min_c[3]) / 2.0;
    let delta = tgt_spring - ref_spring;
    (-delta * 4.0).round() as i16
}

fn shift_window(w: CalendarWindow, shift: i16) -> CalendarWindow {
    CalendarWindow {
        doy_start: shift_doy(w.doy_start, shift),
        doy_end: shift_doy(w.doy_end, shift),
    }
}

fn shift_doy(doy: u16, shift: i16) -> u16 {
    let mut v = doy as i32 + shift as i32;
    while v < 1 {
        v += 365;
    }
    while v > 365 {
        v -= 365;
    }
    v as u16
}

/// Récupère 5 ans de météo quotidienne et calcule un ClimateProfile.
async fn fetch_5y_climate(lat: f64, lon: f64) -> Option<ClimateProfile> {
    use crate::domain::weather_live;
    let today = chrono::Local::now().date_naive();
    let start = chrono::NaiveDate::from_ymd_opt(today.year() - 5, 1, 1)?;
    let end = today - chrono::Duration::days(1); // hier (évite les données manquantes du jour)
    let series_map = weather_live::fetch_live_weather(lat, lon, start, end).await;
    if series_map.is_empty() {
        return None;
    }
    let mut series: Vec<_> = series_map.into_values().collect();
    series.sort_by_key(|w| (w.date.year, w.date.day_of_year));
    Some(ClimateProfile::from_daily_series(
        format!("Calculé (5 ans Open-Meteo, {:.4}°N {:.4}°E)", lat, lon),
        &series,
    ))
}

// Ajout nécessaire pour `today.year() - 5`.
use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<Species> {
        Species::load_from_json("data/species.json").expect("species.json valide")
    }

    #[test]
    fn excludes_parse_known_ids_and_skip_blanks() {
        let sp = catalog();
        assert_eq!(parse_excludes(" beet, ,leek ", &sp), Ok(vec!["beet".into(), "leek".into()]));
        assert_eq!(parse_excludes("", &sp), Ok(vec![]));
    }

    #[test]
    fn excludes_reject_unknown_id() {
        assert_eq!(parse_excludes("beet,licorne", &catalog()), Err("espèce inconnue : licorne".into()));
    }
}
