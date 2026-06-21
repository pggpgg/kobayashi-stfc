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
    /// Display synergy group from canonical data (e.g. shared crew set). Used for heuristics filtering
    /// and search pruning; not passed through to combat unless modeled elsewhere.
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub abilities: Vec<OfficerAbility>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OfficerAbility {
    pub slot: String,
    /// Canonical applicability predicates such as `EnemyPlayer`, used by optimizer
    /// scenario-specific eligibility filters as well as LCARS generation.
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub modifier: Option<String>,
    #[serde(default)]
    pub attributes: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub chance_by_rank: Vec<f64>,
    #[serde(default)]
    pub value_by_rank: Vec<f64>,

    /// Bit-packed cache of the four `applies_*_state` predicates, computed once at load time
    /// (or when this ability is constructed in tests). Bits:
    /// `0`=morale, `1`=assimilated, `2`=hull_breach, `3`=burning. Zero means "no state effect".
    ///
    /// `#[serde(skip)]` so JSON loads start at 0; [`load_canonical_officers`] then runs
    /// [`Self::recompute_state_mask`] to fill it. Test code that builds `OfficerAbility` via
    /// struct literal can default this to 0 — but should call `recompute_state_mask()`
    /// afterwards if it expects `applies_*_state` to fire.
    #[serde(skip, default)]
    pub state_mask: u8,
}

/// Bit positions in [`OfficerAbility::state_mask`].
const STATE_MASK_MORALE: u8 = 1 << 0;
const STATE_MASK_ASSIMILATED: u8 = 1 << 1;
const STATE_MASK_HULL_BREACH: u8 = 1 << 2;
const STATE_MASK_BURNING: u8 = 1 << 3;

impl OfficerAbility {
    pub fn applies_morale_state(&self) -> bool {
        self.state_mask & STATE_MASK_MORALE != 0
    }

    pub fn morale_chance_for_tier(&self, tier: Option<u8>) -> f64 {
        let Some((&first, _rest)) = self.chance_by_rank.split_first() else {
            return 1.0;
        };

        let index = tier
            .and_then(|value| value.checked_sub(1))
            .map(usize::from)
            .unwrap_or(0);
        self.chance_by_rank
            .get(index)
            .copied()
            .unwrap_or(first)
            .clamp(0.0, 1.0)
    }

    pub fn applies_assimilated_state(&self) -> bool {
        self.state_mask & STATE_MASK_ASSIMILATED != 0
    }

    pub fn applies_hull_breach_state(&self) -> bool {
        self.state_mask & STATE_MASK_HULL_BREACH != 0
    }

    pub fn applies_burning_state(&self) -> bool {
        self.state_mask & STATE_MASK_BURNING != 0
    }

    /// Compute and store the bit-packed `state_mask` from this ability's `modifier`, `attributes`,
    /// and `description`. Called once per ability after JSON deserialization (see
    /// [`load_canonical_officers`]) so subsequent `applies_*_state` calls are pure bit tests
    /// rather than per-call string normalization + substring matching. Hot-path-critical: a GA
    /// run was previously spending ~9 % of CPU here.
    ///
    /// Idempotent — safe to call repeatedly. Test code that builds `OfficerAbility` via struct
    /// literal should call this manually if it relies on `applies_*_state` returning true.
    /// Use [`Self::with_state_mask_recomputed`] for a fluent builder-style alternative.
    pub fn recompute_state_mask(&mut self) {
        self.state_mask = 0;
        if !self
            .modifier
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("AddState"))
        {
            return;
        }
        let normalized_attrs = self
            .attributes
            .as_deref()
            .map(normalize_for_lookup)
            .unwrap_or_default();
        let normalized_desc = self
            .description
            .as_deref()
            .map(normalize_for_lookup)
            .unwrap_or_default();

        if normalized_attrs.contains("state8") || normalized_desc.contains("morale") {
            self.state_mask |= STATE_MASK_MORALE;
        }
        if normalized_attrs.contains("state64") || normalized_desc.contains("assimilat") {
            self.state_mask |= STATE_MASK_ASSIMILATED;
        }
        if normalized_attrs.contains("state4") || normalized_desc.contains("hullbreach") {
            self.state_mask |= STATE_MASK_HULL_BREACH;
        }
        if normalized_attrs.contains("state2") || normalized_desc.contains("burning") {
            self.state_mask |= STATE_MASK_BURNING;
        }
    }

    /// Builder-style helper: returns self with `state_mask` populated from current
    /// `modifier`/`attributes`/`description`. Convenience for test code that constructs
    /// `OfficerAbility` via struct literal.
    pub fn with_state_mask_recomputed(mut self) -> Self {
        self.recompute_state_mask();
        self
    }

    pub fn triggers_on_critical_shot(&self) -> bool {
        self.trigger
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case("CriticalShotFired"))
            .unwrap_or(false)
    }

    pub fn state_duration_rounds(&self) -> u32 {
        self.attributes
            .as_deref()
            .and_then(|attributes| {
                attributes.split(',').find_map(|entry| {
                    let mut parts = entry.splitn(2, '=');
                    let key = parts.next()?.trim();
                    let value = parts.next()?.trim();
                    if key.eq_ignore_ascii_case("num_rounds") {
                        value.parse::<u32>().ok().filter(|rounds| *rounds > 0)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(1)
    }

    pub fn is_round_start_trigger(&self) -> bool {
        self.trigger
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case("RoundStart"))
            .unwrap_or(false)
    }

    pub fn modifier_is_apex_shred(&self) -> bool {
        self.modifier
            .as_deref()
            .map(|m| m.eq_ignore_ascii_case("ApexShred"))
            .unwrap_or(false)
    }

    pub fn modifier_is_apex_barrier(&self) -> bool {
        self.modifier
            .as_deref()
            .map(|m| m.eq_ignore_ascii_case("ApexBarrier"))
            .unwrap_or(false)
    }

    /// Value at given tier (1-based); 0 if value_by_rank is empty or index out of range.
    pub fn value_for_tier(&self, tier: Option<u8>) -> f64 {
        let Some((&first, _rest)) = self.value_by_rank.split_first() else {
            return 0.0;
        };
        let index = tier
            .and_then(|t| t.checked_sub(1))
            .map(usize::from)
            .unwrap_or(0);
        self.value_by_rank.get(index).copied().unwrap_or(first)
    }
}

/// Normalized officer name key (alphanumeric only, lowercase) — matches `DataRegistry::officer_index` keys.
///
/// The filter keeps only ASCII alphanumerics, so `to_ascii_lowercase` is correct and far cheaper
/// than `char::to_lowercase` (which returns an iterator per char to handle Unicode case-folding).
///
/// Note: this is duplicated as `crew_resolution::normalize_lookup_key` and
/// `crew_generator::officer_lookup_key`. Consolidation would be cleanup; the four copies are
/// kept identical and performance-equivalent.
pub fn normalize_officer_lookup_key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

fn normalize_for_lookup(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct CanonicalOfficersFile {
    officers: Vec<Officer>,
}

pub fn load_canonical_officers(path: impl AsRef<Path>) -> Result<Vec<Officer>, std::io::Error> {
    let raw = fs::read_to_string(path)?;
    let mut parsed: CanonicalOfficersFile =
        serde_json::from_str(&raw).map_err(std::io::Error::other)?;
    // Pre-compute the per-ability state-flag cache once at load time so the per-candidate
    // `applies_*_state` calls in `seat_from_officer` become bit tests instead of repeated
    // `normalize_for_lookup` allocations + substring matching. See OfficerAbility::state_mask
    // for the bit layout.
    for officer in &mut parsed.officers {
        for ability in &mut officer.abilities {
            ability.recompute_state_mask();
        }
    }
    Ok(parsed.officers)
}
