//! Temps de simulation.
//!
//! Modèle simplifié : 365 jours par an (pas de bissextile), index 1-based.
//! Suffisant pour la phénologie horticole (la perte d'1 jour/4 ans n'a aucun impact).

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Date dans la simulation. `year` commence à 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SimDate {
    pub year: u16,
    /// Jour de l'année, 1..=365.
    pub day_of_year: u16,
}

impl SimDate {
    pub const DAYS_PER_YEAR: u16 = 365;

    /// Premier jour de l'an 1 (1er janvier de la simulation).
    pub fn start() -> Self {
        Self { year: 1, day_of_year: 1 }
    }

    pub fn new(year: u16, day_of_year: u16) -> Self {
        assert!(day_of_year >= 1 && day_of_year <= Self::DAYS_PER_YEAR);
        Self { year, day_of_year }
    }

    /// Avance d'un jour, gère le passage à l'année suivante.
    pub fn next_day(self) -> Self {
        if self.day_of_year == Self::DAYS_PER_YEAR {
            Self { year: self.year + 1, day_of_year: 1 }
        } else {
            Self { year: self.year, day_of_year: self.day_of_year + 1 }
        }
    }

    /// Avance de n jours.
    pub fn advance(self, days: u32) -> Self {
        let total = (self.year as u32 - 1) * Self::DAYS_PER_YEAR as u32
            + (self.day_of_year as u32 - 1)
            + days;
        let year = (total / Self::DAYS_PER_YEAR as u32 + 1) as u16;
        let day = (total % Self::DAYS_PER_YEAR as u32 + 1) as u16;
        Self { year, day_of_year: day }
    }

    /// Différence en jours entre deux dates (self - other), peut être négative.
    pub fn days_since(self, other: Self) -> i32 {
        let a = (self.year as i32 - 1) * Self::DAYS_PER_YEAR as i32 + self.day_of_year as i32;
        let b = (other.year as i32 - 1) * Self::DAYS_PER_YEAR as i32 + other.day_of_year as i32;
        a - b
    }

    /// Mois (1..=12) à partir du jour de l'année. Utilise les longueurs réelles
    /// des mois d'une année non bissextile.
    pub fn month(self) -> u8 {
        const CUMUL: [u16; 13] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365];
        for m in 0..12 {
            if self.day_of_year <= CUMUL[m + 1] {
                return (m + 1) as u8;
            }
        }
        12
    }

    /// Index de mois 0..=11 (utile pour indexer les normales climatiques).
    pub fn month_idx(self) -> usize {
        (self.month() - 1) as usize
    }
}

/// Photopériode (durée du jour en heures) à une latitude donnée pour un jour de l'année.
///
/// Formule classique (NOAA Solar Calculator simplifiée) :
/// - δ = 23.45° · sin(360°/365 · (DOY − 81))      (déclinaison solaire)
/// - cos(H) = −tan(φ) · tan(δ)                     (angle horaire)
/// - durée = 2H / 15°/h
///
/// Renvoie 0.0 (nuit polaire) ou 24.0 (jour polaire) en cas de bornes dépassées.
pub fn photoperiod_hours(latitude_deg: f64, day_of_year: u16) -> f64 {
    let phi = latitude_deg.to_radians();
    let declination_deg = 23.45 * (((360.0 / 365.0) * (day_of_year as f64 - 81.0)) * PI / 180.0).sin();
    let delta = declination_deg.to_radians();

    let cos_h = -phi.tan() * delta.tan();
    if cos_h <= -1.0 {
        return 24.0; // jour polaire
    }
    if cos_h >= 1.0 {
        return 0.0; // nuit polaire
    }
    let h_rad = cos_h.acos();
    let h_deg = h_rad.to_degrees();
    2.0 * h_deg / 15.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_day_wraps_year() {
        let d = SimDate::new(1, 365);
        let n = d.next_day();
        assert_eq!(n, SimDate::new(2, 1));
    }

    #[test]
    fn advance_handles_multi_year() {
        let d = SimDate::start();
        assert_eq!(d.advance(365), SimDate::new(2, 1));
        assert_eq!(d.advance(365 * 3 + 10), SimDate::new(4, 11));
    }

    #[test]
    fn days_since_is_signed() {
        let a = SimDate::new(2, 100);
        let b = SimDate::new(1, 50);
        assert_eq!(a.days_since(b), 365 + 50);
        assert_eq!(b.days_since(a), -(365 + 50));
    }

    #[test]
    fn month_boundaries() {
        assert_eq!(SimDate::new(1, 1).month(), 1);   // 1 janv
        assert_eq!(SimDate::new(1, 31).month(), 1);  // 31 janv
        assert_eq!(SimDate::new(1, 32).month(), 2);  // 1 fév
        assert_eq!(SimDate::new(1, 365).month(), 12);
        assert_eq!(SimDate::new(1, 172).month(), 6); // ~ solstice été
    }

    #[test]
    fn photoperiod_at_equator_is_about_12h() {
        for doy in [1u16, 80, 172, 264, 355] {
            let p = photoperiod_hours(0.0, doy);
            assert!((p - 12.0).abs() < 0.5, "DOY {}: got {}", doy, p);
        }
    }

    #[test]
    fn photoperiod_paris_summer_longer_than_winter() {
        // Paris ~ 48.85°N
        let summer = photoperiod_hours(48.85, 172); // 21 juin
        let winter = photoperiod_hours(48.85, 355); // 21 déc
        assert!(summer > 15.0 && summer < 17.0, "summer: {}", summer);
        assert!(winter > 7.0 && winter < 9.0, "winter: {}", winter);
        assert!(summer - winter > 6.0);
    }

    #[test]
    fn photoperiod_polar_night_and_day() {
        // Cercle polaire arctique
        assert_eq!(photoperiod_hours(80.0, 355), 0.0);  // hiver
        assert_eq!(photoperiod_hours(80.0, 172), 24.0); // été
    }
}
