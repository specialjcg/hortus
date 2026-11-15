//! Type de sol du jardin : module les conseils (feuille de route, aptitude des espèces).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Soil {
    Clay,
    Loam,
    Sandy,
    #[default]
    Unknown,
}

impl Soil {
    pub fn as_str(self) -> &'static str {
        match self {
            Soil::Clay => "clay",
            Soil::Loam => "loam",
            Soil::Sandy => "sandy",
            Soil::Unknown => "unknown",
        }
    }

    pub fn parse(raw: &str) -> Option<Soil> {
        [Soil::Clay, Soil::Loam, Soil::Sandy, Soil::Unknown]
            .into_iter()
            .find(|soil| soil.as_str() == raw)
    }
}

/// Aptitude d'une espèce à la terre argileuse (source : Ferme de Sainte-Marthe).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClayFit {
    Good,
    Poor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trips_every_soil() {
        for soil in [Soil::Clay, Soil::Loam, Soil::Sandy, Soil::Unknown] {
            assert_eq!(Soil::parse(soil.as_str()), Some(soil));
        }
        assert_eq!(Soil::parse("granite"), None);
    }
}
