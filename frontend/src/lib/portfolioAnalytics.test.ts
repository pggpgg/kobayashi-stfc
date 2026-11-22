import { describe, expect, it } from "vitest";
import type { CrewRecommendation } from "./api";
import {
  diffCrewSeats,
  EMPTY_RECOMMENDATION_FILTERS,
  filterRecommendations,
  parseQdCell,
  summarizePortfolioResults,
} from "./portfolioAnalytics";

function makeRec(
  overrides: Partial<CrewRecommendation> & { captain: string },
): CrewRecommendation {
  return {
    bridge: ["Spock", "McCoy"],
    below_decks: ["Uhura", "Scotty"],
    method_provenance: "beam_diversity",
    trials_run: 5000,
    win_rate: 0.9,
    win_rate_ci_low: 0.88,
    win_rate_ci_high: 0.92,
    stall_rate: 0.05,
    stall_rate_ci_low: 0.04,
    stall_rate_ci_high: 0.06,
    loss_rate: 0.05,
    loss_rate_ci_low: 0.04,
    loss_rate_ci_high: 0.06,
    r1_kill_rate: 0.2,
    r1_kill_rate_ci_low: 0.18,
    r1_kill_rate_ci_high: 0.22,
    avg_hull_remaining: 0.7,
    avg_hull_remaining_ci_low: 0.68,
    avg_hull_remaining_ci_high: 0.72,
    avg_defender_hull_remaining: 0.1,
    avg_defender_hull_remaining_ci_low: 0.09,
    avg_defender_hull_remaining_ci_high: 0.11,
    ...overrides,
  };
}

describe("portfolioAnalytics", () => {
  it("parses QD archive cell coordinates into readable archetype labels", () => {
    const parsed = parseQdCell("tos|fast|high_hull");
    expect(parsed).toEqual({
      raw: "tos|fast|high_hull",
      captainGroup: "tos",
      captainGroupLabel: "TOS",
      speedBucket: "fast",
      speedLabel: "Fast",
      survivalBucket: "high_hull",
      survivalLabel: "High hull",
    });
    expect(parseQdCell(null)).toBeNull();
    expect(parseQdCell("  ")).toBeNull();
  });

  it("summarizes archive cells, discovery lanes, fidelity breakdown, and diverse picks", () => {
    const recs = [
      makeRec({
        captain: "Kirk",
        method_provenance: "beam_diversity",
        proposing_methods: ["beam_diversity", "qd_archive"],
        qd_cell: "tos|fast|high_hull",
        pareto_tags: ["pareto_optimal", "fastest_farming"],
        fidelity: {
          implemented: 5,
          partial: 0,
          ignored: 0,
          total_combat_effects: 5,
          coverage_pct: 100,
          grade: "A",
        },
      }),
      makeRec({
        captain: "TOS Kirk",
        method_provenance: "portfolio_neighbor",
        proposing_methods: ["portfolio_neighbor"],
        qd_cell: "tos|fast|high_hull",
        fidelity: {
          implemented: 4,
          partial: 1,
          ignored: 0,
          total_combat_effects: 5,
          coverage_pct: 90,
          grade: "A",
        },
      }),
      makeRec({
        captain: "Pike",
        method_provenance: "random_stratified",
        proposing_methods: ["random_stratified", "qd_archive"],
        qd_cell: "enterprise_crew|medium|high_hull",
        pareto_tags: ["safest"],
        uncertainty_reason: "1 active combat effect(s) ignored or unmodeled",
        fidelity: {
          implemented: 3,
          partial: 0,
          ignored: 1,
          total_combat_effects: 4,
          coverage_pct: 75,
          grade: "C",
          caveats: ["Pike: 1 ignored effect"],
        },
      }),
    ];

    const summary = summarizePortfolioResults(recs);
    expect(summary.hasAnalytics).toBe(true);
    expect(summary.distinctCaptainGroups).toBe(2);
    expect(summary.archiveCells).toHaveLength(2);
    expect(summary.archiveCells[0]).toMatchObject({
      count: 2,
      bestIndex: 0,
    });
    expect(summary.archiveCells[1]).toMatchObject({
      count: 1,
      bestIndex: 2,
    });
    expect(summary.fidelityCounts).toEqual({ A: 2, B: 0, C: 1, D: 0 });
    expect(summary.caveatRowCount).toBe(1);
    expect(summary.paretoRowCount).toBe(2);
    // Diverse picks skip index 1 (same "tos" captain group as index 0) and pick 0 and 2.
    expect(summary.topDiverseIndices).toEqual([0, 2]);
    expect(summary.laneCounts.find((l) => l.lane === "qd_archive")?.count).toBe(
      2,
    );
  });

  it("filters rows while preserving original indices", () => {
    const recs = [
      makeRec({
        captain: "Kirk",
        proposing_methods: ["beam_diversity"],
        qd_cell: "tos|fast|high_hull",
        fidelity: {
          implemented: 5,
          partial: 0,
          ignored: 0,
          total_combat_effects: 5,
          coverage_pct: 100,
          grade: "A",
        },
      }),
      makeRec({
        captain: "Pike",
        proposing_methods: ["random_stratified", "qd_archive"],
        qd_cell: "enterprise|medium|high_hull",
        pareto_tags: ["safest"],
        uncertainty_reason: "Wide CI",
        fidelity: {
          implemented: 2,
          partial: 1,
          ignored: 1,
          total_combat_effects: 4,
          coverage_pct: 62.5,
          grade: "C",
        },
      }),
    ];

    const byCell = filterRecommendations(recs, {
      ...EMPTY_RECOMMENDATION_FILTERS,
      qdCell: "enterprise|medium|high_hull",
    });
    expect(byCell.map((r) => r.originalIndex)).toEqual([1]);

    const byCaveat = filterRecommendations(recs, {
      ...EMPTY_RECOMMENDATION_FILTERS,
      caveatsOnly: true,
    });
    expect(byCaveat.map((r) => r.originalIndex)).toEqual([1]);

    const byGrade = filterRecommendations(recs, {
      ...EMPTY_RECOMMENDATION_FILTERS,
      fidelityGrade: "A",
    });
    expect(byGrade.map((r) => r.originalIndex)).toEqual([0]);
  });

  it("computes seat-by-seat officer diffs between compared crews", () => {
    const base = makeRec({
      captain: "Kirk",
      bridge: ["Spock", "McCoy"],
      below_decks: ["Uhura", "Scotty", "Sulu"],
    });
    const alt = makeRec({
      captain: "Picard",
      bridge: ["Spock", "Data"],
      below_decks: ["Uhura", "Worf", "Sulu"],
    });
    const diff = diffCrewSeats(base, alt);
    expect(diff.captainChanged).toBe(true);
    expect(diff.bridgeShared).toEqual(["Spock"]);
    expect(diff.bridgeAdded).toEqual(["Data"]);
    expect(diff.bridgeRemoved).toEqual(["McCoy"]);
    expect(diff.belowShared).toEqual(["Uhura", "Sulu"]);
    expect(diff.belowAdded).toEqual(["Worf"]);
    expect(diff.belowRemoved).toEqual(["Scotty"]);
    expect(diff.totalSwappedOfficers).toBe(3);
  });
});
