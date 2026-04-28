//! Types météo : état journalier + classification qualitative.

use serde::{Deserialize, Serialize};

use super::time::SimDate;

/// Catégorie qualitative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeatherKind {
    Clear,
    PartlyCloudy,
    Overcast,
    LightRain,
    HeavyRain,
    Storm,
    Frost,
    Snow,
    Heatwave,
    Fog,
}

/// État météo d'un jour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyWeather {
    pub date: SimDate,
    pub kind: WeatherKind,
    pub temp_min_c: f64,
    pub temp_max_c: f64,
    pub temp_mean_c: f64,
    pub precipitation_mm: f64,
    pub solar_radiation_mj_m2: f64,
    pub wind_kmh: f64,
    pub humidity_pct: f64,
    pub photoperiod_h: f64,
    pub frost: bool,
    pub heatwave: bool,
}

pub fn classify_weather(
    precip: f64,
    tmin: f64,
    tmax: f64,
    wind: f64,
) -> WeatherKind {
    if tmin < 0.0 && precip < 0.5 {
        return WeatherKind::Frost;
    }
    if precip > 0.5 && tmax < 1.0 {
        return WeatherKind::Snow;
    }
    if precip >= 15.0 || (precip >= 5.0 && wind > 30.0) {
        return WeatherKind::Storm;
    }
    if precip >= 5.0 {
        return WeatherKind::HeavyRain;
    }
    if precip > 0.0 {
        return WeatherKind::LightRain;
    }
    if tmax >= 32.0 {
        return WeatherKind::Heatwave;
    }
    WeatherKind::Clear
}
