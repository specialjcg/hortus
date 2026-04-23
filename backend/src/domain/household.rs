//! Foyer : besoins quotidiens et consommation.
//!
//! Le foyer pioche chaque jour dans le [`Pantry`] selon une stratégie simple
//! (frais → cellier → autres compartiments → arrêt). On calcule le bilan
//! nutritionnel obtenu et on flagge les jours en déficit.

use serde::{Deserialize, Serialize};

use super::nutrition::{DailyBalance, DailyRequirement, NutritionIntake};
use super::pantry::{nutrition_of, Pantry, StorageCompartment};
use super::species::Species;
use super::time::SimDate;
use std::collections::HashMap;

/// Composition d'un foyer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Household {
    pub adults: u8,
    pub children: u8,
    /// Apport journalier requis (issu de DailyRequirement scaled).
    pub requirement: DailyRequirement,
}

impl Household {
    pub fn solo_adult() -> Self {
        let r = DailyRequirement::adult();
        Self { adults: 1, children: 0, requirement: r }
    }

    pub fn family(adults: u8, children: u8) -> Self {
        // 1 enfant ≈ 0.6 équivalent adulte (approx pour besoins kcal moyen).
        let n = adults as f64 + 0.6 * children as f64;
        let r = DailyRequirement::adult().scaled(n);
        Self { adults, children, requirement: r }
    }

    pub fn equivalent_adults(&self) -> f64 {
        self.adults as f64 + 0.6 * self.children as f64
    }
}

/// Politique de prélèvement journalier : ordre des compartiments, masse cible/personne.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumptionPolicy {
    /// Ordre dans lequel piocher.
    pub compartment_order: Vec<StorageCompartment>,
    /// Masse alimentaire totale cible par personne adulte/jour (g de produits comestibles).
    pub target_mass_per_adult_g: f64,
}

impl Default for ConsumptionPolicy {
    fn default() -> Self {
        Self {
            compartment_order: vec![
                StorageCompartment::Fresh,
                StorageCompartment::Cellar,
                StorageCompartment::Dry,
                StorageCompartment::Lacto,
                StorageCompartment::Canned,
                StorageCompartment::Frozen,
            ],
            // 1.5 kg de produits frais/jour est la borne haute pour un régime
            // 100% végétal couvrant les besoins caloriques. On laissera la
            // consommation s'arrêter dès que les calories sont couvertes.
            target_mass_per_adult_g: 1500.0,
        }
    }
}

/// Bilan d'une journée alimentaire pour un foyer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyConsumptionReport {
    pub date: SimDate,
    pub balance: DailyBalance,
    /// Masse totale consommée (g, tous aliments).
    pub mass_consumed_g: f64,
    /// Détail par espèce.
    pub by_species: Vec<(String, f64)>,
}

/// Effectue la consommation d'une journée : pioche dans le pantry, calcule l'apport,
/// renvoie le bilan + mise à jour du garde-manger.
pub fn consume_day(
    today: SimDate,
    household: &Household,
    pantry: &mut Pantry,
    catalog: &HashMap<String, Species>,
    policy: &ConsumptionPolicy,
) -> DailyConsumptionReport {
    let target_mass_total = policy.target_mass_per_adult_g * household.equivalent_adults();
    let target_kcal = household.requirement.kcal;

    let mut intake = NutritionIntake::default();
    let mut by_species_map: HashMap<String, f64> = HashMap::new();
    let mut consumed_total = 0.0;

    'outer: for compartment in &policy.compartment_order {
        // Boucle tant qu'il reste à consommer dans ce compartiment.
        loop {
            // Trouver le lot le plus ancien dans ce compartiment.
            let candidate = pantry
                .items
                .iter()
                .enumerate()
                .filter(|(_, it)| it.compartment == *compartment && it.mass_g > 0.0)
                .min_by_key(|(_, it)| it.stored_on.days_since(SimDate::start()));
            let Some((_, it)) = candidate else { break };
            let species_id = it.species.clone();
            let Some(species) = catalog.get(species_id.as_str()) else {
                // Espèce inconnue → on jette le lot pour éviter de boucler.
                pantry.take_from(
                    &dummy_species_for_id(&species_id),
                    *compartment,
                    f64::INFINITY,
                );
                continue;
            };
            // Cible de prise : ce qui reste pour atteindre la cible massique
            // OU ce qu'il faut pour atteindre kcal cible (le plus petit).
            let remaining_mass = (target_mass_total - consumed_total).max(0.0);
            let remaining_kcal = (target_kcal - intake.kcal).max(0.0);
            // On suppose que 100 g donne `species.nutrition.kcal` kcal.
            let mass_for_kcal = if species.nutrition.kcal > 0.1 {
                100.0 * remaining_kcal / species.nutrition.kcal
            } else {
                remaining_mass
            };
            let want = remaining_mass.min(mass_for_kcal).max(0.0);
            if want < 1.0 {
                break 'outer;
            }
            // Cap par lot : on ne mange pas plus de 400 g d'une même espèce
            // par "service" pour simuler la diversité.
            let chunk = want.min(400.0);
            let taken = pantry.take_from(species, *compartment, chunk);
            if taken <= 0.0 { break; }
            intake.add(&nutrition_of(species, taken));
            *by_species_map
                .entry(species.name_fr.clone())
                .or_insert(0.0) += taken;
            consumed_total += taken;
            if intake.kcal >= target_kcal && consumed_total >= target_mass_total * 0.8 {
                break 'outer;
            }
        }
    }

    let balance = DailyBalance::compute(intake, household.requirement.clone());
    let mut by_species: Vec<(String, f64)> = by_species_map.into_iter().collect();
    by_species.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    DailyConsumptionReport {
        date: today,
        balance,
        mass_consumed_g: consumed_total,
        by_species,
    }
}

// --- helper interne pour purger un lot orphelin ---
fn dummy_species_for_id(id: &super::species::SpeciesId) -> Species {
    use super::nutrition::NutritionProfile;
    use super::species::*;
    Species {
        id: id.clone(),
        name_fr: String::new(),
        name_latin: String::new(),
        family: Family::Other,
        life_cycle: LifeCycle::Annual,
        layer: Layer::Herbaceous,
        thermal: ThermalRange {
            germination_min_c: 0.0, frost_kill_c: -10.0,
            growth_optimum_c: 20.0, heat_stress_c: 35.0,
        },
        water: WaterNeeds { weekly_optimal_mm: 0.0, stress_below: 0.0 },
        nutrients: NutrientNeeds { n: 0.0, p: 0.0, k: 0.0 },
        growth: GrowthProfile {
            gdd_to_maturity: 0.0, days_to_maturity: 0,
            years_to_first_harvest: 0, photoperiod_min_h: 0.0,
        },
        yields: YieldProfile { g_per_plant_optimal: 0.0, plants_per_m2: 0.0, harvests_per_year: 0 },
        storage: StorageProfile {
            fresh_days: 0, cellar_days: 0, dry_days: 0,
            frozen_days: 0, canned_days: 0, lacto_days: 0,
        },
        nutrition: NutritionProfile {
            kcal: 0.0, protein_g: 0.0, lipid_g: 0.0, carb_g: 0.0, fiber_g: 0.0,
            vit_a_ug: 0.0, vit_c_mg: 0.0, vit_e_mg: 0.0, vit_k_ug: 0.0, vit_b9_ug: 0.0,
            iron_mg: 0.0, calcium_mg: 0.0, magnesium_mg: 0.0, potassium_mg: 0.0, zinc_mg: 0.0,
        },
        companions: Companionship::default(),
        sowing_window: CalendarWindow { doy_start: 1, doy_end: 365 },
        harvest_window: CalendarWindow { doy_start: 1, doy_end: 365 },
        ph_optimum: (6.5, 1.0),
        nitrogen_fixer: false,
        allelopathic: false,
        beneficial_for_pollinators: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::catalog;

    #[test]
    fn solo_adult_requires_2200_kcal() {
        let h = Household::solo_adult();
        assert_eq!(h.requirement.kcal, 2200.0);
    }

    #[test]
    fn family_scales_requirement() {
        let h = Household::family(2, 2);
        // 2 + 0.6*2 = 3.2 adult-eq → 2200 * 3.2 = 7040 kcal
        assert!((h.requirement.kcal - 7040.0).abs() < 1e-6);
    }

    #[test]
    fn empty_pantry_produces_full_deficit() {
        let h = Household::solo_adult();
        let mut p = Pantry::new();
        let cat = catalog::default_catalog();
        let report = consume_day(SimDate::start(), &h, &mut p, &cat, &ConsumptionPolicy::default());
        assert_eq!(report.mass_consumed_g, 0.0);
        assert!(!report.balance.fully_covered);
    }
}
