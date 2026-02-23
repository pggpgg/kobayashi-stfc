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
}

impl OfficerAbility {
    pub fn applies_morale_state(&self) -> bool {
        let is_state_modifier = self
            .modifier
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case("AddState"))
            .unwrap_or(false);
        let has_morale_attribute = self
            .attributes
            .as_deref()
            .map(|value| normalize_for_lookup(value).contains("state8"))
            .unwrap_or(false);
        let description_mentions_morale = self
            .description
            .as_deref()
            .map(|value| normalize_for_lookup(value).contains("morale"))
            .unwrap_or(false);

        is_state_modifier && (has_morale_attribute || description_mentions_morale)
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
        let is_state_modifier = self
            .modifier
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case("AddState"))
            .unwrap_or(false);
        let normalized_attributes = self
            .attributes
            .as_deref()
            .map(normalize_for_lookup)
            .unwrap_or_default();
        let has_assimilated_attribute = normalized_attributes.contains("state64");
        let description_mentions_assimilated = self
            .description
            .as_deref()
            .map(|value| normalize_for_lookup(value).contains("assimilat"))
            .unwrap_or(false);

        is_state_modifier && (has_assimilated_attribute || description_mentions_assimilated)
    }

    pub fn applies_hull_breach_state(&self) -> bool {
        let is_state_modifier = self
            .modifier
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case("AddState"))
            .unwrap_or(false);
        let normalized_attributes = self
            .attributes
            .as_deref()
            .map(normalize_for_lookup)
            .unwrap_or_default();
        let has_hull_breach_attribute = normalized_attributes.contains("state4");
        let description_mentions_hull_breach = self
            .description
            .as_deref()
            .map(|value| normalize_for_lookup(value).contains("hullbreach"))
            .unwrap_or(false);

        is_state_modifier && (has_hull_breach_attribute || description_mentions_hull_breach)
    }

    pub fn applies_burning_state(&self) -> bool {
        let is_state_modifier = self
            .modifier
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case("AddState"))
            .unwrap_or(false);
        let normalized_attributes = self
            .attributes
            .as_deref()
            .map(normalize_for_lookup)
            .unwrap_or_default();
        let has_burning_attribute = normalized_attributes.contains("state2");
        let description_mentions_burning = self
            .description
            .as_deref()
            .map(|value| normalize_for_lookup(value).contains("burning"))
            .unwrap_or(false);

        is_state_modifier && (has_burning_attribute || description_mentions_burning)
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
        self.value_by_rank
            .get(index)
            .copied()
            .unwrap_or(first)
    }
}

fn normalize_for_lookup(input: &str) -> String {
    input
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
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
