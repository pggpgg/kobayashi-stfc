//! PvP scenario field validation (ship-vs-ship defender; mutually exclusive with `hostile`).

use super::requests::ValidationIssue;

/// Resolved scenario target after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioTarget {
    Pve {
        hostile: String,
    },
    Pvp {
        defender_ship: String,
        defender_ship_tier: Option<u32>,
        defender_ship_level: Option<u32>,
        defender_profile_id: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ScenarioTargetFields {
    pub hostile: Option<String>,
    pub defender_ship: Option<String>,
    pub defender_ship_tier: Option<u32>,
    pub defender_ship_level: Option<u32>,
    pub defender_profile_id: Option<String>,
}

/// Validate `hostile` vs `defender_ship` exclusivity and required companion fields.
pub fn validate_scenario_target(
    fields: &ScenarioTargetFields,
) -> Result<ScenarioTarget, Vec<ValidationIssue>> {
    let hostile = fields
        .hostile
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let defender_ship = fields
        .defender_ship
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut errors: Vec<ValidationIssue> = Vec::new();

    match (&hostile, &defender_ship) {
        (Some(_), Some(_)) => {
            errors.push(ValidationIssue {
                field: "defender_ship",
                messages: vec![
                    "defender_ship and hostile are mutually exclusive; send one scenario target"
                        .to_string(),
                ],
            });
        }
        (None, None) => {
            errors.push(ValidationIssue {
                field: "hostile",
                messages: vec!["hostile or defender_ship is required".to_string()],
            });
        }
        (Some(h), None) => {
            if errors.is_empty() {
                return Ok(ScenarioTarget::Pve { hostile: h.clone() });
            }
        }
        (None, Some(ds)) => {
            let pid = fields
                .defender_profile_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if pid.is_none() {
                errors.push(ValidationIssue {
                    field: "defender_profile_id",
                    messages: vec![
                        "defender_profile_id is required when defender_ship is set".to_string()
                    ],
                });
            }
            if errors.is_empty() {
                return Ok(ScenarioTarget::Pvp {
                    defender_ship: ds.clone(),
                    defender_ship_tier: fields.defender_ship_tier,
                    defender_ship_level: fields.defender_ship_level,
                    defender_profile_id: pid.unwrap().to_string(),
                });
            }
        }
    }

    Err(errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pve_requires_hostile_only() {
        let t = validate_scenario_target(&ScenarioTargetFields {
            hostile: Some("h1".into()),
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(t, ScenarioTarget::Pve { .. }));
    }

    #[test]
    fn pvp_requires_defender_profile() {
        let err = validate_scenario_target(&ScenarioTargetFields {
            defender_ship: Some("rotarran".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.iter().any(|e| e.field == "defender_profile_id"));
    }

    #[test]
    fn hostile_and_defender_ship_mutually_exclusive() {
        let err = validate_scenario_target(&ScenarioTargetFields {
            hostile: Some("h".into()),
            defender_ship: Some("s".into()),
            defender_profile_id: Some("p".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.iter().any(|e| e.field == "defender_ship"));
    }
}
