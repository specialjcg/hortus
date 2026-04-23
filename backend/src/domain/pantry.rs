//! Garde-manger : compartiments de stockage avec dégradation temporelle.
//!
//! Chaque [`FoodItem`] est rangé dans un [`StorageCompartment`] et porte une
//! durée de vie (en jours) au-delà de laquelle il est perdu. Le foyer pioche
//! d'abord dans le frais, puis cellier/sec, puis lacto/conserve/congel.

use serde::{Deserialize, Serialize};

use super::nutrition::NutritionIntake;
use super::species::{Species, SpeciesId, StorageProfile};
use super::time::SimDate;

/// Méthode de stockage / transformation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageCompartment {
    Fresh,    // récolte du jour, T° ambiante (durée courte)
    Cellar,   // 8-12°C humide
    Dry,      // séché, ambiant sec
    Frozen,   // -18°C
    Canned,   // bocaux stérilisés
    Lacto,    // lactofermentation
}

impl StorageCompartment {
    pub fn name(self) -> &'static str {
        match self {
            StorageCompartment::Fresh => "frais",
            StorageCompartment::Cellar => "cellier",
            StorageCompartment::Dry => "sec",
            StorageCompartment::Frozen => "congelé",
            StorageCompartment::Canned => "conserve",
            StorageCompartment::Lacto => "lactofermenté",
        }
    }

    /// Durée de conservation typique pour cette espèce dans ce compartiment.
    pub fn shelf_life_days(self, profile: &StorageProfile) -> u16 {
        match self {
            StorageCompartment::Fresh => profile.fresh_days,
            StorageCompartment::Cellar => profile.cellar_days,
            StorageCompartment::Dry => profile.dry_days,
            StorageCompartment::Frozen => profile.frozen_days,
            StorageCompartment::Canned => profile.canned_days,
            StorageCompartment::Lacto => profile.lacto_days,
        }
    }

    /// Pertes en masse lors de la transformation (fraction perdue).
    /// Lacto/conserve = pertes au coupe/épluchage et concentration.
    pub fn processing_loss(self) -> f64 {
        match self {
            StorageCompartment::Fresh | StorageCompartment::Cellar => 0.0,
            StorageCompartment::Dry => 0.85,    // perd l'eau
            StorageCompartment::Frozen => 0.10,
            StorageCompartment::Canned => 0.20,
            StorageCompartment::Lacto => 0.15,
        }
    }
}

/// Lot d'un aliment stocké.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoodItem {
    pub species: SpeciesId,
    pub compartment: StorageCompartment,
    pub mass_g: f64,
    pub stored_on: SimDate,
    /// Date au-delà de laquelle le lot est considéré perdu.
    pub best_before: SimDate,
}

impl FoodItem {
    pub fn new(
        species: SpeciesId,
        compartment: StorageCompartment,
        mass_g: f64,
        stored_on: SimDate,
        shelf_life_days: u16,
    ) -> Self {
        let best_before = stored_on.advance(shelf_life_days as u32);
        Self { species, compartment, mass_g, stored_on, best_before }
    }

    pub fn is_expired(&self, today: SimDate) -> bool {
        today.days_since(self.best_before) > 0
    }
}

/// Garde-manger.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pantry {
    pub items: Vec<FoodItem>,
}

impl Pantry {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Range une récolte fraîche dans le compartiment frais.
    pub fn store_fresh_harvest(&mut self, species: &Species, mass_g: f64, today: SimDate) {
        if mass_g <= 0.0 { return; }
        let item = FoodItem::new(
            species.id.clone(),
            StorageCompartment::Fresh,
            mass_g,
            today,
            species.storage.fresh_days,
        );
        self.items.push(item);
    }

    /// Transforme un lot frais (ou autre) en méthode de conservation longue.
    /// Renvoie la masse réellement stockée après pertes.
    pub fn process(
        &mut self,
        species: &Species,
        from: StorageCompartment,
        to: StorageCompartment,
        mass_g: f64,
        today: SimDate,
    ) -> f64 {
        let available = self.take_from(species, from, mass_g);
        if available <= 0.0 { return 0.0; }
        let kept = available * (1.0 - to.processing_loss());
        let item = FoodItem::new(
            species.id.clone(),
            to,
            kept,
            today,
            to.shelf_life_days(&species.storage),
        );
        self.items.push(item);
        kept
    }

    /// Prélève au plus `mass_g` d'une espèce/compartiment. Renvoie ce qui a été pris.
    pub fn take_from(
        &mut self,
        species: &Species,
        compartment: StorageCompartment,
        mass_g: f64,
    ) -> f64 {
        let mut remaining = mass_g;
        let mut taken = 0.0;
        // Itère du plus ancien au plus récent (FIFO).
        let mut i = 0;
        while i < self.items.len() && remaining > 0.0 {
            let it = &mut self.items[i];
            if it.species == species.id && it.compartment == compartment {
                let take = remaining.min(it.mass_g);
                it.mass_g -= take;
                taken += take;
                remaining -= take;
                if it.mass_g <= 1e-6 {
                    self.items.swap_remove(i);
                    continue;
                }
            }
            i += 1;
        }
        taken
    }

    /// Élimine les lots périmés. Renvoie la masse totale perdue.
    pub fn purge_expired(&mut self, today: SimDate) -> f64 {
        let mut lost = 0.0;
        self.items.retain(|it| {
            if it.is_expired(today) {
                lost += it.mass_g;
                false
            } else {
                true
            }
        });
        lost
    }

    /// Masse totale stockée d'une espèce (tous compartiments confondus).
    pub fn total_mass_of(&self, species: &SpeciesId) -> f64 {
        self.items.iter().filter(|it| &it.species == species).map(|it| it.mass_g).sum()
    }

    /// Masse totale stockée tous aliments.
    pub fn total_mass(&self) -> f64 {
        self.items.iter().map(|it| it.mass_g).sum()
    }

    /// Liste des espèces actuellement disponibles.
    pub fn available_species(&self) -> Vec<SpeciesId> {
        let mut v: Vec<SpeciesId> = self.items.iter().map(|it| it.species.clone()).collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Apport nutritionnel d'une masse d'un aliment donné, sans réellement la consommer.
pub fn nutrition_of(species: &Species, mass_g: f64) -> NutritionIntake {
    species.nutrition.for_mass_g(mass_g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::nutrition::NutritionProfile;
    use crate::domain::species::{
        CalendarWindow, Companionship, Family, GrowthProfile, Layer, LifeCycle, NutrientNeeds,
        Species, SpeciesId, ThermalRange, WaterNeeds, YieldProfile,
    };

    fn dummy_species(id: &str) -> Species {
        Species {
            id: SpeciesId::new(id),
            name_fr: id.into(),
            name_latin: id.into(),
            family: Family::Other,
            life_cycle: LifeCycle::Annual,
            layer: Layer::Herbaceous,
            thermal: ThermalRange {
                germination_min_c: 5.0,
                frost_kill_c: -2.0,
                growth_optimum_c: 20.0,
                heat_stress_c: 35.0,
            },
            water: WaterNeeds { weekly_optimal_mm: 25.0, stress_below: 0.3 },
            nutrients: NutrientNeeds { n: 1.0, p: 1.0, k: 1.0 },
            growth: GrowthProfile {
                gdd_to_maturity: 1000.0,
                days_to_maturity: 90,
                years_to_first_harvest: 0,
                photoperiod_min_h: 0.0,
            },
            yields: YieldProfile {
                g_per_plant_optimal: 200.0,
                plants_per_m2: 4.0,
                harvests_per_year: 1,
            },
            storage: StorageProfile {
                fresh_days: 7,
                cellar_days: 60,
                dry_days: 365,
                frozen_days: 365,
                canned_days: 730,
                lacto_days: 180,
            },
            nutrition: NutritionProfile {
                kcal: 50.0, protein_g: 1.0, lipid_g: 0.1, carb_g: 12.0, fiber_g: 2.0,
                vit_a_ug: 0.0, vit_c_mg: 10.0, vit_e_mg: 0.0, vit_k_ug: 0.0, vit_b9_ug: 0.0,
                iron_mg: 0.5, calcium_mg: 10.0, magnesium_mg: 10.0, potassium_mg: 200.0, zinc_mg: 0.2,
            },
            companions: Companionship::default(),
            sowing_window: CalendarWindow { doy_start: 60, doy_end: 180 },
            harvest_window: CalendarWindow { doy_start: 180, doy_end: 300 },
            ph_optimum: (6.5, 0.5),
            nitrogen_fixer: false,
            allelopathic: false,
            beneficial_for_pollinators: false,
        }
    }

    #[test]
    fn store_then_take() {
        let s = dummy_species("x");
        let mut p = Pantry::new();
        p.store_fresh_harvest(&s, 500.0, SimDate::start());
        let taken = p.take_from(&s, StorageCompartment::Fresh, 200.0);
        assert_eq!(taken, 200.0);
        assert!((p.total_mass() - 300.0).abs() < 1e-9);
    }

    #[test]
    fn fifo_consumption() {
        let s = dummy_species("x");
        let mut p = Pantry::new();
        p.store_fresh_harvest(&s, 100.0, SimDate::start());
        p.store_fresh_harvest(&s, 100.0, SimDate::start().advance(1));
        let taken = p.take_from(&s, StorageCompartment::Fresh, 150.0);
        assert_eq!(taken, 150.0);
        assert!((p.total_mass() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn expired_items_are_purged() {
        let s = dummy_species("x"); // fresh_days = 7
        let mut p = Pantry::new();
        p.store_fresh_harvest(&s, 200.0, SimDate::start());
        // 10 jours plus tard → périmé
        let lost = p.purge_expired(SimDate::start().advance(10));
        assert_eq!(lost, 200.0);
        assert_eq!(p.total_mass(), 0.0);
    }

    #[test]
    fn processing_loses_mass_but_extends_shelf_life() {
        let s = dummy_species("x");
        let mut p = Pantry::new();
        p.store_fresh_harvest(&s, 1000.0, SimDate::start());
        let kept = p.process(
            &s,
            StorageCompartment::Fresh,
            StorageCompartment::Dry,
            1000.0,
            SimDate::start(),
        );
        // dry processing_loss = 85% → kept 150 g
        assert!((kept - 150.0).abs() < 1e-6);
        // 1 an plus tard, encore là car dry_days = 365
        let lost = p.purge_expired(SimDate::start().advance(364));
        assert_eq!(lost, 0.0);
    }

    #[test]
    fn nutrition_of_scales_correctly() {
        let s = dummy_species("x");
        let n = nutrition_of(&s, 200.0);
        assert!((n.kcal - 100.0).abs() < 1e-9);
    }
}
