//! Géolocalisation et profil climatique.
//!
//! Le profil climatique porte les normales mensuelles (1991-2020) qui calibrent
//! le générateur météo. Les valeurs par défaut sont celles de Paris-Montsouris,
//! représentatives d'un climat tempéré océanique dégradé.

use serde::{Deserialize, Serialize};

/// Localisation géographique d'un jardin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub name: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: f64,
    pub climate: ClimateProfile,
}

/// Normales climatiques mensuelles (12 entrées, index 0 = janvier).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClimateProfile {
    pub name: String,
    /// Moyenne mensuelle des températures minimales journalières (°C).
    pub temp_min_c: [f64; 12],
    /// Moyenne mensuelle des températures maximales journalières (°C).
    pub temp_max_c: [f64; 12],
    /// Cumul mensuel moyen de précipitations (mm).
    pub precip_mm: [f64; 12],
    /// Nombre moyen de jours de pluie par mois.
    pub rainy_days: [f64; 12],
    /// Nombre moyen de jours de gel par mois (Tmin < 0°C).
    pub frost_days: [f64; 12],
    /// Vitesse moyenne du vent (km/h).
    pub wind_kmh: [f64; 12],
}

impl ClimateProfile {
    /// Climat tempéré océanique, calé sur Paris-Montsouris (Météo-France 1991-2020).
    pub fn paris() -> Self {
        Self {
            name: "Paris (tempéré océanique dégradé)".into(),
            temp_min_c: [
                2.7, 2.7, 5.0, 7.1, 10.6, 13.6, 15.7, 15.6, 12.6, 9.4, 5.4, 3.4,
            ],
            temp_max_c: [
                7.6, 8.7, 12.7, 16.4, 20.0, 23.4, 25.9, 25.7, 21.7, 16.7, 10.9, 7.6,
            ],
            precip_mm: [
                49.0, 41.0, 48.0, 51.0, 65.0, 53.0, 62.0, 53.0, 48.0, 60.0, 51.0, 58.0,
            ],
            rainy_days: [
                9.7, 9.3, 9.5, 9.7, 9.6, 7.8, 7.6, 6.8, 7.6, 9.6, 10.0, 10.5,
            ],
            frost_days: [
                6.0, 5.0, 2.0, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 5.0,
            ],
            wind_kmh: [
                14.0, 14.0, 14.0, 13.0, 12.0, 11.0, 11.0, 11.0, 11.0, 12.0, 13.0, 14.0,
            ],
        }
    }
}

impl Location {
    /// Localisation par défaut : Paris (Île-de-France), climat tempéré océanique.
    pub fn paris() -> Self {
        Self {
            name: "Paris".into(),
            latitude_deg: 48.8566,
            longitude_deg: 2.3522,
            altitude_m: 35.0,
            climate: ClimateProfile::paris(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paris_climate_arrays_have_12_months() {
        let c = ClimateProfile::paris();
        assert_eq!(c.temp_min_c.len(), 12);
        assert_eq!(c.temp_max_c.len(), 12);
        assert_eq!(c.precip_mm.len(), 12);
    }

    #[test]
    fn paris_summer_warmer_than_winter() {
        let c = ClimateProfile::paris();
        // Juillet (idx 6) > Janvier (idx 0)
        assert!(c.temp_max_c[6] > c.temp_max_c[0] + 10.0);
    }

    #[test]
    fn paris_annual_precip_is_in_expected_range() {
        let total: f64 = ClimateProfile::paris().precip_mm.iter().sum();
        // Paris : ~640 mm/an
        assert!((600.0..700.0).contains(&total), "annual precip = {}", total);
    }

    #[test]
    fn paris_location_has_climate() {
        let loc = Location::paris();
        assert!(loc.latitude_deg > 48.0 && loc.latitude_deg < 49.0);
        assert_eq!(loc.climate.temp_min_c.len(), 12);
    }
}
