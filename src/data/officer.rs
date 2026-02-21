use std::fs;
use std::path::Path;

use serde::Deserialize;

pub const DEFAULT_CANONICAL_OFFICERS_PATH: &str = "data/officers/officers.canonical.json";

#[derive(Debug, Clone, Deserialize)]
pub struct Officer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub slot: Option<String>,
    #[serde(default)]
    pub abilities: Vec<OfficerAbility>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OfficerAbility {
    pub slot: String,
}

#[derive(Debug, Deserialize)]
struct CanonicalOfficersFile {
    officers: Vec<Officer>,
}

pub fn load_canonical_officers(path: impl AsRef<Path>) -> Result<Vec<Officer>, std::io::Error> {
    let raw = fs::read_to_string(path)?;
    let parsed: CanonicalOfficersFile =
        serde_json::from_str(&raw).map_err(std::io::Error::other)?;
    Ok(parsed.officers)
}
