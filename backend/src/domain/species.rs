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
    Boraginaceae,
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
    GreenManure,
}

/// Strate en hauteur à maturité, pour l'étagement des associations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeightClass {
    /// > 1 m (tomate tuteurée, artichaut, arbres).
    Tall,
    /// 0,3 – 1 m.
    Medium,
    /// < 0,3 m ou rampant (salades, radis, fraisier, courges coureuses).
    Ground,
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
    /// Récolte échelonnée : le plant continue de produire après une récolte
    /// (tomate, courgette…) et ne disparaît qu'à l'arrachage.
    #[serde(default)]
    pub multi_harvest: bool,
    /// Strate à maturité ; `None` → défaut selon cycle/catégorie (voir `height`).
    #[serde(default)]
    pub height_class: Option<HeightClass>,
    /// Engrais vert : couvre le sol entre deux cultures, fauché puis laissé en surface.
    #[serde(default)]
    pub green_manure: bool,
}

impl Species {
    /// Charge toutes les espèces depuis un fichier JSON.
    pub fn load_from_json(path: &str) -> Result<Vec<Species>, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("lecture {path} : {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("parse JSON : {e}"))
    }

    /// Occupe la place plusieurs années : emprise fixe pour le planificateur.
    pub fn is_perennial(&self) -> bool {
        matches!(self.life_cycle, LifeCycle::Perennial | LifeCycle::Shrub | LifeCycle::Tree)
    }

    /// Légumineuse : fixe l'azote de l'air (nodosités).
    pub fn fixes_nitrogen(&self) -> bool {
        self.family == Family::Fabaceae
    }

    pub fn height(&self) -> HeightClass {
        self.height_class.unwrap_or(match self.life_cycle {
            LifeCycle::Tree | LifeCycle::Shrub => HeightClass::Tall,
            _ => HeightClass::Medium,
        })
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

    fn real() -> Vec<Species> {
        Species::load_from_json("data/species.json").expect("species.json valide")
    }

    fn by_id<'a>(all: &'a [Species], id: &str) -> &'a Species {
        all.iter().find(|s| s.id == id).unwrap_or_else(|| panic!("espèce {id} absente"))
    }

    #[test]
    fn green_manures_present_and_flagged() {
        let all = real();
        for id in ["phacelia", "white_mustard", "crimson_clover", "winter_rye", "common_vetch"] {
            let s = by_id(&all, id);
            assert!(s.green_manure, "{id} doit être engrais vert");
            assert_eq!(s.category, Category::GreenManure);
        }
    }

    #[test]
    fn nitrogen_fixers_are_fabaceae() {
        let all = real();
        assert!(by_id(&all, "common_vetch").fixes_nitrogen());
        assert!(by_id(&all, "pea").fixes_nitrogen());
        assert!(!by_id(&all, "tomato").fixes_nitrogen());
    }

    #[test]
    fn perennial_from_life_cycle() {
        let all = real();
        assert!(by_id(&all, "apple_tree").is_perennial());
        assert!(by_id(&all, "rhubarb").is_perennial());
        assert!(!by_id(&all, "lettuce").is_perennial());
    }

    #[test]
    fn height_class_explicit_or_default() {
        let all = real();
        assert_eq!(by_id(&all, "tomato").height(), HeightClass::Tall);
        assert_eq!(by_id(&all, "lettuce").height(), HeightClass::Ground);
        assert_eq!(by_id(&all, "walnut_tree").height(), HeightClass::Tall);
        assert_eq!(by_id(&all, "strawberry").height(), HeightClass::Ground);
    }

    #[test]
    fn every_species_has_explicit_height() {
        let missing: Vec<_> = real().into_iter().filter(|s| s.height_class.is_none()).map(|s| s.id).collect();
        assert!(missing.is_empty(), "height_class manquant : {missing:?}");
    }
}
