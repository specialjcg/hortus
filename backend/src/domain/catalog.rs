//! Catalogue d'espèces du MVP.
//!
//! 5 espèces pilotes choisies pour couvrir les axes nutritionnels et stratégiques :
//! - **Tomate** : été, fruit, vit C, conserve coulis
//! - **Carotte** : racine, vit A massive, conservation cellier longue
//! - **Haricot sec** : protéines + folates, fixe l'azote, stockage 2 ans
//! - **Chou kale** : feuille rustique hiver, vit C/K, fer, cuisine soudure
//! - **Pommier** : fruit pluriannuel, conservation cellier hiver
//!
//! Sources :
//! - Composition : CIQUAL ANSES (tables 2020)
//! - Calendrier : Au Potager Bio / Terre Vivante (climat tempéré océanique)
//! - Rendements : ITAB / GAB

use std::collections::HashMap;

use super::nutrition::NutritionProfile;
use super::species::{
    CalendarWindow, Companionship, Family, GrowthProfile, Layer, LifeCycle, NutrientNeeds,
    Species, SpeciesId, StorageProfile, ThermalRange, WaterNeeds, YieldProfile,
};

/// Tomate cerise (Solanum lycopersicum) — référence été, conserve coulis.
pub fn tomato_cherry() -> Species {
    Species {
        id: SpeciesId::new("tomato_cherry"),
        name_fr: "Tomate cerise".into(),
        name_latin: "Solanum lycopersicum".into(),
        family: Family::Solanaceae,
        life_cycle: LifeCycle::Annual,
        layer: Layer::Herbaceous,
        thermal: ThermalRange {
            germination_min_c: 12.0,
            frost_kill_c: 0.0,
            growth_optimum_c: 22.0,
            heat_stress_c: 35.0,
        },
        water: WaterNeeds { weekly_optimal_mm: 30.0, stress_below: 0.35 },
        nutrients: NutrientNeeds { n: 2.5, p: 2.0, k: 3.0 },
        growth: GrowthProfile {
            gdd_to_maturity: 1100.0,
            days_to_maturity: 75,
            years_to_first_harvest: 0,
            photoperiod_min_h: 0.0,
        },
        yields: YieldProfile {
            g_per_plant_optimal: 3000.0,
            plants_per_m2: 3.0,
            harvests_per_year: 1,
        },
        storage: StorageProfile {
            fresh_days: 7,
            cellar_days: 14,
            dry_days: 365,
            frozen_days: 365,
            canned_days: 730,
            lacto_days: 90,
        },
        nutrition: NutritionProfile {
            kcal: 18.0,
            protein_g: 0.9,
            lipid_g: 0.2,
            carb_g: 3.5,
            fiber_g: 1.2,
            vit_a_ug: 42.0,
            vit_c_mg: 14.0,
            vit_e_mg: 0.54,
            vit_k_ug: 7.9,
            vit_b9_ug: 17.0,
            iron_mg: 0.27,
            calcium_mg: 11.0,
            magnesium_mg: 11.0,
            potassium_mg: 237.0,
            zinc_mg: 0.17,
        },
        companions: Companionship {
            friends: vec![SpeciesId::new("carrot"), SpeciesId::new("kale")],
            foes: vec![],
        },
        sowing_window: CalendarWindow { doy_start: 60, doy_end: 150 }, // mars-mai
        harvest_window: CalendarWindow { doy_start: 200, doy_end: 290 }, // juil-mi-oct
        ph_optimum: (6.5, 0.5),
        nitrogen_fixer: false,
        allelopathic: false,
        beneficial_for_pollinators: false,
    }
}

/// Carotte (Daucus carota) — racine vit A, conservation cellier excellente.
pub fn carrot() -> Species {
    Species {
        id: SpeciesId::new("carrot"),
        name_fr: "Carotte".into(),
        name_latin: "Daucus carota".into(),
        family: Family::Apiaceae,
        life_cycle: LifeCycle::Biennial, // récolte en année 1, monte en graine en 2
        layer: Layer::Root,
        thermal: ThermalRange {
            germination_min_c: 7.0,
            frost_kill_c: -5.0,
            growth_optimum_c: 18.0,
            heat_stress_c: 30.0,
        },
        water: WaterNeeds { weekly_optimal_mm: 25.0, stress_below: 0.30 },
        nutrients: NutrientNeeds { n: 1.5, p: 2.0, k: 2.0 },
        growth: GrowthProfile {
            gdd_to_maturity: 900.0,
            days_to_maturity: 100,
            years_to_first_harvest: 0,
            photoperiod_min_h: 0.0,
        },
        yields: YieldProfile {
            g_per_plant_optimal: 60.0,
            plants_per_m2: 50.0,
            harvests_per_year: 1,
        },
        storage: StorageProfile {
            fresh_days: 14,
            cellar_days: 180, // sable humide, excellente conservation
            dry_days: 365,
            frozen_days: 365,
            canned_days: 730,
            lacto_days: 180,
        },
        nutrition: NutritionProfile {
            kcal: 41.0,
            protein_g: 0.93,
            lipid_g: 0.24,
            carb_g: 9.6,
            fiber_g: 2.8,
            vit_a_ug: 835.0, // bêta-carotène massif
            vit_c_mg: 5.9,
            vit_e_mg: 0.66,
            vit_k_ug: 13.2,
            vit_b9_ug: 19.0,
            iron_mg: 0.30,
            calcium_mg: 33.0,
            magnesium_mg: 12.0,
            potassium_mg: 320.0,
            zinc_mg: 0.24,
        },
        companions: Companionship {
            friends: vec![SpeciesId::new("kale"), SpeciesId::new("tomato_cherry")],
            foes: vec![],
        },
        sowing_window: CalendarWindow { doy_start: 60, doy_end: 210 }, // mars-juil
        harvest_window: CalendarWindow { doy_start: 150, doy_end: 330 }, // juin-nov
        ph_optimum: (6.5, 1.0),
        nitrogen_fixer: false,
        allelopathic: false,
        beneficial_for_pollinators: false,
    }
}

/// Haricot sec, type coco / lingot (Phaseolus vulgaris) — protéines, N-fixer.
pub fn dry_bean() -> Species {
    Species {
        id: SpeciesId::new("dry_bean"),
        name_fr: "Haricot sec".into(),
        name_latin: "Phaseolus vulgaris".into(),
        family: Family::Fabaceae,
        life_cycle: LifeCycle::Annual,
        layer: Layer::Herbaceous,
        thermal: ThermalRange {
            germination_min_c: 12.0,
            frost_kill_c: 0.0,
            growth_optimum_c: 22.0,
            heat_stress_c: 32.0,
        },
        water: WaterNeeds { weekly_optimal_mm: 25.0, stress_below: 0.35 },
        nutrients: NutrientNeeds { n: 0.5, p: 1.5, k: 2.0 }, // peu d'N (fixateur)
        growth: GrowthProfile {
            gdd_to_maturity: 1300.0,
            days_to_maturity: 110,
            years_to_first_harvest: 0,
            photoperiod_min_h: 0.0,
        },
        yields: YieldProfile {
            g_per_plant_optimal: 100.0, // poids sec
            plants_per_m2: 8.0,
            harvests_per_year: 1,
        },
        storage: StorageProfile {
            fresh_days: 5,    // gousse fraîche peu intéressante (variété sèche)
            cellar_days: 14,
            dry_days: 730,    // 2 ans en bocal au sec
            frozen_days: 365,
            canned_days: 1825, // 5 ans cuit/stérilisé
            lacto_days: 0,
        },
        nutrition: NutritionProfile {
            kcal: 297.0, // poids sec
            protein_g: 19.1,
            lipid_g: 1.1,
            carb_g: 50.0,
            fiber_g: 16.0,
            vit_a_ug: 0.0,
            vit_c_mg: 0.0,
            vit_e_mg: 0.21,
            vit_k_ug: 5.6,
            vit_b9_ug: 364.0, // énorme : couvre folates
            iron_mg: 6.7,
            calcium_mg: 240.0,
            magnesium_mg: 190.0,
            potassium_mg: 1660.0,
            zinc_mg: 3.8,
        },
        companions: Companionship {
            friends: vec![SpeciesId::new("carrot")],
            foes: vec![SpeciesId::new("garlic")], // garlic non au catalogue, OK pour future
        },
        sowing_window: CalendarWindow { doy_start: 130, doy_end: 180 }, // mai-juin
        harvest_window: CalendarWindow { doy_start: 250, doy_end: 290 }, // sept-mi-oct
        ph_optimum: (6.5, 0.5),
        nitrogen_fixer: true,
        allelopathic: false,
        beneficial_for_pollinators: true,
    }
}

/// Chou kale (Brassica oleracea var. acephala) — feuille rustique hiver.
pub fn kale() -> Species {
    Species {
        id: SpeciesId::new("kale"),
        name_fr: "Chou kale".into(),
        name_latin: "Brassica oleracea var. acephala".into(),
        family: Family::Brassicaceae,
        life_cycle: LifeCycle::Biennial,
        layer: Layer::Herbaceous,
        thermal: ThermalRange {
            germination_min_c: 5.0,
            frost_kill_c: -10.0, // très rustique, le gel améliore le goût
            growth_optimum_c: 18.0,
            heat_stress_c: 28.0,
        },
        water: WaterNeeds { weekly_optimal_mm: 25.0, stress_below: 0.30 },
        nutrients: NutrientNeeds { n: 2.5, p: 1.5, k: 2.0 },
        growth: GrowthProfile {
            gdd_to_maturity: 800.0,
            days_to_maturity: 90,
            years_to_first_harvest: 0,
            photoperiod_min_h: 0.0,
        },
        yields: YieldProfile {
            g_per_plant_optimal: 1500.0, // récolte étalée, total saison
            plants_per_m2: 3.0,
            harvests_per_year: 1,
        },
        storage: StorageProfile {
            fresh_days: 14,
            cellar_days: 30,
            dry_days: 90,    // chips déshydratées
            frozen_days: 365,
            canned_days: 365,
            lacto_days: 180,
        },
        nutrition: NutritionProfile {
            kcal: 49.0,
            protein_g: 4.3,
            lipid_g: 0.9,
            carb_g: 8.7,
            fiber_g: 3.6,
            vit_a_ug: 500.0,
            vit_c_mg: 120.0, // énorme couverture C
            vit_e_mg: 1.5,
            vit_k_ug: 705.0, // record
            vit_b9_ug: 141.0,
            iron_mg: 1.5,
            calcium_mg: 150.0,
            magnesium_mg: 47.0,
            potassium_mg: 491.0,
            zinc_mg: 0.56,
        },
        companions: Companionship {
            friends: vec![SpeciesId::new("carrot"), SpeciesId::new("tomato_cherry")],
            foes: vec![],
        },
        sowing_window: CalendarWindow { doy_start: 90, doy_end: 220 }, // avril-août
        harvest_window: CalendarWindow { doy_start: 270, doy_end: 90 }, // oct → mars (chevauche)
        ph_optimum: (6.5, 1.0),
        nitrogen_fixer: false,
        allelopathic: false,
        beneficial_for_pollinators: false,
    }
}

/// Pommier (Malus domestica) — arbre fruitier rente automne-hiver.
pub fn apple_tree() -> Species {
    Species {
        id: SpeciesId::new("apple_tree"),
        name_fr: "Pommier".into(),
        name_latin: "Malus domestica".into(),
        family: Family::Rosaceae,
        life_cycle: LifeCycle::Tree,
        layer: Layer::Tree,
        thermal: ThermalRange {
            germination_min_c: 5.0,
            frost_kill_c: -25.0,
            growth_optimum_c: 18.0,
            heat_stress_c: 32.0,
        },
        water: WaterNeeds { weekly_optimal_mm: 20.0, stress_below: 0.25 },
        nutrients: NutrientNeeds { n: 1.5, p: 1.5, k: 2.0 },
        growth: GrowthProfile {
            gdd_to_maturity: 2500.0, // par cycle annuel
            days_to_maturity: 365, // arbre stable
            years_to_first_harvest: 4,
            photoperiod_min_h: 0.0,
        },
        yields: YieldProfile {
            g_per_plant_optimal: 25_000.0, // 25 kg/arbre adulte
            plants_per_m2: 0.06, // ~16 m²/arbre haute-tige
            harvests_per_year: 1,
        },
        storage: StorageProfile {
            fresh_days: 30,
            cellar_days: 180, // variétés tardives type Reinette, Belle de Boskoop
            dry_days: 365,
            frozen_days: 365,
            canned_days: 730,
            lacto_days: 0,
        },
        nutrition: NutritionProfile {
            kcal: 52.0,
            protein_g: 0.3,
            lipid_g: 0.2,
            carb_g: 14.0,
            fiber_g: 2.4,
            vit_a_ug: 3.0,
            vit_c_mg: 4.6,
            vit_e_mg: 0.18,
            vit_k_ug: 2.2,
            vit_b9_ug: 3.0,
            iron_mg: 0.12,
            calcium_mg: 6.0,
            magnesium_mg: 5.0,
            potassium_mg: 107.0,
            zinc_mg: 0.04,
        },
        companions: Companionship::default(),
        sowing_window: CalendarWindow { doy_start: 305, doy_end: 90 }, // nov-mars (plantation racines nues)
        harvest_window: CalendarWindow { doy_start: 230, doy_end: 310 }, // mi-août - début nov
        ph_optimum: (6.5, 1.0),
        nitrogen_fixer: false,
        allelopathic: false,
        beneficial_for_pollinators: true,
    }
}

/// Catalogue MVP : 5 espèces référencées par leur ID string pour lookup rapide.
pub fn default_catalog() -> HashMap<String, Species> {
    let species = vec![tomato_cherry(), carrot(), dry_bean(), kale(), apple_tree()];
    species.into_iter().map(|s| (s.id.0.clone(), s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_5_species() {
        assert_eq!(default_catalog().len(), 5);
    }

    #[test]
    fn each_species_has_unique_id() {
        let cat = default_catalog();
        for (key, sp) in &cat {
            assert_eq!(*key, sp.id.0);
        }
    }

    #[test]
    fn dry_bean_is_nitrogen_fixer() {
        assert!(dry_bean().nitrogen_fixer);
    }

    #[test]
    fn apple_tree_takes_years_to_first_harvest() {
        assert_eq!(apple_tree().growth.years_to_first_harvest, 4);
        assert!(matches!(apple_tree().life_cycle, LifeCycle::Tree));
    }

    #[test]
    fn kale_is_frost_hardy() {
        assert!(kale().thermal.frost_kill_c < -5.0);
    }

    #[test]
    fn carrot_has_high_vit_a() {
        assert!(carrot().nutrition.vit_a_ug > 500.0);
    }

    #[test]
    fn dry_bean_has_high_protein_and_folate() {
        let b = dry_bean();
        assert!(b.nutrition.protein_g > 15.0);
        assert!(b.nutrition.vit_b9_ug > 300.0);
    }

    #[test]
    fn apple_storage_lasts_winter() {
        assert!(apple_tree().storage.cellar_days >= 150);
    }
}
