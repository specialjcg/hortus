//! Lance une simulation Hortus et imprime un résumé mensuel + bilan annuel.
//!
//! Usage :
//!   cargo run --release --bin run_year
//!   cargo run --release --bin run_year -- --years 3 --seed 42

use hortus::domain::garden::CellCoord;
use hortus::domain::sim::Simulation;

fn parse_args() -> (u32, u64) {
    let mut years = 1u32;
    let mut seed = 42u64;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--years" => years = args.next().and_then(|s| s.parse().ok()).unwrap_or(1),
            "--seed" => seed = args.next().and_then(|s| s.parse().ok()).unwrap_or(42),
            _ => {}
        }
    }
    (years, seed)
}

fn sow_pilot_plan(s: &mut Simulation) {
    s.plant_established_tree(CellCoord::new(0, 0), "apple_tree", 5).unwrap();
    while s.date.day_of_year < 75 { s.tick(); }
    for col in 0..5 { s.sow(CellCoord::new(col, 1), "carrot").unwrap(); }
    while s.date.day_of_year < 100 { s.tick(); }
    for col in 0..3 { s.sow(CellCoord::new(col, 2), "kale").unwrap(); }
    while s.date.day_of_year < 140 { s.tick(); }
    for col in 0..3 { s.sow(CellCoord::new(col, 3), "tomato_cherry").unwrap(); }
    for col in 0..4 { s.sow(CellCoord::new(col, 4), "dry_bean").unwrap(); }
}

fn main() {
    let (years, seed) = parse_args();
    let mut s = Simulation::new_default(seed);
    sow_pilot_plan(&mut s);

    // Compléter l'année 1 puis poursuivre N-1 années en replantant chaque printemps.
    let total_days = years * 365;
    while s.stats.days_simulated < total_days {
        // Replantation au 1er mars de chaque année (sauf l'année 1, déjà semée).
        if s.date.day_of_year == 60 && s.date.year > 1 {
            for col in 0..5 {
                let c = CellCoord::new(col, 1);
                if s.garden.get(c).map(|x| x.plant.as_ref()).is_none()
                    || matches!(s.garden.get(c).and_then(|x| x.plant.as_ref()).map(|p| p.stage),
                                Some(hortus::domain::plant::GrowthStage::Harvested)
                                | Some(hortus::domain::plant::GrowthStage::Dead)
                                | None)
                {
                    if let Some(cell) = s.garden.get_mut(c) {
                        cell.plant = None;
                    }
                    let _ = s.sow(c, "carrot");
                }
            }
        }
        s.tick();
    }

    print_summary(&s, years);
}

fn print_summary(s: &Simulation, years: u32) {
    println!("\n╔═══ HORTUS — résumé sur {years} an(s) ═══╗\n");
    println!("Lieu     : {}", s.weather_gen.location().name);
    println!("Foyer    : {} adulte(s), {} enfant(s)", s.household.adults, s.household.children);
    println!("Jardin   : {:.1} m² ({} cellules)", s.garden.total_area_m2(), s.garden.n_cells());
    println!();

    // En-tête tableau mensuel
    println!(
        "  {:>4} {:>3} | {:>5} {:>5} {:>5} | {:>7} | {:>4}/{:>4}",
        "an", "mo", "Tmin", "Tmax", "pluie", "récolte", "ok", "déf"
    );
    println!("  {:-<4} {:-<3} + {:-<5} {:-<5} {:-<5} + {:-<7} + {:-<4} {:-<4}",
             "", "", "", "", "", "", "", "");

    let mut buckets: std::collections::BTreeMap<(u16, u8), MonthBucket> =
        std::collections::BTreeMap::new();
    for r in &s.history {
        let k = (r.date.year, r.date.month());
        let b = buckets.entry(k).or_default();
        b.tmin_sum += r.weather.temp_min_c;
        b.tmax_sum += r.weather.temp_max_c;
        b.precip_sum += r.weather.precipitation_mm;
        b.harvest_sum += r.harvests_g;
        b.days += 1;
        if r.consumption.balance.fully_covered { b.days_ok += 1; }
        else { b.days_deficit += 1; }
    }
    for ((y, m), b) in &buckets {
        let n = b.days as f64;
        println!(
            "  {:>4} {:>3} | {:>5.1} {:>5.1} {:>5.0} | {:>6.0}g | {:>4} {:>4}",
            y, m,
            b.tmin_sum / n, b.tmax_sum / n, b.precip_sum,
            b.harvest_sum,
            b.days_ok, b.days_deficit
        );
    }

    println!();
    println!("─── Bilan annuel ──────────────────────────────");
    println!("  Jours simulés      : {}", s.stats.days_simulated);
    println!("  Jours autonomie    : {} ({:.1}%)",
             s.stats.days_fully_covered,
             100.0 * s.stats.days_fully_covered as f64 / s.stats.days_simulated.max(1) as f64);
    println!("  Jours déficit      : {}", s.stats.days_in_deficit);
    println!("  Récolte totale     : {:.1} kg", s.stats.total_harvest_g / 1000.0);
    println!("  Pertes pantry      : {:.1} kg", s.stats.total_food_lost_g / 1000.0);
    println!("  Pantry restant     : {:.1} kg", s.pantry.total_mass() / 1000.0);

    println!();
    println!("─── Pantry par espèce ─────────────────────────");
    let mut by: std::collections::BTreeMap<String, f64> = Default::default();
    for it in &s.pantry.items {
        if let Some(sp) = s.catalog.get(it.species.as_str()) {
            *by.entry(sp.name_fr.clone()).or_insert(0.0) += it.mass_g;
        }
    }
    if by.is_empty() {
        println!("  (vide)");
    } else {
        for (name, g) in &by {
            println!("  {:<18} {:>6.0} g", name, g);
        }
    }

    println!();
    println!("─── Top 10 événements récents ─────────────────");
    for ev in s.journal.iter().rev().take(10).rev() {
        println!("  {:>3}/{:<3} {:?} — {}", ev.date.year, ev.date.day_of_year, ev.kind, ev.message);
    }
}

#[derive(Default)]
struct MonthBucket {
    tmin_sum: f64,
    tmax_sum: f64,
    precip_sum: f64,
    harvest_sum: f64,
    days: u32,
    days_ok: u32,
    days_deficit: u32,
}
