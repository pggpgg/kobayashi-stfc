use std::collections::HashSet;
use std::fs;

use serde_json::Value;

pub fn validate_officer_dataset(path: &str) -> Result<(), Vec<String>> {
    let raw =
        fs::read_to_string(path).map_err(|err| vec![format!("unable to read '{path}': {err}")])?;
    let payload: Value = serde_json::from_str(&raw)
        .map_err(|err| vec![format!("unable to parse json '{path}': {err}")])?;

    let Some(entries) = payload.as_array() else {
        return Err(vec!["expected top-level JSON array".to_string()]);
    };

    let mut errors = Vec::new();
    let mut seen_ids = HashSet::new();

    for (index, entry) in entries.iter().enumerate() {
        let Some(object) = entry.as_object() else {
            errors.push(format!("entry[{index}] is not an object"));
            continue;
        };

        match object.get("id").and_then(Value::as_str) {
            Some(id) if !id.trim().is_empty() => {
                if !seen_ids.insert(id.to_string()) {
                    errors.push(format!("entry[{index}] duplicate id '{id}'"));
                }
            }
            _ => errors.push(format!("entry[{index}] missing non-empty 'id'")),
        }

        match object.get("name").and_then(Value::as_str) {
            Some(name) if !name.trim().is_empty() => {}
            _ => errors.push(format!("entry[{index}] missing non-empty 'name'")),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
