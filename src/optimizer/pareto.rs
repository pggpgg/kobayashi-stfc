//! Pareto tagging and recommendation reasons over already-ranked optimize rows.
//!
//! The scalar [`RankingScore`](super::ranking::RankingScore) stays the default sort; this pass only
//! annotates. It answers "why would I pick this row over the one above it?" using metrics the
//! simulation already produced, so it costs no extra trials and can never reorder or delete a crew.
//!
//! Two kinds of annotation:
//!
//! - **Front membership** ([`ParetoTag::ParetoOptimal`]) — no other considered row is at least as
//!   good on every objective and clearly better on one.
//! - **Named views** — the single best row for a way of playing: safest, fastest farming, best
//!   chain crew, most different competitive crew.
//!
//! Deliberate exclusions:
//!
//! - **Confidence-interval width is not an objective.** A row measured on 20 scout trials has wider
//!   intervals than one confirmed on 2,000, and folding that into dominance would let noisy rows
//!   crowd the front for being under-measured. Interval depth is surfaced as a caveat inside
//!   [`RowHighlights::reason`] instead.
//! - **`linear_eval` runs are skipped entirely.** Those rows carry no Monte Carlo rates, so every
//!   objective ties at zero and every row would be "non-dominated" — a tag on all of them says
//!   nothing.
//! - **Substitute/rarity views are not built here.** Ranking a crew by roster accessibility needs
//!   officer rarity and ownership, which these rows do not carry; that belongs with the substitute
//!   planner, not with the metrics pass.

use crate::optimizer::ranking::{material_jaccard_similarity, RankedCrewResult};

/// Head of the strength-sorted list that gets tagged. The dominance pass is O(n²) in the rows it
/// considers and optimize returns every simulated crew, so tagging is bounded to the part of the
/// table a user actually reads. Rows past this point stay untagged rather than mis-tagged.
pub const PARETO_MAX_ROWS_CONSIDERED: usize = 200;

/// Objective differences at or below this are treated as ties (0.5 percentage points on a rate).
/// Without it, Monte Carlo jitter alone would push near-identical crews onto the front.
pub const PARETO_EPSILON: f64 = 0.005;

/// How far below the best ranking score a row may sit and still be offered as a "competitive"
/// alternative in the most-different view.
const COMPETITIVE_SCORE_MARGIN: f32 = 0.05;

/// A row can only be called materially different if it shares less than this fraction of its
/// officers with the top crew.
const MOST_DIFFERENT_MAX_SIMILARITY: f64 = 0.99;

/// Why a row is worth looking at, beyond its rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParetoTag {
    /// On the Pareto front: nothing else considered is at least as good everywhere.
    ParetoOptimal,
    /// Lowest loss rate in the considered head.
    Safest,
    /// Highest round-1 kill rate — the fewest real-time seconds per grind cycle.
    FastestFarming,
    /// Highest chain-grind primary success rate (chain runs only).
    BestChain,
    /// Strongest crew that shares the fewest officers with the top row.
    MostDifferent,
}

impl ParetoTag {
    /// Stable wire label. Clients switch on these, so they are part of the API surface.
    pub fn label(self) -> &'static str {
        match self {
            Self::ParetoOptimal => "pareto_optimal",
            Self::Safest => "safest",
            Self::FastestFarming => "fastest_farming",
            Self::BestChain => "best_chain",
            Self::MostDifferent => "most_different",
        }
    }
}

/// Tags and prose for one row. Untagged rows carry no reason.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowHighlights {
    pub tags: Vec<ParetoTag>,
    pub reason: Option<String>,
}

impl RowHighlights {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.reason.is_none()
    }
}

/// Objectives for one row, all oriented so that higher is better.
fn objectives(row: &RankedCrewResult, chain_mode: bool) -> Vec<f64> {
    if chain_mode {
        if let Some(chain) = row.chain.as_ref() {
            return vec![
                chain.primary_success_rate,
                chain.secondary_mean_given_primary,
                row.r1_kill_rate,
                1.0 - row.loss_rate,
            ];
        }
    }
    vec![
        row.win_rate,
        row.avg_hull_remaining,
        row.r1_kill_rate,
        // Less enemy hull left is more damage dealt — real progress even on a loss or stall.
        1.0 - row.avg_defender_hull_remaining,
        1.0 - row.loss_rate,
    ]
}

/// `left` is nowhere clearly worse than `right` (differences inside [`PARETO_EPSILON`] count as
/// ties). Weak: a pair the simulation cannot tell apart satisfies this in both directions.
fn not_clearly_worse_anywhere(left: &[f64], right: &[f64]) -> bool {
    left.iter()
        .zip(right.iter())
        .all(|(l, r)| *l >= *r - PARETO_EPSILON)
}

/// `left` dominates `right` when it is nowhere clearly worse and somewhere clearly better.
fn dominates(left: &[f64], right: &[f64]) -> bool {
    not_clearly_worse_anywhere(left, right)
        && left
            .iter()
            .zip(right.iter())
            .any(|(l, r)| *l > *r + PARETO_EPSILON)
}

/// Index of the best row under `better`, scanning in order so ties keep the stronger-ranked row.
fn best_index<F>(considered: usize, better: F) -> Option<usize>
where
    F: Fn(usize, usize) -> bool,
{
    (0..considered).reduce(|best, i| if better(i, best) { i } else { best })
}

/// Whether a metric varies enough across the considered head to be worth a badge. Tagging one row
/// "safest" when every crew loses at the same rate is noise dressed as advice.
fn metric_has_spread(
    rows: &[RankedCrewResult],
    considered: usize,
    value: fn(&RankedCrewResult) -> f64,
) -> bool {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for row in rows.iter().take(considered) {
        let v = value(row);
        min = min.min(v);
        max = max.max(v);
    }
    max - min > PARETO_EPSILON
}

fn officer_count(row: &RankedCrewResult) -> usize {
    1 + row.bridge.len() + row.below_decks.len()
}

/// Officers this row shares with `other` (captain + bridge + below decks, by name).
fn shared_officer_count(row: &RankedCrewResult, other: &RankedCrewResult) -> usize {
    let names = |r: &RankedCrewResult| -> Vec<String> {
        let mut v = Vec::with_capacity(officer_count(r));
        v.push(r.captain.clone());
        v.extend(r.bridge.iter().cloned());
        v.extend(r.below_decks.iter().cloned());
        v
    };
    let other_names = names(other);
    let mut remaining = other_names.clone();
    let mut shared = 0;
    for name in names(row) {
        if let Some(pos) = remaining.iter().position(|n| *n == name) {
            remaining.remove(pos);
            shared += 1;
        }
    }
    shared
}

fn pct(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

/// Compute front membership and named views for a strength-sorted result list.
///
/// The returned vector is the same length and order as `rows`; rows outside the considered head, or
/// in a run this pass declines to tag, come back empty.
pub fn highlight_rows(rows: &[RankedCrewResult]) -> Vec<RowHighlights> {
    let mut out = vec![RowHighlights::default(); rows.len()];
    if rows.len() < 2 {
        return out;
    }
    // Closed-form rows have no simulated rates to trade off against each other.
    if rows.iter().any(|r| r.expected_hull_damage.is_some()) {
        return out;
    }

    let considered = rows.len().min(PARETO_MAX_ROWS_CONSIDERED);
    let chain_mode = rows.iter().take(considered).all(|r| r.chain.is_some());

    let objective_rows: Vec<Vec<f64>> = rows
        .iter()
        .take(considered)
        .map(|r| objectives(r, chain_mode))
        .collect();

    for i in 0..considered {
        // Dominated by anything: off the front.
        let dominated = (0..considered)
            .filter(|&j| j != i)
            .any(|j| dominates(&objective_rows[j], &objective_rows[i]));
        // Statistically indistinguishable from a better-ranked row: on the front in the strict
        // sense, but it offers the user nothing the row above it does not. Badging a run of
        // near-identical crews turns the column into wallpaper, so ties resolve to the stronger
        // row and the rest stay quiet.
        let duplicate_of_stronger =
            (0..i).any(|j| not_clearly_worse_anywhere(&objective_rows[j], &objective_rows[i]));
        if !dominated && !duplicate_of_stronger {
            out[i].tags.push(ParetoTag::ParetoOptimal);
        }
    }

    if metric_has_spread(rows, considered, |r| r.loss_rate) {
        if let Some(i) = best_index(considered, |a, b| {
            let (ra, rb) = (&rows[a], &rows[b]);
            match rb.loss_rate.total_cmp(&ra.loss_rate) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal => {
                    ra.avg_hull_remaining > rb.avg_hull_remaining
                        || (ra.avg_hull_remaining == rb.avg_hull_remaining
                            && ra.win_rate > rb.win_rate)
                }
            }
        }) {
            out[i].tags.push(ParetoTag::Safest);
        }
    }

    if metric_has_spread(rows, considered, |r| r.r1_kill_rate) {
        if let Some(i) = best_index(considered, |a, b| {
            let (ra, rb) = (&rows[a], &rows[b]);
            match ra.r1_kill_rate.total_cmp(&rb.r1_kill_rate) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal => ra.win_rate > rb.win_rate,
            }
        }) {
            out[i].tags.push(ParetoTag::FastestFarming);
        }
    }

    if chain_mode
        && metric_has_spread(rows, considered, |r| {
            r.chain.as_ref().map_or(0.0, |c| c.primary_success_rate)
        })
    {
        if let Some(i) = best_index(considered, |a, b| {
            let primary =
                |r: &RankedCrewResult| r.chain.as_ref().map_or(0.0, |c| c.primary_success_rate);
            let secondary = |r: &RankedCrewResult| {
                r.chain
                    .as_ref()
                    .map_or(0.0, |c| c.secondary_mean_given_primary)
            };
            let (ra, rb) = (&rows[a], &rows[b]);
            match primary(ra).total_cmp(&primary(rb)) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal => secondary(ra) > secondary(rb),
            }
        }) {
            out[i].tags.push(ParetoTag::BestChain);
        }
    }

    if let Some(i) = most_different_competitive_index(rows, considered) {
        out[i].tags.push(ParetoTag::MostDifferent);
    }

    let max_trials = rows
        .iter()
        .take(considered)
        .map(|r| r.trials_run)
        .max()
        .unwrap_or(0);
    for (i, highlights) in out.iter_mut().enumerate().take(considered) {
        if highlights.tags.is_empty() {
            continue;
        }
        highlights.reason = Some(reason_for(
            rows,
            i,
            &highlights.tags,
            chain_mode,
            max_trials,
        ));
    }

    out
}

/// Strongest crew that is still close to the top score while sharing the fewest officers with it.
fn most_different_competitive_index(rows: &[RankedCrewResult], considered: usize) -> Option<usize> {
    if considered < 2 {
        return None;
    }
    let top = &rows[0];
    // In a matchup nothing wins, every crew scores zero and "a competitive alternative" would just
    // be the least-overlapping row of an undifferentiated pile.
    if top.score.value <= 0.0 {
        return None;
    }
    let cutoff = rows[0].score.value - COMPETITIVE_SCORE_MARGIN;
    let mut best: Option<(usize, f64)> = None;
    for (i, row) in rows.iter().enumerate().take(considered).skip(1) {
        if row.score.value < cutoff {
            continue;
        }
        let similarity = material_jaccard_similarity(row, top);
        if similarity >= MOST_DIFFERENT_MAX_SIMILARITY {
            continue;
        }
        if best.is_none_or(|(_, best_sim)| similarity < best_sim) {
            best = Some((i, similarity));
        }
    }
    best.map(|(i, _)| i)
}

/// One or two short clauses explaining the row's strongest tag, plus an evidence caveat when the
/// row was confirmed less deeply than the deepest row in the table.
fn reason_for(
    rows: &[RankedCrewResult],
    index: usize,
    tags: &[ParetoTag],
    chain_mode: bool,
    max_trials: usize,
) -> String {
    let row = &rows[index];
    let mut clauses: Vec<String> = Vec::new();

    for tag in tags {
        let clause = match tag {
            ParetoTag::Safest => Some(format!(
                "Lowest loss rate at {}, keeping {} hull on average.",
                pct(row.loss_rate),
                pct(row.avg_hull_remaining)
            )),
            ParetoTag::FastestFarming => Some(format!(
                "Fastest grind: kills on round 1 in {} of fights.",
                pct(row.r1_kill_rate)
            )),
            ParetoTag::BestChain => row.chain.as_ref().map(|chain| {
                format!(
                    "Best chain crew: finishes the {}-kill chain {} of the time.",
                    chain.kills_target,
                    pct(chain.primary_success_rate)
                )
            }),
            ParetoTag::MostDifferent => {
                let shared = shared_officer_count(row, &rows[0]);
                Some(format!(
                    "Competitive alternative sharing {shared} of {} officers with the top crew.",
                    officer_count(row)
                ))
            }
            ParetoTag::ParetoOptimal => None,
        };
        if let Some(clause) = clause {
            clauses.push(clause);
        }
        if clauses.len() == 2 {
            break;
        }
    }

    if clauses.is_empty() {
        clauses.push(if chain_mode {
            format!(
                "No other crew matches it on chain success, speed, and risk at once ({} chain success).",
                row.chain
                    .as_ref()
                    .map(|c| pct(c.primary_success_rate))
                    .unwrap_or_else(|| pct(row.win_rate))
            )
        } else {
            format!(
                "Offers something no better-ranked crew does on win rate, hull left, speed, and risk ({} win, {} hull).",
                pct(row.win_rate),
                pct(row.avg_hull_remaining)
            )
        });
    }

    if row.trials_run > 0 && row.trials_run < max_trials {
        clauses.push(format!(
            "Backed by {} of the {max_trials} trials the deepest row got.",
            row.trials_run
        ));
    }

    clauses.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimizer::chain::{ChainSecondaryObjective, ChainSimulationSummary};
    use crate::optimizer::ranking::RankingScore;

    fn row(captain: &str, win: f64, hull: f64, r1: f64, loss: f64) -> RankedCrewResult {
        RankedCrewResult {
            captain: captain.to_string(),
            bridge: vec!["B1".into(), "B2".into()],
            below_decks: vec!["D1".into(), "D2".into(), "D3".into()],
            trials_run: 100,
            win_rate: win,
            win_rate_ci_low: win,
            win_rate_ci_high: win,
            stall_rate: 0.0,
            stall_rate_ci_low: 0.0,
            stall_rate_ci_high: 0.0,
            loss_rate: loss,
            loss_rate_ci_low: loss,
            loss_rate_ci_high: loss,
            r1_kill_rate: r1,
            r1_kill_rate_ci_low: r1,
            r1_kill_rate_ci_high: r1,
            avg_hull_remaining: hull,
            avg_hull_remaining_ci_low: hull,
            avg_hull_remaining_ci_high: hull,
            avg_defender_hull_remaining: 0.0,
            avg_defender_hull_remaining_ci_low: 0.0,
            avg_defender_hull_remaining_ci_high: 0.0,
            score: RankingScore {
                value: (win * 0.8 + hull * 0.2) as f32,
            },
            chain: None,
            expected_hull_damage: None,
        }
    }

    fn tags_of(highlights: &RowHighlights) -> Vec<&'static str> {
        highlights.tags.iter().map(|t| t.label()).collect()
    }

    #[test]
    fn dominated_row_is_not_on_the_front() {
        // B is worse than A on every objective by more than epsilon.
        let a = row("A", 0.90, 0.70, 0.40, 0.10);
        let b = row("B", 0.60, 0.20, 0.05, 0.40);
        let out = highlight_rows(&[a, b]);
        assert!(out[0].tags.contains(&ParetoTag::ParetoOptimal));
        assert!(!out[1].tags.contains(&ParetoTag::ParetoOptimal));
    }

    #[test]
    fn trade_off_rows_are_both_on_the_front() {
        // A wins more; B survives better and kills faster. Neither dominates.
        let a = row("A", 0.90, 0.30, 0.10, 0.10);
        let b = row("B", 0.80, 0.80, 0.50, 0.20);
        let out = highlight_rows(&[a, b]);
        assert!(out[0].tags.contains(&ParetoTag::ParetoOptimal));
        assert!(out[1].tags.contains(&ParetoTag::ParetoOptimal));
    }

    /// Rows the simulation cannot tell apart are not each a separate recommendation: the badge goes
    /// to the better-ranked one so a run of near-identical crews does not fill the column.
    #[test]
    fn near_identical_rows_badge_only_the_stronger_one() {
        // Every objective differs by less than PARETO_EPSILON.
        let a = row("A", 0.9000, 0.5000, 0.2000, 0.1000);
        let b = row("B", 0.9001, 0.5001, 0.2001, 0.0999);
        let out = highlight_rows(&[a, b]);
        assert!(out[0].tags.contains(&ParetoTag::ParetoOptimal));
        assert!(
            !out[1].tags.contains(&ParetoTag::ParetoOptimal),
            "a statistical twin of the row above it adds nothing"
        );
    }

    #[test]
    fn safest_is_the_lowest_loss_rate_row_not_the_top_row() {
        let top = row("Top", 0.90, 0.30, 0.50, 0.10);
        let safe = row("Safe", 0.85, 0.75, 0.05, 0.01);
        let out = highlight_rows(&[top, safe]);
        assert!(!out[0].tags.contains(&ParetoTag::Safest));
        assert!(out[1].tags.contains(&ParetoTag::Safest));
        assert!(out[1]
            .reason
            .as_deref()
            .unwrap()
            .contains("Lowest loss rate"));
    }

    #[test]
    fn fastest_farming_is_the_highest_r1_kill_row() {
        let a = row("A", 0.90, 0.60, 0.10, 0.10);
        let b = row("B", 0.88, 0.55, 0.65, 0.12);
        let out = highlight_rows(&[a, b]);
        assert_eq!(
            out[1]
                .tags
                .iter()
                .filter(|t| **t == ParetoTag::FastestFarming)
                .count(),
            1
        );
        assert!(out[1].reason.as_deref().unwrap().contains("round 1"));
    }

    #[test]
    fn flat_metrics_get_no_named_badge() {
        // Identical loss rate and r1 rate across rows: those views carry no information.
        let a = row("A", 0.90, 0.60, 0.20, 0.10);
        let b = row("B", 0.70, 0.40, 0.20, 0.10);
        let out = highlight_rows(&[a, b]);
        for h in &out {
            assert!(
                !h.tags.contains(&ParetoTag::Safest),
                "tags={:?}",
                tags_of(h)
            );
            assert!(!h.tags.contains(&ParetoTag::FastestFarming));
        }
    }

    #[test]
    fn most_different_prefers_a_competitive_crew_with_fewer_shared_officers() {
        let mut top = row("Top", 0.90, 0.60, 0.20, 0.10);
        top.score.value = 0.90;
        let mut near_dup = row("Top", 0.89, 0.59, 0.20, 0.10);
        near_dup.below_decks = vec!["D1".into(), "D2".into(), "D9".into()];
        near_dup.score.value = 0.89;
        let mut different = row("Other", 0.88, 0.58, 0.20, 0.10);
        different.bridge = vec!["X1".into(), "X2".into()];
        different.below_decks = vec!["Y1".into(), "Y2".into(), "Y3".into()];
        different.score.value = 0.88;
        let out = highlight_rows(&[top, near_dup, different]);
        assert!(!out[1].tags.contains(&ParetoTag::MostDifferent));
        assert!(out[2].tags.contains(&ParetoTag::MostDifferent));
        assert!(out[2]
            .reason
            .as_deref()
            .unwrap()
            .contains("sharing 0 of 6 officers"));
    }

    #[test]
    fn most_different_ignores_rows_that_fell_off_the_score_pace() {
        let mut top = row("Top", 0.90, 0.60, 0.20, 0.10);
        top.score.value = 0.90;
        let mut far_behind = row("Weak", 0.10, 0.10, 0.20, 0.10);
        far_behind.bridge = vec!["X1".into(), "X2".into()];
        far_behind.below_decks = vec!["Y1".into(), "Y2".into(), "Y3".into()];
        far_behind.score.value = 0.10;
        let out = highlight_rows(&[top, far_behind]);
        assert!(!out[1].tags.contains(&ParetoTag::MostDifferent));
    }

    #[test]
    fn a_hopeless_matchup_offers_no_competitive_alternative() {
        let mut top = row("Top", 0.0, 0.0, 0.0, 1.0);
        top.score.value = 0.0;
        let mut other = row("Other", 0.0, 0.0, 0.0, 1.0);
        other.bridge = vec!["X1".into(), "X2".into()];
        other.below_decks = vec!["Y1".into(), "Y2".into(), "Y3".into()];
        other.score.value = 0.0;
        let out = highlight_rows(&[top, other]);
        assert!(out
            .iter()
            .all(|h| !h.tags.contains(&ParetoTag::MostDifferent)));
    }

    #[test]
    fn linear_eval_rows_are_left_untagged() {
        let mut a = row("A", 0.0, 0.0, 0.0, 0.0);
        a.expected_hull_damage = Some(9_000.0);
        let mut b = row("B", 0.0, 0.0, 0.0, 0.0);
        b.expected_hull_damage = Some(4_000.0);
        let out = highlight_rows(&[a, b]);
        assert!(out.iter().all(RowHighlights::is_empty));
    }

    #[test]
    fn rows_past_the_considered_head_are_untagged() {
        let rows: Vec<RankedCrewResult> = (0..(PARETO_MAX_ROWS_CONSIDERED + 5))
            .map(|i| {
                let win = 0.9 - (i as f64) * 0.001;
                row(&format!("C{i}"), win, 0.5, 0.2, 0.1)
            })
            .collect();
        let out = highlight_rows(&rows);
        assert_eq!(out.len(), rows.len());
        assert!(out
            .iter()
            .skip(PARETO_MAX_ROWS_CONSIDERED)
            .all(RowHighlights::is_empty));
    }

    #[test]
    fn shallow_rows_say_how_deeply_they_were_confirmed() {
        let mut deep = row("Deep", 0.90, 0.30, 0.10, 0.20);
        deep.trials_run = 2_000;
        let mut shallow = row("Shallow", 0.80, 0.80, 0.50, 0.05);
        shallow.trials_run = 40;
        let out = highlight_rows(&[deep, shallow]);
        assert!(!out[0].reason.as_deref().unwrap().is_empty());
        assert!(!out[0].reason.as_deref().unwrap().contains("Backed by"));
        assert!(out[1]
            .reason
            .as_deref()
            .unwrap()
            .contains("Backed by 40 of the 2000 trials"));
    }

    fn chain_summary(primary: f64, secondary: f64) -> ChainSimulationSummary {
        ChainSimulationSummary {
            kills_target: 5,
            secondary_objective: ChainSecondaryObjective::MinHullDamage,
            primary_success_rate: primary,
            primary_ci_low: primary,
            primary_ci_high: primary,
            secondary_mean_given_primary: secondary,
            secondary_ci_low: secondary,
            secondary_ci_high: secondary,
            n_primary_successes: 10,
        }
    }

    #[test]
    fn chain_runs_rank_the_best_chain_crew_by_chain_success() {
        let mut a = row("A", 0.95, 0.30, 0.20, 0.05);
        a.chain = Some(chain_summary(0.40, 0.30));
        let mut b = row("B", 0.94, 0.60, 0.20, 0.06);
        b.chain = Some(chain_summary(0.85, 0.55));
        let out = highlight_rows(&[a, b]);
        assert!(!out[0].tags.contains(&ParetoTag::BestChain));
        assert!(out[1].tags.contains(&ParetoTag::BestChain));
        assert!(out[1]
            .reason
            .as_deref()
            .unwrap()
            .contains("finishes the 5-kill chain 85.0%"));
    }

    #[test]
    fn single_row_and_empty_input_are_untagged() {
        assert!(highlight_rows(&[]).is_empty());
        let out = highlight_rows(&[row("Solo", 0.9, 0.5, 0.2, 0.1)]);
        assert_eq!(out.len(), 1);
        assert!(out[0].is_empty());
    }
}
