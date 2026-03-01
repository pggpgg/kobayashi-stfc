//! Heuristics seeds: player-provided crew lists tested first by the optimizer.
//!
//! File format (one crew per line):
//!   `label:Captain,Bridge1,Bridge2:BelowDeck1,BelowDeck2,...`
//!
//! Lines starting with `#` are comments; blank lines are ignored.
//! Officer names are resolved case-insensitively against the canonical database
//! and `name_aliases.json`. Unknown names are skipped with a warning.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::data::officer::{load_canonical_officers, DEFAULT_CANONICAL_OFFICERS_PATH};

pub const DEFAULT_HEURISTICS_DIR: &str = "data/heuristics";
const BRIDGE_SLOTS: usize = 2;

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
            if p.extension().map_or(false, |ext| ext == "txt") {
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
            eprintln!("heuristics: could not read '{path}': {e}", path = path.display());
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
    crews.into_iter().flat_map(|crew| expand_crew(crew, below_decks_slots, strategy)).collect()
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
        let used: std::collections::HashSet<&str> =
            std::iter::once(captain.as_str()).chain(bridge.iter().map(String::as_str)).collect();
        bd_section
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter_map(|raw| resolve_name(raw, aliases, canonical_names))
            .filter(|name| !used.contains(name.as_str()))
            .collect()
    };

    Some(ParsedHeuristicsCrew { label, captain, bridge, below_decks_candidates })
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
    if let Some(name) = canonical_names.iter().find(|n| n.eq_ignore_ascii_case(trimmed)) {
        return Some(name.clone());
    }

    // 3. Unique substring match
    let lower = trimmed.to_lowercase();
    let matches: Vec<&String> =
        canonical_names.iter().filter(|n| n.to_lowercase().contains(&lower)).collect();
    match matches.len() {
        1 => Some(matches[0].clone()),
        0 => {
            eprintln!("heuristics: no match for officer name '{trimmed}'; skipping");
            None
        }
        n => {
            eprintln!(
                "heuristics: ambiguous officer name '{trimmed}' ({n} matches); skipping. \
                 Use a more specific name."
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
    use super::{combinations, BelowDecksStrategy, ParsedHeuristicsCrew};

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
            below_decks_candidates: vec!["D1".into(), "D2".into(), "D3".into(), "D4".into(), "D5".into()],
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
