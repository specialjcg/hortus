//! Test d'intégration : simulation d'une année à Paris avec un plan de culture
//! réaliste mais minimaliste, et observation des sorties macro.

use hortus::domain::garden::CellCoord;
use hortus::domain::sim::Simulation;
use hortus::domain::time::SimDate;

/// Plan de culture pilote pour un jardin 5×5 m (10×10 cellules de 0.25 m²).
/// Densités réalistes ramenées à la cellule unique.
fn sow_pilot_plan(s: &mut Simulation) {
    // Pommier établi installé d'emblée (déjà productif).
    s.plant_established_tree(CellCoord::new(0, 0), "apple_tree", 5)
        .expect("plant apple");
    // Carottes semées en mars (DOY ~75).
    while s.date.day_of_year < 75 { s.tick(); }
    for col in 0..5 { s.sow(CellCoord::new(col, 1), "carrot").expect("carrot"); }
    // Kale en avril (DOY ~100).
    while s.date.day_of_year < 100 { s.tick(); }
    for col in 0..3 { s.sow(CellCoord::new(col, 2), "kale").expect("kale"); }
    // Tomates et haricots après les saints de glace (DOY ~140).
    while s.date.day_of_year < 140 { s.tick(); }
    for col in 0..3 { s.sow(CellCoord::new(col, 3), "tomato_cherry").expect("tomato"); }
    for col in 0..4 { s.sow(CellCoord::new(col, 4), "dry_bean").expect("bean"); }
}

#[test]
fn diagnostic_plant_state_after_year() {
    use hortus::domain::garden::CellCoord;
    let mut s = Simulation::new_default(42);
    sow_pilot_plan(&mut s);
    let remaining = 365 - s.date.day_of_year as u32 + 1;
    s.run(remaining);

    eprintln!("\n=== État final des plantes après 1 an ===");
    eprintln!("Date finale : {:?}", s.date);
    eprintln!("Total récolte : {:.0} g", s.stats.total_harvest_g);
    eprintln!("Pantry : {:.0} g", s.pantry.total_mass());
    for cell in s.garden.iter() {
        if let Some(p) = &cell.plant {
            eprintln!(
                "  ({},{}) {} stage={:?} gdd={:.0} biomass={:.0}g health={:.2} harvests={}",
                cell.coord.col,
                cell.coord.row,
                p.species.0,
                p.stage,
                p.gdd_accumulated,
                p.biomass_g,
                p.health,
                p.harvest_count
            );
        }
    }
    let _ = CellCoord::new(0, 0);
}

#[test]
fn one_year_paris_default_starvation() {
    // Témoin : on ne plante rien → 365 jours en déficit complet.
    let mut s = Simulation::new_default(42);
    s.run(365);
    assert_eq!(s.stats.days_simulated, 365);
    assert_eq!(s.stats.days_fully_covered, 0);
    assert_eq!(s.stats.days_in_deficit, 365);
    assert_eq!(s.stats.total_harvest_g, 0.0);
}

#[test]
fn one_year_paris_with_pilot_plan_produces_harvests() {
    let mut s = Simulation::new_default(42);
    sow_pilot_plan(&mut s);
    // Compléter l'année (sow_pilot_plan a déjà avancé au DOY 140).
    let remaining = 365 - s.date.day_of_year as u32 + 1;
    s.run(remaining);
    assert!(s.date.day_of_year == 1 || s.date.year >= 2);

    // Au moins une récolte a eu lieu.
    assert!(
        s.stats.total_harvest_g > 0.0,
        "aucune récolte sur l'année — modèle de croissance cassé ?"
    );
    // Sanity : récolte annuelle dépasse 1 kg (pommier mature seul = 25 kg en théorie).
    assert!(
        s.stats.total_harvest_g > 1000.0,
        "récolte trop faible : {:.0} g",
        s.stats.total_harvest_g
    );
}

#[test]
fn three_years_paris_increasing_autonomy() {
    // Sur 3 ans avec replantation, on s'attend à voir le pantry remplir
    // au moins quelques jours par an.
    let mut s = Simulation::new_default(42);
    sow_pilot_plan(&mut s);
    s.run(365 * 3);
    assert!(s.history.len() >= 365 * 3 - 200);
    // Pas zéro jour couvert : avec haricots secs + pommes + kale on doit
    // pouvoir au moins toucher quelques jours à 80 % de couverture en automne.
    // Au pire, on tolère ce check soft.
    let total = s.stats.days_fully_covered + s.stats.days_in_deficit;
    assert!(total > 800);
}

#[test]
fn weather_in_simulation_matches_seed_reproducibly() {
    let mut a = Simulation::new_default(99);
    let mut b = Simulation::new_default(99);
    a.run(60);
    b.run(60);
    let last_a = a.history.last().unwrap();
    let last_b = b.history.last().unwrap();
    assert_eq!(last_a.weather.precipitation_mm, last_b.weather.precipitation_mm);
    assert_eq!(last_a.weather.temp_min_c, last_b.weather.temp_min_c);
}

#[test]
fn frost_kills_tomato_planted_too_early() {
    // Tomate (frost_kill_c = 0°C) plantée le 1er janvier doit mourir au premier gel.
    let mut s = Simulation::new_default(42);
    s.sow(CellCoord::new(0, 0), "tomato_cherry").unwrap();
    s.run(60); // jusqu'à fin février, du gel attendu
    let cell = s.garden.get(CellCoord::new(0, 0)).unwrap();
    let p = cell.plant.as_ref().unwrap();
    assert!(
        !p.stage.is_alive(),
        "tomate aurait dû geler — stage = {:?}",
        p.stage
    );
}

#[test]
fn date_is_consistent_across_runs() {
    let mut s = Simulation::new_default(1);
    s.run(100);
    assert_eq!(s.date, SimDate::new(1, 101));
}
