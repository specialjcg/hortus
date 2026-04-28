//! Fiche espèce pour un calendrier de plantations.
//!
//! Modèle léger : tout ce qu'il faut pour un calendrier de jardinage,
//! rien de plus. Les données sont chargées depuis un fichier JSON.

use serde::{Deserialize, Serialize};

/// Fenêtre calendaire en jours julien (1..=365). Peut enjamber le 1er janvier
/// (`doy_start > doy_end`).
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
            doy >= self.doy_start || doy <= self.doy_end
        }
    }
}

/// Famille botanique — utile pour les rotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Family {
    Solanaceae,
    Brassicaceae,
    Fabaceae,
    Apiaceae,
    Cucurbitaceae,
    Alliaceae,
    Asteraceae,
    Amaranthaceae,
    Rosaceae,
    Grossulariaceae,
    Ericaceae,
    Juglandaceae,
    Lamiaceae,
    Poaceae,
    Chenopodiaceae,
    Polygonaceae,
    Malvaceae,
    Apocynaceae,
    Moraceae,
    Vitaceae,
    Oleaceae,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifeCycle {
    Annual,
    Biennial,
    Perennial,
    Shrub,
    Tree,
}

/// Grande catégorie pour filtres UX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    LeafyVegetable,
    FruitVegetable,
    RootVegetable,
    Legume,
    Herb,
    Berry,
    TreeFruit,
    Nut,
    Grain,
    Mushroom,
    Ornamental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Beginner,
    Intermediate,
    Advanced,
}

/// Fiche d'une espèce. Tout ce qu'il faut pour un calendrier + quelques notes pratiques.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Species {
    pub id: String,
    pub name_fr: String,
    pub name_latin: String,
    pub family: Family,
    pub life_cycle: LifeCycle,
    pub category: Category,
    pub difficulty: Difficulty,
    /// Fenêtre de semis sous abri / intérieur (optionnel).
    pub indoor_sow: Option<CalendarWindow>,
    /// Fenêtre de semis direct en pleine terre (optionnel).
    pub direct_sow: Option<CalendarWindow>,
    /// Fenêtre de repiquage (optionnel, pertinent si indoor_sow défini).
    pub transplant: Option<CalendarWindow>,
    /// Fenêtre de récolte.
    pub harvest: CalendarWindow,
    /// Profondeur de semis (cm).
    pub depth_cm: f32,
    /// Espacement entre plants (cm).
    pub spacing_cm: u16,
    /// Nombre de jours typique semis → récolte (indicatif).
    pub days_to_harvest: u16,
    /// Notes libres : exposition, arrosage, compagnonnage, astuces.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Espèces compagnes favorables (ids).
    #[serde(default)]
    pub friends: Vec<String>,
    /// Espèces antagonistes (ids).
    #[serde(default)]
    pub foes: Vec<String>,
}

impl Species {
    /// Charge toutes les espèces depuis un fichier JSON.
    pub fn load_from_json(path: &str) -> Result<Vec<Species>, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("lecture {path} : {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("parse JSON : {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_contains_normal() {
        let w = CalendarWindow { doy_start: 60, doy_end: 150 };
        assert!(w.contains(100));
        assert!(!w.contains(10));
    }

    #[test]
    fn window_contains_wraps() {
        // nov-mars
        let w = CalendarWindow { doy_start: 305, doy_end: 90 };
        assert!(w.contains(350));
        assert!(w.contains(10));
        assert!(!w.contains(200));
    }

    #[test]
    fn load_species_from_real_json() {
        let species = Species::load_from_json("data/species.json")
            .expect("data/species.json must be valid");
        assert!(species.len() >= 30, "expected ≥30 species, got {}", species.len());
    }
}
