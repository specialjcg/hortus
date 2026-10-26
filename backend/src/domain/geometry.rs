//! Géométrie des planches : polygones sur le terrain (pixels SVG 800×560).

use serde::{Deserialize, Serialize};

/// Dimensions du terrain dans l'UI (pixels SVG).
pub const FIELD_W: i64 = 800;
pub const FIELD_H: i64 = 560;
/// Échelle terrain, alignée sur l'UI : 1 px = 1 cm (8 m × 5,6 m).
pub const PX_PER_M: f64 = 100.0;

pub type Pt = [i64; 2];

/// Largeur minimale d'une planche générée (la dernière bande peut être plus étroite).
const MIN_BED_W_CM: i64 = 60;
/// Surface minimale d'une planche générée ; en dessous, morceau écarté.
const MIN_BED_M2: f64 = 0.5;
/// Pas d'échantillonnage de la largeur d'une bande.
const SAMPLE_STEP_CM: i64 = 5;

/// Polygone simple (non croisé), ≥ 3 sommets, inclus dans le terrain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Vec<Pt>", into = "Vec<Pt>")]
pub struct Polygon(Vec<Pt>);

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ShapeError {
    #[error("au moins 3 sommets requis")]
    TooFewVertices,
    #[error("sommet hors du terrain : ({0}, {1})")]
    OutOfBounds(i64, i64),
    #[error("surface nulle")]
    Degenerate,
    #[error("contour croisé")]
    SelfIntersecting,
}

impl TryFrom<Vec<Pt>> for Polygon {
    type Error = ShapeError;

    fn try_from(pts: Vec<Pt>) -> Result<Self, Self::Error> {
        if pts.len() < 3 {
            return Err(ShapeError::TooFewVertices);
        }
        if let Some(&[x, y]) = pts
            .iter()
            .find(|[x, y]| !(0..=FIELD_W).contains(x) || !(0..=FIELD_H).contains(y))
        {
            return Err(ShapeError::OutOfBounds(x, y));
        }
        // Croisement d'abord : un nœud papillon symétrique a une aire signée nulle.
        if self_intersects(&pts) {
            return Err(ShapeError::SelfIntersecting);
        }
        if twice_signed_area(&pts) == 0 {
            return Err(ShapeError::Degenerate);
        }
        Ok(Polygon(pts))
    }
}

impl From<Polygon> for Vec<Pt> {
    fn from(p: Polygon) -> Self {
        p.0
    }
}

impl Polygon {
    pub fn vertices(&self) -> &[Pt] {
        &self.0
    }

    /// Surface en px² (formule du lacet).
    pub fn area_px2(&self) -> f64 {
        twice_signed_area(&self.0).unsigned_abs() as f64 / 2.0
    }

    pub fn area_m2(&self) -> f64 {
        self.area_px2() / (PX_PER_M * PX_PER_M)
    }

    /// Point dans le polygone (lancer de rayon horizontal).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let pts = &self.0;
        let mut inside = false;
        let mut j = pts.len() - 1;
        for i in 0..pts.len() {
            let (xi, yi) = (pts[i][0] as f64, pts[i][1] as f64);
            let (xj, yj) = (pts[j][0] as f64, pts[j][1] as f64);
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// Boîte englobante (x_min, y_min, x_max, y_max).
    pub fn bbox(&self) -> (i64, i64, i64, i64) {
        self.0.iter().fold((i64::MAX, i64::MAX, i64::MIN, i64::MIN), |(a, b, c, d), &[x, y]| {
            (a.min(x), b.min(y), c.max(x), d.max(y))
        })
    }

    fn rect(x0: i64, y0: i64, x1: i64, y1: i64) -> Result<Self, ShapeError> {
        Polygon::try_from(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
    }
}

/// Planches rectangulaires en bandes nord-sud (`bed_cm` de large, `path_cm`
/// d'allée) remplissant `zone`, à au moins une allée des planches `taken`.
/// Rectangles inscrits : faciles à tracer au cordeau, les bords obliques
/// de la zone sont perdus.
pub fn strip_beds(zone: &Polygon, taken: &[Polygon], bed_cm: i64, path_cm: i64) -> Vec<Polygon> {
    let (x_min, y_min, x_max, y_max) = zone.bbox();
    let p = path_cm as f64;
    let near_taken = |x: f64, y: f64| {
        taken.iter().any(|t| {
            [(x, y), (x - p, y), (x + p, y), (x, y - p), (x, y + p)]
                .iter()
                .any(|&(tx, ty)| t.contains(tx, ty))
        })
    };
    let usable = |x: f64, y: f64| zone.contains(x, y) && !near_taken(x, y);

    let mut beds = Vec::new();
    let mut x0 = x_min;
    while x0 + MIN_BED_W_CM <= x_max {
        let x1 = (x0 + bed_cm).min(x_max);
        let xs: Vec<f64> = (x0..x1)
            .step_by(SAMPLE_STEP_CM as usize)
            .map(|x| x as f64 + 0.5)
            .chain([x1 as f64 - 0.5])
            .collect();
        // Une ligne (1 cm) est cultivable si toute la largeur de la bande l'est.
        let row_ok = |y: i64| xs.iter().all(|&x| usable(x, y as f64 + 0.5));
        let mut y = y_min;
        while y < y_max {
            if !row_ok(y) {
                y += 1;
                continue;
            }
            let start = y;
            while y < y_max && row_ok(y) {
                y += 1;
            }
            if let Ok(bed) = Polygon::rect(x0, start, x1, y) {
                if bed.area_m2() >= MIN_BED_M2 {
                    beds.push(bed);
                }
            }
        }
        x0 = x1 + path_cm;
    }
    beds
}

fn twice_signed_area(pts: &[Pt]) -> i64 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let [x1, y1] = pts[i];
            let [x2, y2] = pts[(i + 1) % n];
            x1 * y2 - x2 * y1
        })
        .sum()
}

fn cross(o: Pt, a: Pt, b: Pt) -> i64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn on_segment(p: Pt, q: Pt, r: Pt) -> bool {
    q[0] >= p[0].min(r[0]) && q[0] <= p[0].max(r[0]) && q[1] >= p[1].min(r[1]) && q[1] <= p[1].max(r[1])
}

fn segments_intersect(p1: Pt, p2: Pt, p3: Pt, p4: Pt) -> bool {
    let d1 = cross(p3, p4, p1).signum();
    let d2 = cross(p3, p4, p2).signum();
    let d3 = cross(p1, p2, p3).signum();
    let d4 = cross(p1, p2, p4).signum();
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    (d1 == 0 && on_segment(p3, p1, p4))
        || (d2 == 0 && on_segment(p3, p2, p4))
        || (d3 == 0 && on_segment(p1, p3, p2))
        || (d4 == 0 && on_segment(p1, p4, p2))
}

/// Deux arêtes non adjacentes qui se touchent = contour croisé.
fn self_intersects(pts: &[Pt]) -> bool {
    let n = pts.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let adjacent = j == i + 1 || (i == 0 && j == n - 1);
            if adjacent {
                continue;
            }
            if segments_intersect(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n]) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly(pts: &[Pt]) -> Result<Polygon, ShapeError> {
        Polygon::try_from(pts.to_vec())
    }

    #[test]
    fn square_area_and_m2() {
        let p = poly(&[[0, 0], [200, 0], [200, 100], [0, 100]]).unwrap();
        assert_eq!(p.area_px2(), 20000.0);
        assert_eq!(p.area_m2(), 2.0);
    }

    #[test]
    fn area_ignores_winding() {
        let p = poly(&[[0, 40], [80, 40], [80, 0], [0, 0]]).unwrap();
        assert_eq!(p.area_px2(), 3200.0);
    }

    #[test]
    fn l_shape_area() {
        // L : 40×40 + 40×20 → 1600 + 800
        let p = poly(&[[0, 0], [40, 0], [40, 20], [80, 20], [80, 40], [0, 40]]).unwrap();
        assert_eq!(p.area_px2(), 2400.0);
    }

    #[test]
    fn contains_inside_outside_and_concave_notch() {
        let p = poly(&[[0, 0], [40, 0], [40, 20], [80, 20], [80, 40], [0, 40]]).unwrap();
        assert!(p.contains(10.0, 10.0));
        assert!(p.contains(60.0, 30.0));
        assert!(!p.contains(60.0, 10.0)); // encoche du L
        assert!(!p.contains(100.0, 30.0));
    }

    #[test]
    fn rejects_too_few_vertices() {
        assert_eq!(poly(&[[0, 0], [10, 0]]), Err(ShapeError::TooFewVertices));
    }

    #[test]
    fn rejects_out_of_bounds() {
        assert_eq!(poly(&[[0, 0], [900, 0], [0, 10]]), Err(ShapeError::OutOfBounds(900, 0)));
    }

    #[test]
    fn rejects_collinear() {
        assert_eq!(poly(&[[0, 0], [10, 0], [20, 0]]), Err(ShapeError::Degenerate));
    }

    #[test]
    fn rejects_bowtie() {
        assert_eq!(
            poly(&[[0, 0], [40, 40], [40, 0], [0, 40]]),
            Err(ShapeError::SelfIntersecting)
        );
    }

    #[test]
    fn strips_fill_full_rectangle() {
        let zone = poly(&[[0, 0], [800, 0], [800, 560], [0, 560]]).unwrap();
        let beds = strip_beds(&zone, &[], 120, 40);
        let boxes: Vec<_> = beds.iter().map(Polygon::bbox).collect();
        assert_eq!(
            boxes,
            vec![(0, 0, 120, 560), (160, 0, 280, 560), (320, 0, 440, 560), (480, 0, 600, 560), (640, 0, 760, 560)]
        );
    }

    #[test]
    fn last_strip_narrower_but_kept() {
        let zone = poly(&[[0, 0], [250, 0], [250, 200], [0, 200]]).unwrap();
        let boxes: Vec<_> = strip_beds(&zone, &[], 120, 40).iter().map(Polygon::bbox).collect();
        assert_eq!(boxes, vec![(0, 0, 120, 200), (160, 0, 250, 200)]);
    }

    #[test]
    fn l_zone_cuts_strips_at_notch() {
        // L : pleine hauteur sur x 0..200, seulement y 300..500 sur x 200..500.
        let zone = poly(&[[0, 0], [200, 0], [200, 300], [500, 300], [500, 500], [0, 500]]).unwrap();
        let boxes: Vec<_> = strip_beds(&zone, &[], 120, 40).iter().map(Polygon::bbox).collect();
        assert_eq!(boxes, vec![(0, 0, 120, 500), (160, 300, 280, 500), (320, 300, 440, 500)]);
    }

    #[test]
    fn too_small_zone_gives_nothing() {
        let zone = poly(&[[0, 0], [100, 0], [100, 40], [0, 40]]).unwrap();
        assert!(strip_beds(&zone, &[], 120, 40).is_empty());
    }

    #[test]
    fn keeps_a_path_around_existing_bed() {
        let zone = poly(&[[0, 0], [120, 0], [120, 500], [0, 500]]).unwrap();
        let taken = poly(&[[0, 200], [120, 200], [120, 300], [0, 300]]).unwrap();
        let boxes: Vec<_> = strip_beds(&zone, &[taken], 120, 40).iter().map(Polygon::bbox).collect();
        assert_eq!(boxes, vec![(0, 0, 120, 160), (0, 340, 120, 500)]);
    }

    #[test]
    fn serde_roundtrip_validates() {
        let p: Polygon = serde_json::from_str("[[0,0],[10,0],[0,10]]").unwrap();
        assert_eq!(serde_json::to_string(&p).unwrap(), "[[0,0],[10,0],[0,10]]");
        assert!(serde_json::from_str::<Polygon>("[[0,0],[10,0]]").is_err());
    }
}
