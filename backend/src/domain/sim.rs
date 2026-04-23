//! Boucle de simulation : orchestration météo → sol → plantes → récolte → consommation.
//!
//! Modélise une journée à la fois. Approche simple mais cohérente :
//! 1. Tirer la météo du jour
//! 2. Mettre à jour la T° du sol (suit l'air avec inertie)
//! 3. Distribuer la pluie sur toutes les cellules
//! 4. Évaporer (ETP modulée par couverture)
//! 5. Minéraliser la matière organique (libère N selon T° sol)
//! 6. Pour chaque plante : transpiration, stress, GDD, biomasse, transitions de stade
//! 7. Auto-récolte si stade Mature dans la fenêtre calendaire
//! 8. Foyer : pioche dans le pantry et calcule le bilan nutritionnel
//! 9. Purger les lots périmés
//! 10. Avancer la date

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::catalog;
use super::garden::{CellCoord, Garden};
use super::geo::Location;
use super::household::{consume_day, ConsumptionPolicy, DailyConsumptionReport, Household};
use super::pantry::Pantry;
use super::plant::{growing_degree_day, GrowthStage, Plant};
use super::soil::{GroundCover, SoilType};
use super::species::{LifeCycle, Species, SpeciesId};
use super::time::SimDate;
use super::weather::{DailyWeather, WeatherGenerator};

/// Catégorie d'événement consigné dans le journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Sown,
    Germinated,
    StageAdvanced,
    Harvested,
    FrostKilled,
    HeatStress,
    DroughtStress,
    Stored,
    Consumed,
    Deficit,
    Storm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyEvent {
    pub date: SimDate,
    pub kind: EventKind,
    pub message: String,
}

/// Bilan d'un tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickReport {
    pub date: SimDate,
    pub weather: DailyWeather,
    pub harvests_g: f64,
    pub events: Vec<DailyEvent>,
    pub consumption: DailyConsumptionReport,
}

/// État global de la simulation.
pub struct Simulation {
    pub date: SimDate,
    pub garden: Garden,
    pub pantry: Pantry,
    pub household: Household,
    pub catalog: HashMap<String, Species>,
    pub weather_gen: WeatherGenerator,
    pub policy: ConsumptionPolicy,
    pub journal: Vec<DailyEvent>,
    pub history: Vec<TickReport>,
    /// Statistiques agrégées (mises à jour à chaque tick).
    pub stats: Stats,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub days_simulated: u32,
    pub days_fully_covered: u32,
    pub days_in_deficit: u32,
    pub total_harvest_g: f64,
    pub total_food_lost_g: f64,
}

impl Simulation {
    /// Crée une simulation par défaut : 5×5 m de jardin loam, foyer adulte solo, climat Paris.
    pub fn new_default(seed: u64) -> Self {
        let location = Location::paris();
        Self::new(
            location,
            Garden::new_uniform(10, 10, 0.25, SoilType::Loam),
            Household::solo_adult(),
            seed,
        )
    }

    pub fn new(location: Location, garden: Garden, household: Household, seed: u64) -> Self {
        Self {
            date: SimDate::start(),
            garden,
            pantry: Pantry::new(),
            household,
            catalog: catalog::default_catalog(),
            weather_gen: WeatherGenerator::new(location, seed),
            policy: ConsumptionPolicy::default(),
            journal: Vec::new(),
            history: Vec::new(),
            stats: Stats::default(),
        }
    }

    /// Plante une graine sur la cellule indiquée.
    pub fn sow(&mut self, coord: CellCoord, species_id: &str) -> Result<(), String> {
        if !self.garden.in_bounds(coord) {
            return Err(format!("hors limites : {:?}", coord));
        }
        let species = self
            .catalog
            .get(species_id)
            .ok_or_else(|| format!("espèce inconnue : {species_id}"))?
            .clone();
        let cell = self.garden.get_mut(coord).unwrap();
        if cell.plant.is_some() {
            return Err("cellule déjà occupée".into());
        }
        cell.plant = Some(Plant::new(species.id.clone(), self.date));
        cell.state.cover = GroundCover::Crop;
        self.journal.push(DailyEvent {
            date: self.date,
            kind: EventKind::Sown,
            message: format!("{} semé en {:?}", species.name_fr, coord),
        });
        Ok(())
    }

    /// Plante directement un arbre déjà établi (saute les premières années stériles).
    /// Utile pour démarrer une simulation avec un verger existant.
    pub fn plant_established_tree(
        &mut self,
        coord: CellCoord,
        species_id: &str,
        years_already_grown: u16,
    ) -> Result<(), String> {
        if !self.garden.in_bounds(coord) {
            return Err(format!("hors limites : {:?}", coord));
        }
        let species = self
            .catalog
            .get(species_id)
            .ok_or_else(|| format!("espèce inconnue : {species_id}"))?
            .clone();
        if !matches!(species.life_cycle, LifeCycle::Tree | LifeCycle::Shrub) {
            return Err("plant_established_tree réservé aux ligneux".into());
        }
        let cell = self.garden.get_mut(coord).unwrap();
        if cell.plant.is_some() {
            return Err("cellule déjà occupée".into());
        }
        let mut plant = Plant::new(species.id.clone(), self.date);
        // L'arbre est déjà adulte : assez de GDD accumulé et un âge effectif
        // suffisant pour passer le gate `years_to_first_harvest`.
        plant.stage = GrowthStage::Vegetative;
        plant.gdd_accumulated = species.growth.gdd_to_maturity * 0.6;
        plant.biomass_g = species.yields.g_per_plant_optimal * 0.6;
        plant.years_grown_before_planting = years_already_grown;
        cell.plant = Some(plant);
        Ok(())
    }

    /// Arrosage manuel d'une cellule (mm équivalent pluie). Renvoie le runoff.
    pub fn water(&mut self, coord: CellCoord, mm: f64) -> Result<f64, String> {
        if mm <= 0.0 || mm > 200.0 {
            return Err("mm doit être dans (0, 200]".into());
        }
        let cell = self.garden.get_mut(coord).ok_or_else(|| format!("hors limites : {:?}", coord))?;
        let ev = cell.state.add_water(mm);
        Ok(ev.runoff_mm)
    }

    /// Pose du paillage organique sur une cellule. Réduit l'évaporation,
    /// améliore lentement la matière organique.
    pub fn mulch(&mut self, coord: CellCoord) -> Result<(), String> {
        let cell = self.garden.get_mut(coord).ok_or_else(|| format!("hors limites : {:?}", coord))?;
        cell.state.cover = GroundCover::Mulch;
        // Apport instantané de MO modeste (paillage = matière sèche en surface).
        cell.state.organic_matter_pct = (cell.state.organic_matter_pct + 0.2).min(15.0);
        Ok(())
    }

    /// Apport de compost (kg/m²). Augmente l'azote disponible et la matière organique.
    /// Conversion approximative : 1 kg compost mûr / m² ≈ +1 N (échelle 0-10) + 0.5 % MO.
    pub fn add_compost(&mut self, coord: CellCoord, kg_per_m2: f64) -> Result<(), String> {
        if kg_per_m2 <= 0.0 || kg_per_m2 > 10.0 {
            return Err("compost en kg/m² doit être dans (0, 10]".into());
        }
        let cell = self.garden.get_mut(coord).ok_or_else(|| format!("hors limites : {:?}", coord))?;
        cell.state.n = (cell.state.n + kg_per_m2).min(10.0);
        cell.state.p = (cell.state.p + kg_per_m2 * 0.6).min(10.0);
        cell.state.k = (cell.state.k + kg_per_m2 * 0.8).min(10.0);
        cell.state.organic_matter_pct = (cell.state.organic_matter_pct + kg_per_m2 * 0.5).min(15.0);
        Ok(())
    }

    /// Arrache une plante (récolte d'une vivace, démontage d'une annuelle finie).
    /// Renvoie la biomasse perdue (g).
    pub fn uproot(&mut self, coord: CellCoord) -> Result<f64, String> {
        let cell = self.garden.get_mut(coord).ok_or_else(|| format!("hors limites : {:?}", coord))?;
        let mass = cell.plant.as_ref().map(|p| p.biomass_g).unwrap_or(0.0);
        cell.plant = None;
        cell.state.cover = GroundCover::Bare;
        Ok(mass)
    }

    /// Transforme un lot du pantry (par exemple frais → lacto/conserve/séché).
    /// Renvoie la masse réellement stockée après pertes de transformation.
    pub fn transform(
        &mut self,
        species_id: &str,
        from: super::pantry::StorageCompartment,
        to: super::pantry::StorageCompartment,
        mass_g: f64,
    ) -> Result<f64, String> {
        if mass_g <= 0.0 {
            return Err("mass_g doit être > 0".into());
        }
        let species = self
            .catalog
            .get(species_id)
            .ok_or_else(|| format!("espèce inconnue : {species_id}"))?
            .clone();
        let kept = self.pantry.process(&species, from, to, mass_g, self.date);
        if kept <= 0.0 {
            return Err("aucun stock disponible pour cette transformation".into());
        }
        Ok(kept)
    }

    /// Avance la simulation d'un jour. Renvoie le rapport du tick.
    pub fn tick(&mut self) -> TickReport {
        let weather = self.weather_gen.step(self.date);
        let mut events = Vec::new();
        let mut harvests_g = 0.0;

        // 1. Mise à jour T° sol + apport pluie + évaporation + minéralisation
        for cell in self.garden.iter_mut() {
            // T° sol suit l'air avec inertie selon conductivité
            let conductivity = cell.state.soil_type.thermal_conductivity();
            let alpha = (0.15 * conductivity).min(0.5);
            cell.state.soil_temp_c += alpha * (weather.temp_mean_c - cell.state.soil_temp_c);

            // Pluie
            if weather.precipitation_mm > 0.0 {
                let _ = cell.state.add_water(weather.precipitation_mm);
            }
            // Évaporation
            cell.state.evaporate(weather.etp_mm());
            // Minéralisation
            cell.state.mineralize(cell.state.soil_temp_c);
        }

        // 2. Plantes : croissance + stress + transitions + auto-récolte
        let coords: Vec<CellCoord> = self.garden.iter().map(|c| c.coord).collect();
        for coord in coords {
            let cell = self.garden.get_mut(coord).unwrap();
            let Some(plant) = cell.plant.as_mut() else { continue };
            if !plant.stage.is_alive() { continue; }
            let species_id = plant.species.clone();
            let species = match self.catalog.get(species_id.as_str()) {
                Some(s) => s.clone(),
                None => continue,
            };

            // --- Mortalité par gel ---
            if weather.temp_min_c < species.thermal.frost_kill_c {
                plant.kill();
                cell.state.cover = GroundCover::Bare;
                events.push(DailyEvent {
                    date: self.date,
                    kind: EventKind::FrostKilled,
                    message: format!(
                        "{} gelé (Tmin {:.1}°C < seuil {:.1}°C)",
                        species.name_fr, weather.temp_min_c, species.thermal.frost_kill_c
                    ),
                });
                continue;
            }

            // --- Identifie les stress du jour ---
            let need_mm = species.water.weekly_optimal_mm / 7.0;
            let took = need_mm.min(cell.state.water_mm);
            cell.state.water_mm -= took;
            let stress_threshold =
                species.water.stress_below * cell.state.soil_type.field_capacity_mm();
            let water_below = cell.state.water_mm < stress_threshold;
            let heat_event = weather.temp_max_c > species.thermal.heat_stress_c;
            let cold_growth = weather.temp_mean_c < 5.0;

            // --- Stress hydrique : compteur consécutif ---
            if water_below {
                plant.water_stress_days = plant.water_stress_days.saturating_add(1);
                // Dégât santé uniquement après 5 jours consécutifs, et progressif.
                if plant.water_stress_days > 5 {
                    let extra = (plant.water_stress_days - 5).min(20) as f64;
                    plant.health = (plant.health - 0.005 * extra / 5.0).max(0.0);
                }
                if plant.water_stress_days == 7 {
                    events.push(DailyEvent {
                        date: self.date,
                        kind: EventKind::DroughtStress,
                        message: format!("{} en stress hydrique (7 jours)", species.name_fr),
                    });
                }
            } else {
                plant.water_stress_days = 0;
            }

            // --- Stress thermique : dégât léger uniquement sur événement extrême ---
            if heat_event {
                plant.health = (plant.health - 0.01).max(0.0);
            }

            // --- Consommation NPK ---
            let n_use = 0.005 * species.nutrients.n;
            let p_use = 0.003 * species.nutrients.p;
            let k_use = 0.005 * species.nutrients.k;
            cell.state.n = (cell.state.n - n_use).max(0.0);
            cell.state.p = (cell.state.p - p_use).max(0.0);
            cell.state.k = (cell.state.k - k_use).max(0.0);

            // --- Fixation d'azote (légumineuses) ---
            if species.nitrogen_fixer && plant.gdd_accumulated > 200.0 {
                cell.state.n = (cell.state.n + 0.015).min(10.0);
            }

            // --- Stress nutritif ---
            let n_short = cell.state.n < 0.4 * species.nutrients.n;
            let k_short = cell.state.k < 0.4 * species.nutrients.k;

            // --- Récupération de santé si conditions correctes ---
            if !water_below && !heat_event && !cold_growth {
                plant.health = (plant.health + 0.02).min(1.0);
            }

            // --- Facteur de croissance : multiplicatif mais doux ---
            let mut growth_factor: f64 = 1.0;
            if water_below { growth_factor *= 0.5; }
            if heat_event { growth_factor *= 0.6; }
            if n_short || k_short { growth_factor *= 0.7; }
            // Pas de facteur "cold" : raw_gdd est déjà nul si T_mean < 5°C.

            // --- Croissance : GDD effectif (santé n'amplifie PAS, évite la spirale) ---
            let raw_gdd = growing_degree_day(weather.temp_mean_c, 5.0);
            let effective_gdd = raw_gdd * growth_factor;
            plant.gdd_accumulated += effective_gdd;
            let progress = (plant.gdd_accumulated / species.growth.gdd_to_maturity).min(1.0);
            // Biomasse = potentiel × progrès × moyenne(health, 0.5) pour éviter d'effondrer
            // l'unique récolte annuelle si une mauvaise semaine survient juste avant.
            plant.biomass_g =
                species.yields.g_per_plant_optimal * progress * (0.5 + 0.5 * plant.health);

            // --- Stade dérivé du progrès (autorise plusieurs transitions/tick) ---
            let target_stage = if progress >= 1.0 {
                GrowthStage::Mature
            } else if progress >= 0.75 {
                GrowthStage::Fruiting
            } else if progress >= 0.50 {
                GrowthStage::Flowering
            } else if progress >= 0.15 {
                GrowthStage::Vegetative
            } else if progress >= 0.05 {
                GrowthStage::Seedling
            } else {
                GrowthStage::Seed
            };
            // Garde-fou germination : si on est encore Seed, vérifier la T° sol.
            let new_stage = if matches!(plant.stage, GrowthStage::Seed)
                && cell.state.soil_temp_c < species.thermal.germination_min_c
            {
                GrowthStage::Seed
            } else {
                target_stage
            };
            if new_stage != plant.stage {
                let was_seed = matches!(plant.stage, GrowthStage::Seed);
                plant.stage = new_stage;
                let kind = if was_seed && matches!(new_stage, GrowthStage::Seedling) {
                    EventKind::Germinated
                } else {
                    EventKind::StageAdvanced
                };
                events.push(DailyEvent {
                    date: self.date,
                    kind,
                    message: format!("{} → {:?}", species.name_fr, new_stage),
                });
            }

            // --- Auto-récolte ---
            let should_harvest = match species.life_cycle {
                LifeCycle::Annual | LifeCycle::Biennial => {
                    // Annuelles : on récolte dès que mature, la date résulte
                    // naturellement du GDD (pas de contrainte de fenêtre).
                    plant.stage == GrowthStage::Mature && plant.biomass_g > 1.0
                }
                LifeCycle::Tree | LifeCycle::Shrub | LifeCycle::Perennial => {
                    // Vivaces / arbres : 1 récolte/an, dans la fenêtre calendaire,
                    // et seulement une fois l'arbre établi (years_to_first_harvest).
                    let years_since = (self.date.year as i32
                        - plant.planted_at.year as i32)
                        .max(0) as u16
                        + plant.years_grown_before_planting;
                    let mature_age =
                        years_since >= species.growth.years_to_first_harvest;
                    let already_this_year = match plant.last_harvested_at {
                        Some(d) => d.year == self.date.year,
                        None => false,
                    };
                    mature_age
                        && species.harvest_window.contains(self.date.day_of_year)
                        && !already_this_year
                        && plant.biomass_g > 1.0
                        && plant.stage == GrowthStage::Mature
                }
            };
            if should_harvest {
                let mass = plant.biomass_g;
                self.pantry.store_fresh_harvest(&species, mass, self.date);
                harvests_g += mass;
                self.stats.total_harvest_g += mass;
                events.push(DailyEvent {
                    date: self.date,
                    kind: EventKind::Harvested,
                    message: format!("{} récolté : {:.0} g", species.name_fr, mass),
                });
                plant.harvest_count += 1;
                plant.last_harvested_at = Some(self.date);
                match species.life_cycle {
                    LifeCycle::Tree | LifeCycle::Shrub | LifeCycle::Perennial => {
                        // Retour au repos : baisse du GDD pour repasser 1 cycle annuel.
                        plant.stage = GrowthStage::Vegetative;
                        plant.gdd_accumulated = species.growth.gdd_to_maturity * 0.3;
                        plant.biomass_g = species.yields.g_per_plant_optimal * 0.3;
                    }
                    LifeCycle::Annual | LifeCycle::Biennial => {
                        plant.stage = GrowthStage::Harvested;
                        plant.biomass_g = 0.0;
                        cell.state.cover = GroundCover::Bare;
                    }
                }
            }
        }

        // 3. Pantry : purger les lots périmés
        let lost = self.pantry.purge_expired(self.date);
        if lost > 0.0 {
            self.stats.total_food_lost_g += lost;
            events.push(DailyEvent {
                date: self.date,
                kind: EventKind::Stored,
                message: format!("Lot périmé éliminé : {:.0} g", lost),
            });
        }

        // 4. Foyer : consommation
        let consumption = consume_day(
            self.date,
            &self.household,
            &mut self.pantry,
            &self.catalog,
            &self.policy,
        );
        if consumption.balance.fully_covered {
            self.stats.days_fully_covered += 1;
        } else {
            self.stats.days_in_deficit += 1;
            if !consumption.balance.deficits.is_empty() {
                events.push(DailyEvent {
                    date: self.date,
                    kind: EventKind::Deficit,
                    message: format!(
                        "Déficits : {}",
                        consumption.balance.deficits.join(", ")
                    ),
                });
            }
        }

        if matches!(weather.kind, super::weather::WeatherKind::Storm) {
            events.push(DailyEvent {
                date: self.date,
                kind: EventKind::Storm,
                message: format!(
                    "Orage : {:.1} mm, vent {:.0} km/h",
                    weather.precipitation_mm, weather.wind_kmh
                ),
            });
        }

        self.stats.days_simulated += 1;
        let report = TickReport {
            date: self.date,
            weather: weather.clone(),
            harvests_g,
            events: events.clone(),
            consumption,
        };

        // Append au journal et historique
        self.journal.extend(events);
        self.history.push(report.clone());

        // Avancer la date
        self.date = self.date.next_day();
        report
    }

    /// Avance de plusieurs jours.
    pub fn run(&mut self, days: u32) {
        for _ in 0..days {
            self.tick();
        }
    }
}

/// Compte les espèces actuellement plantées dans le jardin.
pub fn count_plants_by_species(garden: &Garden) -> HashMap<SpeciesId, usize> {
    let mut h: HashMap<SpeciesId, usize> = HashMap::new();
    for cell in garden.iter() {
        if let Some(p) = &cell.plant {
            if p.stage.is_alive() {
                *h.entry(p.species.clone()).or_insert(0) += 1;
            }
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sim_runs_one_day() {
        let mut s = Simulation::new_default(1);
        let r = s.tick();
        assert_eq!(r.date, SimDate::start());
        assert_eq!(s.date, SimDate::new(1, 2));
        assert_eq!(r.harvests_g, 0.0);
        // Pas de récolte → consommation impossible → en déficit.
        assert!(!r.consumption.balance.fully_covered);
    }

    #[test]
    fn sow_then_grow_advances_stage() {
        let mut s = Simulation::new_default(7);
        // Avancer jusqu'à fin avril (~120) pour avoir un sol assez chaud.
        s.run(110);
        s.sow(CellCoord::new(0, 0), "tomato_cherry").unwrap();
        // Run 4 mois — devrait germer puis monter en stade.
        s.run(120);
        let cell = s.garden.get(CellCoord::new(0, 0)).unwrap();
        let p = cell.plant.as_ref().unwrap();
        // Plus avancé que Seed et toujours en vie (ou récolté = mort pour annuelle).
        assert!(
            !matches!(p.stage, GrowthStage::Seed),
            "stage = {:?}, gdd = {}",
            p.stage,
            p.gdd_accumulated
        );
    }

    #[test]
    fn full_year_records_365_ticks() {
        let mut s = Simulation::new_default(2);
        s.run(365);
        assert_eq!(s.history.len(), 365);
        assert_eq!(s.stats.days_simulated, 365);
        assert_eq!(
            s.stats.days_fully_covered + s.stats.days_in_deficit,
            365
        );
    }

    #[test]
    fn established_tree_can_be_planted() {
        let mut s = Simulation::new_default(3);
        s.plant_established_tree(CellCoord::new(2, 2), "apple_tree", 5)
            .unwrap();
        let cell = s.garden.get(CellCoord::new(2, 2)).unwrap();
        let p = cell.plant.as_ref().unwrap();
        assert!(matches!(p.stage, GrowthStage::Vegetative));
        assert!(p.gdd_accumulated > 0.0);
    }

    #[test]
    fn duplicate_sow_fails() {
        let mut s = Simulation::new_default(4);
        s.sow(CellCoord::new(0, 0), "kale").unwrap();
        assert!(s.sow(CellCoord::new(0, 0), "kale").is_err());
    }

    #[test]
    fn unknown_species_fails() {
        let mut s = Simulation::new_default(5);
        assert!(s.sow(CellCoord::new(0, 0), "banana").is_err());
    }

    #[test]
    fn water_action_increases_cell_moisture() {
        let mut s = Simulation::new_default(1);
        let c = CellCoord::new(0, 0);
        let before = s.garden.get(c).unwrap().state.water_mm;
        s.water(c, 20.0).unwrap();
        let after = s.garden.get(c).unwrap().state.water_mm;
        assert!(after > before);
    }

    #[test]
    fn mulch_action_changes_cover() {
        let mut s = Simulation::new_default(1);
        let c = CellCoord::new(0, 0);
        s.mulch(c).unwrap();
        let cover = s.garden.get(c).unwrap().state.cover;
        assert!(matches!(cover, GroundCover::Mulch));
    }

    #[test]
    fn compost_increases_npk_and_om() {
        let mut s = Simulation::new_default(1);
        let c = CellCoord::new(0, 0);
        let n_before = s.garden.get(c).unwrap().state.n;
        let om_before = s.garden.get(c).unwrap().state.organic_matter_pct;
        s.add_compost(c, 2.0).unwrap();
        let cell = s.garden.get(c).unwrap();
        assert!(cell.state.n > n_before);
        assert!(cell.state.organic_matter_pct > om_before);
    }

    #[test]
    fn uproot_removes_plant() {
        let mut s = Simulation::new_default(1);
        let c = CellCoord::new(0, 0);
        s.sow(c, "kale").unwrap();
        assert!(s.garden.get(c).unwrap().plant.is_some());
        let _ = s.uproot(c).unwrap();
        assert!(s.garden.get(c).unwrap().plant.is_none());
    }

    #[test]
    fn transform_lacto_extends_shelf_life() {
        use super::super::pantry::StorageCompartment;
        let mut s = Simulation::new_default(1);
        // Pousse de la masse fraîche dans le pantry directement (sans tick).
        let species = s.catalog.get("kale").unwrap().clone();
        s.pantry.store_fresh_harvest(&species, 1000.0, s.date);
        let kept = s
            .transform("kale", StorageCompartment::Fresh, StorageCompartment::Lacto, 500.0)
            .unwrap();
        assert!(kept > 0.0 && kept < 500.0); // pertes de transformation
    }
}
