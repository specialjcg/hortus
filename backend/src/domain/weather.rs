//! Météo journalière : état observé + générateur procédural.
//!
//! Le générateur est calé sur les normales mensuelles d'un [`ClimateProfile`] et
//! reproductible via une graine RNG. Modèle simple mais réaliste :
//! - Markov à 2 états pour pluie/sec (capture les épisodes pluvieux)
//! - Tmin/Tmax échantillonnés autour des moyennes mensuelles avec variabilité saisonnière
//! - Quantité de pluie ~ exponentielle, calibrée pour reproduire le cumul mensuel
//! - Radiation solaire dérivée de la photopériode et de la nébulosité

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use super::geo::{ClimateProfile, Location};
use super::time::{photoperiod_hours, SimDate};

/// Catégorie qualitative pour journal de bord et UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeatherKind {
    Clear,         // ciel dégagé, sec
    PartlyCloudy,  // sec, nuages épars
    Overcast,      // sec, couvert
    LightRain,     // < 5 mm
    HeavyRain,     // ≥ 5 mm
    Storm,         // ≥ 15 mm OU vent fort
    Frost,         // Tmin < 0
    Snow,          // précipitation et Tmax < 1
    Heatwave,      // Tmax ≥ 32
    Fog,           // humidité haute, peu de vent, peu de soleil
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
    /// Rayonnement solaire global journalier (MJ/m²).
    pub solar_radiation_mj_m2: f64,
    pub wind_kmh: f64,
    pub humidity_pct: f64,
    pub photoperiod_h: f64,
    pub frost: bool,
    pub heatwave: bool,
}

impl DailyWeather {
    /// Évapotranspiration potentielle journalière (mm) — formule de Hargreaves simplifiée.
    /// ETP = 0.0023 · Ra · (T_mean + 17.8) · √(Tmax − Tmin)
    /// avec Ra approximée par solar_radiation_mj_m2.
    pub fn etp_mm(&self) -> f64 {
        let dt = (self.temp_max_c - self.temp_min_c).max(0.0);
        0.0023 * self.solar_radiation_mj_m2 * (self.temp_mean_c + 17.8) * dt.sqrt()
    }
}

/// Générateur procédural reproductible.
pub struct WeatherGenerator {
    location: Location,
    rng: ChaCha8Rng,
    yesterday_was_wet: bool,
}

impl WeatherGenerator {
    pub fn new(location: Location, seed: u64) -> Self {
        Self {
            location,
            rng: ChaCha8Rng::seed_from_u64(seed),
            yesterday_was_wet: false,
        }
    }

    pub fn climate(&self) -> &ClimateProfile {
        &self.location.climate
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Tire la météo pour `date`. Avance l'état interne.
    pub fn step(&mut self, date: SimDate) -> DailyWeather {
        let m = date.month_idx();
        let c = self.location.climate.clone();

        // ---- Pluie : Markov à 2 états ----
        // Probabilité de base d'un jour humide ce mois-ci (rainy_days/30).
        let p_wet_base = (c.rainy_days[m] / 30.0).clamp(0.05, 0.9);
        // Persistance : un jour humide a plus de chances d'être suivi d'un humide.
        let p_wet = if self.yesterday_was_wet {
            (p_wet_base + 0.25).min(0.9)
        } else {
            (p_wet_base * 0.85).max(0.05)
        };
        let is_wet = self.rng.gen::<f64>() < p_wet;
        let precip = if is_wet {
            // Cumul moyen sur jours humides ; tirer en exponentielle.
            let mean_per_wet = c.precip_mm[m] / c.rainy_days[m].max(1.0);
            sample_exponential(&mut self.rng, mean_per_wet).clamp(0.1, 80.0)
        } else {
            0.0
        };
        self.yesterday_was_wet = is_wet;

        // ---- Températures ----
        // Variabilité jour-à-jour ~ 2-3°C l'hiver, 3-4°C l'été.
        let sigma_min = 2.5 + 0.4 * ((m as f64 - 6.0).abs() / 6.0);
        let sigma_max = 3.0 + 0.5 * ((m as f64 - 6.0).abs() / 6.0);
        let mut temp_min = sample_normal(&mut self.rng, c.temp_min_c[m], sigma_min);
        let mut temp_max = sample_normal(&mut self.rng, c.temp_max_c[m], sigma_max);
        // Si pluie épaisse → refroidi de 1-2°C, plage compressée.
        if precip > 5.0 {
            temp_max -= 2.0;
            temp_min += 0.5;
        }
        if temp_max < temp_min + 1.0 {
            temp_max = temp_min + 1.0;
        }
        let temp_mean = 0.5 * (temp_min + temp_max);

        // ---- Photopériode ----
        let photo = photoperiod_hours(self.location.latitude_deg, date.day_of_year);

        // ---- Radiation solaire ----
        // Borne supérieure ≈ 0.0820 · Ra_extraterrestre · (1 - 0.1·nuages).
        // Approximation : Ra_max(Paris) ~ 35 MJ/m²/j en juin, ~8 en décembre.
        let ra_extra = 8.0 + 27.0 * ((photo - 8.0) / 8.0).clamp(0.0, 1.0);
        // Clearness selon humidité du jour : sec → 0.7, pluie épaisse → 0.25.
        let clearness = if precip > 10.0 {
            0.20
        } else if precip > 1.0 {
            0.40
        } else if c.rainy_days[m] / 30.0 > 0.4 {
            0.55
        } else {
            0.70
        };
        let solar = (ra_extra * clearness).max(1.0);

        // ---- Vent ----
        let wind = sample_normal(&mut self.rng, c.wind_kmh[m], 4.0).max(0.0);

        // ---- Humidité ----
        let humidity = if is_wet {
            sample_normal(&mut self.rng, 85.0, 6.0).clamp(50.0, 100.0)
        } else {
            sample_normal(&mut self.rng, 65.0, 10.0).clamp(20.0, 95.0)
        };

        // ---- Catégorisation ----
        let frost = temp_min < 0.0;
        let heatwave = temp_max >= 32.0;
        let kind = classify_weather(precip, temp_min, temp_max, wind, humidity, solar, ra_extra);

        DailyWeather {
            date,
            kind,
            temp_min_c: temp_min,
            temp_max_c: temp_max,
            temp_mean_c: temp_mean,
            precipitation_mm: precip,
            solar_radiation_mj_m2: solar,
            wind_kmh: wind,
            humidity_pct: humidity,
            photoperiod_h: photo,
            frost,
            heatwave,
        }
    }
}

fn classify_weather(
    precip: f64,
    tmin: f64,
    tmax: f64,
    wind: f64,
    humidity: f64,
    solar: f64,
    ra_max: f64,
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
    if humidity > 90.0 && wind < 5.0 && solar < 0.4 * ra_max {
        return WeatherKind::Fog;
    }
    let clearness = solar / ra_max.max(1.0);
    if clearness > 0.65 {
        WeatherKind::Clear
    } else if clearness > 0.45 {
        WeatherKind::PartlyCloudy
    } else {
        WeatherKind::Overcast
    }
}

// --- échantillonnage ---

fn sample_normal(rng: &mut ChaCha8Rng, mean: f64, sigma: f64) -> f64 {
    // Box-Muller
    let u1: f64 = rng.gen_range(1e-10..1.0);
    let u2: f64 = rng.gen::<f64>();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mean + sigma * z
}

fn sample_exponential(rng: &mut ChaCha8Rng, mean: f64) -> f64 {
    let u: f64 = rng.gen_range(1e-10..1.0);
    -mean * u.ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paris_gen(seed: u64) -> WeatherGenerator {
        WeatherGenerator::new(Location::paris(), seed)
    }

    fn run_year(seed: u64) -> Vec<DailyWeather> {
        let mut gen = paris_gen(seed);
        let mut date = SimDate::start();
        (0..365)
            .map(|_| {
                let w = gen.step(date);
                date = date.next_day();
                w
            })
            .collect()
    }

    #[test]
    fn weather_is_reproducible_with_same_seed() {
        let a = run_year(42);
        let b = run_year(42);
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.temp_min_c, y.temp_min_c);
            assert_eq!(x.precipitation_mm, y.precipitation_mm);
        }
    }

    #[test]
    fn annual_precip_in_realistic_range() {
        // Sur une année, devrait ressembler aux ~640 mm Paris à ±30%.
        let year = run_year(1);
        let total: f64 = year.iter().map(|w| w.precipitation_mm).sum();
        assert!(
            (450.0..900.0).contains(&total),
            "annual precip = {} (target ~640)",
            total
        );
    }

    #[test]
    fn summer_avg_warmer_than_winter() {
        let year = run_year(7);
        let jul: Vec<_> = year.iter().filter(|w| w.date.month() == 7).collect();
        let jan: Vec<_> = year.iter().filter(|w| w.date.month() == 1).collect();
        let mean_jul: f64 =
            jul.iter().map(|w| w.temp_mean_c).sum::<f64>() / jul.len() as f64;
        let mean_jan: f64 =
            jan.iter().map(|w| w.temp_mean_c).sum::<f64>() / jan.len() as f64;
        assert!(
            mean_jul > mean_jan + 12.0,
            "jul {:.1} vs jan {:.1}",
            mean_jul,
            mean_jan
        );
    }

    #[test]
    fn frost_days_only_in_cold_months() {
        let year = run_year(3);
        let frost_count: usize = year.iter().filter(|w| w.frost).count();
        // Paris : ~20 jours de gel/an, large tolérance pour stochasticité.
        assert!(frost_count > 0 && frost_count < 60, "frost = {}", frost_count);
        // Pas de gel en juillet.
        let frost_jul = year
            .iter()
            .filter(|w| w.frost && w.date.month() == 7)
            .count();
        assert_eq!(frost_jul, 0);
    }

    #[test]
    fn etp_is_positive_in_summer() {
        let year = run_year(5);
        let jul = year.iter().find(|w| w.date.month() == 7).unwrap();
        assert!(jul.etp_mm() > 1.0, "ETP juillet trop faible: {}", jul.etp_mm());
    }
}
