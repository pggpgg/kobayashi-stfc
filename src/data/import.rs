use std::fmt;
use std::fs;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub source_path: String,
    pub record_count: usize,
}

#[derive(Debug)]
pub enum ImportError {
    Read(std::io::Error),
    Parse(serde_json::Error),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(err) => write!(f, "failed to read import file: {err}"),
            Self::Parse(err) => write!(f, "failed to parse import JSON: {err}"),
        }
    }
}

pub fn import_spocks_export(path: &str) -> Result<ImportReport, ImportError> {
    let raw = fs::read_to_string(path).map_err(ImportError::Read)?;
    let payload: Value = serde_json::from_str(&raw).map_err(ImportError::Parse)?;

    let record_count = match payload {
        Value::Array(items) => items.len(),
        Value::Object(map) => map.len(),
        _ => 1,
    };

    Ok(ImportReport {
        source_path: path.to_string(),
        record_count,
    })
}
