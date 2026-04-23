//! Instance d'une plante installée sur une cellule.
//!
//! Une [`Plant`] vit, accumule de la biomasse, traverse des stades de croissance,
//! peut être stressée (eau, froid, parasites). Elle référence sa [`Species`] par ID.

use serde::{Deserialize, Serialize};

use super::species::SpeciesId;
use super::time::SimDate;

/// Stade phénologique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GrowthStage {
    /// Graine semée, pas encore germée.
    Seed,
    /// Plantule (cotylédons + premières vraies feuilles).
    Seedling,
    /// Croissance végétative active.
    Vegetative,
    /// Floraison.
    Flowering,
    /// Fructification / formation du tubercule.
    Fruiting,
    /// Mature, prête à récolter.
    Mature,
    /// Récoltée (annuelles : fin de cycle ; vivaces : retour à végétatif).
    Harvested,
    /// Morte (gel, sécheresse, maladie).
    Dead,
}

impl GrowthStage {
    pub fn is_alive(self) -> bool {
        !matches!(self, GrowthStage::Dead | GrowthStage::Harvested)
    }
}

/// Plante installée sur une cellule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plant {
    pub species: SpeciesId,
    pub planted_at: SimDate,
    pub stage: GrowthStage,
    /// Degrés-jours cumulés depuis le semis (base 5°C).
    pub gdd_accumulated: f64,
    /// Biomasse aérienne en grammes (ordre de grandeur ; convertie en récolte à maturité).
    pub biomass_g: f64,
    /// État de santé 0..1 (1 = parfait, 0 = mort).
    pub health: f64,
    /// Stress hydrique cumulé sur fenêtre récente (jours sous seuil).
    pub water_stress_days: u16,
    /// Compteur de récoltes effectuées (vivaces / arbres).
    pub harvest_count: u16,
    /// Date de la dernière récolte (utilisé par les vivaces / arbres pour
    /// éviter de re-récolter dans la même année).
    pub last_harvested_at: Option<SimDate>,
    /// Pour les arbres/arbustes plantés "établis" (déjà adultes) : nombre
    /// d'années à ajouter à l'âge calculé pour franchir `years_to_first_harvest`.
    pub years_grown_before_planting: u16,
}

impl Plant {
    pub fn new(species: SpeciesId, planted_at: SimDate) -> Self {
        Self {
            species,
            planted_at,
            stage: GrowthStage::Seed,
            gdd_accumulated: 0.0,
            biomass_g: 0.0,
            health: 1.0,
            water_stress_days: 0,
            harvest_count: 0,
            last_harvested_at: None,
            years_grown_before_planting: 0,
        }
    }

    /// Âge de la plante en jours.
    pub fn age_days(&self, now: SimDate) -> i32 {
        now.days_since(self.planted_at)
    }

    /// Tue la plante.
    pub fn kill(&mut self) {
        self.stage = GrowthStage::Dead;
        self.health = 0.0;
    }
}

/// Calcule les degrés-jours d'un jour avec une température base donnée.
/// GDD = max(0, T_mean − T_base).
pub fn growing_degree_day(temp_mean_c: f64, base_c: f64) -> f64 {
    (temp_mean_c - base_c).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_plant_starts_as_seed() {
        let p = Plant::new(SpeciesId::new("tomato"), SimDate::start());
        assert_eq!(p.stage, GrowthStage::Seed);
        assert_eq!(p.gdd_accumulated, 0.0);
        assert_eq!(p.health, 1.0);
        assert!(p.stage.is_alive());
    }

    #[test]
    fn killing_marks_dead() {
        let mut p = Plant::new(SpeciesId::new("tomato"), SimDate::start());
        p.kill();
        assert_eq!(p.stage, GrowthStage::Dead);
        assert!(!p.stage.is_alive());
    }

    #[test]
    fn gdd_zero_below_base() {
        assert_eq!(growing_degree_day(3.0, 5.0), 0.0);
        assert_eq!(growing_degree_day(5.0, 5.0), 0.0);
        assert_eq!(growing_degree_day(20.0, 5.0), 15.0);
    }

    #[test]
    fn age_days_accumulates() {
        let p = Plant::new(SpeciesId::new("x"), SimDate::start());
        let later = SimDate::new(1, 50);
        assert_eq!(p.age_days(later), 49);
    }
}
