//! Native integer officer CoreStats, before account or ability multipliers.
//!
//! The reviewed static catalog contains OfficerSpec integer axis weights (group
//! 7) and OfficerCoreStatSpec float32 growth by level/rarity (group 18). The
//! retained client computes a float32 product, rounds it to an integer, and then
//! exposes that CoreStats row to later consumers. This module does not select a
//! Tom Paris combat source or implement officer-stat percentage buffs.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;
use sha2::{Digest, Sha256};

const EMBEDDED_CATALOG: &str = include_str!("../../data/native_officer_core_stats.json");
const REVIEWED_CACHE_SHA256: &str =
    "b472e4460e3869cc8abb44e456a0873e3c5702ed6f3a9701b18d27938022f3cf";
const REFERENCE_METADATA_SHA256: &str =
    "ab0ccdc3e9e09e59d16a13431e2a0722255a71d5ff373cc09d95a47cd130ca04";
const REFERENCE_BINARY_SHA256: &str =
    "a5df6314bd78f15892a232a44e77cdcf2e3d20e9a6192f99120c0f3e52632274";

/// One native officer's rounded base stats. Account/seat/ability multipliers
/// and ship officer-bonus breakpoints have not yet been applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeOfficerCoreStats {
    pub attack: i32,
    pub defense: i32,
    pub health: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeOfficerCoreStatsCatalog {
    schema_version: u32,
    provenance: Provenance,
    officers: BTreeMap<u64, OfficerSpec>,
    growth_by_level: BTreeMap<u32, GrowthSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    static_cache_sha256: String,
    reference_client_build: u32,
    reference_metadata_sha256: String,
    reference_binary_sha256: String,
    officer_specs_group: GroupProvenance,
    core_stat_specs_group: GroupProvenance,
    bound_upstream_count: usize,
    upstream_binding_policy: String,
    algorithm_verification_273: AlgorithmVerification,
    source_note: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlgorithmVerification {
    client_build: u32,
    metadata_sha256: String,
    binary_sha256: String,
    create_core_stats_address: String,
    create_core_stats_evidence_sha256: String,
    round_helper_address: String,
    round_helper_evidence_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupProvenance {
    id: u32,
    payload_sha256: String,
    count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OfficerSpec {
    id: u64,
    rarity: u32,
    weights: Weights,
    source_sha256: String,
    upstream_byte_bindings: Option<UpstreamByteBindings>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpstreamByteBindings {
    lf_sha256: String,
    crlf_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Weights {
    attack: i32,
    defense: i32,
    health: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrowthSpec {
    source_sha256: String,
    by_rarity: BTreeMap<u32, f32>,
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl NativeOfficerCoreStatsCatalog {
    fn validate(&self) -> Result<(), &'static str> {
        let source = &self.provenance;
        if self.schema_version != 1
            || source.reference_client_build != 272
            || source.static_cache_sha256 != REVIEWED_CACHE_SHA256
            || source.reference_metadata_sha256 != REFERENCE_METADATA_SHA256
            || source.reference_binary_sha256 != REFERENCE_BINARY_SHA256
            || source.officer_specs_group.id != 7
            || source.core_stat_specs_group.id != 18
            || source.upstream_binding_policy != "exact_utf8_lf_or_crlf_v1"
            || source.source_note.trim().is_empty()
        {
            return Err("unsupported native officer CoreStats provenance");
        }
        if self.officers.is_empty()
            || self.growth_by_level.is_empty()
            || source.officer_specs_group.count != self.officers.len()
            || source.core_stat_specs_group.count != self.growth_by_level.len()
            || !is_sha256(&source.officer_specs_group.payload_sha256)
            || !is_sha256(&source.core_stat_specs_group.payload_sha256)
            || source.bound_upstream_count
                != self
                    .officers
                    .values()
                    .filter(|officer| officer.upstream_byte_bindings.is_some())
                    .count()
        {
            return Err("invalid native officer CoreStats groups");
        }
        let algorithm = &source.algorithm_verification_273;
        if algorithm.client_build != 273
            || algorithm.metadata_sha256
                != "2b586d6f23eb130c210ddca13d8ebdd21f09f807e2eb94f4972bf321b5f821da"
            || algorithm.binary_sha256
                != "a9ab2edbc143cfba3aff35665e129006f73853070dbff206884fda04a235ab94"
            || algorithm.create_core_stats_address != "0x1818848c0"
            || algorithm.round_helper_address != "0x1816f7870"
            || !is_sha256(&algorithm.create_core_stats_evidence_sha256)
            || !is_sha256(&algorithm.round_helper_evidence_sha256)
        {
            return Err("unsupported native officer CoreStats algorithm verification");
        }
        for (&id, officer) in &self.officers {
            if id == 0
                || officer.id != id
                || officer.rarity == 0
                || !is_sha256(&officer.source_sha256)
                || officer
                    .upstream_byte_bindings
                    .as_ref()
                    .is_some_and(|binding| {
                        !is_sha256(&binding.lf_sha256) || !is_sha256(&binding.crlf_sha256)
                    })
                || officer.weights.attack < 0
                || officer.weights.defense < 0
                || officer.weights.health < 0
            {
                return Err("invalid native officer CoreStats identity or weights");
            }
        }
        for (&level, growth) in &self.growth_by_level {
            if level == 0
                || !is_sha256(&growth.source_sha256)
                || growth.by_rarity.is_empty()
                || growth
                    .by_rarity
                    .iter()
                    .any(|(&rarity, &value)| rarity == 0 || !value.is_finite() || value <= 0.0)
            {
                return Err("invalid native officer CoreStats growth");
            }
        }
        Ok(())
    }

    fn stats_for_officer(&self, id: u64, level: u32) -> Option<NativeOfficerCoreStats> {
        let officer = self.officers.get(&id)?;
        let growth = *self
            .growth_by_level
            .get(&level)?
            .by_rarity
            .get(&officer.rarity)?;
        let axis = |weight: i32| native_round_to_i32(weight as f32 * growth);
        Some(NativeOfficerCoreStats {
            attack: axis(officer.weights.attack)?,
            defense: axis(officer.weights.defense)?,
            health: axis(officer.weights.health)?,
        })
    }
}

fn parse_catalog(json: &str) -> Result<NativeOfficerCoreStatsCatalog, String> {
    let catalog: NativeOfficerCoreStatsCatalog =
        serde_json::from_str(json).map_err(|error| error.to_string())?;
    catalog.validate().map_err(str::to_string)?;
    Ok(catalog)
}

/// Exact retained helper 0x1816f7300: ADDSS/SUBSS 0.5, promote to double,
/// System.Math.Floor/Ceiling, then CVTTSD2SI. The preceding MULSS is evaluated
/// separately in `stats_for_officer`; generic f64 rounding is not equivalent.
fn native_round_to_i32(value: f32) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    let rounded = if value > 0.0 {
        f64::from(value + 0.5_f32).floor()
    } else {
        f64::from(value - 0.5_f32).ceil()
    };
    // Rust's float-to-int cast saturates outside this range. Do not silently
    // substitute that behavior for the native conversion on invalid inputs.
    if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return None;
    }
    Some(rounded as i32)
}

fn embedded_catalog() -> &'static NativeOfficerCoreStatsCatalog {
    static CATALOG: OnceLock<NativeOfficerCoreStatsCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        parse_catalog(EMBEDDED_CATALOG)
            .expect("reviewed embedded native officer CoreStats catalog must validate")
    })
}

/// A retained native snapshot must not override a refreshed or custom upstream
/// file just because its filename/ID is familiar. Compare exact raw file bytes
/// before parsing. Only the catalog's explicit LF/CRLF byte hashes are accepted;
/// runtime never normalizes input. Changed/unbound sources keep their stat table.
pub(crate) fn upstream_source_matches_native_snapshot(
    source_officer_id: u64,
    raw_upstream_file: &[u8],
) -> bool {
    embedded_catalog()
        .officers
        .get(&source_officer_id)
        .and_then(|officer| officer.upstream_byte_bindings.as_ref())
        .is_some_and(|binding| {
            let digest = format!("{:x}", Sha256::digest(raw_upstream_file));
            digest == binding.lf_sha256 || digest == binding.crlf_sha256
        })
}

/// Look up an exact native source ID and level. Unknown IDs, unavailable native
/// levels/rarities or out-of-range results return `None`; the model loader keeps
/// its existing upstream row in that case. Direct/custom LCARS rows never pass
/// through this source-specific provider.
pub fn stats_for_native_officer(
    source_officer_id: u64,
    level: u32,
) -> Option<NativeOfficerCoreStats> {
    embedded_catalog().stats_for_officer(source_officer_id, level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_product_is_rounded_before_any_account_multiplier() {
        assert_eq!(
            stats_for_native_officer(3_849_424_588, 20).unwrap().defense,
            421
        );
        assert_eq!(
            stats_for_native_officer(1_916_661_181, 5).unwrap().defense,
            19
        );
    }

    #[test]
    fn rounding_retains_float32_half_adjustment_and_away_from_zero_ties() {
        for (input, expected) in [
            (0.0, 0),
            (0.5, 1),
            (1.5, 2),
            (2.5, 3),
            (-0.5, -1),
            (-1.5, -2),
            (-2.5, -3),
            (f32::from_bits(0x3eff_ffff), 1),
            (-f32::from_bits(0x3eff_ffff), -1),
            (8_388_609.0, 8_388_610),
        ] {
            assert_eq!(native_round_to_i32(input), Some(expected), "{input}");
        }
        assert_eq!(native_round_to_i32(f32::from_bits(0x3eff_fffe)), Some(0));
    }

    #[test]
    fn nonfinite_and_overflow_are_not_saturated_into_core_stats() {
        for input in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, i32::MAX as f32] {
            assert_eq!(native_round_to_i32(input), None);
        }
        assert_eq!(native_round_to_i32(i32::MIN as f32), Some(i32::MIN));
    }

    #[test]
    fn missing_source_or_exact_level_has_no_synthetic_fallback() {
        assert_eq!(stats_for_native_officer(0, 20), None);
        assert_eq!(stats_for_native_officer(u64::MAX, 20), None);
        assert_eq!(stats_for_native_officer(3_849_424_588, 0), None);
        assert_eq!(stats_for_native_officer(3_849_424_588, 101), None);
    }

    #[test]
    fn catalog_rejects_identity_provenance_and_growth_mutations() {
        for mutation in ["identity", "group", "cache", "growth", "negative_weight"] {
            let mut value: serde_json::Value = serde_json::from_str(EMBEDDED_CATALOG).unwrap();
            match mutation {
                "identity" => value["officers"]["3849424588"]["id"] = 1.into(),
                "group" => value["provenance"]["officer_specs_group"]["id"] = 18.into(),
                "cache" => value["provenance"]["static_cache_sha256"] = "0".repeat(64).into(),
                "growth" => value["growth_by_level"]["20"]["by_rarity"]["3"] = 0.into(),
                "negative_weight" => {
                    value["officers"]["3849424588"]["weights"]["defense"] = (-1).into()
                }
                _ => unreachable!(),
            }
            assert!(parse_catalog(&value.to_string()).is_err(), "{mutation}");
        }
    }
}
