//! Routes HTTP — calendrier des plantations (stateless).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::db::{self, Db};
use crate::domain::geo::{ClimateProfile, Location};
use crate::domain::species::{CalendarWindow, Species};
use crate::journal::{
    self, Action, ActionFilter, ActionInput, Parcel, ParcelInput, ACTION_KINDS,
};

/// État partagé.
#[derive(Clone)]
pub struct AppState {
    pub species: Arc<Vec<Species>>,
    pub db: Db,
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
    let state = AppState { species: Arc::new(species), db };

    Router::new()
        .route("/health", get(health))
        .route("/species", get(list_species))
        .route("/cities", get(list_cities))
        .route("/calendar", get(calendar))
        .route("/forecast", get(forecast))
        .route("/historical-year", get(historical_year))
        .route("/action-kinds", get(list_action_kinds))
        .route("/parcels", get(get_parcels).post(create_parcel))
        .route("/parcels/{id}", put(put_parcel).delete(del_parcel))
        .route("/actions", get(get_actions).post(create_action).delete(del_all_actions))
        .route("/actions/{id}", put(put_action).delete(del_action))
        .with_state(state)
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
