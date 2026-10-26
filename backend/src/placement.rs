//! Placement des plants du plan : échantillonnage de Poisson (Bridson) par
//! planche, chaque culture en îlot compact. Deux plants en terre en même
//! temps restent à distance ; un plant récolté libère sa place.

use std::collections::{HashMap, VecDeque};
use std::f64::consts::TAU;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::Serialize;

use crate::domain::geometry::Polygon;
use crate::domain::species::Species;
use crate::planner::{PlanItem, WEEKS};

/// Un tirage de Poisson saturé loge ≈ 0,7 point par d² : avec d = 0,8 ×
/// espacement, on retrouve les surface / espacement² plants du plan.
const MIN_DIST_FACTOR: f64 = 0.8;
/// Au-delà, chaque point représente une touffe (semis serrés : carotte, radis).
const MAX_PLANTS_PER_ITEM: usize = 150;
/// Candidats tirés autour d'un plant actif (Bridson).
const CANDIDATES: usize = 30;
/// Tirages pour trouver la graine d'un nouvel îlot.
const SEED_TRIES: usize = 2000;
/// Marge au bord de planche : ¼ d'espacement, plafonnée.
const MAX_EDGE_MARGIN_CM: f64 = 15.0;

#[derive(Debug, Clone, Serialize)]
pub struct Placement {
    pub bed_id: i64,
    pub species_id: String,
    pub x: i64,
    pub y: i64,
    /// Semaine d'arrivée en planche (1..=52) et durée, comme `PlanItem`.
    pub week: u8,
    pub weeks_on_bed: u8,
    /// Rayon d'emprise (cm) ; plus grand que l'espacement pour une touffe.
    pub r: f64,
}

struct Placed {
    x: f64,
    y: f64,
    spacing: f64,
    start: usize,
    len: usize,
}

/// Intervalles de semaines (année cyclique, base 0) qui se chevauchent.
fn overlaps(a_start: usize, a_len: usize, b_start: usize, b_len: usize) -> bool {
    (0..a_len).any(|k| ((a_start + k) % WEEKS + WEEKS - b_start) % WEEKS < b_len)
}

/// Graine d'un îlot : le point libre le plus au nord parmi des tirages
/// (les plants partent du nord, l'espace libre reste groupé au sud).
fn north_seed(shape: &Polygon, rng: &mut ChaCha8Rng, fits: impl Fn(f64, f64) -> bool) -> Option<(f64, f64)> {
    let (x_min, y_min, x_max, y_max) = shape.bbox();
    (0..SEED_TRIES)
        .map(|_| (rng.gen_range(x_min as f64..x_max as f64), rng.gen_range(y_min as f64..y_max as f64)))
        .filter(|&(x, y)| fits(x, y))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Positions des plants de chaque culture du plan, reproductibles pour une graine.
pub fn place(items: &[PlanItem], beds: &[(i64, &Polygon)], species: &[Species], seed: u64) -> Vec<Placement> {
    let spacing_of: HashMap<&str, f64> = species.iter().map(|s| (s.id.as_str(), s.spacing_cm as f64)).collect();
    let mut out = Vec::new();
    for &(bed_id, shape) in beds {
        let mut bed_items: Vec<(&PlanItem, f64)> = items
            .iter()
            .filter(|i| i.bed_id == bed_id)
            .filter_map(|i| Some((i, *spacing_of.get(i.species_id.as_str())?)))
            .collect();
        // À semaine égale, les grands plants d'abord : plus durs à caser.
        bed_items.sort_by(|(a, sa), (b, sb)| a.week.cmp(&b.week).then(sb.total_cmp(sa)));

        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ bed_id as u64);
        let mut placed: Vec<Placed> = Vec::new();
        for (item, sp) in bed_items {
            let wanted = (item.area_m2 * 10_000.0 / (sp * sp)).round().max(1.0);
            let n = (wanted as usize).min(MAX_PLANTS_PER_ITEM);
            // Touffe : espacement agrandi pour garder la même emprise totale.
            let spacing = sp * (wanted / n as f64).sqrt();
            let new = Placed {
                x: 0.0,
                y: 0.0,
                spacing,
                start: (item.week as usize + WEEKS - 1) % WEEKS,
                len: item.weeks_on_bed as usize,
            };
            let fits = |x: f64, y: f64, placed: &[Placed]| {
                shape.contains(x, y)
                    && shape.edge_distance(x, y) >= (spacing / 4.0).min(MAX_EDGE_MARGIN_CM)
                    && !placed.iter().any(|p| {
                        let min_d = MIN_DIST_FACTOR * (p.spacing + spacing) / 2.0;
                        (p.x - x).hypot(p.y - y) < min_d && overlaps(p.start, p.len, new.start, new.len)
                    })
            };
            let d = MIN_DIST_FACTOR * spacing;
            // Bridson : l'îlot grandit autour de ses plants « actifs » ; bloqué,
            // il repart d'une nouvelle graine.
            let mut active: VecDeque<(f64, f64)> = VecDeque::new();
            let mut count = 0;
            while count < n {
                let next = match active.front() {
                    None => match north_seed(shape, &mut rng, |x, y| fits(x, y, &placed)) {
                        Some(pt) => Some(pt),
                        None => break, // planche pleine pour cette culture
                    },
                    Some(&(ax, ay)) => {
                        let found = (0..CANDIDATES).find_map(|_| {
                            let (angle, radius) = (rng.gen_range(0.0..TAU), rng.gen_range(d..2.0 * d));
                            let (x, y) = (ax + radius * angle.cos(), ay + radius * angle.sin());
                            fits(x, y, &placed).then_some((x, y))
                        });
                        if found.is_none() {
                            active.pop_front();
                        }
                        found
                    }
                };
                if let Some((x, y)) = next {
                    active.push_back((x, y));
                    placed.push(Placed { x, y, ..new });
                    out.push(Placement {
                        bed_id,
                        species_id: item.species_id.clone(),
                        x: x.round() as i64,
                        y: y.round() as i64,
                        week: item.week,
                        weeks_on_bed: item.weeks_on_bed,
                        r: (spacing / 2.0).round(),
                    });
                    count += 1;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::Mode;

    fn species() -> Vec<Species> {
        Species::load_from_json("data/species.json").expect("species.json valide")
    }

    fn bed() -> Polygon {
        Polygon::try_from(vec![[0, 0], [120, 0], [120, 500], [0, 500]]).unwrap()
    }

    fn item(species_id: &str, week: u8, weeks_on_bed: u8, area_m2: f64) -> PlanItem {
        PlanItem {
            bed_id: 1,
            species_id: species_id.into(),
            mode: Mode::Direct,
            week,
            weeks_on_bed,
            area_m2,
            expected_kg: 0.0,
            expected_eur: 0.0,
        }
    }

    fn count(ps: &[Placement], sid: &str) -> usize {
        ps.iter().filter(|p| p.species_id == sid).count()
    }

    #[test]
    fn overlaps_handles_year_wrap() {
        assert!(overlaps(50, 4, 0, 2));
        assert!(!overlaps(10, 5, 15, 5));
        assert!(overlaps(10, 6, 15, 5));
    }

    #[test]
    fn plants_inside_bed_and_spaced() {
        let shape = bed();
        let items = [item("tomato", 18, 20, 2.0), item("lettuce", 18, 8, 2.0)];
        let ps = place(&items, &[(1, &shape)], &species(), 7);
        assert!(ps.iter().all(|p| shape.contains(p.x as f64, p.y as f64)));
        for (i, a) in ps.iter().enumerate() {
            for b in &ps[i + 1..] {
                let min_d = MIN_DIST_FACTOR * (a.r + b.r) - 2.0; // arrondis
                let d = ((a.x - b.x) as f64).hypot((a.y - b.y) as f64);
                assert!(d >= min_d, "{} et {} à {d} cm", a.species_id, b.species_id);
            }
        }
    }

    #[test]
    fn count_matches_plan_area() {
        // 3 m² de laitue (25 cm) → 48 plants.
        let shape = bed();
        let ps = place(&[item("lettuce", 15, 8, 3.0)], &[(1, &shape)], &species(), 1);
        assert!(count(&ps, "lettuce") >= 43, "{} laitues", count(&ps, "lettuce"));
    }

    #[test]
    fn harvested_crop_frees_its_spot() {
        // Deux laitues qui remplissent chacune la planche, l'une après l'autre.
        let shape = bed();
        let items = [item("lettuce", 10, 8, 4.0), item("lettuce", 20, 8, 4.0)];
        let ps = place(&items, &[(1, &shape)], &species(), 3);
        let late = ps.iter().filter(|p| p.week == 20).count();
        assert!(late >= 58, "{late} laitues tardives sur 64");
    }

    #[test]
    fn crops_form_patches_leaving_room_for_others() {
        // Radis en touffes sur la moitié de la planche, puis laitue à côté.
        let shape = bed();
        let items = [item("radish", 11, 6, 3.0), item("lettuce", 15, 7, 2.5)];
        let ps = place(&items, &[(1, &shape)], &species(), 5);
        assert!(count(&ps, "lettuce") >= 36, "{} laitues sur 40", count(&ps, "lettuce"));
    }

    #[test]
    fn dense_sowing_capped_as_clumps() {
        let shape = bed();
        let ps = place(&[item("carrot", 12, 15, 2.0)], &[(1, &shape)], &species(), 1);
        assert!(count(&ps, "carrot") <= MAX_PLANTS_PER_ITEM);
        assert!(ps[0].r > 2.5, "touffe plus large qu'un plant");
    }

    #[test]
    fn same_seed_same_layout() {
        let shape = bed();
        let items = [item("tomato", 18, 20, 2.0)];
        let a = place(&items, &[(1, &shape)], &species(), 42);
        let b = place(&items, &[(1, &shape)], &species(), 42);
        assert_eq!(a.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(), b.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>());
    }
}
