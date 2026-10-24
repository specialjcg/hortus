//! Client VigiEau : restrictions d'eau sécheresse en vigueur à une position.
//!
//! API : https://api.vigieau.gouv.fr/api/zones?lon=&lat=&profil=particulier&zoneType=AEP|SUP|SOU
//! Gratuite, sans clé. Une liste vide = aucune restriction pour ce type de ressource.

use serde::{Deserialize, Serialize};

const ZONES_URL: &str = "https://api.vigieau.gouv.fr/api/zones";

/// Types de ressource VigiEau, dans l'ordre d'affichage :
/// réseau (eau potable), eaux superficielles (cours d'eau), eaux souterraines (puits, nappe).
pub const RESOURCE_KINDS: [&str; 3] = ["AEP", "SUP", "SOU"];

/// Restriction en vigueur pour un type de ressource. `level = None` = aucune restriction.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Restriction {
    pub kind: String,
    pub zone: Option<String>,
    pub level: Option<String>,
    /// Règle « Arrosage des potagers domestiques » de l'arrêté.
    pub potager: Option<String>,
    /// Fin de validité de l'arrêté, YYYY-MM-DD.
    pub valid_until: Option<String>,
    pub arrete_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Zone {
    nom: String,
    #[serde(rename = "niveauGravite")]
    niveau_gravite: Option<String>,
    arrete: Option<Arrete>,
    #[serde(default)]
    usages: Vec<Usage>,
}

#[derive(Debug, Deserialize)]
struct Arrete {
    #[serde(rename = "cheminFichier")]
    chemin_fichier: Option<String>,
    #[serde(rename = "dateFinValidite")]
    date_fin_validite: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    nom: String,
    description: String,
}

/// Rang de gravité, pour garder la zone la plus sévère quand plusieurs se superposent.
pub fn severity(level: Option<&str>) -> u8 {
    match level {
        Some("vigilance") => 1,
        Some("alerte") => 2,
        Some(l) if l.contains("renforc") => 3,
        Some("crise") => 4,
        Some(_) => 1,
        None => 0,
    }
}

/// Parse la réponse `/api/zones` d'un type de ressource.
pub fn parse_zones(kind: &str, json: &str) -> Result<Restriction, serde_json::Error> {
    let zones: Vec<Zone> = serde_json::from_str(json)?;
    let worst = zones
        .into_iter()
        .max_by_key(|z| severity(z.niveau_gravite.as_deref()));
    let Some(z) = worst else {
        return Ok(Restriction {
            kind: kind.to_string(),
            zone: None,
            level: None,
            potager: None,
            valid_until: None,
            arrete_url: None,
        });
    };
    let potager = z
        .usages
        .iter()
        .find(|u| u.nom.to_lowercase().contains("potager"))
        .map(|u| u.description.replace("\r\n", "\n").trim().to_string());
    let (arrete_url, valid_until) = match z.arrete {
        Some(a) => (
            a.chemin_fichier,
            a.date_fin_validite.map(|d| d.chars().take(10).collect()),
        ),
        None => (None, None),
    };
    Ok(Restriction {
        kind: kind.to_string(),
        zone: Some(z.nom),
        level: z.niveau_gravite,
        potager,
        valid_until,
        arrete_url,
    })
}

/// Récupère les restrictions des 3 types de ressource. Erreur si un seul appel échoue :
/// une réponse partielle ferait croire à une levée de restriction.
pub async fn fetch_restrictions(latitude: f64, longitude: f64) -> Result<Vec<Restriction>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client VigiEau : {e}"))?;
    let (aep, sup, sou) = tokio::join!(
        fetch_kind(&client, latitude, longitude, RESOURCE_KINDS[0]),
        fetch_kind(&client, latitude, longitude, RESOURCE_KINDS[1]),
        fetch_kind(&client, latitude, longitude, RESOURCE_KINDS[2]),
    );
    Ok(vec![aep?, sup?, sou?])
}

async fn fetch_kind(
    client: &reqwest::Client,
    latitude: f64,
    longitude: f64,
    kind: &str,
) -> Result<Restriction, String> {
    let url = format!(
        "{ZONES_URL}?lon={longitude:.4}&lat={latitude:.4}&profil=particulier&zoneType={kind}"
    );
    tracing::info!("VigiEau fetch: {}", url);
    let body = client
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("VigiEau {kind} : {e}"))?
        .text()
        .await
        .map_err(|e| format!("VigiEau {kind} : {e}"))?;
    parse_zones(kind, &body).map_err(|e| format!("VigiEau {kind} JSON : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONE_1_CRISE: &str = r#"[{"id":7256006,"nom":"ZONE 1","code":"84_69_0001","type":"SUP",
        "niveauGravite":"crise",
        "arrete":{"id":37857,"cheminFichier":"https://example.org/arrete.pdf",
                  "dateFinValidite":"2026-10-31T00:00:00.000Z","dateDebutValidite":"2026-08-24T00:00:00.000Z"},
        "usages":[{"id":1,"nom":"Abreuvement des animaux","description":"Pas de restriction"},
                  {"id":2,"nom":"Arrosage des potagers domestiques","description":"Interdiction de 9h à 20h.\r\n"}]}]"#;

    #[test]
    fn parses_zone_in_crisis() {
        let r = parse_zones("SUP", ZONE_1_CRISE).expect("parse");
        assert_eq!(r.zone.as_deref(), Some("ZONE 1"));
        assert_eq!(r.level.as_deref(), Some("crise"));
        assert_eq!(r.potager.as_deref(), Some("Interdiction de 9h à 20h."));
        assert_eq!(r.valid_until.as_deref(), Some("2026-10-31"));
        assert_eq!(r.arrete_url.as_deref(), Some("https://example.org/arrete.pdf"));
    }

    #[test]
    fn empty_list_means_no_restriction() {
        let r = parse_zones("SOU", "[]").expect("parse");
        assert_eq!(r.kind, "SOU");
        assert_eq!(r.level, None);
        assert_eq!(r.potager, None);
    }

    #[test]
    fn keeps_most_severe_zone() {
        let json = r#"[{"nom":"A","niveauGravite":"vigilance"},
                       {"nom":"B","niveauGravite":"alerte_renforcee"},
                       {"nom":"C","niveauGravite":"alerte"}]"#;
        let r = parse_zones("AEP", json).expect("parse");
        assert_eq!(r.zone.as_deref(), Some("B"));
    }

    #[test]
    fn severity_orders_levels() {
        assert!(severity(None) < severity(Some("vigilance")));
        assert!(severity(Some("alerte")) < severity(Some("alerte_renforcee")));
        assert!(severity(Some("alerte_renforcee")) < severity(Some("crise")));
    }
}
