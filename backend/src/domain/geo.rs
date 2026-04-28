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

    /// Lyon-Bron : semi-continental. Étés chauds et secs, hivers froids.
    pub fn lyon() -> Self {
        Self {
            name: "Lyon (semi-continental)".into(),
            temp_min_c: [0.7, 1.2, 4.0, 6.7, 10.8, 14.4, 16.4, 16.0, 12.5, 9.3, 4.5, 1.7],
            temp_max_c: [6.7, 8.5, 13.2, 16.7, 21.1, 25.0, 28.0, 27.5, 22.7, 17.0, 10.6, 7.0],
            precip_mm: [52.0, 47.0, 54.0, 71.0, 87.0, 80.0, 68.0, 71.0, 82.0, 91.0, 80.0, 65.0],
            rainy_days: [9.0, 8.0, 9.0, 9.5, 10.5, 8.5, 7.0, 7.5, 8.0, 9.5, 10.0, 9.5],
            frost_days: [12.0, 9.0, 4.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 5.0, 10.0],
            wind_kmh: [11.0, 12.0, 13.0, 12.0, 11.0, 10.0, 10.0, 10.0, 10.0, 11.0, 11.0, 11.0],
        }
    }

    /// Marseille-Marignane : méditerranéen. Étés secs, hivers doux.
    pub fn marseille() -> Self {
        Self {
            name: "Marseille (méditerranéen)".into(),
            temp_min_c: [3.3, 3.6, 6.3, 9.0, 12.8, 16.4, 19.0, 18.9, 15.6, 12.4, 7.4, 4.5],
            temp_max_c: [11.5, 12.7, 16.0, 18.6, 22.8, 27.0, 30.1, 29.9, 25.5, 20.5, 15.1, 11.9],
            precip_mm: [46.0, 34.0, 38.0, 48.0, 41.0, 23.0, 12.0, 25.0, 60.0, 82.0, 65.0, 49.0],
            rainy_days: [5.0, 4.5, 5.0, 6.0, 5.5, 3.5, 1.5, 3.0, 5.0, 7.0, 6.5, 6.0],
            frost_days: [3.0, 2.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.3, 2.0],
            wind_kmh: [18.0, 18.0, 17.0, 16.0, 14.0, 13.0, 13.0, 13.0, 14.0, 15.0, 17.0, 17.0],
        }
    }

    /// Nantes : océanique doux et humide. Hivers doux, étés tempérés.
    pub fn nantes() -> Self {
        Self {
            name: "Nantes (océanique)".into(),
            temp_min_c: [3.3, 3.3, 5.0, 6.6, 10.0, 13.0, 15.0, 14.7, 12.0, 9.5, 6.0, 4.0],
            temp_max_c: [8.9, 9.9, 13.1, 15.6, 19.0, 22.3, 24.7, 24.9, 22.0, 17.4, 12.3, 9.4],
            precip_mm: [90.0, 71.0, 69.0, 65.0, 69.0, 51.0, 46.0, 52.0, 65.0, 92.0, 92.0, 105.0],
            rainy_days: [13.0, 11.0, 11.0, 11.0, 10.5, 8.0, 7.0, 7.5, 9.0, 12.0, 13.0, 13.5],
            frost_days: [4.0, 3.5, 1.5, 0.3, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5, 3.5],
            wind_kmh: [16.0, 16.0, 15.0, 14.0, 13.0, 12.0, 11.0, 11.0, 12.0, 13.0, 14.0, 16.0],
        }
    }

    /// Bordeaux : océanique aquitain, tendance chaude. Étés plus chauds que Nantes.
    pub fn bordeaux() -> Self {
        Self {
            name: "Bordeaux (océanique aquitain)".into(),
            temp_min_c: [3.0, 3.1, 5.1, 7.0, 10.7, 13.8, 15.8, 15.6, 12.8, 10.1, 6.0, 3.6],
            temp_max_c: [10.3, 11.5, 14.9, 17.0, 21.1, 24.6, 27.2, 27.3, 24.0, 19.0, 13.7, 10.6],
            precip_mm: [87.0, 70.0, 68.0, 78.0, 81.0, 61.0, 50.0, 60.0, 75.0, 100.0, 100.0, 105.0],
            rainy_days: [12.0, 10.5, 10.5, 11.0, 11.0, 8.0, 7.0, 8.0, 9.0, 11.0, 12.0, 12.5],
            frost_days: [6.0, 4.5, 2.0, 0.3, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 5.0],
            wind_kmh: [14.0, 14.0, 14.0, 13.0, 12.0, 11.0, 10.0, 10.0, 11.0, 12.0, 13.0, 14.0],
        }
    }

    /// Toulouse : océanique dégradé à influence méditerranéenne. Étés chauds.
    pub fn toulouse() -> Self {
        Self {
            name: "Toulouse (océanique à influence méditerranéenne)".into(),
            temp_min_c: [2.8, 3.1, 5.4, 7.6, 11.2, 14.5, 16.7, 16.5, 13.5, 10.4, 5.9, 3.4],
            temp_max_c: [9.6, 11.2, 14.6, 17.0, 21.1, 25.1, 28.0, 27.9, 24.3, 19.1, 13.1, 10.0],
            precip_mm: [50.0, 41.0, 50.0, 70.0, 78.0, 60.0, 40.0, 50.0, 51.0, 57.0, 50.0, 53.0],
            rainy_days: [9.0, 8.0, 9.0, 10.0, 10.5, 8.0, 6.0, 6.5, 7.5, 8.5, 9.0, 9.5],
            frost_days: [8.0, 6.0, 2.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.2, 3.0, 7.0],
            wind_kmh: [15.0, 15.0, 15.0, 14.0, 13.0, 12.0, 12.0, 12.0, 12.0, 13.0, 14.0, 15.0],
        }
    }

    /// Strasbourg : semi-continental franc. Hivers froids, étés chauds, amplitude forte.
    pub fn strasbourg() -> Self {
        Self {
            name: "Strasbourg (semi-continental)".into(),
            temp_min_c: [-0.9, -0.2, 2.6, 5.4, 9.9, 13.2, 15.1, 14.7, 11.2, 7.6, 3.0, 0.0],
            temp_max_c: [4.4, 6.2, 11.4, 15.6, 20.1, 23.2, 25.7, 25.6, 21.1, 15.1, 8.7, 4.9],
            precip_mm: [34.0, 36.0, 40.0, 49.0, 76.0, 73.0, 70.0, 60.0, 54.0, 58.0, 45.0, 46.0],
            rainy_days: [10.0, 9.0, 9.0, 9.5, 11.0, 9.5, 9.0, 8.5, 8.0, 9.5, 10.0, 10.5],
            frost_days: [17.0, 12.0, 7.0, 2.0, 0.2, 0.0, 0.0, 0.0, 0.0, 1.0, 7.0, 15.0],
            wind_kmh: [10.0, 10.0, 11.0, 11.0, 10.0, 10.0, 10.0, 9.0, 9.0, 9.0, 9.0, 10.0],
        }
    }

    /// Lille : océanique frais et humide, tendance nordique.
    pub fn lille() -> Self {
        Self {
            name: "Lille (océanique nord)".into(),
            temp_min_c: [1.7, 1.7, 3.9, 5.7, 9.3, 12.1, 14.3, 14.0, 11.5, 8.5, 4.5, 2.3],
            temp_max_c: [6.5, 7.5, 11.0, 14.0, 17.8, 20.9, 23.3, 23.2, 19.9, 15.3, 10.0, 6.8],
            precip_mm: [60.0, 48.0, 53.0, 48.0, 62.0, 64.0, 66.0, 72.0, 62.0, 69.0, 74.0, 73.0],
            rainy_days: [12.0, 10.0, 11.0, 10.0, 10.5, 9.5, 9.0, 9.5, 10.0, 11.5, 12.5, 13.0],
            frost_days: [9.0, 7.0, 3.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.2, 3.5, 7.5],
            wind_kmh: [15.0, 15.0, 15.0, 14.0, 13.0, 12.0, 12.0, 12.0, 13.0, 13.0, 14.0, 15.0],
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

    pub fn lyon() -> Self {
        Self { name: "Lyon".into(), latitude_deg: 45.7640, longitude_deg: 4.8357, altitude_m: 173.0, climate: ClimateProfile::lyon() }
    }
    pub fn marseille() -> Self {
        Self { name: "Marseille".into(), latitude_deg: 43.2965, longitude_deg: 5.3698, altitude_m: 12.0, climate: ClimateProfile::marseille() }
    }
    pub fn nantes() -> Self {
        Self { name: "Nantes".into(), latitude_deg: 47.2184, longitude_deg: -1.5536, altitude_m: 18.0, climate: ClimateProfile::nantes() }
    }
    pub fn bordeaux() -> Self {
        Self { name: "Bordeaux".into(), latitude_deg: 44.8378, longitude_deg: -0.5792, altitude_m: 20.0, climate: ClimateProfile::bordeaux() }
    }
    pub fn toulouse() -> Self {
        Self { name: "Toulouse".into(), latitude_deg: 43.6047, longitude_deg: 1.4442, altitude_m: 150.0, climate: ClimateProfile::toulouse() }
    }
    pub fn strasbourg() -> Self {
        Self { name: "Strasbourg".into(), latitude_deg: 48.5734, longitude_deg: 7.7521, altitude_m: 140.0, climate: ClimateProfile::strasbourg() }
    }
    pub fn lille() -> Self {
        Self { name: "Lille".into(), latitude_deg: 50.6292, longitude_deg: 3.0573, altitude_m: 23.0, climate: ClimateProfile::lille() }
    }

    /// Le Bois-d'Oingt (Rhône, Beaujolais, ~350m) — climat semi-continental
    /// à nette influence méditerranéenne (étés chauds, hivers froids moins rigoureux
    /// qu'en Alsace grâce à l'abri du Beaujolais). Normales dérivées de Lyon + ajustement altitude.
    pub fn le_bois_doingt() -> Self {
        Self {
            name: "Le Bois-d'Oingt".into(),
            latitude_deg: 45.9294,
            longitude_deg: 4.5822,
            altitude_m: 350.0,
            // Par défaut on part du profil Lyon avec -1°C sur Tmin/Tmax pour l'altitude.
            climate: ClimateProfile {
                name: "Le Bois-d'Oingt (Beaujolais, semi-continental)".into(),
                temp_min_c: [-0.3, 0.2, 3.0, 5.7, 9.8, 13.4, 15.4, 15.0, 11.5, 8.3, 3.5, 0.7],
                temp_max_c: [5.7, 7.5, 12.2, 15.7, 20.1, 24.0, 27.0, 26.5, 21.7, 16.0, 9.6, 6.0],
                precip_mm: [60.0, 55.0, 60.0, 80.0, 100.0, 90.0, 75.0, 80.0, 95.0, 100.0, 90.0, 75.0],
                rainy_days: [9.5, 8.5, 9.5, 10.0, 11.0, 9.0, 7.5, 8.0, 8.5, 10.0, 10.5, 10.0],
                frost_days: [14.0, 10.0, 5.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 6.5, 11.5],
                wind_kmh: [10.0, 11.0, 12.0, 11.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0],
            },
        }
    }

    /// Résout un slug en Location. "le_bois_doingt" par défaut.
    pub fn by_slug(slug: &str) -> Self {
        match slug.to_ascii_lowercase().as_str() {
            "paris" => Self::paris(),
            "lyon" => Self::lyon(),
            "marseille" => Self::marseille(),
            "nantes" => Self::nantes(),
            "bordeaux" => Self::bordeaux(),
            "toulouse" => Self::toulouse(),
            "strasbourg" => Self::strasbourg(),
            "lille" => Self::lille(),
            _ => Self::le_bois_doingt(),
        }
    }

    /// Liste des villes supportées.
    pub fn available() -> Vec<(&'static str, &'static str)> {
        vec![
            ("le_bois_doingt", "Le Bois-d'Oingt"),
            ("paris", "Paris"),
            ("lyon", "Lyon"),
            ("marseille", "Marseille"),
            ("nantes", "Nantes"),
            ("bordeaux", "Bordeaux"),
            ("toulouse", "Toulouse"),
            ("strasbourg", "Strasbourg"),
            ("lille", "Lille"),
        ]
    }
}

impl ClimateProfile {
    /// Calcule un profil climatique à partir d'une série de jours de météo live
    /// (typiquement 5 ans), via moyennes mensuelles.
    pub fn from_daily_series(
        name: String,
        series: &[crate::domain::weather::DailyWeather],
    ) -> Self {
        let mut sum_tmin = [0.0f64; 12];
        let mut sum_tmax = [0.0f64; 12];
        let mut sum_precip = [0.0f64; 12];
        let mut sum_wind = [0.0f64; 12];
        let mut count_rainy = [0u32; 12];
        let mut count_frost = [0u32; 12];
        let mut count_total = [0u32; 12];

        for d in series {
            let m = d.date.month_idx();
            sum_tmin[m] += d.temp_min_c;
            sum_tmax[m] += d.temp_max_c;
            sum_precip[m] += d.precipitation_mm;
            sum_wind[m] += d.wind_kmh;
            if d.precipitation_mm >= 1.0 {
                count_rainy[m] += 1;
            }
            if d.temp_min_c < 0.0 {
                count_frost[m] += 1;
            }
            count_total[m] += 1;
        }

        // Nombre d'années observées par mois (pour les totaux mensuels de pluie / jours).
        let years = [31.0, 28.25, 31.0, 30.0, 31.0, 30.0, 31.0, 31.0, 30.0, 31.0, 30.0, 31.0];
        let mut temp_min_c = [0.0; 12];
        let mut temp_max_c = [0.0; 12];
        let mut precip_mm = [0.0; 12];
        let mut rainy_days = [0.0; 12];
        let mut frost_days = [0.0; 12];
        let mut wind_kmh = [0.0; 12];
        for m in 0..12 {
            let n = count_total[m].max(1) as f64;
            let n_years = (n / years[m]).max(1.0);
            temp_min_c[m] = sum_tmin[m] / n;
            temp_max_c[m] = sum_tmax[m] / n;
            precip_mm[m] = sum_precip[m] / n_years;
            rainy_days[m] = (count_rainy[m] as f64) / n_years;
            frost_days[m] = (count_frost[m] as f64) / n_years;
            wind_kmh[m] = sum_wind[m] / n;
        }

        ClimateProfile {
            name,
            temp_min_c,
            temp_max_c,
            precip_mm,
            rainy_days,
            frost_days,
            wind_kmh,
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
