//! Planificateur de saison : MILP planche × espèce × semaine (good_lp + HiGHS).
//!
//! Variable `a[b,c,w]` = m² de la culture `c` semée/plantée semaine `w` sur la
//! planche `b`. Année cyclique de 52 semaines : un engrais vert semé en octobre
//! occupe la planche jusqu'en avril. Plusieurs espèces partagent une planche
//! (plants mélangés) tant que l'occupation hebdomadaire ≤ surface.

use std::collections::{HashMap, HashSet};

use good_lp::{
    constraint, highs, variable, Expression, ProblemVariables, ResolutionError, Solution,
    SolutionStatus, SolverModel, Variable,
};
use serde::Serialize;

use crate::domain::species::{CalendarWindow, Category, Family, HeightClass, Species};
use crate::journal::{self, Action, Parcel};

pub const WEEKS: usize = 52;
/// Espèce semable en direct ET repiquable : le plant repiqué gagne ~3 semaines
/// en planche. Sans semis direct, `days_to_harvest` compte déjà depuis le repiquage.
const TRANSPLANT_HEAD_START_DAYS: u16 = 21;
/// Récolte d'une culture à récolte unique : 2 semaines en place après maturité.
const SINGLE_HARVEST_DAYS: u16 = 14;
/// Objectif en € de récolte. Bonus « sol couvert » par m²·semaine.
const W_COVER: f64 = 0.04;
/// Bonus par paire de compagnes sur une même planche (€).
const W_FRIEND: f64 = 1.2;
/// Bonus d'étagement : une strate haute et une strate rase sur une planche (€).
const W_STRATA: f64 = 1.2;
/// Pénalité par m² d'envie non satisfaite : domine toute valeur de récolte.
const W_WISH: f64 = 1000.0;
/// Prix par défaut quand l'espèce n'en a pas (€/kg).
const DEFAULT_PRICE: f32 = 3.0;
/// Les aromatiques se consomment peu : plafond propre (part de la surface).
const HERB_MAX_SHARE: f64 = 0.05;
/// Parcelles plus petites ignorées à la lecture du plan (bruit du solveur).
const MIN_PATCH_M2: f64 = 0.1;
/// Écart relatif accepté à l'optimum : 1 % suffit pour un potager.
const MIP_GAP: f32 = 0.01;

pub struct Bed {
    pub id: i64,
    pub name: String,
    pub area_m2: f64,
}

/// Espèce avec fenêtres déjà décalées au climat local.
pub struct Crop<'a> {
    pub species: &'a Species,
    pub direct: Option<CalendarWindow>,
    pub transplant: Option<CalendarWindow>,
    pub harvest: CalendarWindow,
}

/// Culture passée sur une planche (rotation).
pub struct PastCrop {
    pub bed_id: i64,
    pub year: i32,
    pub family: Family,
}

pub struct PlanParams {
    pub year: i32,
    /// Part max de la surface totale semée par espèce (diversité), hors engrais verts.
    pub max_species_share: f64,
    pub time_limit_s: f64,
    /// Envies : (espèce, m² minimum semés dans l'année). Contrainte souple.
    pub wishes: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Direct,
    Transplant,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanItem {
    pub bed_id: i64,
    pub species_id: String,
    pub mode: Mode,
    /// Semaine de semis/plantation (1..=52).
    pub week: u8,
    /// Nombre de semaines d'occupation de la planche.
    pub weeks_on_bed: u8,
    pub area_m2: f64,
    pub expected_kg: f64,
    pub expected_eur: f64,
}

#[derive(Debug, Serialize)]
pub struct UnmetWish {
    pub species_id: String,
    pub missing_m2: f64,
}

#[derive(Debug, Serialize)]
pub struct Plan {
    pub items: Vec<PlanItem>,
    pub total_kg: f64,
    pub total_eur: f64,
    /// Envies impossibles (fenêtre, rotation, antagonistes) ou partielles.
    pub unmet_wishes: Vec<UnmetWish>,
    /// Part des m²·semaines couverts sur l'année.
    pub cover_ratio: f64,
    /// false = limite de temps atteinte, solution non prouvée à 1 % de l'optimum.
    pub optimal: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("aucune planche dessinée")]
    NoBeds,
    #[error("solveur : {0}")]
    Solver(#[from] ResolutionError),
}

/// Années minimales avant de remettre la même famille sur une planche (0 = libre).
pub fn return_delay_years(f: Family) -> i32 {
    match f {
        Family::Solanaceae | Family::Brassicaceae => 4,
        Family::Alliaceae
        | Family::Cucurbitaceae
        | Family::Apiaceae
        | Family::Fabaceae
        | Family::Amaranthaceae
        | Family::Chenopodiaceae => 3,
        Family::Asteraceae => 2,
        _ => 0,
    }
}

/// Une façon de cultiver `c` : semaine de départ, mode, occupation, part de rendement.
#[derive(Debug, Clone, PartialEq)]
struct CropOption {
    week: usize,
    mode: Mode,
    weeks_on_bed: usize,
    yield_factor: f64,
}

fn wrap_doy(doy: u32) -> u16 {
    (((doy - 1) % 365) + 1) as u16
}

/// Jours de `from` à `to` en avançant dans l'année (cyclique).
fn days_forward(from: u16, to: u16) -> u16 {
    if to >= from {
        to - from
    } else {
        365 - from + to
    }
}

fn window_len(w: CalendarWindow) -> u16 {
    days_forward(w.doy_start, w.doy_end) + 1
}

fn crop_options(crop: &Crop) -> Vec<CropOption> {
    let sp = crop.species;
    (0..WEEKS)
        .filter_map(|week| {
            let mid = (7 * week + 4) as u16;
            let direct = crop.direct.filter(|w| w.contains(mid)).map(|_| (Mode::Direct, sp.days_to_harvest));
            let transplant = crop
                .transplant
                .filter(|w| w.contains(mid))
                .map(|_| {
                    let head_start = if sp.direct_sow.is_some() { TRANSPLANT_HEAD_START_DAYS } else { 0 };
                    (Mode::Transplant, sp.days_to_harvest.saturating_sub(head_start).max(21))
                });
            // Les deux possibles : le repiquage libère la planche plus tôt.
            let (mode, days_on_bed) = [direct, transplant].into_iter().flatten().min_by_key(|(_, d)| *d)?;
            let maturity = wrap_doy(mid as u32 + days_on_bed as u32);
            if !crop.harvest.contains(maturity) {
                return None;
            }
            let (harvest_days, yield_factor) = if sp.multi_harvest {
                let span = days_forward(maturity, crop.harvest.doy_end);
                let f = ((span + SINGLE_HARVEST_DAYS) as f64 / window_len(crop.harvest) as f64).clamp(0.2, 1.0);
                (span.max(SINGLE_HARVEST_DAYS), f)
            } else {
                (SINGLE_HARVEST_DAYS, 1.0)
            };
            let weeks_on_bed = ((days_on_bed + harvest_days) as usize).div_ceil(7).clamp(1, WEEKS);
            Some(CropOption { week, mode, weeks_on_bed, yield_factor })
        })
        .collect()
}

fn eur_per_m2(sp: &Species, o: &CropOption) -> f64 {
    (sp.yield_kg_m2.unwrap_or(0.0) * sp.price_eur_kg.unwrap_or(DEFAULT_PRICE)) as f64 * o.yield_factor
}

fn occupies(o: &CropOption, t: usize) -> bool {
    (t + WEEKS - o.week) % WEEKS < o.weeks_on_bed
}

pub fn plan(beds: &[Bed], crops: &[Crop], history: &[PastCrop], params: &PlanParams) -> Result<Plan, PlanError> {
    if beds.is_empty() {
        return Err(PlanError::NoBeds);
    }
    let total_area: f64 = beds.iter().map(|b| b.area_m2).sum();
    let options: Vec<Vec<CropOption>> = crops.iter().map(crop_options).collect();

    let blocked: HashSet<(i64, Family)> = history
        .iter()
        .filter(|h| h.year < params.year && params.year - h.year < return_delay_years(h.family))
        .map(|h| (h.bed_id, h.family))
        .collect();

    let mut vars = ProblemVariables::new();
    // y[(b, c)] : la culture c apparaît sur la planche b dans l'année.
    let mut y: HashMap<(usize, usize), Variable> = HashMap::new();
    // a[(b, c, k)] : m² pour l'option k.
    let mut a: Vec<((usize, usize, usize), Variable)> = Vec::new();
    for (bi, bed) in beds.iter().enumerate() {
        for (ci, crop) in crops.iter().enumerate() {
            if options[ci].is_empty() || blocked.contains(&(bed.id, crop.species.family)) {
                continue;
            }
            y.insert((bi, ci), vars.add(variable().binary()));
            for k in 0..options[ci].len() {
                a.push(((bi, ci, k), vars.add(variable().min(0.0).max(bed.area_m2))));
            }
        }
    }

    let used = |bi: usize, pred: &dyn Fn(&Species) -> bool| -> Expression {
        y.iter()
            .filter(|((b, c), _)| *b == bi && pred(crops[*c].species))
            .map(|(_, v)| *v)
            .sum()
    };

    // Bonus compagnes et étagement : variables auxiliaires ≤ présences.
    let mut friend_pairs: Vec<(Variable, Variable, Variable)> = Vec::new();
    let mut strata: Vec<(Variable, usize)> = Vec::new();
    for bi in 0..beds.len() {
        for (c1, s1) in crops.iter().enumerate() {
            for (c2, s2) in crops.iter().enumerate().skip(c1 + 1) {
                let friends = s1.species.friends.contains(&s2.species.id) || s2.species.friends.contains(&s1.species.id);
                if let (true, Some(&y1), Some(&y2)) = (friends, y.get(&(bi, c1)), y.get(&(bi, c2))) {
                    friend_pairs.push((vars.add(variable().binary()), y1, y2));
                }
            }
        }
        strata.push((vars.add(variable().binary()), bi));
    }

    let mut objective = Expression::default();
    for ((_, ci, k), v) in &a {
        let o = &options[*ci][*k];
        objective += (eur_per_m2(crops[*ci].species, o) + W_COVER * o.weeks_on_bed as f64) * *v;
    }
    // Envies : manque de m² pénalisé (souple → toujours un plan, manques signalés).
    let wish_slacks: Vec<(usize, f64, Variable)> = params
        .wishes
        .iter()
        .filter_map(|(id, m2)| {
            let ci = crops.iter().position(|c| &c.species.id == id);
            Some((ci?, *m2, vars.add(variable().min(0.0).max(*m2))))
        })
        .collect();
    for (_, _, slack) in &wish_slacks {
        objective -= W_WISH * *slack;
    }
    for (z, _, _) in &friend_pairs {
        objective += W_FRIEND * *z;
    }
    for (s, _) in &strata {
        objective += W_STRATA * *s;
    }

    let mut model = vars
        .maximise(objective)
        .using(highs)
        .set_time_limit(params.time_limit_s)
        .set_mip_rel_gap(MIP_GAP)
        .expect("écart MIP constant valide");
    model.set_verbose(false);

    // Occupation hebdomadaire ≤ surface de la planche.
    for (bi, bed) in beds.iter().enumerate() {
        for t in 0..WEEKS {
            let load: Expression = a
                .iter()
                .filter(|((b, c, k), _)| *b == bi && occupies(&options[*c][*k], t))
                .map(|(_, v)| *v)
                .sum();
            model = model.with(constraint!(load <= bed.area_m2));
        }
    }
    // a > 0 seulement si la culture est retenue sur la planche.
    for ((bi, ci, _), v) in &a {
        let yv = y[&(*bi, *ci)];
        model = model.with(constraint!(*v <= beds[*bi].area_m2 * yv));
    }
    let sown = |ci: usize| -> Expression { a.iter().filter(|((_, c, _), _)| *c == ci).map(|(_, v)| *v).sum() };
    // Diversité : surface semée par espèce plafonnée (engrais verts exclus),
    // relevée au besoin pour honorer une envie.
    for (ci, crop) in crops.iter().enumerate() {
        if crop.species.green_manure {
            continue;
        }
        let share = if crop.species.category == Category::Herb {
            HERB_MAX_SHARE.min(params.max_species_share)
        } else {
            params.max_species_share
        };
        let wished = wish_slacks.iter().filter(|(c, _, _)| *c == ci).map(|(_, m2, _)| *m2).fold(0.0, f64::max);
        model = model.with(constraint!(sown(ci) <= (share * total_area).max(wished)));
    }
    for (ci, m2, slack) in &wish_slacks {
        model = model.with(constraint!(sown(*ci) + *slack >= *m2));
    }
    for bi in 0..beds.len() {
        // Rotation dans l'année : une seule espèce par famille à délai de retour.
        let families: HashSet<Family> = crops.iter().map(|c| c.species.family).collect();
        for f in families.into_iter().filter(|f| return_delay_years(*f) > 0) {
            let n = used(bi, &|s: &Species| s.family == f);
            model = model.with(constraint!(n <= 1));
        }
        // Antagonistes jamais sur la même planche.
        for (c1, s1) in crops.iter().enumerate() {
            for (c2, s2) in crops.iter().enumerate().skip(c1 + 1) {
                let foes = s1.species.foes.contains(&s2.species.id) || s2.species.foes.contains(&s1.species.id);
                if let (true, Some(&y1), Some(&y2)) = (foes, y.get(&(bi, c1)), y.get(&(bi, c2))) {
                    model = model.with(constraint!(y1 + y2 <= 1));
                }
            }
        }
    }
    for (z, y1, y2) in &friend_pairs {
        model = model.with(constraint!(*z <= *y1)).with(constraint!(*z <= *y2));
    }
    for (s, bi) in &strata {
        let tall = used(*bi, &|sp: &Species| sp.height() == HeightClass::Tall);
        let ground = used(*bi, &|sp: &Species| sp.height() == HeightClass::Ground);
        model = model.with(constraint!(*s <= tall)).with(constraint!(*s <= ground));
    }

    let solution = model.solve()?;
    // GapLimit = optimum à MIP_GAP près ; seul TimeLimit laisse un plan non prouvé.
    let optimal = !matches!(solution.status(), SolutionStatus::TimeLimit);

    let mut items: Vec<PlanItem> = a
        .iter()
        .filter_map(|((bi, ci, k), v)| {
            let area = solution.value(*v);
            if area < MIN_PATCH_M2 {
                return None;
            }
            let o = &options[*ci][*k];
            let sp = crops[*ci].species;
            let kg = area * sp.yield_kg_m2.unwrap_or(0.0) as f64 * o.yield_factor;
            Some(PlanItem {
                bed_id: beds[*bi].id,
                species_id: sp.id.clone(),
                mode: o.mode,
                week: (o.week + 1) as u8,
                weeks_on_bed: o.weeks_on_bed as u8,
                area_m2: (area * 100.0).round() / 100.0,
                expected_kg: (kg * 10.0).round() / 10.0,
                expected_eur: (kg * sp.price_eur_kg.unwrap_or(DEFAULT_PRICE) as f64).round(),
            })
        })
        .collect();
    items.sort_by_key(|i| (i.bed_id, i.week));

    let total_kg = items.iter().map(|i| i.expected_kg).sum::<f64>();
    let total_eur = items.iter().map(|i| i.expected_eur).sum::<f64>();
    let unmet_wishes = wish_slacks
        .iter()
        .map(|(ci, _, slack)| (ci, solution.value(*slack)))
        .filter(|(_, missing)| *missing > MIN_PATCH_M2)
        .map(|(ci, missing)| UnmetWish { species_id: crops[*ci].species.id.clone(), missing_m2: (missing * 10.0).round() / 10.0 })
        .collect();
    let covered: f64 = items.iter().map(|i| i.area_m2 * i.weeks_on_bed as f64).sum();
    Ok(Plan {
        items,
        total_kg: (total_kg * 10.0).round() / 10.0,
        total_eur,
        unmet_wishes,
        cover_ratio: (covered / (total_area * WEEKS as f64)).min(1.0),
        optimal,
    })
}

const PLANTING_KINDS: [&str; 2] = ["semis_direct", "repiquage"];

/// Plant encore en place : aucun arrachage ni mort aux mêmes coordonnées depuis.
fn still_standing(a: &Action, actions: &[Action]) -> bool {
    !actions.iter().any(|b| {
        matches!(b.kind.as_str(), "arrachage" | "plant_mort")
            && b.grid_x == a.grid_x
            && b.grid_y == a.grid_y
            && b.date >= a.date
    })
}

/// Planches dessinées ; surface diminuée de l'emprise des vivaces en place.
pub fn beds_from_parcels(parcels: &[Parcel], actions: &[Action], species: &[Species]) -> Vec<Bed> {
    let by_id: HashMap<&str, &Species> = species.iter().map(|s| (s.id.as_str(), s)).collect();
    parcels
        .iter()
        .filter(|p| p.shape.is_some())
        .map(|p| {
            let perennial_m2: f64 = actions
                .iter()
                .filter(|a| PLANTING_KINDS.contains(&a.kind.as_str()))
                .filter_map(|a| {
                    let sp = by_id.get(a.species_id.as_deref()?)?;
                    let (x, y) = (a.grid_x?, a.grid_y?);
                    let inside = journal::parcel_at(std::slice::from_ref(p), x, y).is_some();
                    (sp.is_perennial() && inside && still_standing(a, actions))
                        .then(|| (sp.spacing_cm as f64 / 100.0).powi(2))
                })
                .sum();
            Bed {
                id: p.id,
                name: p.name.clone(),
                area_m2: (p.surface_m2.unwrap_or(0.0) - perennial_m2).max(0.0),
            }
        })
        .filter(|b| b.area_m2 > 0.0)
        .collect()
}

/// Familles cultivées par planche et par année, d'après les semis/repiquages du carnet.
pub fn history_from_actions(parcels: &[Parcel], actions: &[Action], species: &[Species]) -> Vec<PastCrop> {
    let by_id: HashMap<&str, &Species> = species.iter().map(|s| (s.id.as_str(), s)).collect();
    actions
        .iter()
        .filter(|a| PLANTING_KINDS.contains(&a.kind.as_str()))
        .filter_map(|a| {
            let sp = by_id.get(a.species_id.as_deref()?)?;
            let bed_id = journal::parcel_at(parcels, a.grid_x?, a.grid_y?)?;
            let year = a.date.get(..4)?.parse().ok()?;
            Some(PastCrop { bed_id, year, family: sp.family })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sp(id: &str, family: &str, days: u16, yield_kg: Option<f32>, extra: serde_json::Value) -> Species {
        let mut v = json!({
            "id": id, "name_fr": id, "name_latin": id, "family": family,
            "life_cycle": "Annual", "category": "LeafyVegetable", "difficulty": "Beginner",
            "indoor_sow": null, "direct_sow": null, "transplant": null,
            "harvest": { "doy_start": 1, "doy_end": 365 },
            "depth_cm": 1.0, "spacing_cm": 20, "days_to_harvest": days,
            "yield_kg_m2": yield_kg,
        });
        for (k, val) in extra.as_object().expect("objet").iter() {
            v[k] = val.clone();
        }
        serde_json::from_value(v).expect("Species de test")
    }

    fn crop(s: &Species, direct: (u16, u16)) -> Crop<'_> {
        Crop {
            species: s,
            direct: Some(CalendarWindow { doy_start: direct.0, doy_end: direct.1 }),
            transplant: None,
            harvest: s.harvest,
        }
    }

    fn bed(id: i64, area: f64) -> Bed {
        Bed { id, name: format!("P{id}"), area_m2: area }
    }

    fn params() -> PlanParams {
        PlanParams { year: 2027, max_species_share: 1.0, time_limit_s: 10.0, wishes: vec![] }
    }

    /// Occupation hebdomadaire reconstruite depuis le plan.
    fn load(plan: &Plan, bed_id: i64) -> Vec<f64> {
        (0..WEEKS)
            .map(|t| {
                plan.items
                    .iter()
                    .filter(|i| i.bed_id == bed_id)
                    .filter(|i| (t + WEEKS - (i.week as usize - 1)) % WEEKS < i.weeks_on_bed as usize)
                    .map(|i| i.area_m2)
                    .sum()
            })
            .collect()
    }

    #[test]
    fn options_respect_window_and_harvest() {
        let s = sp("radis", "Brassicaceae", 30, Some(1.5), json!({}));
        let opts = crop_options(&crop(&s, (60, 120)));
        assert!(!opts.is_empty());
        assert!(opts.iter().all(|o| (60..=120).contains(&((7 * o.week + 4) as u16))));
        assert!(opts.iter().all(|o| o.weeks_on_bed == 7)); // 30 j + 14 j de récolte
    }

    #[test]
    fn options_skip_sowing_that_matures_out_of_harvest_window() {
        let s = sp("x", "Asteraceae", 60, Some(1.0), json!({"harvest": {"doy_start": 150, "doy_end": 200}}));
        let opts = crop_options(&crop(&s, (1, 365)));
        assert!(opts.iter().all(|o| {
            let m = wrap_doy((7 * o.week + 4 + 60) as u32);
            (150..=200).contains(&m)
        }));
    }

    #[test]
    fn transplant_only_counts_days_from_planting() {
        // Tomate : repiquée S19, mûre 80 j plus tard → dans la fenêtre de récolte.
        let s = sp("tomate", "Solanaceae", 80, Some(5.0), json!({"harvest": {"doy_start": 200, "doy_end": 290}}));
        let c = Crop { species: &s, direct: None, transplant: Some(CalendarWindow { doy_start: 130, doy_end: 150 }), harvest: s.harvest };
        let opts = crop_options(&c);
        assert!(!opts.is_empty());
        assert!(opts.iter().all(|o| o.mode == Mode::Transplant));
    }

    #[test]
    fn transplant_shortens_time_on_bed_when_direct_sow_exists() {
        let s = sp("laitue", "Asteraceae", 60, Some(3.0), json!({"direct_sow": {"doy_start": 60, "doy_end": 200}}));
        let w = Some(CalendarWindow { doy_start: 100, doy_end: 110 });
        let c = Crop { species: &s, direct: w, transplant: w, harvest: s.harvest };
        let o = &crop_options(&c)[0];
        assert_eq!(o.mode, Mode::Transplant);
        assert_eq!(o.weeks_on_bed, (39usize + 14).div_ceil(7));
    }

    #[test]
    fn plan_never_exceeds_bed_area() {
        let r = sp("radis", "Brassicaceae", 30, Some(1.5), json!({}));
        let l = sp("laitue", "Asteraceae", 60, Some(3.0), json!({}));
        let s = [r, l];
        let crops = vec![crop(&s[0], (60, 250)), crop(&s[1], (60, 220))];
        let plan = plan(&[bed(1, 2.0)], &crops, &[], &params()).unwrap();
        assert!(plan.total_kg > 0.0);
        assert!(load(&plan, 1).iter().all(|&w| w <= 2.0 + 1e-6), "{:?}", load(&plan, 1));
    }

    #[test]
    fn foes_never_share_a_bed() {
        let t = sp("tomate", "Solanaceae", 90, Some(5.0), json!({"foes": ["fenouil"]}));
        let f = sp("fenouil", "Apiaceae", 90, Some(5.0), json!({}));
        let s = [t, f];
        let crops = vec![crop(&s[0], (100, 160)), crop(&s[1], (100, 160))];
        let plan = plan(&[bed(1, 2.0)], &crops, &[], &params()).unwrap();
        let ids: HashSet<_> = plan.items.iter().map(|i| i.species_id.as_str()).collect();
        assert_eq!(ids.len(), 1, "{ids:?}");
    }

    #[test]
    fn rotation_history_blocks_family() {
        let t = sp("tomate", "Solanaceae", 90, Some(5.0), json!({}));
        let l = sp("laitue", "Asteraceae", 60, Some(1.0), json!({}));
        let s = [t, l];
        let crops = vec![crop(&s[0], (100, 160)), crop(&s[1], (60, 220))];
        let history = [PastCrop { bed_id: 1, year: 2025, family: Family::Solanaceae }];
        let plan = plan(&[bed(1, 2.0)], &crops, &history, &params()).unwrap();
        assert!(plan.items.iter().all(|i| i.species_id != "tomate"));
        assert!(plan.items.iter().any(|i| i.species_id == "laitue"));
    }

    #[test]
    fn old_history_no_longer_blocks() {
        let t = sp("tomate", "Solanaceae", 90, Some(5.0), json!({}));
        let s = [t];
        let crops = vec![crop(&s[0], (100, 160))];
        let history = [PastCrop { bed_id: 1, year: 2022, family: Family::Solanaceae }];
        let plan = plan(&[bed(1, 2.0)], &crops, &history, &params()).unwrap();
        assert!(plan.items.iter().any(|i| i.species_id == "tomate"));
    }

    #[test]
    fn species_share_cap_forces_diversity() {
        let t = sp("tomate", "Solanaceae", 90, Some(5.0), json!({}));
        let l = sp("laitue", "Asteraceae", 60, Some(1.0), json!({}));
        let s = [t, l];
        let crops = vec![crop(&s[0], (100, 160)), crop(&s[1], (60, 220))];
        let p = PlanParams { max_species_share: 0.5, ..params() };
        let plan = plan(&[bed(1, 4.0)], &crops, &[], &p).unwrap();
        let tomato: f64 = plan.items.iter().filter(|i| i.species_id == "tomate").map(|i| i.area_m2).sum();
        assert!(tomato <= 2.0 + 1e-6);
        assert!(plan.items.iter().any(|i| i.species_id == "laitue"));
    }

    #[test]
    fn green_manure_covers_winter_gap() {
        let t = sp("tomate", "Solanaceae", 90, Some(5.0), json!({"harvest": {"doy_start": 180, "doy_end": 280}}));
        let g = sp("seigle", "Poaceae", 180, None, json!({"green_manure": true, "category": "GreenManure", "harvest": {"doy_start": 60, "doy_end": 130}}));
        let s = [t, g];
        let crops = vec![crop(&s[0], (120, 160)), crop(&s[1], (258, 305))];
        let plan = plan(&[bed(1, 2.0)], &crops, &[], &params()).unwrap();
        assert!(plan.items.iter().any(|i| i.species_id == "seigle"), "{:?}", plan.items);
        assert!(plan.cover_ratio > 0.6, "cover {}", plan.cover_ratio);
    }

    #[test]
    fn no_beds_is_an_error() {
        assert!(matches!(plan(&[], &[], &[], &params()), Err(PlanError::NoBeds)));
    }

    #[test]
    fn price_favours_valuable_crop() {
        // Même rendement, même cycle : la tomate (5 €/kg) bat le navet (2 €/kg).
        let t = sp("tomate", "Solanaceae", 60, Some(3.0), json!({"price_eur_kg": 5.0}));
        let n = sp("navet", "Brassicaceae", 60, Some(3.0), json!({"price_eur_kg": 2.0}));
        let s = [t, n];
        let crops = vec![crop(&s[0], (100, 110)), crop(&s[1], (100, 110))];
        let plan = plan(&[bed(1, 2.0)], &crops, &[], &params()).unwrap();
        assert!(plan.items.iter().all(|i| i.species_id == "tomate"), "{:?}", plan.items);
        assert!(plan.total_eur > 0.0);
    }

    #[test]
    fn wish_forces_low_value_crop() {
        let t = sp("tomate", "Solanaceae", 60, Some(3.0), json!({"price_eur_kg": 5.0}));
        let n = sp("navet", "Brassicaceae", 60, Some(3.0), json!({"price_eur_kg": 2.0}));
        let s = [t, n];
        let crops = vec![crop(&s[0], (100, 110)), crop(&s[1], (100, 110))];
        let p = PlanParams { wishes: vec![("navet".into(), 1.0)], ..params() };
        let plan = plan(&[bed(1, 2.0)], &crops, &[], &p).unwrap();
        let navet: f64 = plan.items.iter().filter(|i| i.species_id == "navet").map(|i| i.area_m2).sum();
        assert!(navet >= 1.0 - 1e-6, "{:?}", plan.items);
        assert!(plan.unmet_wishes.is_empty());
    }

    #[test]
    fn impossible_wish_is_reported_not_fatal() {
        let t = sp("tomate", "Solanaceae", 60, Some(3.0), json!({}));
        let s = [t];
        let crops = vec![crop(&s[0], (100, 110))];
        let history = [PastCrop { bed_id: 1, year: 2026, family: Family::Solanaceae }];
        let p = PlanParams { wishes: vec![("tomate".into(), 1.0)], ..params() };
        let plan = plan(&[bed(1, 2.0)], &crops, &history, &p).unwrap();
        assert_eq!(plan.unmet_wishes.len(), 1);
        assert_eq!(plan.unmet_wishes[0].missing_m2, 1.0);
    }

    #[test]
    fn herbs_capped_lower_than_vegetables() {
        let b = sp("basilic", "Lamiaceae", 60, Some(1.0), json!({"price_eur_kg": 30.0, "category": "Herb"}));
        let s = [b];
        let crops = vec![crop(&s[0], (100, 200))];
        let plan = plan(&[bed(1, 10.0)], &crops, &[], &params()).unwrap();
        let sown: f64 = plan.items.iter().map(|i| i.area_m2).sum();
        assert!(sown <= 0.5 + 1e-6, "sown {sown}");
    }
}
