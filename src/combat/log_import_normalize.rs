//! Optional structural normalization for client/toolbox captures → [`crate::combat::log_ingest::IngestedCombatLog`].
//!
//! No invented combat numbers — only documented key moves / duplication. See `docs/combat_log_format.md`.

use serde_json::{json, Map, Value};

use crate::combat::log_ingest::{IngestedCombatLog, IngestedEvent};

const MAX_COLLAPSED_REPEAT: u64 = 256;

/// Tag plain `stats_snapshot` maps with `_provenance.source = "client"` when `_provenance` is absent (import helper).
pub fn tag_stats_snapshot_sources_client_default(log: &mut IngestedCombatLog) {
    for ev in &mut log.events {
        if let Some(ref mut snap) = ev.stats_snapshot {
            if snap.get("_provenance").is_none() {
                let mut provenance = Map::new();
                provenance.insert("source".into(), json!("client"));
                snap.insert("_provenance".into(), Value::Object(provenance));
            }
        }
    }
}

/// Expand `values.collapsed_repeat_count` into N consecutive duplicate events (structural only).
///
/// Strips `collapsed_repeat_count` and adds `application_index` / `application_count` on each copy.
/// Preserves other `values` keys (e.g. `repeat_group_id`).
pub fn expand_collapsed_repeat_events(log: &mut IngestedCombatLog) -> Result<(), String> {
    let mut out: Vec<IngestedEvent> = Vec::with_capacity(log.events.len());
    for ev in log.events.drain(..) {
        let n = ev
            .values
            .get("collapsed_repeat_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        if n <= 1 {
            out.push(ev);
            continue;
        }
        if n > MAX_COLLAPSED_REPEAT {
            return Err(format!(
                "collapsed_repeat_count {n} exceeds max {MAX_COLLAPSED_REPEAT}"
            ));
        }
        let n_usize = n as usize;
        for i in 0..n_usize {
            let mut clone = ev.clone();
            clone.values.remove("collapsed_repeat_count");
            clone
                .values
                .insert("application_index".into(), json!(i as u64));
            clone.values.insert("application_count".into(), json!(n));
            out.push(clone);
        }
    }
    log.events = out;
    Ok(())
}
