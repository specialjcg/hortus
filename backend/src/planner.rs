//! Planificateur de saison : MILP planche × espèce × semaine (good_lp + HiGHS).
//!
//! Variable `a[b,c,w]` = m² de la culture `c` semée/plantée semaine `w` sur la
//! planche `b`. Année cyclique de 52 semaines : un engrais vert semé en octobre
//! occupe la planche jusqu'en avril. Plusieurs espèces partagent une planche
//! (plants mélangés) tant que l'occupation hebdomadaire ≤ surface.

use std::collections::{BTreeMap, HashMap, HashSet};

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
/// Écart relatif accepté à l'optimum : sous ~4 %, HiGHS piétine à prouver
/// une solution déjà trouvée (borne qui stagne) ; 5 % suffit pour un potager.
const MIP_GAP: f32 = 0.05;
/// Production tolérée au-delà du besoin du foyer (pertes, dons, conserves).
const NEED_MARGIN: f64 = 1.1;
/// Une semaine de semis candidate sur N dans chaque fenêtre (plus la dernière).
/// Les semaines voisines rapportent presque autant ; moins d'options = MILP
/// résolu à MIP_GAP en ~8 s au lieu de dépasser 60 s.
const SOW_WEEK_STEP: usize = 4;

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
    /// Taille du foyer : plafonne chaque espèce à ce qu'il consomme.
    pub persons: Option<u32>,
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

/// Besoin annuel du foyer pour une espèce et production prévue.
#[derive(Debug, Serialize)]
pub struct NeedCover {
    pub species_id: String,
    pub need_kg: f64,
    pub planned_kg: f64,
    /// Surface cultivée qu'il faudrait en plus pour couvrir le besoin (une culture par m²).
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
    /// false = limite de temps atteinte, solution non prouvée à MIP_GAP de l'optimum.
    pub optimal: bool,
    /// Besoins du foyer (vide sans `persons`), espèces du plan possibles.
    pub needs: Vec<NeedCover>,
    /// Part des besoins couverts (excédents non comptés) ; `None` sans foyer.
    pub need_cover_ratio: Option<f64>,
    /// Somme des `missing_m2` : planches à ajouter pour 100 % ; `None` sans foyer.
    pub need_missing_m2: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("aucune planche dessinée")]
    NoBeds,
    #[error("solveur : {0}")]
    Solver(#[from] ResolutionError),
}

/// Écartement des lignes de goutte-à-goutte (m) : celui des plants, borné.
const DRIP_LINE_MIN_M: f64 = 0.3;
const DRIP_LINE_MAX_M: f64 = 0.5;

/// Matériel et apports d'une planche pour le plan de l'année.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BedSupplies {
    pub bed_id: i64,
    /// Mètres de gaine goutte-à-goutte (lignes parallèles sur toute la planche).
    pub drip_m: f64,
    /// Écartement retenu entre lignes (m).
    pub drip_spacing_m: f64,
    /// Compost mûr à apporter sur l'année (kg), selon la gourmandise des cultures.
    pub compost_kg: f64,
    /// Surface à pailler (m²).
    pub mulch_m2: f64,
}

/// Compost recommandé (kg/m²) selon la famille : gourmandes, moyennes, sobres.
pub fn compost_kg_per_m2(sp: &Species) -> f64 {
    if sp.green_manure {
        return 0.0;
    }
    match sp.family {
        Family::Solanaceae | Family::Cucurbitaceae | Family::Brassicaceae | Family::Poaceae => 4.0,
        Family::Fabaceae | Family::Alliaceae | Family::Lamiaceae | Family::Apiaceae => 1.0,
        _ => 2.0,
    }
}

/// Goutte-à-goutte, compost et paillis par planche cultivée dans le plan.
pub fn bed_supplies(beds: &[Bed], items: &[PlanItem], species: &[Species]) -> Vec<BedSupplies> {
    let by_id: HashMap<&str, &Species> = species.iter().map(|s| (s.id.as_str(), s)).collect();
    beds.iter()
        .filter_map(|bed| {
            let crops: Vec<(&PlanItem, &Species)> = items
                .iter()
                .filter(|it| it.bed_id == bed.id)
                .filter_map(|it| Some((it, *by_id.get(it.species_id.as_str())?)))
                .filter(|(_, sp)| !sp.green_manure)
                .collect();
            let tightest = crops.iter().map(|(_, sp)| sp.spacing_cm as f64 / 100.0).reduce(f64::min)?;
            let spacing = tightest.clamp(DRIP_LINE_MIN_M, DRIP_LINE_MAX_M);
            let compost: f64 = crops.iter().map(|(it, sp)| it.area_m2 * compost_kg_per_m2(sp)).sum();
            Some(BedSupplies {
                bed_id: bed.id,
                drip_m: (bed.area_m2 / spacing).ceil(),
                drip_spacing_m: spacing,
                compost_kg: compost.round(),
                mulch_m2: (bed.area_m2 * 10.0).round() / 10.0,
            })
        })
        .collect()
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

/// Options élaguées : une semaine sur `SOW_WEEK_STEP`, bornes de fenêtre gardées.
fn sparse_options(crop: &Crop) -> Vec<CropOption> {
    let all = crop_options(crop);
    let last = all.len().saturating_sub(1);
    all.into_iter().enumerate().filter(|(k, _)| k % SOW_WEEK_STEP == 0 || *k == last).map(|(_, o)| o).collect()
}

fn kg_per_m2(sp: &Species, o: &CropOption) -> f64 {
    sp.yield_kg_m2.unwrap_or(0.0) as f64 * o.yield_factor
}

fn eur_per_m2(sp: &Species, o: &CropOption) -> f64 {
    kg_per_m2(sp, o) * sp.price_eur_kg.unwrap_or(DEFAULT_PRICE) as f64
}

/// Besoin annuel du foyer (kg), si l'espèce en a un et produit quelque chose.
fn need_kg(sp: &Species, persons: Option<u32>) -> Option<f64> {
    let need = sp.need_kg_person_year.filter(|_| sp.yield_kg_m2.unwrap_or(0.0) > 0.0)?;
    Some(persons? as f64 * need as f64)
}

fn occupies(o: &CropOption, t: usize) -> bool {
    (t + WEEKS - o.week) % WEEKS < o.weeks_on_bed
}

pub fn plan(beds: &[Bed], crops: &[Crop], history: &[PastCrop], params: &PlanParams) -> Result<Plan, PlanError> {
    if beds.is_empty() {
        return Err(PlanError::NoBeds);
    }
    let total_area: f64 = beds.iter().map(|b| b.area_m2).sum();
    let options: Vec<Vec<CropOption>> = crops.iter().map(sparse_options).collect();

    let blocked: HashSet<(i64, Family)> = history
        .iter()
        .filter(|h| h.year < params.year && params.year - h.year < return_delay_years(h.family))
        .map(|h| (h.bed_id, h.family))
        .collect();

    let mut vars = ProblemVariables::new();
    // y[(b, c)] : la culture c apparaît sur la planche b dans l'année.
    // BTreeMap : ordre des contraintes stable, donc même plan d'un appel à l'autre.
    let mut y: BTreeMap<(usize, usize), Variable> = BTreeMap::new();
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

    // Bonus d'étagement : variable auxiliaire ≤ présences. Pas de bonus par paire
    // de compagnes : ~450 binaires dont la relaxation LP vaut ~500 € fictifs,
    // le solveur ne convergeait plus et semait des bouts de 0,1 m² pour le bonus.
    let strata: Vec<(Variable, usize)> = (0..beds.len()).map(|bi| (vars.add(variable().binary()), bi)).collect();

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
        // Foyer : pas plus que ce qu'il mange (une envie l'emporte).
        if let Some(cap) = need_kg(crop.species, params.persons).filter(|_| wished == 0.0) {
            let kg: Expression = a
                .iter()
                .filter(|((_, c, _), _)| *c == ci)
                .map(|((_, _, k), v)| kg_per_m2(crop.species, &options[ci][*k]) * *v)
                .sum();
            model = model.with(constraint!(kg <= NEED_MARGIN * cap));
        }
    }
    for (ci, m2, slack) in &wish_slacks {
        model = model.with(constraint!(sown(*ci) + *slack >= *m2));
    }
    for bi in 0..beds.len() {
        // Rotation dans l'année : une seule espèce par famille à délai de retour.
        // Ordre d'apparition (pas de HashSet) : modèle identique d'un appel à l'autre.
        let mut families: Vec<Family> = Vec::new();
        for c in crops {
            if !families.contains(&c.species.family) {
                families.push(c.species.family);
            }
        }
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
    let needs: Vec<NeedCover> = crops
        .iter()
        .filter_map(|c| {
            let need = need_kg(c.species, params.persons)?;
            let planned = items.iter().filter(|i| i.species_id == c.species.id).map(|i| i.expected_kg).sum::<f64>();
            let missing_m2 = (need - planned).max(0.0) / c.species.yield_kg_m2.unwrap_or(1.0) as f64;
            Some(NeedCover {
                species_id: c.species.id.clone(),
                need_kg: need,
                planned_kg: (planned * 10.0).round() / 10.0,
                missing_m2: (missing_m2 * 10.0).round() / 10.0,
            })
        })
        .collect();
    let need_total: f64 = needs.iter().map(|n| n.need_kg).sum();
    let need_cover_ratio = (need_total > 0.0)
        .then(|| needs.iter().map(|n| n.planned_kg.min(n.need_kg)).sum::<f64>() / need_total);
    let need_missing_m2 = (need_total > 0.0).then(|| (needs.iter().map(|n| n.missing_m2).sum::<f64>()).round());
    Ok(Plan {
        items,
        total_kg: (total_kg * 10.0).round() / 10.0,
        total_eur,
        unmet_wishes,
        cover_ratio: (covered / (total_area * WEEKS as f64)).min(1.0),
        optimal,
        needs,
        need_cover_ratio,
        need_missing_m2,
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
        PlanParams { year: 2027, max_species_share: 1.0, time_limit_s: 10.0, wishes: vec![], persons: None }
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

    #[test]
    fn household_need_caps_production() {
        // 2 personnes × 10 kg de tomates : au plus 22 kg, la laitue prend le reste.
        let t = sp("tomate", "Solanaceae", 60, Some(5.0), json!({"price_eur_kg": 5.0, "need_kg_person_year": 10.0}));
        let l = sp("laitue", "Asteraceae", 60, Some(3.0), json!({"price_eur_kg": 2.0, "need_kg_person_year": 50.0}));
        let s = [t, l];
        let crops = vec![crop(&s[0], (100, 110)), crop(&s[1], (100, 110))];
        let p = PlanParams { persons: Some(2), ..params() };
        let plan = plan(&[bed(1, 10.0)], &crops, &[], &p).unwrap();
        let tomato: f64 = plan.items.iter().filter(|i| i.species_id == "tomate").map(|i| i.expected_kg).sum();
        assert!(tomato <= 22.0 + 0.5, "{tomato} kg");
        assert!(plan.items.iter().any(|i| i.species_id == "laitue"));
        assert_eq!(plan.needs.len(), 2);
        let ratio = plan.need_cover_ratio.expect("foyer donné");
        assert!(ratio > 0.0 && ratio <= 1.0, "{ratio}");
        // 100 kg de laitue voulus sur 10 m² : il manque de la surface, à 3 kg/m².
        let lettuce = plan.needs.iter().find(|n| n.species_id == "laitue").expect("besoin laitue");
        let expected = (lettuce.need_kg - lettuce.planned_kg) / 3.0;
        assert!((lettuce.missing_m2 - expected).abs() < 0.1, "{} vs {expected}", lettuce.missing_m2);
        assert!(plan.need_missing_m2.expect("foyer donné") >= lettuce.missing_m2.floor());
    }

    #[test]
    fn wish_overrides_household_cap() {
        let t = sp("tomate", "Solanaceae", 60, Some(5.0), json!({"need_kg_person_year": 1.0}));
        let s = [t];
        let crops = vec![crop(&s[0], (100, 110))];
        let p = PlanParams { persons: Some(1), wishes: vec![("tomate".into(), 2.0)], ..params() };
        let plan = plan(&[bed(1, 4.0)], &crops, &[], &p).unwrap();
        assert!(plan.unmet_wishes.is_empty(), "{:?}", plan.unmet_wishes);
    }

    #[test]
    fn no_household_no_needs() {
        let t = sp("tomate", "Solanaceae", 60, Some(5.0), json!({"need_kg_person_year": 10.0}));
        let s = [t];
        let plan = plan(&[bed(1, 2.0)], &[crop(&s[0], (100, 110))], &[], &params()).unwrap();
        assert!(plan.needs.is_empty());
        assert!(plan.need_cover_ratio.is_none());
        assert!(plan.need_missing_m2.is_none());
    }

    #[test]
    fn bed_supplies_follow_spacing_and_appetite() {
        let catalog = Species::load_from_json("data/species.json").expect("species.json valide");
        let heavy = catalog.iter().find(|s| s.family == Family::Solanaceae && !s.green_manure).expect("une solanacée");
        let light = catalog.iter().find(|s| s.family == Family::Fabaceae && !s.green_manure).expect("une fabacée");
        let beds = vec![
            Bed { id: 1, name: "A".into(), area_m2: 6.0 },
            Bed { id: 2, name: "B".into(), area_m2: 4.0 },
        ];
        let item = |bed_id, sp: &Species, area_m2| PlanItem {
            bed_id,
            species_id: sp.id.clone(),
            mode: Mode::Direct,
            week: 10,
            weeks_on_bed: 10,
            area_m2,
            expected_kg: 0.0,
            expected_eur: 0.0,
        };
        let items = vec![item(1, heavy, 6.0), item(1, light, 2.0)];
        let out = bed_supplies(&beds, &items, &catalog);
        assert_eq!(out.len(), 1, "planche sans culture ignorée");
        let a = &out[0];
        assert!((DRIP_LINE_MIN_M..=DRIP_LINE_MAX_M).contains(&a.drip_spacing_m));
        assert_eq!(a.drip_m, (6.0 / a.drip_spacing_m).ceil());
        assert_eq!(a.compost_kg, 26.0);
        assert_eq!(a.mulch_m2, 6.0);
    }
}
