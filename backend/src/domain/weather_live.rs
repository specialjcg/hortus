//! Client Open-Meteo : récupère la météo historique + prévision J+7 pour une localisation.
//!
//! API utilisée :
//! - Archive : https://archive-api.open-meteo.com/v1/archive (données quotidiennes passées)
//! - Forecast : https://api.open-meteo.com/v1/forecast (prévision 7 jours)
//!
//! Gratuite, sans clé, pas de rate limit agressif pour usage personnel.

use chrono::NaiveDate;
use serde::Deserialize;
use std::collections::HashMap;

use super::time::{photoperiod_hours, SimDate};
use super::weather::{classify_weather, DailyWeather, WeatherKind};

#[derive(Debug, Deserialize)]
struct DailyResponse {
    latitude: f64,
    #[allow(dead_code)]
    longitude: f64,
    daily: Daily,
}

#[derive(Debug, Deserialize)]
struct Daily {
    time: Vec<String>,
    temperature_2m_max: Vec<Option<f64>>,
    temperature_2m_min: Vec<Option<f64>>,
    precipitation_sum: Vec<Option<f64>>,
    wind_speed_10m_max: Vec<Option<f64>>,
    shortwave_radiation_sum: Vec<Option<f64>>,
}

const ARCHIVE_URL: &str = "https://archive-api.open-meteo.com/v1/archive";
const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";

const DAILY_VARS: &str = "temperature_2m_max,temperature_2m_min,precipitation_sum,wind_speed_10m_max,shortwave_radiation_sum";

/// Récupère la météo quotidienne de `start` à `end` (inclus) pour une latitude/longitude.
/// Renvoie un map `SimDate → DailyWeather`. En cas d'erreur réseau, renvoie un map vide.
///
/// Split automatique : dates ≤ aujourd'hui via Archive API, dates futures via Forecast API.
pub async fn fetch_live_weather(
    latitude: f64,
    longitude: f64,
    start: NaiveDate,
    end: NaiveDate,
) -> HashMap<SimDate, DailyWeather> {
    let mut out: HashMap<SimDate, DailyWeather> = HashMap::new();
    let today = chrono::Local::now().date_naive();

    // Passé (incluant aujourd'hui pour consolider)
    if start <= today {
        let archive_end = end.min(today);
        if let Ok(map) =
            fetch_range(ARCHIVE_URL, latitude, longitude, start, archive_end).await
        {
            out.extend(map);
        }
    }

    // Futur (prévision) : strictement > aujourd'hui
    if end > today {
        let fc_start = if start > today { start } else { today + chrono::Duration::days(1) };
        if let Ok(map) =
            fetch_range(FORECAST_URL, latitude, longitude, fc_start, end).await
        {
            out.extend(map);
        }
    }

    out
}

async fn fetch_range(
    base_url: &str,
    latitude: f64,
    longitude: f64,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<HashMap<SimDate, DailyWeather>, reqwest::Error> {
    let url = format!(
        "{base_url}?latitude={lat:.4}&longitude={lon:.4}&start_date={s}&end_date={e}&daily={vars}&timezone=auto&wind_speed_unit=kmh",
        lat = latitude,
        lon = longitude,
        s = start.format("%Y-%m-%d"),
        e = end.format("%Y-%m-%d"),
        vars = DAILY_VARS,
    );
    tracing::info!("Open-Meteo fetch: {}", url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let resp: DailyResponse = client.get(&url).send().await?.error_for_status()?.json().await?;

    let mut out: HashMap<SimDate, DailyWeather> = HashMap::new();
    for (i, date_str) in resp.daily.time.iter().enumerate() {
        let Ok(nd) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else { continue };
        let sim_date = naive_to_sim_date(nd);
        let tmax = resp.daily.temperature_2m_max.get(i).and_then(|x| *x);
        let tmin = resp.daily.temperature_2m_min.get(i).and_then(|x| *x);
        let precip = resp.daily.precipitation_sum.get(i).and_then(|x| *x).unwrap_or(0.0);
        let wind = resp
            .daily
            .wind_speed_10m_max
            .get(i)
            .and_then(|x| *x)
            .unwrap_or(10.0);
        let solar = resp
            .daily
            .shortwave_radiation_sum
            .get(i)
            .and_then(|x| *x)
            .unwrap_or(15.0);
        let (tmin, tmax) = match (tmin, tmax) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        let tmean = (tmin + tmax) / 2.0;
        let photo = photoperiod_hours(resp.latitude, sim_date.day_of_year);
        let frost = tmin < 0.0;
        let heatwave = tmax >= 32.0;
        let kind: WeatherKind = classify_weather(precip, tmin, tmax, wind);
        let dw = DailyWeather {
            date: sim_date,
            kind,
            temp_min_c: tmin,
            temp_max_c: tmax,
            temp_mean_c: tmean,
            precipitation_mm: precip,
            solar_radiation_mj_m2: solar,
            wind_kmh: wind,
            humidity_pct: 70.0, // approximation (pas retourné par cette requête)
            photoperiod_h: photo,
            frost,
            heatwave,
        };
        out.insert(sim_date, dw);
    }
    Ok(out)
}

fn naive_to_sim_date(nd: NaiveDate) -> SimDate {
    use chrono::Datelike;
    let doy = (nd.ordinal() as u16).min(SimDate::DAYS_PER_YEAR);
    SimDate {
        year: nd.year() as u16,
        day_of_year: doy,
    }
}
