//! Event tracing and serialization.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::combat::types::{CombatEvent, EventSource};

/// Round to 6 decimal places for stable trace output.
pub(crate) fn round_f64(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

pub(crate) fn serialize_source(source: &EventSource) -> Value {
    let mut object = Map::new();
    if let Some(officer_id) = &source.officer_id {
        object.insert("officer_id".to_string(), Value::String(officer_id.clone()));
    }
    if let Some(ship_ability_id) = &source.ship_ability_id {
        object.insert(
            "ship_ability_id".to_string(),
            Value::String(ship_ability_id.clone()),
        );
    }
    if let Some(hostile_ability_id) = &source.hostile_ability_id {
        object.insert(
            "hostile_ability_id".to_string(),
            Value::String(hostile_ability_id.clone()),
        );
    }
    if let Some(player_bonus_source) = &source.player_bonus_source {
        object.insert(
            "player_bonus_source".to_string(),
            Value::String(player_bonus_source.clone()),
        );
    }
    Value::Object(object)
}

fn sort_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> =
                map.into_iter().map(|(k, v)| (k, sort_json(v))).collect();
            let ordered = sorted.into_iter().collect::<Map<String, Value>>();
            Value::Object(ordered)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_json).collect()),
        _ => value,
    }
}

fn to_canonical_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let sorted = sort_json(serde_json::to_value(value)?);
    serde_json::to_string_pretty(&sorted)
}

pub fn serialize_events_json(events: &[CombatEvent]) -> Result<String, serde_json::Error> {
    let payload: Vec<Value> = events
        .iter()
        .map(|event| {
            let mut object = Map::new();
            object.insert(
                "event_type".to_string(),
                Value::String(event.event_type.clone()),
            );
            object.insert("round_index".to_string(), Value::from(event.round_index));
            object.insert("phase".to_string(), Value::String(event.phase.clone()));
            object.insert("source".to_string(), serialize_source(&event.source));
            object.insert("values".to_string(), Value::Object(event.values.clone()));
            if let Some(wi) = event.weapon_index {
                object.insert("weapon_index".to_string(), Value::from(wi));
            }
            Value::Object(object)
        })
        .collect();

    to_canonical_json(&payload)
}
