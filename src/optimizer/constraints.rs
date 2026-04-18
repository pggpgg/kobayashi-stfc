//! Optional search constraints for crew optimization (must-include, exclude, groups, seating).

use crate::optimizer::crew_generator::CrewCandidate;
use std::collections::HashSet;

/// Normalized officer name for case-insensitive matching (trim + lowercase).
pub fn normalize_officer_name(s: &str) -> String {
    s.trim().to_lowercase()
}

/// At least `min_count` distinct officers from `officers` must appear somewhere on the crew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficerGroupConstraint {
    pub officers: Vec<String>,
    pub min_count: u32,
}

/// Full constraint set applied after candidate generation (and enforced in genetic search).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrewSearchConstraints {
    pub must_include: Vec<String>,
    pub exclude: Vec<String>,
    pub groups: Vec<OfficerGroupConstraint>,
    /// Captain must be one of these officers (OR). Parsed from API `captain_must_be` comma/semicolon lists.
    pub captain_must_be: Vec<String>,
    pub bridge_must_include: Vec<String>,
    pub below_decks_must_include: Vec<String>,
}

impl CrewSearchConstraints {
    pub fn is_empty(&self) -> bool {
        self.must_include.is_empty()
            && self.exclude.is_empty()
            && self.groups.is_empty()
            && self.captain_must_be.is_empty()
            && self.bridge_must_include.is_empty()
            && self.below_decks_must_include.is_empty()
    }

    /// True if `candidate` satisfies all rules.
    pub fn satisfies(&self, candidate: &CrewCandidate) -> bool {
        let cap_n = normalize_officer_name(&candidate.captain);
        let bridge_n: Vec<String> = candidate
            .bridge
            .iter()
            .map(|s| normalize_officer_name(s))
            .collect();
        let below_n: Vec<String> = candidate
            .below_decks
            .iter()
            .map(|s| normalize_officer_name(s))
            .collect();

        let mut all_on_ship: HashSet<String> = HashSet::new();
        all_on_ship.insert(cap_n.clone());
        for s in &bridge_n {
            all_on_ship.insert(s.clone());
        }
        for s in &below_n {
            all_on_ship.insert(s.clone());
        }

        for ex in &self.exclude {
            let n = normalize_officer_name(ex);
            if n.is_empty() {
                continue;
            }
            if all_on_ship.contains(&n) {
                return false;
            }
        }

        for req in &self.must_include {
            let n = normalize_officer_name(req);
            if n.is_empty() {
                continue;
            }
            if !all_on_ship.contains(&n) {
                return false;
            }
        }

        if !self.captain_must_be.is_empty() {
            let ok = self.captain_must_be.iter().any(|rule| {
                let want = normalize_officer_name(rule);
                !want.is_empty() && cap_n == want
            });
            if !ok {
                return false;
            }
        }

        for b in &self.bridge_must_include {
            let n = normalize_officer_name(b);
            if n.is_empty() {
                continue;
            }
            if !bridge_n.iter().any(|x| x == &n) {
                return false;
            }
        }

        for b in &self.below_decks_must_include {
            let n = normalize_officer_name(b);
            if n.is_empty() {
                continue;
            }
            if !below_n.iter().any(|x| x == &n) {
                return false;
            }
        }

        for g in &self.groups {
            let mut on = 0_u32;
            for o in &g.officers {
                let n = normalize_officer_name(o);
                if n.is_empty() {
                    continue;
                }
                if all_on_ship.contains(&n) {
                    on = on.saturating_add(1);
                }
            }
            if on < g.min_count {
                return false;
            }
        }

        true
    }
}

/// Drop candidates that violate constraints.
pub fn filter_candidates(
    candidates: Vec<CrewCandidate>,
    constraints: &CrewSearchConstraints,
) -> Vec<CrewCandidate> {
    if constraints.is_empty() {
        return candidates;
    }
    candidates
        .into_iter()
        .filter(|c| constraints.satisfies(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crew(cap: &str, b: &[&str], bd: &[&str]) -> CrewCandidate {
        CrewCandidate {
            captain: cap.into(),
            bridge: b.iter().map(|s| (*s).into()).collect(),
            below_decks: bd.iter().map(|s| (*s).into()).collect(),
        }
    }

    #[test]
    fn exclude_and_must_include() {
        let c = CrewSearchConstraints {
            must_include: vec!["Alice".into()],
            exclude: vec!["Bob".into()],
            ..Default::default()
        };
        assert!(c.satisfies(&crew("Alice", &["Eve", "Quinn"], &["Zed", "Y", "X"])));
        assert!(c.satisfies(&crew("Eve", &["Alice", "Quinn"], &["Zed", "Y", "X"])));
        assert!(!c.satisfies(&crew("Alice", &["Eve", "Bob"], &["Zed", "Y", "X"])));
    }

    #[test]
    fn captain_and_seat_rules() {
        let c = CrewSearchConstraints {
            captain_must_be: vec!["Picard".into()],
            bridge_must_include: vec!["Riker".into()],
            below_decks_must_include: vec!["Data".into()],
            ..Default::default()
        };
        assert!(c.satisfies(&crew(
            "Picard",
            &["Riker", "Troi"],
            &["Data", "La Forge", "Crusher"]
        )));
        assert!(!c.satisfies(&crew(
            "Riker",
            &["Picard", "Troi"],
            &["Data", "La Forge", "Crusher"]
        )));
        assert!(!c.satisfies(&crew(
            "Picard",
            &["Troi", "Worf"],
            &["Data", "La Forge", "Crusher"]
        )));
    }

    #[test]
    fn captain_must_be_alternatives_or() {
        let c = CrewSearchConstraints {
            captain_must_be: vec!["Picard".into(), "Riker".into()],
            ..Default::default()
        };
        assert!(c.satisfies(&crew("Picard", &["A", "B"], &["C", "D", "E"])));
        assert!(c.satisfies(&crew("Riker", &["A", "B"], &["C", "D", "E"])));
        assert!(!c.satisfies(&crew("Data", &["A", "B"], &["C", "D", "E"])));
    }

    #[test]
    fn group_min_count() {
        let c = CrewSearchConstraints {
            groups: vec![OfficerGroupConstraint {
                officers: vec!["A".into(), "B".into(), "C".into()],
                min_count: 2,
            }],
            ..Default::default()
        };
        assert!(c.satisfies(&crew("A", &["B", "Z"], &["Q", "W", "E"])));
        assert!(!c.satisfies(&crew("A", &["Z", "Y"], &["Q", "W", "E"])));
    }
}
