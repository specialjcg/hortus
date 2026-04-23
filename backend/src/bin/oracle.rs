//! Comparateur de stratégies de plantation.
//!
//! Lance 4 stratégies sur N graines × 1 année, mesure jours d'autonomie + récolte,
//! affiche moyenne et IC bootstrap 95 %.
//!
//! Objectif : valider que greedy ≠ optimal (signal exploitable pour ML).
//!
//! Usage : cargo run --release --bin oracle [-- --seeds 50 --years 1]

use hortus::domain::garden::CellCoord;
use hortus::domain::sim::Simulation;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

#[derive(Debug, Clone)]
struct Outcome {
    autonomy_days: u32,
    harvest_kg: f64,
    deficit_days: u32,
    pantry_lost_kg: f64,
    species_eaten: usize,
}

fn parse_args() -> (usize, u32) {
    let mut seeds = 20;
    let mut years = 1;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--seeds" => seeds = args.next().and_then(|s| s.parse().ok()).unwrap_or(20),
            "--years" => years = args.next().and_then(|s| s.parse().ok()).unwrap_or(1),
            _ => {}
        }
    }
    (seeds, years)
}

// ============================================================================
// STRATÉGIES
// ============================================================================

/// 100 % tomates partout après les saints de glace.
fn strat_monoculture(s: &mut Simulation) {
    while s.date.day_of_year < 130 { s.tick(); }
    for r in 0..10 { for c in 0..10 {
        let _ = s.sow(CellCoord::new(c, r), "tomato_cherry");
    }}
}

/// Top rendement brut : 50 % pomme de terre, 30 % butternut, 20 % courgette.
fn strat_greedy_yield(s: &mut Simulation) {
    while s.date.day_of_year < 110 { s.tick(); }
    let mut i = 0;
    for r in 0..10 { for c in 0..10 {
        let sp = if i < 50 { "potato" }
                 else if i < 80 { "butternut_squash" }
                 else { "zucchini" };
        let _ = s.sow(CellCoord::new(c, r), sp);
        i += 1;
    }}
}

/// Mix saisonnier réaliste sans vivaces.
fn strat_diversified(s: &mut Simulation) {
    while s.date.day_of_year < 75 { s.tick(); }
    for c in 0..10 { let _ = s.sow(CellCoord::new(c, 0), "carrot"); }
    while s.date.day_of_year < 95 { s.tick(); }
    for c in 0..10 { let _ = s.sow(CellCoord::new(c, 1), "spinach"); }
    while s.date.day_of_year < 110 { s.tick(); }
    for c in 0..5 { let _ = s.sow(CellCoord::new(c, 2), "kale"); }
    for c in 5..10 { let _ = s.sow(CellCoord::new(c, 2), "onion"); }
    for c in 0..10 { let _ = s.sow(CellCoord::new(c, 3), "potato"); }
    while s.date.day_of_year < 140 { s.tick(); }
    for c in 0..3 { let _ = s.sow(CellCoord::new(c, 4), "tomato_cherry"); }
    for c in 3..7 { let _ = s.sow(CellCoord::new(c, 4), "dry_bean"); }
    for c in 7..10 { let _ = s.sow(CellCoord::new(c, 4), "butternut_squash"); }
    for c in 0..5 { let _ = s.sow(CellCoord::new(c, 5), "zucchini"); }
    for c in 5..10 { let _ = s.sow(CellCoord::new(c, 5), "leek"); }
    while s.date.day_of_year < 230 { s.tick(); }
    for r in 6..10 { for c in 0..10 {
        let _ = s.sow(CellCoord::new(c, r), "lambs_lettuce");
    }}
}

/// Approche permaculture : vivaces + soudure + diversité + storage.
fn strat_permaculture(s: &mut Simulation) {
    // Ligature pérenne (rangée 0) : pommier + framboisier + cassissier + topinambour
    let _ = s.plant_established_tree(CellCoord::new(0, 0), "apple_tree", 5);
    for c in 1..3 { let _ = s.plant_established_tree(CellCoord::new(c, 0), "raspberry", 2); }
    for c in 3..5 { let _ = s.plant_established_tree(CellCoord::new(c, 0), "blackcurrant", 3); }
    while s.date.day_of_year < 80 { s.tick(); }
    for c in 5..10 { let _ = s.sow(CellCoord::new(c, 0), "jerusalem_artichoke"); }

    // Rangée 1 : alliums + épinard
    for c in 0..5 { let _ = s.sow(CellCoord::new(c, 1), "onion"); }
    for c in 5..10 { let _ = s.sow(CellCoord::new(c, 1), "spinach"); }

    // Rangée 2 : carotte + poireau
    while s.date.day_of_year < 100 { s.tick(); }
    for c in 0..6 { let _ = s.sow(CellCoord::new(c, 2), "carrot"); }
    for c in 6..10 { let _ = s.sow(CellCoord::new(c, 2), "leek"); }

    // Rangée 3 : pomme de terre + kale
    while s.date.day_of_year < 110 { s.tick(); }
    for c in 0..6 { let _ = s.sow(CellCoord::new(c, 3), "potato"); }
    for c in 6..10 { let _ = s.sow(CellCoord::new(c, 3), "kale"); }

    // Rangées 4-5 : été (tomates, haricots, courge, courgette)
    while s.date.day_of_year < 140 { s.tick(); }
    for c in 0..3 { let _ = s.sow(CellCoord::new(c, 4), "tomato_cherry"); }
    for c in 3..7 { let _ = s.sow(CellCoord::new(c, 4), "dry_bean"); }
    for c in 7..10 { let _ = s.sow(CellCoord::new(c, 4), "butternut_squash"); }
    for c in 0..5 { let _ = s.sow(CellCoord::new(c, 5), "zucchini"); }
    for c in 5..10 { let _ = s.sow(CellCoord::new(c, 5), "dry_bean"); }

    // Rangées 6-9 : mâche d'hiver (semis aoû-sept)
    while s.date.day_of_year < 240 { s.tick(); }
    for r in 6..10 { for c in 0..10 {
        let _ = s.sow(CellCoord::new(c, r), "lambs_lettuce");
    }}
}

// ============================================================================
// EXÉCUTION
// ============================================================================

fn run_one(plant: fn(&mut Simulation), seed: u64, total_days: u32) -> Outcome {
    let mut s = Simulation::new_default(seed);
    plant(&mut s);
    while s.stats.days_simulated < total_days { s.tick(); }
    let species_eaten: std::collections::HashSet<String> =
        s.history
            .iter()
            .flat_map(|r| r.consumption.by_species.iter().map(|(name, _)| name.clone()))
            .collect();
    Outcome {
        autonomy_days: s.stats.days_fully_covered,
        harvest_kg: s.stats.total_harvest_g / 1000.0,
        deficit_days: s.stats.days_in_deficit,
        pantry_lost_kg: s.stats.total_food_lost_g / 1000.0,
        species_eaten: species_eaten.len(),
    }
}

// ============================================================================
// STATISTIQUES (bootstrap CI 95 %)
// ============================================================================

fn bootstrap_ci(values: &[f64], n_resamples: usize, seed: u64) -> (f64, f64, f64) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut means = Vec::with_capacity(n_resamples);
    for _ in 0..n_resamples {
        let resample: Vec<f64> = (0..values.len())
            .map(|_| *values.choose(&mut rng).unwrap())
            .collect();
        let m = resample.iter().sum::<f64>() / resample.len() as f64;
        means.push(m);
    }
    means.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let lo = means[(0.025 * n_resamples as f64) as usize];
    let hi = means[(0.975 * n_resamples as f64).min(n_resamples as f64 - 1.0) as usize];
    (mean, lo, hi)
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    let (n_seeds, years) = parse_args();
    let total_days = years * 365;
    let seeds: Vec<u64> = (1..=n_seeds as u64).collect();

    let strategies: &[(&str, fn(&mut Simulation))] = &[
        ("Monoculture tomate ", strat_monoculture),
        ("Greedy haut-rendement", strat_greedy_yield),
        ("Diversifié          ", strat_diversified),
        ("Permaculture        ", strat_permaculture),
    ];

    println!("\n╔═══ HORTUS — comparateur de stratégies ═══╗\n");
    println!("Configuration : {} seeds × {} an(s) = {} runs", n_seeds, years, n_seeds);
    println!("Métrique principale : jours d'autonomie alimentaire complète\n");

    println!("  {:<22} | {:>14} {:>14} | {:>10} {:>9} {:>9}",
             "Stratégie", "autonomie (j)", "récolte (kg)", "déficit", "pertes", "espèces");
    println!("  {:-<22} + {:-<14} {:-<14} + {:-<10} {:-<9} {:-<9}",
             "", "", "", "", "", "");

    let mut all_results: Vec<(&str, Vec<Outcome>)> = Vec::new();
    for (name, f) in strategies {
        let outcomes: Vec<Outcome> = seeds.iter().map(|&s| run_one(*f, s, total_days)).collect();
        let autonomies: Vec<f64> = outcomes.iter().map(|o| o.autonomy_days as f64).collect();
        let harvests: Vec<f64> = outcomes.iter().map(|o| o.harvest_kg).collect();
        let deficits: Vec<f64> = outcomes.iter().map(|o| o.deficit_days as f64).collect();
        let losses: Vec<f64> = outcomes.iter().map(|o| o.pantry_lost_kg).collect();
        let species: Vec<f64> = outcomes.iter().map(|o| o.species_eaten as f64).collect();

        let (a_mean, a_lo, a_hi) = bootstrap_ci(&autonomies, 1000, 42);
        let h_mean = harvests.iter().sum::<f64>() / harvests.len() as f64;
        let d_mean = deficits.iter().sum::<f64>() / deficits.len() as f64;
        let l_mean = losses.iter().sum::<f64>() / losses.len() as f64;
        let sp_mean = species.iter().sum::<f64>() / species.len() as f64;

        println!(
            "  {:<22} | {:>5.1} [{:>4.1};{:>4.1}] | {:>5.1}        | {:>10.1} {:>9.1} {:>9.1}",
            name, a_mean, a_lo, a_hi, h_mean, d_mean, l_mean, sp_mean
        );
        all_results.push((name, outcomes));
    }

    println!();
    println!("─── Lecture ─────────────────────────────────");
    println!("  • autonomie : moyenne [IC bootstrap 95 %], plus haut = mieux");
    println!("  • récolte   : kg / an");
    println!("  • déficit   : jours sans couverture nutritionnelle complète");
    println!("  • pertes    : kg gaspillés (péremption)");
    println!("  • espèces   : diversité alimentaire effective");
    println!();
    println!("Si Permaculture > Greedy → signal stratégique exploitable");
    println!("(ML doit pouvoir au moins égaler Permaculture, idéalement la dépasser).");
}
