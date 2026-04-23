//! Composition nutritionnelle et besoins alimentaires.
//!
//! Toutes les valeurs sont en unités CIQUAL :
//! - énergie en **kcal** (1 kcal = 4.184 kJ)
//! - macros en **grammes** par 100 g d'aliment cru (portion comestible)
//! - vitamines : A en **µg** (RAE), C/E/K/B9 en **mg** ou **µg** selon convention
//! - minéraux en **mg**
//!
//! Les apports journaliers de référence (AJR) sont calés sur ANSES (adulte type).

use serde::{Deserialize, Serialize};

/// Composition nutritionnelle pour 100 g d'aliment cru, portion comestible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NutritionProfile {
    pub kcal: f64,
    pub protein_g: f64,
    pub lipid_g: f64,
    pub carb_g: f64,
    pub fiber_g: f64,

    pub vit_a_ug: f64,    // µg RAE
    pub vit_c_mg: f64,
    pub vit_e_mg: f64,
    pub vit_k_ug: f64,
    pub vit_b9_ug: f64,   // folates

    pub iron_mg: f64,
    pub calcium_mg: f64,
    pub magnesium_mg: f64,
    pub potassium_mg: f64,
    pub zinc_mg: f64,
}

impl NutritionProfile {
    /// Multiplie un profil par une masse en grammes (pour calculer un apport réel).
    pub fn for_mass_g(&self, mass_g: f64) -> NutritionIntake {
        let f = mass_g / 100.0;
        NutritionIntake {
            kcal: self.kcal * f,
            protein_g: self.protein_g * f,
            lipid_g: self.lipid_g * f,
            carb_g: self.carb_g * f,
            fiber_g: self.fiber_g * f,
            vit_a_ug: self.vit_a_ug * f,
            vit_c_mg: self.vit_c_mg * f,
            vit_e_mg: self.vit_e_mg * f,
            vit_k_ug: self.vit_k_ug * f,
            vit_b9_ug: self.vit_b9_ug * f,
            iron_mg: self.iron_mg * f,
            calcium_mg: self.calcium_mg * f,
            magnesium_mg: self.magnesium_mg * f,
            potassium_mg: self.potassium_mg * f,
            zinc_mg: self.zinc_mg * f,
        }
    }
}

/// Apport nutritionnel effectif (somme de plusieurs aliments consommés).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NutritionIntake {
    pub kcal: f64,
    pub protein_g: f64,
    pub lipid_g: f64,
    pub carb_g: f64,
    pub fiber_g: f64,
    pub vit_a_ug: f64,
    pub vit_c_mg: f64,
    pub vit_e_mg: f64,
    pub vit_k_ug: f64,
    pub vit_b9_ug: f64,
    pub iron_mg: f64,
    pub calcium_mg: f64,
    pub magnesium_mg: f64,
    pub potassium_mg: f64,
    pub zinc_mg: f64,
}

impl NutritionIntake {
    pub fn add(&mut self, other: &NutritionIntake) {
        self.kcal += other.kcal;
        self.protein_g += other.protein_g;
        self.lipid_g += other.lipid_g;
        self.carb_g += other.carb_g;
        self.fiber_g += other.fiber_g;
        self.vit_a_ug += other.vit_a_ug;
        self.vit_c_mg += other.vit_c_mg;
        self.vit_e_mg += other.vit_e_mg;
        self.vit_k_ug += other.vit_k_ug;
        self.vit_b9_ug += other.vit_b9_ug;
        self.iron_mg += other.iron_mg;
        self.calcium_mg += other.calcium_mg;
        self.magnesium_mg += other.magnesium_mg;
        self.potassium_mg += other.potassium_mg;
        self.zinc_mg += other.zinc_mg;
    }
}

/// Apports journaliers de référence — adulte sédentaire moyen.
/// Source : ANSES (références nutritionnelles 2017).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyRequirement {
    pub kcal: f64,
    pub protein_g: f64,
    pub lipid_g: f64,
    pub carb_g: f64,
    pub fiber_g: f64,
    pub vit_a_ug: f64,
    pub vit_c_mg: f64,
    pub vit_e_mg: f64,
    pub vit_k_ug: f64,
    pub vit_b9_ug: f64,
    pub iron_mg: f64,
    pub calcium_mg: f64,
    pub magnesium_mg: f64,
    pub potassium_mg: f64,
    pub zinc_mg: f64,
}

impl DailyRequirement {
    /// Adulte type (réf ANSES). Femmes/hommes moyennés.
    pub fn adult() -> Self {
        Self {
            kcal: 2200.0,
            protein_g: 50.0,
            lipid_g: 75.0,
            carb_g: 280.0,
            fiber_g: 30.0,
            vit_a_ug: 750.0,
            vit_c_mg: 110.0,
            vit_e_mg: 10.5,
            vit_k_ug: 70.0,
            vit_b9_ug: 330.0,
            iron_mg: 11.0,
            calcium_mg: 950.0,
            magnesium_mg: 380.0,
            potassium_mg: 3500.0,
            zinc_mg: 9.4,
        }
    }

    /// Multiplie pour un foyer de N personnes adultes équivalentes.
    pub fn scaled(&self, n_adult_eq: f64) -> Self {
        Self {
            kcal: self.kcal * n_adult_eq,
            protein_g: self.protein_g * n_adult_eq,
            lipid_g: self.lipid_g * n_adult_eq,
            carb_g: self.carb_g * n_adult_eq,
            fiber_g: self.fiber_g * n_adult_eq,
            vit_a_ug: self.vit_a_ug * n_adult_eq,
            vit_c_mg: self.vit_c_mg * n_adult_eq,
            vit_e_mg: self.vit_e_mg * n_adult_eq,
            vit_k_ug: self.vit_k_ug * n_adult_eq,
            vit_b9_ug: self.vit_b9_ug * n_adult_eq,
            iron_mg: self.iron_mg * n_adult_eq,
            calcium_mg: self.calcium_mg * n_adult_eq,
            magnesium_mg: self.magnesium_mg * n_adult_eq,
            potassium_mg: self.potassium_mg * n_adult_eq,
            zinc_mg: self.zinc_mg * n_adult_eq,
        }
    }
}

/// Bilan d'une journée alimentaire : intake vs requirement → couverture par nutriment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBalance {
    pub intake: NutritionIntake,
    pub requirement: DailyRequirement,
    /// Liste des nutriments en déficit (couverture < 80 % du besoin).
    pub deficits: Vec<String>,
    /// Couverture moyenne pondérée 0..1.
    pub coverage_avg: f64,
    /// True si tous les nutriments critiques sont couverts à au moins 80 %.
    pub fully_covered: bool,
}

impl DailyBalance {
    pub fn compute(intake: NutritionIntake, requirement: DailyRequirement) -> Self {
        let pairs: &[(&str, f64, f64)] = &[
            ("kcal", intake.kcal, requirement.kcal),
            ("protéines", intake.protein_g, requirement.protein_g),
            ("lipides", intake.lipid_g, requirement.lipid_g),
            ("glucides", intake.carb_g, requirement.carb_g),
            ("fibres", intake.fiber_g, requirement.fiber_g),
            ("vit A", intake.vit_a_ug, requirement.vit_a_ug),
            ("vit C", intake.vit_c_mg, requirement.vit_c_mg),
            ("vit E", intake.vit_e_mg, requirement.vit_e_mg),
            ("vit K", intake.vit_k_ug, requirement.vit_k_ug),
            ("folates", intake.vit_b9_ug, requirement.vit_b9_ug),
            ("fer", intake.iron_mg, requirement.iron_mg),
            ("calcium", intake.calcium_mg, requirement.calcium_mg),
            ("magnésium", intake.magnesium_mg, requirement.magnesium_mg),
            ("potassium", intake.potassium_mg, requirement.potassium_mg),
            ("zinc", intake.zinc_mg, requirement.zinc_mg),
        ];
        let mut deficits = Vec::new();
        let mut sum_cov = 0.0;
        for (name, got, need) in pairs {
            let cov = if *need > 0.0 { (got / need).min(2.0) } else { 1.0 };
            sum_cov += cov.min(1.0);
            if cov < 0.8 {
                deficits.push((*name).to_string());
            }
        }
        let coverage_avg = sum_cov / pairs.len() as f64;
        let fully_covered = deficits.is_empty();
        Self { intake, requirement, deficits, coverage_avg, fully_covered }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_apple() -> NutritionProfile {
        NutritionProfile {
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
        }
    }

    #[test]
    fn for_mass_scales_linearly() {
        let p = fake_apple();
        let i = p.for_mass_g(200.0);
        assert!((i.kcal - 104.0).abs() < 1e-9);
        assert!((i.carb_g - 28.0).abs() < 1e-9);
    }

    #[test]
    fn intake_add_accumulates() {
        let p = fake_apple();
        let mut total = NutritionIntake::default();
        total.add(&p.for_mass_g(100.0));
        total.add(&p.for_mass_g(100.0));
        assert!((total.kcal - 104.0).abs() < 1e-9);
    }

    #[test]
    fn balance_flags_pure_apple_diet_as_deficient() {
        let p = fake_apple();
        let i = p.for_mass_g(2000.0); // 2 kg de pommes
        let b = DailyBalance::compute(i, DailyRequirement::adult());
        assert!(!b.fully_covered);
        assert!(b.deficits.contains(&"protéines".to_string()));
        assert!(b.deficits.contains(&"calcium".to_string()));
    }

    #[test]
    fn requirement_scales_to_household() {
        let r = DailyRequirement::adult().scaled(2.5);
        assert!((r.kcal - 5500.0).abs() < 1e-9);
        assert!((r.iron_mg - 27.5).abs() < 1e-9);
    }
}
