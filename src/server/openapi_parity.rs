//! Field-name parity between bundled OpenAPI component schemas and Rust handler DTOs.
//!
//! Used by integration tests so a new serde field on a request/response struct cannot ship
//! without updating [`crate::server::openapi::OPENAPI_YAML`] (and the frontend `gen:api` types).

use std::collections::BTreeSet;

use schemars::schema::SchemaObject;
use schemars::{schema_for, JsonSchema};
use serde_yaml::Value;

use crate::data::profile_index::ProfileEntry;
use crate::optimizer::sensitivity::{SensitivityRequest, SensitivityResponse, SensitivityRow};
use crate::optimizer::sensitivity_morris::{MorrisRequest, MorrisResponse, MorrisRow};
use crate::optimizer::sensitivity_sobol::{SobolPairRow, SobolRequest, SobolResponse, SobolRow};
use crate::server::api::{ChainGrindRequest, OfficerGroupConstraintDto, OptimizeConstraintsDto};
use crate::server::api::{
    CompareCrewsRequest, CompareCrewsResponse, DataVersionResponse, HostileListItem,
    OfficerListItem, OptimizeRequest, OptimizeResponse, OptimizeStartResponse,
    OptimizeStatusResponse, Preset, PresetSummary, ReplaySeedCrew, ReplaySeedRequest, ShipListItem,
    SimulateCrew, SimulateRequest, SimulateResponse, SimulateStats, ValidationErrorResponse,
    ValidationIssue, WarmStartCrewDto,
};
use crate::server::openapi::OPENAPI_YAML;
use crate::server::sensitivity_jobs::{SensitivityStartResponse, SensitivityStatusResponse};

/// One OpenAPI component schema name paired with Rust-derived property names.
struct SchemaPair {
    openapi_name: &'static str,
    rust_fields: BTreeSet<String>,
}

/// Extract top-level `properties` keys from an OpenAPI component schema.
pub fn openapi_property_names(spec: &Value, schema_name: &str) -> BTreeSet<String> {
    spec.get("components")
        .and_then(|v| v.get("schemas"))
        .and_then(|v| v.get(schema_name))
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.as_mapping())
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Property keys from a Rust type's JSON Schema (via schemars).
pub fn rust_property_names<T: JsonSchema>() -> BTreeSet<String> {
    let root = schema_for!(T);
    object_property_names(&root.schema)
}

fn object_property_names(obj: &SchemaObject) -> BTreeSet<String> {
    obj.object
        .as_ref()
        .map(|meta| meta.properties.keys().cloned().collect())
        .unwrap_or_default()
}

// --- Wrapper DTOs for responses assembled with `serde_json::json!` in handlers ---

mod response_wrappers {
    #![allow(dead_code)]

    use super::*;
    use schemars::JsonSchema;

    #[derive(JsonSchema)]
    pub(super) struct HealthResponse {
        status: String,
        service: String,
        build: HealthBuild,
        server: HealthServer,
        data: HealthData,
    }

    #[derive(JsonSchema)]
    pub(super) struct HealthBuild {
        cargo_pkg_version: String,
        git_sha_short: Option<String>,
    }

    #[derive(JsonSchema)]
    pub(super) struct HealthServer {
        started_at_utc: String,
        max_concurrent_cpu_jobs: u64,
        max_concurrent_cpu_jobs_from_env: bool,
        cpu_job_permits_available: u64,
        cpu_job_permits_total: u64,
        cpu_job_queue_wait_ms: Option<u64>,
        cpu_job_queue_wait_ms_from_env: bool,
    }

    #[derive(JsonSchema)]
    pub(super) struct HealthData {
        officer_count: usize,
        hostile_data_version: Option<String>,
        ship_data_version: Option<String>,
        hostile_index_loaded: bool,
        ship_index_loaded: bool,
    }

    #[derive(JsonSchema)]
    pub(super) struct OfficersListResponse {
        officers: Vec<OfficerListItem>,
    }

    #[derive(JsonSchema)]
    pub(super) struct ShipsListResponse {
        ships: Vec<ShipListItem>,
    }

    #[derive(JsonSchema)]
    pub(super) struct HostilesListResponse {
        hostiles: Vec<HostileListItem>,
    }

    #[derive(JsonSchema)]
    pub(super) struct ShipTiersLevelsResponse {
        tiers: Vec<u32>,
        levels: Vec<u32>,
        crew_slots: Vec<serde_json::Value>,
    }

    #[derive(JsonSchema)]
    pub(super) struct HeuristicsListResponse {
        seeds: Vec<String>,
    }

    #[derive(JsonSchema)]
    pub(super) struct ProfilesListResponse {
        profiles: Vec<ProfileEntry>,
        default_id: Option<String>,
    }

    #[derive(JsonSchema)]
    pub(super) struct CreateProfileRequest {
        id: Option<String>,
        name: String,
    }

    #[derive(JsonSchema)]
    pub(super) struct PresetsListResponse {
        presets: Vec<PresetSummary>,
    }

    #[derive(JsonSchema)]
    pub(super) struct OptimizeEstimateResponse {
        estimated_candidates: usize,
        sims_per_crew: u32,
        estimated_seconds: f64,
    }

    #[derive(JsonSchema)]
    pub(super) struct SensitivityDefaultsResponse {
        deltas: Vec<SensitivityDefaultDeltaRow>,
    }

    #[derive(JsonSchema)]
    pub(super) struct SensitivityDefaultDeltaRow {
        stat: String,
        delta: f64,
        multiplicative: bool,
    }

    #[derive(JsonSchema)]
    pub(super) struct MorrisSensitivityDefaultsResponse {
        deltas: Vec<SensitivityDefaultDeltaRow>,
        r_trajectories_default: u32,
        r_trajectories_max: u32,
        num_sims_default: u32,
        num_sims_max: u32,
    }

    #[derive(JsonSchema)]
    pub(super) struct SobolSensitivityDefaultsResponse {
        deltas: Vec<SensitivityDefaultDeltaRow>,
        n_samples_default: u32,
        n_samples_max: u32,
    }
}

use response_wrappers::{
    CreateProfileRequest, HealthResponse, HeuristicsListResponse, HostilesListResponse,
    MorrisSensitivityDefaultsResponse, OfficersListResponse, OptimizeEstimateResponse,
    PresetsListResponse, ProfilesListResponse, SensitivityDefaultsResponse,
    ShipTiersLevelsResponse, ShipsListResponse, SobolSensitivityDefaultsResponse,
};

fn all_schema_pairs() -> Vec<SchemaPair> {
    vec![
        SchemaPair {
            openapi_name: "HealthResponse",
            rust_fields: rust_property_names::<HealthResponse>(),
        },
        SchemaPair {
            openapi_name: "DataVersionResponse",
            rust_fields: rust_property_names::<DataVersionResponse>(),
        },
        SchemaPair {
            openapi_name: "OfficersListResponse",
            rust_fields: rust_property_names::<OfficersListResponse>(),
        },
        SchemaPair {
            openapi_name: "ShipsListResponse",
            rust_fields: rust_property_names::<ShipsListResponse>(),
        },
        SchemaPair {
            openapi_name: "HostilesListResponse",
            rust_fields: rust_property_names::<HostilesListResponse>(),
        },
        SchemaPair {
            openapi_name: "ShipTiersLevelsResponse",
            rust_fields: rust_property_names::<ShipTiersLevelsResponse>(),
        },
        SchemaPair {
            openapi_name: "HeuristicsListResponse",
            rust_fields: rust_property_names::<HeuristicsListResponse>(),
        },
        SchemaPair {
            openapi_name: "ProfilesListResponse",
            rust_fields: rust_property_names::<ProfilesListResponse>(),
        },
        SchemaPair {
            openapi_name: "ProfileEntry",
            rust_fields: rust_property_names::<ProfileEntry>(),
        },
        SchemaPair {
            openapi_name: "CreateProfileRequest",
            rust_fields: rust_property_names::<CreateProfileRequest>(),
        },
        SchemaPair {
            openapi_name: "PresetsListResponse",
            rust_fields: rust_property_names::<PresetsListResponse>(),
        },
        SchemaPair {
            openapi_name: "Preset",
            rust_fields: rust_property_names::<Preset>(),
        },
        SchemaPair {
            openapi_name: "OptimizeEstimateResponse",
            rust_fields: rust_property_names::<OptimizeEstimateResponse>(),
        },
        SchemaPair {
            openapi_name: "OptimizeStartResponse",
            rust_fields: rust_property_names::<OptimizeStartResponse>(),
        },
        SchemaPair {
            openapi_name: "OptimizeStatusResponse",
            rust_fields: rust_property_names::<OptimizeStatusResponse>(),
        },
        SchemaPair {
            openapi_name: "OptimizeResponse",
            rust_fields: rust_property_names::<OptimizeResponse>(),
        },
        SchemaPair {
            openapi_name: "SimulateCrew",
            rust_fields: rust_property_names::<SimulateCrew>(),
        },
        SchemaPair {
            openapi_name: "SimulateRequest",
            rust_fields: rust_property_names::<SimulateRequest>(),
        },
        SchemaPair {
            openapi_name: "SimulateStats",
            rust_fields: rust_property_names::<SimulateStats>(),
        },
        SchemaPair {
            openapi_name: "SimulateResponse",
            rust_fields: rust_property_names::<SimulateResponse>(),
        },
        SchemaPair {
            openapi_name: "CompareCrewsRequest",
            rust_fields: rust_property_names::<CompareCrewsRequest>(),
        },
        SchemaPair {
            openapi_name: "CompareCrewsResponse",
            rust_fields: rust_property_names::<CompareCrewsResponse>(),
        },
        SchemaPair {
            openapi_name: "OfficerGroupConstraintDto",
            rust_fields: rust_property_names::<OfficerGroupConstraintDto>(),
        },
        SchemaPair {
            openapi_name: "OptimizeConstraintsDto",
            rust_fields: rust_property_names::<OptimizeConstraintsDto>(),
        },
        SchemaPair {
            openapi_name: "WarmStartCrewDto",
            rust_fields: rust_property_names::<WarmStartCrewDto>(),
        },
        SchemaPair {
            openapi_name: "ChainGrindRequestDto",
            rust_fields: rust_property_names::<ChainGrindRequest>(),
        },
        SchemaPair {
            openapi_name: "OptimizeRequest",
            rust_fields: rust_property_names::<OptimizeRequest>(),
        },
        SchemaPair {
            openapi_name: "ReplaySeedCrew",
            rust_fields: rust_property_names::<ReplaySeedCrew>(),
        },
        SchemaPair {
            openapi_name: "ReplaySeedRequest",
            rust_fields: rust_property_names::<ReplaySeedRequest>(),
        },
        SchemaPair {
            openapi_name: "ValidationIssue",
            rust_fields: rust_property_names::<ValidationIssue>(),
        },
        SchemaPair {
            openapi_name: "ValidationErrorResponse",
            rust_fields: rust_property_names::<ValidationErrorResponse>(),
        },
        SchemaPair {
            openapi_name: "SensitivityRequest",
            rust_fields: rust_property_names::<SensitivityRequest>(),
        },
        SchemaPair {
            openapi_name: "SensitivityRow",
            rust_fields: rust_property_names::<SensitivityRow>(),
        },
        SchemaPair {
            openapi_name: "SensitivityResponse",
            rust_fields: rust_property_names::<SensitivityResponse>(),
        },
        SchemaPair {
            openapi_name: "SensitivityDefaultsResponse",
            rust_fields: rust_property_names::<SensitivityDefaultsResponse>(),
        },
        SchemaPair {
            openapi_name: "MorrisSensitivityRequest",
            rust_fields: rust_property_names::<MorrisRequest>(),
        },
        SchemaPair {
            openapi_name: "MorrisSensitivityRow",
            rust_fields: rust_property_names::<MorrisRow>(),
        },
        SchemaPair {
            openapi_name: "MorrisSensitivityResponse",
            rust_fields: rust_property_names::<MorrisResponse>(),
        },
        SchemaPair {
            openapi_name: "MorrisSensitivityDefaultsResponse",
            rust_fields: rust_property_names::<MorrisSensitivityDefaultsResponse>(),
        },
        SchemaPair {
            openapi_name: "SobolSensitivityRequest",
            rust_fields: rust_property_names::<SobolRequest>(),
        },
        SchemaPair {
            openapi_name: "SobolSensitivityRow",
            rust_fields: rust_property_names::<SobolRow>(),
        },
        SchemaPair {
            openapi_name: "SobolSensitivityPairRow",
            rust_fields: rust_property_names::<SobolPairRow>(),
        },
        SchemaPair {
            openapi_name: "SobolSensitivityResponse",
            rust_fields: rust_property_names::<SobolResponse>(),
        },
        SchemaPair {
            openapi_name: "SobolSensitivityDefaultsResponse",
            rust_fields: rust_property_names::<SobolSensitivityDefaultsResponse>(),
        },
        SchemaPair {
            openapi_name: "SensitivityStartResponse",
            rust_fields: rust_property_names::<SensitivityStartResponse>(),
        },
        SchemaPair {
            openapi_name: "SensitivityStatusResponse",
            rust_fields: rust_property_names::<SensitivityStatusResponse>(),
        },
    ]
}

/// Compare every mapped Rust DTO against the bundled OpenAPI document.
pub fn verify_openapi_rust_field_parity() -> Result<(), String> {
    let spec: Value =
        serde_yaml::from_str(OPENAPI_YAML).map_err(|e| format!("OpenAPI YAML parse: {e}"))?;
    for pair in all_schema_pairs() {
        let openapi_fields = openapi_property_names(&spec, pair.openapi_name);
        if openapi_fields.is_empty() {
            return Err(format!(
                "missing OpenAPI component schema {}",
                pair.openapi_name
            ));
        }
        let only_rust: Vec<_> = pair
            .rust_fields
            .difference(&openapi_fields)
            .cloned()
            .collect();
        let only_openapi: Vec<_> = openapi_fields
            .difference(&pair.rust_fields)
            .cloned()
            .collect();
        if !only_rust.is_empty() || !only_openapi.is_empty() {
            return Err(format!(
                "{}: property mismatch\n  only in Rust: {only_rust:?}\n  only in OpenAPI: {only_openapi:?}",
                pair.openapi_name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimizer::monte_carlo::scenario::DefenderOpponent;

    #[test]
    fn bundled_openapi_matches_rust_dto_fields() {
        verify_openapi_rust_field_parity().expect("OpenAPI ↔ Rust field parity");
    }

    #[test]
    fn defender_opponent_serializes_as_snake_case_string() {
        let v = serde_json::to_value(DefenderOpponent::Player).expect("serialize");
        assert_eq!(v, "player");
    }

    #[test]
    fn optimize_defender_crew_matches_simulate_crew_shape() {
        use crate::server::api::DefenderOfficerCrewDto;
        assert_eq!(
            rust_property_names::<SimulateCrew>(),
            rust_property_names::<DefenderOfficerCrewDto>()
        );
    }
}
