//! Cœur de simulation Hortus : sol, plantes, météo, foyer, garde-manger.
//!
//! Toutes les unités SI sauf indication explicite :
//! - températures en °C
//! - eau en mm (équivalent hauteur précipitation) ou L (volume)
//! - masse en grammes (g)
//! - énergie en kilocalories (kcal) — convention nutritionnelle
//! - surface en m²
//! - durée en jours

pub mod time;
pub mod geo;
pub mod weather;
pub mod soil;
pub mod garden;
pub mod species;
pub mod plant;
pub mod nutrition;
pub mod pantry;
pub mod household;
pub mod catalog;
