//! État partagé entre tous les handlers HTTP.
//!
//! Stocke les simulations en mémoire dans une `Mutex<HashMap>` (modèle MVP).
//! Persistance disque à venir si nécessaire.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::domain::sim::Simulation;

#[derive(Clone)]
pub struct AppState {
    pub sims: Arc<Mutex<HashMap<String, Simulation>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sims: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Génère un ID court (8 caractères hex) à partir d'une seed et d'un compteur.
pub fn fresh_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:08x}", (n & 0xFFFF_FFFF) as u32)
}
