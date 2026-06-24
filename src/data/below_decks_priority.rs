//! Curated below-decks officer priority for the optimizer.
//!
//! Data file: [`DEFAULT_BELOW_DECKS_PRIORITY_PATH`] — one canonical officer name per line,
//! ranked best-first (`#` comments and blank lines ignored). These officers are floated to the
//! top of the below-decks pool ordering ([`crate::optimizer`]'s `sort_below_decks_by_rank_and_power`)
//! and seed a guaranteed "proven crew" warm-start, so a low `max_candidates` combined with many
//! below-decks slots never starves the search of a viable lineup.
//!
//! This is a **search-quality nudge only**: it changes candidate ordering/seeding, never combat
//! math or officer eligibility. An unmatched name is skipped (graceful degradation).

use std::path::Path;
use std::sync::OnceLock;

pub const DEFAULT_BELOW_DECKS_PRIORITY_PATH: &str = "data/optimizer/below_decks_priority.txt";

static PRIORITY: OnceLock<Vec<String>> = OnceLock::new();

fn load() -> Vec<String> {
    let raw = std::fs::read_to_string(crate::runtime_paths::resolve(
        DEFAULT_BELOW_DECKS_PRIORITY_PATH,
    ))
    .or_else(|_| std::fs::read_to_string(Path::new(DEFAULT_BELOW_DECKS_PRIORITY_PATH)));
    let Ok(raw) = raw else {
        return Vec::new();
    };
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Curated below-decks officer names, ranked best-first. Empty when the data file is absent.
pub fn curated_below_decks_priority() -> &'static [String] {
    PRIORITY.get_or_init(load).as_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_curated_names_in_order_without_comments() {
        let names = curated_below_decks_priority();
        // The shipped data file is non-empty and starts with the strongest entry.
        assert!(
            !names.is_empty(),
            "curated below-decks priority should load"
        );
        assert_eq!(names.first().map(String::as_str), Some("B'Elanna Torres"));
        assert!(
            names
                .iter()
                .all(|n| !n.starts_with('#') && !n.trim().is_empty()),
            "comments/blank lines must be stripped"
        );
    }
}
