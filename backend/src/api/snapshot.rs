//! DTOs renvoyés par l'API. Vues *aplaties* du moteur, sans `Plant` brut ni
//! types complexes, pour rester stables côté frontend.

use serde::{Deserialize, Serialize};

use crate::domain::nutrition::{DailyBalance, DailyRequirement};
use crate::domain::plant::GrowthStage;
use crate::domain::sim::{DailyEvent, Simulation, Stats};
use crate::domain::time::SimDate;
use crate::domain::weather::DailyWeather;

#[derive(Debug, Serialize, Deserialize)]
pub struct SimSnapshot {
    pub id: String,
    pub date: SimDate,
    pub stats: Stats,
    pub garden: GardenSnapshot,
    pub pantry: PantrySnapshot,
    pub household: HouseholdSnapshot,
    pub recent_events: Vec<DailyEvent>,
    pub last_weather: Option<DailyWeather>,
    pub last_balance: Option<DailyBalance>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GardenSnapshot {
    pub cols: u16,
    pub rows: u16,
    pub cell_area_m2: f64,
    pub cells: Vec<CellSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CellSnapshot {
    pub col: u16,
    pub row: u16,
    pub soil_type: String,
    pub cover: String,
    pub n: f64,
    pub p: f64,
    pub k: f64,
    pub organic_matter_pct: f64,
    pub ph: f64,
    pub water_mm: f64,
    pub soil_temp_c: f64,
    pub plant: Option<PlantSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlantSnapshot {
    pub species_id: String,
    pub species_name: String,
    pub stage: GrowthStage,
    pub progress: f64,
    pub health: f64,
    pub biomass_g: f64,
    pub planted_at: SimDate,
    pub harvest_count: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PantrySnapshot {
    pub total_mass_g: f64,
    pub by_species: Vec<(String, f64)>,
    pub by_compartment: Vec<(String, f64)>,
    /// Liste détaillée des lots — utilisée par la cuisine pour transformer.
    pub items: Vec<PantryItemSnap>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PantryItemSnap {
    pub species_id: String,
    pub species_name: String,
    pub compartment: String,
    pub mass_g: f64,
    pub days_left: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HouseholdSnapshot {
    pub adults: u8,
    pub children: u8,
    pub equivalent_adults: f64,
    pub requirement: DailyRequirement,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpeciesCard {
    pub id: String,
    pub name_fr: String,
    pub name_latin: String,
    pub family: String,
    pub life_cycle: String,
    pub layer: String,
    pub frost_kill_c: f64,
    pub days_to_maturity: u16,
    pub years_to_first_harvest: u16,
    pub g_per_plant_optimal: f64,
    pub plants_per_m2: f64,
    pub kcal_per_100g: f64,
    pub protein_per_100g: f64,
    pub vit_c_per_100g: f64,
    pub sowing_window: (u16, u16),
    pub harvest_window: (u16, u16),
    pub nitrogen_fixer: bool,
}

pub fn snapshot(sim: &Simulation, id: &str) -> SimSnapshot {
    let cells = sim
        .garden
        .iter()
        .map(|c| {
            let plant = c.plant.as_ref().map(|p| {
                let species = sim.catalog.get(p.species.as_str());
                let species_name = species
                    .map(|s| s.name_fr.clone())
                    .unwrap_or_else(|| p.species.0.clone());
                let progress = species
                    .map(|s| (p.gdd_accumulated / s.growth.gdd_to_maturity).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                PlantSnapshot {
                    species_id: p.species.0.clone(),
                    species_name,
                    stage: p.stage,
                    progress,
                    health: p.health,
                    biomass_g: p.biomass_g,
                    planted_at: p.planted_at,
                    harvest_count: p.harvest_count,
                }
            });
            let cover = match c.state.cover {
                crate::domain::soil::GroundCover::Bare => "bare",
                crate::domain::soil::GroundCover::Mulch => "mulch",
                crate::domain::soil::GroundCover::Living => "living",
                crate::domain::soil::GroundCover::Crop => "crop",
            };
            CellSnapshot {
                col: c.coord.col,
                row: c.coord.row,
                soil_type: c.state.soil_type.name().into(),
                cover: cover.into(),
                n: c.state.n,
                p: c.state.p,
                k: c.state.k,
                organic_matter_pct: c.state.organic_matter_pct,
                ph: c.state.ph,
                water_mm: c.state.water_mm,
                soil_temp_c: c.state.soil_temp_c,
                plant,
            }
        })
        .collect();

    let mut by_species: std::collections::BTreeMap<String, f64> = Default::default();
    let mut by_compartment: std::collections::BTreeMap<String, f64> = Default::default();
    let mut items: Vec<PantryItemSnap> = Vec::new();
    for it in &sim.pantry.items {
        let name = sim
            .catalog
            .get(it.species.as_str())
            .map(|s| s.name_fr.clone())
            .unwrap_or_else(|| it.species.0.clone());
        *by_species.entry(name.clone()).or_insert(0.0) += it.mass_g;
        *by_compartment.entry(it.compartment.name().into()).or_insert(0.0) += it.mass_g;
        items.push(PantryItemSnap {
            species_id: it.species.0.clone(),
            species_name: name,
            compartment: it.compartment.name().into(),
            mass_g: it.mass_g,
            days_left: it.best_before.days_since(sim.date),
        });
    }
    items.sort_by(|a, b| a.species_name.cmp(&b.species_name));

    SimSnapshot {
        id: id.to_string(),
        date: sim.date,
        stats: sim.stats.clone(),
        garden: GardenSnapshot {
            cols: sim.garden.cols,
            rows: sim.garden.rows,
            cell_area_m2: sim.garden.cell_area_m2,
            cells,
        },
        pantry: PantrySnapshot {
            total_mass_g: sim.pantry.total_mass().max(0.0),
            by_species: by_species.into_iter().collect(),
            by_compartment: by_compartment.into_iter().collect(),
            items,
        },
        household: HouseholdSnapshot {
            adults: sim.household.adults,
            children: sim.household.children,
            equivalent_adults: sim.household.equivalent_adults(),
            requirement: sim.household.requirement.clone(),
        },
        recent_events: sim.journal.iter().rev().take(30).cloned().collect(),
        last_weather: sim.history.last().map(|r| r.weather.clone()),
        last_balance: sim.history.last().map(|r| r.consumption.balance.clone()),
    }
}

pub fn species_cards(sim: &Simulation) -> Vec<SpeciesCard> {
    let mut v: Vec<_> = sim
        .catalog
        .values()
        .map(|s| SpeciesCard {
            id: s.id.0.clone(),
            name_fr: s.name_fr.clone(),
            name_latin: s.name_latin.clone(),
            family: s.family.name().into(),
            life_cycle: format!("{:?}", s.life_cycle),
            layer: format!("{:?}", s.layer),
            frost_kill_c: s.thermal.frost_kill_c,
            days_to_maturity: s.growth.days_to_maturity,
            years_to_first_harvest: s.growth.years_to_first_harvest,
            g_per_plant_optimal: s.yields.g_per_plant_optimal,
            plants_per_m2: s.yields.plants_per_m2,
            kcal_per_100g: s.nutrition.kcal,
            protein_per_100g: s.nutrition.protein_g,
            vit_c_per_100g: s.nutrition.vit_c_mg,
            sowing_window: (s.sowing_window.doy_start, s.sowing_window.doy_end),
            harvest_window: (s.harvest_window.doy_start, s.harvest_window.doy_end),
            nitrogen_fixer: s.nitrogen_fixer,
        })
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}
