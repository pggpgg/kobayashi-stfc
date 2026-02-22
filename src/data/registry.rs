//! Data registry: versioning and source tracking for each dataset.
//! Written by the normalizer and spreadsheet importers; read by the app to show "data as of".

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSetEntry {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    pub path: String,
}

pub type Registry = HashMap<String, DataSetEntry>;

pub const DEFAULT_REGISTRY_PATH: &str = "data/registry.json";
