//! Sol et chimie agronomique.
//!
//! Modèle simplifié mais cohérent :
//! - NPK en échelle 0..=10 (0 = carencé, 5 = adéquat, 10 = très riche)
//! - Matière organique en % (0..=15, optimal autour de 4-6)
//! - pH en échelle réelle (3.5..=9.0), optimum dépend de la culture
//! - Humidité en fraction de capacité au champ (0..=1.2 ; >1 = saturé)
//! - Température sol distincte de l'air (inertie thermique)

use serde::{Deserialize, Serialize};

/// Type de sol — détermine capacité de rétention, drainage, fertilité initiale, pH naturel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoilType {
    /// Sol argileux : rétention forte, lent à se réchauffer, lourd à travailler.
    Clay,
    /// Sol limoneux : équilibré, idéal potager.
    Loam,
    /// Sol sablonneux : drainant, se réchauffe vite, peu fertile.
    Sand,
    /// Sol calcaire : pH élevé, problèmes carences fer.
    Limestone,
    /// Sol tourbeux : MO élevée, rétention forte, acide.
    Peat,
}

impl SoilType {
    pub fn name(self) -> &'static str {
        match self {
            SoilType::Clay => "argileux",
            SoilType::Loam => "limoneux",
            SoilType::Sand => "sablonneux",
            SoilType::Limestone => "calcaire",
            SoilType::Peat => "tourbeux",
        }
    }

    /// Capacité au champ (mm d'eau retenue dans la zone racinaire ~30cm).
    /// Sable retient peu, argile/tourbe beaucoup.
    pub fn field_capacity_mm(self) -> f64 {
        match self {
            SoilType::Sand => 30.0,
            SoilType::Loam => 60.0,
            SoilType::Clay => 90.0,
            SoilType::Limestone => 50.0,
            SoilType::Peat => 110.0,
        }
    }

    /// Conductivité thermique relative (sable se réchauffe vite, tourbe lentement).
    pub fn thermal_conductivity(self) -> f64 {
        match self {
            SoilType::Sand => 1.3,
            SoilType::Loam => 1.0,
            SoilType::Clay => 0.7,
            SoilType::Limestone => 0.9,
            SoilType::Peat => 0.5,
        }
    }

    /// pH naturel typique de ce type de sol.
    pub fn natural_ph(self) -> f64 {
        match self {
            SoilType::Sand => 6.0,
            SoilType::Loam => 6.7,
            SoilType::Clay => 6.5,
            SoilType::Limestone => 7.8,
            SoilType::Peat => 5.0,
        }
    }

    /// Réserves NPK initiales (échelle 0-10).
    pub fn baseline_npk(self) -> (f64, f64, f64) {
        match self {
            SoilType::Sand => (2.0, 2.0, 2.0),
            SoilType::Loam => (5.0, 5.0, 5.0),
            SoilType::Clay => (4.5, 5.0, 5.5),
            SoilType::Limestone => (4.0, 4.0, 4.0),
            SoilType::Peat => (3.0, 3.0, 3.5),
        }
    }

    /// Matière organique typique en % (poids sec).
    pub fn baseline_organic_matter(self) -> f64 {
        match self {
            SoilType::Sand => 1.0,
            SoilType::Loam => 3.0,
            SoilType::Clay => 2.5,
            SoilType::Limestone => 2.0,
            SoilType::Peat => 12.0,
        }
    }
}

/// Couverture de sol — influence évaporation, érosion, T° sol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroundCover {
    Bare,    // sol nu (pertes max)
    Mulch,   // paillage organique
    Living,  // engrais vert / couvre-sol vivant
    Crop,    // culture en place
}

/// État chimique et physique d'une cellule de jardin (1 unité = surface définie par la grille).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellState {
    pub soil_type: SoilType,
    /// Azote disponible (échelle 0-10).
    pub n: f64,
    /// Phosphore disponible.
    pub p: f64,
    /// Potassium disponible.
    pub k: f64,
    /// Matière organique (% poids sec, typique 1-10%).
    pub organic_matter_pct: f64,
    /// pH (3.5 à 9.0).
    pub ph: f64,
    /// Eau disponible (mm équivalent hauteur), bornée par capacité au champ.
    pub water_mm: f64,
    /// Température du sol à 10 cm (°C).
    pub soil_temp_c: f64,
    /// Pression parasitaire (0-10) — augmente avec monoculture, baisse avec diversité/auxiliaires.
    pub pest_pressure: f64,
    /// Couverture actuelle.
    pub cover: GroundCover,
    /// Historique des familles cultivées sur les N dernières saisons (pour rotation).
    pub recent_families: Vec<String>,
}

impl CellState {
    pub fn new(soil: SoilType) -> Self {
        let (n, p, k) = soil.baseline_npk();
        Self {
            soil_type: soil,
            n,
            p,
            k,
            organic_matter_pct: soil.baseline_organic_matter(),
            ph: soil.natural_ph(),
            water_mm: soil.field_capacity_mm() * 0.6,
            soil_temp_c: 10.0,
            pest_pressure: 0.0,
            cover: GroundCover::Bare,
            recent_families: Vec::new(),
        }
    }

    /// Apport d'eau (pluie ou arrosage), bornée par capacité au champ.
    /// L'excédent est considéré "lessivé" et peut emporter une fraction des nitrates.
    pub fn add_water(&mut self, mm: f64) -> WaterEvent {
        let capacity = self.soil_type.field_capacity_mm();
        let new_total = self.water_mm + mm;
        if new_total <= capacity {
            self.water_mm = new_total;
            return WaterEvent { runoff_mm: 0.0, n_leached: 0.0 };
        }
        let excess = new_total - capacity;
        self.water_mm = capacity;
        // Lessivage : l'excédent emporte ~3 % de l'azote disponible par 10 mm.
        let leach_factor = (excess / 10.0).min(1.0) * 0.03;
        let n_leached = self.n * leach_factor;
        self.n = (self.n - n_leached).max(0.0);
        WaterEvent { runoff_mm: excess, n_leached }
    }

    /// Évaporation potentielle (mm) appliquée à l'eau du sol, modulée par couverture.
    pub fn evaporate(&mut self, etp_mm: f64) {
        let factor = match self.cover {
            GroundCover::Bare => 1.0,
            GroundCover::Mulch => 0.4,
            GroundCover::Living => 0.6,
            GroundCover::Crop => 0.7, // la transpiration est gérée côté plante
        };
        self.water_mm = (self.water_mm - etp_mm * factor).max(0.0);
    }

    /// Décompose lentement la MO en libérant azote (minéralisation).
    /// 1 % de MO → libère ~20 kg N/ha/an (très approximatif).
    /// On convertit vers l'échelle 0-10 par jour (~ + 0.001 N par % MO).
    pub fn mineralize(&mut self, soil_temp_c: f64) {
        let temp_factor = ((soil_temp_c - 5.0) / 25.0).clamp(0.0, 1.0);
        let release = 0.001 * self.organic_matter_pct * temp_factor;
        self.n = (self.n + release).min(10.0);
        // La MO se consomme très lentement (~0.05 %/an de la MO existante).
        self.organic_matter_pct = (self.organic_matter_pct - release * 0.5).max(0.0);
    }
}

/// Conséquences d'un apport d'eau.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WaterEvent {
    pub runoff_mm: f64,
    pub n_leached: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loam_is_balanced_default() {
        let c = CellState::new(SoilType::Loam);
        assert_eq!(c.n, 5.0);
        assert!(c.ph > 6.0 && c.ph < 7.5);
        assert!(c.water_mm > 0.0);
    }

    #[test]
    fn add_water_caps_at_field_capacity() {
        let mut c = CellState::new(SoilType::Sand);
        c.water_mm = 10.0;
        let ev = c.add_water(50.0); // sable cap = 30 mm
        assert_eq!(c.water_mm, c.soil_type.field_capacity_mm());
        assert!(ev.runoff_mm > 25.0);
        assert!(ev.n_leached > 0.0);
    }

    #[test]
    fn add_water_below_capacity_no_runoff() {
        let mut c = CellState::new(SoilType::Loam);
        let n_before = c.n;
        let ev = c.add_water(5.0);
        assert_eq!(ev.runoff_mm, 0.0);
        assert_eq!(ev.n_leached, 0.0);
        assert_eq!(c.n, n_before);
    }

    #[test]
    fn evaporation_reduced_by_mulch() {
        let mut bare = CellState::new(SoilType::Loam);
        let mut mulched = CellState::new(SoilType::Loam);
        mulched.cover = GroundCover::Mulch;
        bare.evaporate(5.0);
        mulched.evaporate(5.0);
        assert!(mulched.water_mm > bare.water_mm);
    }

    #[test]
    fn mineralization_releases_n_warm_only() {
        let mut warm = CellState::new(SoilType::Loam);
        let mut cold = CellState::new(SoilType::Loam);
        warm.organic_matter_pct = 5.0;
        cold.organic_matter_pct = 5.0;
        warm.n = 0.0;
        cold.n = 0.0;
        for _ in 0..30 {
            warm.mineralize(20.0);
            cold.mineralize(2.0);
        }
        assert!(warm.n > cold.n, "warm: {}, cold: {}", warm.n, cold.n);
    }
}
