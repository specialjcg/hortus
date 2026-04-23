//! Couche HTTP (Axum) — sert le moteur de simulation derrière une API REST.

pub mod snapshot;
pub mod state;
pub mod routes;

pub use routes::router;
pub use state::AppState;
