//! Heuristics seeds: player-provided crew lists tested first by the optimizer.
//!
//! File format (one crew per line):
//!   `label:Captain,Bridge1,Bridge2:BelowDeck1,BelowDeck2,...`
//!
//! Lines starting with `#` are comments; blank lines are ignored.
//! Officer names are resolved case-insensitively against the canonical database
//! and `name_aliases.json`. Unknown names are skipped with a warning.
//!
//! **Bridge filtering:** after name resolution, bridge officers are dropped unless they either share a
//! non-empty synergy group (`group` on canonical officers) with the captain, or have at least one
//! canonical bridge ability (`OfficerAbility.slot == "officer"`). This mirrors “synergy or bridge
//! combat ability” so heuristic seeds do not keep dead bridge picks.
//!
//! **Below-decks filtering (strict):** when `apply_below_decks_combat_heuristic_filter` is true (server
//! default), below-decks candidates are dropped unless they have at least one below-decks-slot ability
//! (`OfficerAbility.slot == "below_decks"`) whose canonical `modifier` is not classified as economy /
//! non-combat for seeds (aligned with `generate_lcars` `:non_combat` modifier arms plus common loot
//! and exploration modifiers). Missing `modifier` is treated as ambiguous and kept. Pass `false` when
//! the user allows non-combat below-decks picks (`allow_below_decks_without_combat_ability`).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use tracing::{debug, warn};

use crate::data::officer::{
    load_canonical_officers, normalize_officer_lookup_key, Officer, DEFAULT_CANONICAL_OFFICERS_PATH,
};

pub const DEFAULT_HEURISTICS_DIR: &str = "data/heuristics";
const BRIDGE_SLOTS: usize = 2;

/// Canonical ability `modifier` strings treated as economy / non-combat for heuristic below-decks picks.
/// Keep in sync with `generate_lcars` `MappedEffect::Tag(…:non_combat)` arms and loot/exploration rows.
const NON_COMBAT_BELOW_DECKS_MODIFIERS: &[&str] = &[
    "ActianVenomAndNanoprobeLoot",
    "ArmadaLoot",
    "ArtifactTokenLoot",
    "BrokenShipPartsLoot",
    "CargoCapacity",
    "CargoProtection",
    "CombatDilithiumReward",
    "CombatParsteelReward",
    "CombatPveRewards",
    "CombatScavenger",
    "CombatTritaniumReward",
    "CombatXPReward",
    "FactionPointsGain",
    "GornHostileVolatileLoot",
    "HirogenRelicAndBiotoxinLoot",
    "HostileLoot",
    "ImpulseSpeed",
    "JumpAndTowCostEff",
    "MiningRate",
    "MiningReward",
    "OffAbilityEffect",
    "Omega13Cooldown",
    "PveChestLootMultiplierLimitedResources",
    "RepairCostsPost",
    "RepairTime",
    "SkillCloakingCooldown",
    "SkillCloakingDuration",
    "SkillCuttingBeamAbilityCost",
    "SkillCuttingBeamPvPBaseDamagePercentage",
    "TrelliumRewards",
    "VoyagerAsaCE",
    "WarpDistance",
    "WarpSpeed",
    "WokAugmentAllLootRewards",
    "XindiHostileLoot",
];

/// How to assign below-decks officers when the seed lists more candidates than
/// the ship has slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BelowDecksStrategy {
    /// Take the first k officers from the seed's BD list (author controls priority by list order).
    #[default]
    Ordered,
    /// Try all C(n, k) combinations of k officers from the seed's n candidates.
    Exploration,
}

/// A parsed crew entry before expansion into candidates.
#[derive(Debug, Clone)]
pub struct ParsedHeuristicsCrew {
    pub label: String,
    pub captain: String,
    /// Exactly up to BRIDGE_SLOTS resolved bridge officers.
    pub bridge: Vec<String>,
    /// All resolved below-decks candidates from the seed (may be more than the ship has slots).
    pub below_decks_candidates: Vec<String>,
}

/// A fully expanded candidate ready to be passed to the Monte Carlo runner.
#[derive(Debug, Clone)]
pub struct HeuristicsCandidate {
    pub label: String,
    pub captain: String,
    pub bridge: Vec<String>,
    pub below_decks: Vec<String>,
}

/// List available heuristics seed file stems (filenames without `.txt` extension).
pub fn list_heuristics_seeds(dir: &str) -> Vec<String> {
    let path = Path::new(dir);
    if !path.exists() {
        return Vec::new();
    }
    let mut seeds: Vec<String> = fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().is_some_and(|ext| ext == "txt") {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    seeds.sort();
    seeds
}

/// Load and parse a named seed file from the heuristics directory.
/// Returns parsed crews (not yet expanded into candidates).
/// When `canonical_names_override` is Some, use it for name resolution instead of loading from disk.
pub fn load_seed_file(
    seed_name: &str,
    dir: &str,
    canonical_names_override: Option<&[String]>,
) -> Vec<ParsedHeuristicsCrew> {
    let path = Path::new(dir).join(format!("{seed_name}.txt"));
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "heuristics: could not read seed file");
            return Vec::new();
        }
    };

    let aliases = load_name_aliases();
    let canonical_names: Vec<String> = match canonical_names_override {
        Some(names) => names.to_vec(),
        None => load_canonical_officers(DEFAULT_CANONICAL_OFFICERS_PATH)
            .map(|officers| officers.into_iter().map(|o| o.name).collect())
            .unwrap_or_default(),
    };

    content
        .lines()
        .filter_map(|line| parse_line(line.trim(), &aliases, &canonical_names))
        .collect()
}

/// Expand parsed crews into simulation candidates according to the chosen BD strategy.
pub fn expand_crews(
    crews: Vec<ParsedHeuristicsCrew>,
    below_decks_slots: usize,
    strategy: BelowDecksStrategy,
) -> Vec<HeuristicsCandidate> {
    crews
        .into_iter()
        .flat_map(|crew| expand_crew(crew, below_decks_slots, strategy))
        .collect()
}

/// True if the officer has a bridge officer-slot ability in canonical data (`slot: officer`).
pub fn has_bridge_officer_slot_ability(officer: &Officer) -> bool {
    officer
        .abilities
        .iter()
        .any(|a| a.slot.eq_ignore_ascii_case("officer"))
}

fn non_empty_synergy_group(officer: &Officer) -> Option<&str> {
    officer
        .group
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
}

/// Same non-empty [`Officer::group`] as the captain (STFC officer-group synergy metadata).
pub fn bridge_shares_synergy_group_with_captain(captain: &Officer, bridge: &Officer) -> bool {
    match (
        non_empty_synergy_group(captain),
        non_empty_synergy_group(bridge),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Keep a heuristic bridge pick if it synergizes with the captain or contributes a bridge-slot ability.
pub fn keep_bridge_officer_for_heuristic_seed(captain: &Officer, bridge: &Officer) -> bool {
    bridge_shares_synergy_group_with_captain(captain, bridge)
        || has_bridge_officer_slot_ability(bridge)
}

#[inline]
fn canonical_modifier_is_heuristic_non_combat(modifier: &str) -> bool {
    NON_COMBAT_BELOW_DECKS_MODIFIERS
        .iter()
        .any(|m| modifier.eq_ignore_ascii_case(m))
}

/// True if the officer has at least one below-decks-slot ability that is not economy-only for seeds.
pub fn has_combat_below_decks_slot_ability(officer: &Officer) -> bool {
    officer.abilities.iter().any(|a| {
        if !a.slot.eq_ignore_ascii_case("below_decks") {
            return false;
        }
        match a.modifier.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            None => true,
            Some(m) => !canonical_modifier_is_heuristic_non_combat(m),
        }
    })
}

/// Apply bridge rules ([`keep_bridge_officer_for_heuristic_seed`]) and, when `apply_below_decks_combat_heuristic_filter`,
/// below-decks combat rules ([`has_combat_below_decks_slot_ability`]) to parsed heuristic crews.
pub fn filter_heuristic_seed_crews(
    crews: Vec<ParsedHeuristicsCrew>,
    officer_index: &HashMap<String, Officer>,
    apply_below_decks_combat_heuristic_filter: bool,
) -> Vec<ParsedHeuristicsCrew> {
    crews
        .into_iter()
        .filter_map(|mut crew| {
            let cap_key = normalize_officer_lookup_key(&crew.captain);
            let Some(captain_off) = officer_index.get(&cap_key) else {
                warn!(
                    label = %crew.label,
                    captain = %crew.captain,
                    "heuristics: captain not in officer index; skipping bridge/below-decks filter for this crew"
                );
                return Some(crew);
            };

            let bridge_before = crew.bridge.len();
            crew.bridge.retain(|bridge_name| {
                let key = normalize_officer_lookup_key(bridge_name);
                let Some(b_off) = officer_index.get(&key) else {
                    warn!(
                        label = %crew.label,
                        bridge_name = %bridge_name,
                        "heuristics: bridge officer not in officer index; dropping"
                    );
                    return false;
                };
                let keep = keep_bridge_officer_for_heuristic_seed(captain_off, b_off);
                if !keep {
                    debug!(
                        label = %crew.label,
                        captain = %crew.captain,
                        bridge = %bridge_name,
                        "heuristics: dropping bridge officer (no shared synergy group and no bridge-slot ability)"
                    );
                }
                keep
            });

            if crew.bridge.len() != bridge_before {
                debug!(
                    label = %crew.label,
                    captain = %crew.captain,
                    before = bridge_before,
                    after = crew.bridge.len(),
                    "heuristics: filtered bridge officers"
                );
            }

            if apply_below_decks_combat_heuristic_filter {
                let bd_before = crew.below_decks_candidates.len();
                crew.below_decks_candidates.retain(|bd_name| {
                    let key = normalize_officer_lookup_key(bd_name);
                    let Some(bd_off) = officer_index.get(&key) else {
                        warn!(
                            label = %crew.label,
                            bd_name = %bd_name,
                            "heuristics: below-decks officer not in officer index; dropping"
                        );
                        return false;
                    };
                    let keep = has_combat_below_decks_slot_ability(bd_off);
                    if !keep {
                        debug!(
                            label = %crew.label,
                            below_decks = %bd_name,
                            "heuristics: dropping below-decks officer (no combat-relevant below-decks-slot ability)"
                        );
                    }
                    keep
                });

                if crew.below_decks_candidates.len() != bd_before {
                    debug!(
                        label = %crew.label,
                        captain = %crew.captain,
                        before = bd_before,
                        after = crew.below_decks_candidates.len(),
                        "heuristics: filtered below-decks candidates"
                    );
                }
            }

            Some(crew)
        })
        .collect()
}

fn parse_line(
    line: &str,
    aliases: &HashMap<String, String>,
    canonical_names: &[String],
) -> Option<ParsedHeuristicsCrew> {
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    // Split into at most 3 parts: label : bridge_section : bd_section
    let mut parts = line.splitn(3, ':');
    let label = parts.next()?.trim().to_string();
    if label.is_empty() {
        return None;
    }
    let bridge_section = parts.next()?.trim();
    let bd_section = parts.next().unwrap_or("").trim();

    let bridge_officers: Vec<&str> = bridge_section
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    if bridge_officers.is_empty() {
        return None;
    }

    let captain = resolve_name(bridge_officers[0], aliases, canonical_names)?;

    let bridge: Vec<String> = bridge_officers
        .iter()
        .skip(1)
        .take(BRIDGE_SLOTS)
        .filter_map(|raw| resolve_name(raw, aliases, canonical_names))
        .filter(|name| name != &captain)
        .collect();

    let below_decks_candidates: Vec<String> = if bd_section.is_empty() {
        Vec::new()
    } else {
        let used: std::collections::HashSet<&str> = std::iter::once(captain.as_str())
            .chain(bridge.iter().map(String::as_str))
            .collect();
        bd_section
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter_map(|raw| resolve_name(raw, aliases, canonical_names))
            .filter(|name| !used.contains(name.as_str()))
            .collect()
    };

    Some(ParsedHeuristicsCrew {
        label,
        captain,
        bridge,
        below_decks_candidates,
    })
}

fn expand_crew(
    crew: ParsedHeuristicsCrew,
    below_decks_slots: usize,
    strategy: BelowDecksStrategy,
) -> Vec<HeuristicsCandidate> {
    let n = crew.below_decks_candidates.len();
    let k = below_decks_slots.min(n);

    let bd_selections: Vec<Vec<String>> = if n == 0 {
        vec![Vec::new()]
    } else {
        match strategy {
            BelowDecksStrategy::Ordered => vec![crew.below_decks_candidates[..k].to_vec()],
            BelowDecksStrategy::Exploration => combinations(&crew.below_decks_candidates, k),
        }
    };

    bd_selections
        .into_iter()
        .map(|bd| HeuristicsCandidate {
            label: crew.label.clone(),
            captain: crew.captain.clone(),
            bridge: crew.bridge.clone(),
            below_decks: bd,
        })
        .collect()
}

/// All C(n, k) combinations of `k` elements from `items`, preserving relative order.
fn combinations<T: Clone>(items: &[T], k: usize) -> Vec<Vec<T>> {
    if k == 0 {
        return vec![Vec::new()];
    }
    if k > items.len() {
        return Vec::new();
    }
    let mut result = Vec::new();
    combine(items, k, 0, &mut Vec::new(), &mut result);
    result
}

fn combine<T: Clone>(
    items: &[T],
    k: usize,
    start: usize,
    current: &mut Vec<T>,
    result: &mut Vec<Vec<T>>,
) {
    if current.len() == k {
        result.push(current.clone());
        return;
    }
    for i in start..items.len() {
        current.push(items[i].clone());
        combine(items, k, i + 1, current, result);
        current.pop();
    }
}

/// Resolve an officer name from a heuristics file to its canonical display name.
/// Tries: name_aliases.json (uppercase key) → exact case-insensitive match → unique substring match.
fn resolve_name(
    raw: &str,
    aliases: &HashMap<String, String>,
    canonical_names: &[String],
) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let key = trimmed.to_uppercase();

    // 1. Alias lookup (aliases keys are uppercase)
    if let Some(name) = aliases.get(&key) {
        return Some(name.clone());
    }

    // 2. Exact case-insensitive match
    if let Some(name) = canonical_names
        .iter()
        .find(|n| n.eq_ignore_ascii_case(trimmed))
    {
        return Some(name.clone());
    }

    // 3. Unique substring match
    let lower = trimmed.to_lowercase();
    let matches: Vec<&String> = canonical_names
        .iter()
        .filter(|n| n.to_lowercase().contains(&lower))
        .collect();
    match matches.len() {
        1 => Some(matches[0].clone()),
        0 => {
            warn!(%trimmed, "heuristics: no match for officer name; skipping");
            None
        }
        n => {
            warn!(
                %trimmed,
                match_count = n,
                "heuristics: ambiguous officer name; skipping (use a more specific name)"
            );
            None
        }
    }
}

fn load_name_aliases() -> HashMap<String, String> {
    const ALIASES_PATH: &str = "data/officers/name_aliases.json";
    fs::read_to_string(ALIASES_PATH)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::data::officer::{Officer, OfficerAbility};

    use super::{
        combinations, filter_heuristic_seed_crews, BelowDecksStrategy, ParsedHeuristicsCrew,
    };

    fn officer_named(name: &str, group: Option<&str>, ability_slots: &[&str]) -> Officer {
        Officer {
            id: format!("id-{name}"),
            name: name.to_string(),
            slot: None,
            group: group.map(String::from),
            abilities: ability_slots
                .iter()
                .map(|s| OfficerAbility {
                    slot: (*s).to_string(),
                    trigger: None,
                    modifier: None,
                    attributes: None,
                    description: None,
                    chance_by_rank: vec![],
                    value_by_rank: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn keep_bridge_synergy_same_group_without_officer_ability() {
        let cap = officer_named("Kirk", Some("TOS"), &["captain"]);
        let br = officer_named("Spock", Some("TOS"), &["captain"]);
        assert!(super::keep_bridge_officer_for_heuristic_seed(&cap, &br));
    }

    #[test]
    fn keep_bridge_officer_slot_ability_different_group() {
        let cap = officer_named("Kirk", Some("TOS"), &["captain"]);
        let br = officer_named("Worf", Some("TNG"), &["officer"]);
        assert!(super::keep_bridge_officer_for_heuristic_seed(&cap, &br));
    }

    #[test]
    fn drop_bridge_no_synergy_no_officer_ability() {
        let cap = officer_named("Kirk", Some("TOS"), &["captain"]);
        let br = officer_named("Worf", Some("TNG"), &["captain"]);
        assert!(!super::keep_bridge_officer_for_heuristic_seed(&cap, &br));
    }

    #[test]
    fn filter_heuristic_crews_drops_unqualified_bridge() {
        let crew = ParsedHeuristicsCrew {
            label: "t".into(),
            captain: "Kirk".into(),
            bridge: vec!["Worf".into()],
            below_decks_candidates: vec![],
        };
        let mut idx = HashMap::new();
        idx.insert(
            super::normalize_officer_lookup_key("Kirk"),
            officer_named("Kirk", Some("TOS"), &["captain"]),
        );
        idx.insert(
            super::normalize_officer_lookup_key("Worf"),
            officer_named("Worf", Some("TNG"), &["captain"]),
        );
        let out = filter_heuristic_seed_crews(vec![crew], &idx, true);
        assert_eq!(out.len(), 1);
        assert!(out[0].bridge.is_empty());
    }

    #[test]
    fn keep_below_decks_when_modifier_not_economy_only() {
        let o = officer_named("Bob", None, &[]);
        let mut o_combat = o.clone();
        o_combat.abilities.push(OfficerAbility {
            slot: "below_decks".into(),
            trigger: None,
            modifier: Some("AllDamage".into()),
            attributes: None,
            description: None,
            chance_by_rank: vec![],
            value_by_rank: vec![],
        });
        assert!(super::has_combat_below_decks_slot_ability(&o_combat));

        let mut o_loot = o;
        o_loot.abilities.push(OfficerAbility {
            slot: "below_decks".into(),
            trigger: None,
            modifier: Some("HostileLoot".into()),
            attributes: None,
            description: None,
            chance_by_rank: vec![],
            value_by_rank: vec![],
        });
        assert!(!super::has_combat_below_decks_slot_ability(&o_loot));
    }

    #[test]
    fn filter_heuristic_seed_crews_drops_economy_only_below_decks() {
        let crew = ParsedHeuristicsCrew {
            label: "bd".into(),
            captain: "Kirk".into(),
            bridge: vec![],
            below_decks_candidates: vec!["LootOnly".into()],
        };
        let mut idx = HashMap::new();
        idx.insert(
            super::normalize_officer_lookup_key("Kirk"),
            officer_named("Kirk", Some("TOS"), &["captain"]),
        );
        let mut loot_only = officer_named("LootOnly", None, &[]);
        loot_only.abilities.push(OfficerAbility {
            slot: "below_decks".into(),
            trigger: None,
            modifier: Some("MiningRate".into()),
            attributes: None,
            description: None,
            chance_by_rank: vec![],
            value_by_rank: vec![],
        });
        idx.insert(super::normalize_officer_lookup_key("LootOnly"), loot_only);
        let out = filter_heuristic_seed_crews(vec![crew], &idx, true);
        assert_eq!(out.len(), 1);
        assert!(out[0].below_decks_candidates.is_empty());
    }

    #[test]
    fn filter_heuristic_seed_crews_relaxed_keeps_economy_only_below_decks() {
        let crew = ParsedHeuristicsCrew {
            label: "bd".into(),
            captain: "Kirk".into(),
            bridge: vec![],
            below_decks_candidates: vec!["LootOnly".into()],
        };
        let mut idx = HashMap::new();
        idx.insert(
            super::normalize_officer_lookup_key("Kirk"),
            officer_named("Kirk", Some("TOS"), &["captain"]),
        );
        let mut loot_only = officer_named("LootOnly", None, &[]);
        loot_only.abilities.push(OfficerAbility {
            slot: "below_decks".into(),
            trigger: None,
            modifier: Some("MiningRate".into()),
            attributes: None,
            description: None,
            chance_by_rank: vec![],
            value_by_rank: vec![],
        });
        idx.insert(super::normalize_officer_lookup_key("LootOnly"), loot_only);
        let out = filter_heuristic_seed_crews(vec![crew], &idx, false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].below_decks_candidates, vec!["LootOnly".to_string()]);
    }

    #[test]
    fn combinations_c3_2() {
        let items = vec!["A", "B", "C"];
        let result = combinations(&items, 2);
        assert_eq!(result.len(), 3);
        assert!(result.contains(&vec!["A", "B"]));
        assert!(result.contains(&vec!["A", "C"]));
        assert!(result.contains(&vec!["B", "C"]));
    }

    #[test]
    fn combinations_k_equals_n() {
        let items = vec!["X", "Y"];
        let result = combinations(&items, 2);
        assert_eq!(result, vec![vec!["X", "Y"]]);
    }

    #[test]
    fn combinations_k_zero() {
        let items = vec!["A", "B"];
        let result = combinations(&items, 0);
        assert_eq!(result, vec![vec![] as Vec<&str>]);
    }

    #[test]
    fn expand_crew_ordered_takes_first_k() {
        let crew = ParsedHeuristicsCrew {
            label: "test".into(),
            captain: "Alpha".into(),
            bridge: vec!["Beta".into(), "Gamma".into()],
            below_decks_candidates: vec![
                "D1".into(),
                "D2".into(),
                "D3".into(),
                "D4".into(),
                "D5".into(),
            ],
        };
        let candidates = super::expand_crew(crew, 3, BelowDecksStrategy::Ordered);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].below_decks, vec!["D1", "D2", "D3"]);
    }

    #[test]
    fn expand_crew_exploration_generates_combinations() {
        let crew = ParsedHeuristicsCrew {
            label: "test".into(),
            captain: "Alpha".into(),
            bridge: vec!["Beta".into(), "Gamma".into()],
            below_decks_candidates: vec!["D1".into(), "D2".into(), "D3".into(), "D4".into()],
        };
        // C(4, 3) = 4
        let candidates = super::expand_crew(crew, 3, BelowDecksStrategy::Exploration);
        assert_eq!(candidates.len(), 4);
    }

    #[test]
    fn expand_crew_fewer_bd_than_slots() {
        let crew = ParsedHeuristicsCrew {
            label: "test".into(),
            captain: "Alpha".into(),
            bridge: vec!["Beta".into()],
            below_decks_candidates: vec!["D1".into(), "D2".into()],
        };
        let candidates = super::expand_crew(crew, 3, BelowDecksStrategy::Ordered);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].below_decks.len(), 2); // uses all available
    }
}
