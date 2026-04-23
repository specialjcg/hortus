//! Catalogue d'espèces : fiche botanique et agronomique d'une plante cultivable.
//!
//! Une [`Species`] décrit le *type* (carotte, tomate cerise, pommier...). Les
//! instances plantées sur le jardin sont des [`super::plant::Plant`] qui pointent
//! vers une `Species` via son [`SpeciesId`].

use serde::{Deserialize, Serialize};

use super::nutrition::NutritionProfile;

/// Identifiant unique stable d'une espèce dans le catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SpeciesId(pub String);

impl SpeciesId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Famille botanique — utilisée pour la rotation et les maladies partagées.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Family {
    Solanaceae,    // tomate, pomme de terre, poivron, aubergine
    Brassicaceae,  // chou, radis, navet, roquette
    Fabaceae,      // haricot, pois, fève, lentille (fixateurs N)
    Apiaceae,      // carotte, persil, céleri
    Cucurbitaceae, // courgette, courge, concombre
    Liliaceae,     // ail, oignon, poireau, asperge
    Asteraceae,    // laitue, chicorée, topinambour
    Chenopodiaceae,// betterave, blette, épinard
    Rosaceae,      // pommier, poirier, framboisier, fraisier
    Grossulariaceae,// cassissier, groseillier
    Poaceae,       // céréales, maïs
    Other,
}

impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Family::Solanaceae => "solanacées",
            Family::Brassicaceae => "brassicacées",
            Family::Fabaceae => "fabacées",
            Family::Apiaceae => "apiacées",
            Family::Cucurbitaceae => "cucurbitacées",
            Family::Liliaceae => "liliacées",
            Family::Asteraceae => "astéracées",
            Family::Chenopodiaceae => "chénopodiacées",
            Family::Rosaceae => "rosacées",
            Family::Grossulariaceae => "grossulariacées",
            Family::Poaceae => "graminées",
            Family::Other => "autre",
        }
    }
}

/// Cycle biologique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifeCycle {
    Annual,    // 1 saison
    Biennial,  // 2 saisons (souvent fleur en année 2)
    Perennial, // pluriannuel herbacé
    Shrub,     // arbuste
    Tree,      // arbre
}

/// Strate écologique (forêt-jardin / agroforesterie).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layer {
    Canopy,      // > 8 m
    Tree,        // 3-8 m
    Shrub,       // 1-3 m
    Herbaceous,  // 30 cm - 1 m
    GroundCover, // < 30 cm
    Root,        // sous terre (tubercules)
    Vine,        // grimpant
}

/// Préférences thermiques d'une espèce.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ThermalRange {
    /// Température minimale du sol pour la germination (°C).
    pub germination_min_c: f64,
    /// Tmin air en dessous de laquelle la plante meurt (°C). Plus négatif = plus rustique.
    pub frost_kill_c: f64,
    /// Optimum croissance (°C, air mean).
    pub growth_optimum_c: f64,
    /// Tmax air au-delà de laquelle stress thermique majeur.
    pub heat_stress_c: f64,
}

/// Préférences hydriques (mm équivalent par cellule par semaine, indicatif).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WaterNeeds {
    /// Besoin hebdomadaire optimal (mm).
    pub weekly_optimal_mm: f64,
    /// Sous ce seuil de réserve sol (fraction capacité), stress hydrique.
    pub stress_below: f64,
}

/// Besoins NPK relatifs (échelle 0-3, 0 = pas exigeant, 3 = très gourmand).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NutrientNeeds {
    pub n: f64,
    pub p: f64,
    pub k: f64,
}

/// Profil de croissance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthProfile {
    /// Degrés-jours cumulés (base 5°C) pour atteindre la maturité depuis le semis.
    pub gdd_to_maturity: f64,
    /// Durée typique semis → récolte (jours, indicatif).
    pub days_to_maturity: u16,
    /// Pour les vivaces/arbres : années avant 1ère récolte sérieuse.
    pub years_to_first_harvest: u16,
    /// Photopériode minimale pour fructifier (h, ou 0 si insensible).
    pub photoperiod_min_h: f64,
}

/// Rendement attendu, en grammes par plante par récolte (en conditions optimales).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct YieldProfile {
    /// Rendement par plante en conditions optimales (g par cycle).
    pub g_per_plant_optimal: f64,
    /// Densité recommandée (plantes par m²).
    pub plants_per_m2: f64,
    /// Pour vivaces : nombre typique de récoltes/an une fois en production.
    pub harvests_per_year: u16,
}

/// Profil de conservation : où stocker et combien de temps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageProfile {
    pub fresh_days: u16,        // durée à T° ambiante (cuisine)
    pub cellar_days: u16,       // 8-12°C humide (cellier)
    pub dry_days: u16,           // ambiant sec
    pub frozen_days: u16,        // -18°C
    pub canned_days: u16,        // bocaux stérilisés
    pub lacto_days: u16,         // lactofermentation
}

/// Compagnonnage : effets sur la pression parasitaire et la croissance des voisins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Companionship {
    /// Espèces favorables (donnent un bonus si plantées en voisinage).
    pub friends: Vec<SpeciesId>,
    /// Espèces antagonistes (malus si voisinage).
    pub foes: Vec<SpeciesId>,
}

/// Fenêtres de calendrier (jours julien 1..=365).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CalendarWindow {
    pub doy_start: u16,
    pub doy_end: u16,
}

impl CalendarWindow {
    pub fn contains(self, doy: u16) -> bool {
        if self.doy_start <= self.doy_end {
            (self.doy_start..=self.doy_end).contains(&doy)
        } else {
            // Fenêtre qui chevauche le 1er janvier.
            doy >= self.doy_start || doy <= self.doy_end
        }
    }
}

/// Fiche complète d'une espèce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Species {
    pub id: SpeciesId,
    pub name_fr: String,
    pub name_latin: String,
    pub family: Family,
    pub life_cycle: LifeCycle,
    pub layer: Layer,

    pub thermal: ThermalRange,
    pub water: WaterNeeds,
    pub nutrients: NutrientNeeds,
    pub growth: GrowthProfile,
    pub yields: YieldProfile,
    pub storage: StorageProfile,
    pub nutrition: NutritionProfile,
    pub companions: Companionship,

    /// Fenêtre de semis recommandée.
    pub sowing_window: CalendarWindow,
    /// Fenêtre de récolte typique.
    pub harvest_window: CalendarWindow,

    /// pH optimal (centre, demi-largeur).
    pub ph_optimum: (f64, f64),

    /// Fixe l'azote atmosphérique (légumineuses → bonus N voisins).
    pub nitrogen_fixer: bool,
    /// Allélopathique (inhibe la croissance des voisins).
    pub allelopathic: bool,
    /// Vivace mellifère / attire pollinisateurs et auxiliaires.
    pub beneficial_for_pollinators: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_window_normal() {
        let w = CalendarWindow { doy_start: 90, doy_end: 180 };
        assert!(w.contains(90));
        assert!(w.contains(180));
        assert!(w.contains(135));
        assert!(!w.contains(89));
        assert!(!w.contains(181));
    }

    #[test]
    fn calendar_window_wraps_year() {
        // ail : semis octobre → mars
        let w = CalendarWindow { doy_start: 274, doy_end: 90 };
        assert!(w.contains(274));
        assert!(w.contains(365));
        assert!(w.contains(1));
        assert!(w.contains(90));
        assert!(!w.contains(150));
        assert!(!w.contains(273));
    }

    #[test]
    fn family_names_localized() {
        assert_eq!(Family::Fabaceae.name(), "fabacées");
        assert_eq!(Family::Solanaceae.name(), "solanacées");
    }
}
