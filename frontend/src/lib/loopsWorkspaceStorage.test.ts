import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CrewRecommendation } from "./api";
import type { LoopGoalId } from "./loopsCatalog";
import {
  getLoopBestRecord,
  isGenuineImprovement,
  type LoopBestRecord,
  type LoopRunContext,
  primaryMetricForGoal,
  saveLoopOptimizationResult,
} from "./loopsWorkspaceStorage";

const context: LoopRunContext = {
  loopId: "actian",
  loopName: "Actian",
  targetId: "apex-49",
  targetName: "Actian Apex",
  targetLevel: 49,
  goalId: "one_round",
  shipId: "mantis",
  shipPolicy: "recommended",
  specialtyShipIds: ["mantis"],
};

function recommendation(
  overrides: Partial<CrewRecommendation>,
): CrewRecommendation {
  return {
    captain: "Five of Eleven",
    bridge: ["Khan", "Georgiou"],
    below_decks: ["The Doctor"],
    win_rate: 0.9,
    win_rate_ci_low: 0.88,
    win_rate_ci_high: 0.92,
    stall_rate: 0,
    stall_rate_ci_low: 0,
    stall_rate_ci_high: 0,
    loss_rate: 0.1,
    loss_rate_ci_low: 0.08,
    loss_rate_ci_high: 0.12,
    r1_kill_rate: 0.4,
    r1_kill_rate_ci_low: 0.38,
    r1_kill_rate_ci_high: 0.42,
    avg_hull_remaining: 0.75,
    avg_hull_remaining_ci_low: 0.72,
    avg_hull_remaining_ci_high: 0.78,
    avg_defender_hull_remaining: 0.02,
    avg_defender_hull_remaining_ci_low: 0.01,
    avg_defender_hull_remaining_ci_high: 0.03,
    ...overrides,
  };
}

describe("loops workspace persistence", () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
  });

  it("keeps an earlier best when a later run is weaker for the selected goal", () => {
    const first = saveLoopOptimizationResult({
      profileId: "p1",
      context,
      shipId: "mantis",
      shipTier: 5,
      shipLevel: 25,
      recommendations: [recommendation({ r1_kill_rate: 0.7 })],
    });
    const weaker = saveLoopOptimizationResult({
      profileId: "p1",
      context,
      shipId: "mantis",
      shipTier: 6,
      shipLevel: 30,
      recommendations: [recommendation({ r1_kill_rate: 0.6 })],
    });

    expect(first.saved).toBe(true);
    expect(weaker.saved).toBe(false);
    expect(
      getLoopBestRecord("p1", "actian", "apex-49", "one_round")
        ?.roundOneKillRate,
    ).toBe(0.7);
  });

  it("isolates saved ladders by player profile", () => {
    saveLoopOptimizationResult({
      profileId: "p1",
      context,
      shipId: "mantis",
      shipTier: 5,
      shipLevel: 25,
      recommendations: [recommendation({})],
    });

    expect(
      getLoopBestRecord("p1", "actian", "apex-49", "one_round"),
    ).not.toBeNull();
    expect(
      getLoopBestRecord("p2", "actian", "apex-49", "one_round"),
    ).toBeNull();
  });

  it("does not record a best set on a ship the loop requires you not to use", () => {
    const outcome = saveLoopOptimizationResult({
      profileId: "p1",
      context: { ...context, shipPolicy: "required" },
      shipId: "uss_enterprise",
      shipTier: 9,
      shipLevel: 45,
      recommendations: [recommendation({ r1_kill_rate: 0.99 })],
    });

    expect(outcome.saved).toBe(false);
    expect(outcome.reason).toBe("off_policy_ship");
    expect(
      getLoopBestRecord("p1", "actian", "apex-49", "one_round"),
    ).toBeNull();
  });

  it("reports a persistence failure as an improvement that could not be stored", () => {
    saveLoopOptimizationResult({
      profileId: "p1",
      context,
      shipId: "mantis",
      shipTier: 5,
      shipLevel: 25,
      recommendations: [recommendation({ r1_kill_rate: 0.5 })],
    });
    const spy = vi
      .spyOn(Storage.prototype, "setItem")
      .mockImplementation(() => {
        throw new Error("QuotaExceededError");
      });
    try {
      const outcome = saveLoopOptimizationResult({
        profileId: "p1",
        context,
        shipId: "mantis",
        shipTier: 5,
        shipLevel: 25,
        recommendations: [recommendation({ r1_kill_rate: 0.95 })],
      });
      expect(outcome.saved).toBe(false);
      // The distinction that matters: it *was* better, storage just refused it.
      expect(outcome.improved).toBe(true);
      expect(outcome.reason).toBe("storage_failed");
    } finally {
      spy.mockRestore();
    }
  });
});

function record(overrides: Partial<LoopBestRecord> = {}): LoopBestRecord {
  return {
    id: "r1",
    loopId: "actian",
    targetId: "apex-49",
    targetName: "Actian Apex",
    targetLevel: 49,
    goalId: "one_round",
    shipId: "mantis",
    shipTier: 5,
    shipLevel: 25,
    crew: { captain: "Khan", bridge: ["A", "B"], belowDecks: ["C"] },
    winRate: 0.9,
    winRateCiLow: 0.88,
    winRateCiHigh: 0.92,
    roundOneKillRate: 0.5,
    roundOneKillRateCiLow: 0.45,
    roundOneKillRateCiHigh: 0.55,
    averageHullRemaining: 0.7,
    averageHullRemainingCiLow: 0.68,
    averageHullRemainingCiHigh: 0.72,
    averageDefenderHullRemaining: 0.1,
    averageDefenderHullRemainingCiLow: 0.08,
    averageDefenderHullRemainingCiHigh: 0.12,
    recordedAt: "2026-07-24T00:00:00.000Z",
    ...overrides,
  };
}

describe("isGenuineImprovement", () => {
  it("accepts the first record for a rung", () => {
    expect(isGenuineImprovement(record(), null)).toBe(true);
  });

  it("accepts a candidate whose interval sits entirely above the incumbent's", () => {
    const incumbent = record({
      roundOneKillRate: 0.5,
      roundOneKillRateCiLow: 0.45,
      roundOneKillRateCiHigh: 0.55,
    });
    const candidate = record({
      id: "r2",
      roundOneKillRate: 0.8,
      roundOneKillRateCiLow: 0.75,
      roundOneKillRateCiHigh: 0.85,
    });
    expect(isGenuineImprovement(candidate, incumbent)).toBe(true);
  });

  it("rejects a candidate whose interval sits entirely below the incumbent's", () => {
    const incumbent = record({
      roundOneKillRate: 0.8,
      roundOneKillRateCiLow: 0.75,
      roundOneKillRateCiHigh: 0.85,
    });
    const candidate = record({
      id: "r2",
      roundOneKillRate: 0.5,
      roundOneKillRateCiLow: 0.45,
      roundOneKillRateCiHigh: 0.55,
    });
    expect(isGenuineImprovement(candidate, incumbent)).toBe(false);
  });

  it("accepts a better point estimate on overlapping intervals at comparable sampling", () => {
    const incumbent = record({ simsPerCrew: 5000 });
    const candidate = record({
      id: "r2",
      roundOneKillRate: 0.54,
      simsPerCrew: 5000,
    });
    expect(isGenuineImprovement(candidate, incumbent)).toBe(true);
  });

  it("refuses a thinly-sampled challenger that only wins on a noisy point estimate", () => {
    const incumbent = record({ simsPerCrew: 20000 });
    const candidate = record({
      id: "r2",
      roundOneKillRate: 0.54,
      simsPerCrew: 200,
    });
    expect(isGenuineImprovement(candidate, incumbent)).toBe(false);
  });

  it("never accepts an equal or lower point estimate when intervals overlap", () => {
    const incumbent = record({ simsPerCrew: 5000 });
    expect(
      isGenuineImprovement(record({ id: "r2", simsPerCrew: 5000 }), incumbent),
    ).toBe(false);
  });

  it("lets a rigorous run overturn a lucky low-sim best", () => {
    // The regression this gate exists for: a 200-sim fluke previously locked the
    // rung permanently, because a raw point-estimate comparison had no way to know
    // the new run was better sampled.
    const lucky = record({
      roundOneKillRate: 0.62,
      roundOneKillRateCiLow: 0.4,
      roundOneKillRateCiHigh: 0.84,
      simsPerCrew: 200,
    });
    const rigorous = record({
      id: "r2",
      roundOneKillRate: 0.86,
      roundOneKillRateCiLow: 0.85,
      roundOneKillRateCiHigh: 0.87,
      simsPerCrew: 20000,
    });
    expect(isGenuineImprovement(rigorous, lucky)).toBe(true);
  });
});

describe("primaryMetricForGoal", () => {
  it.each<[LoopGoalId, number]>([
    ["one_round", 0.5],
    ["no_hits", 0.9],
    ["smallest_ship", 0.9],
    ["damage_dealt", 0.9],
  ])("uses the goal's own metric for %s", (goalId, expected) => {
    expect(primaryMetricForGoal(goalId, record({ goalId })).value).toBeCloseTo(
      expected,
      6,
    );
  });

  it("inverts defender hull for damage dealt, swapping interval bounds", () => {
    const metric = primaryMetricForGoal(
      "damage_dealt",
      record({ goalId: "damage_dealt" }),
    );
    expect(metric.value).toBeCloseTo(0.9, 6);
    expect(metric.ciLow).toBeCloseTo(0.88, 6);
    expect(metric.ciHigh).toBeCloseTo(0.92, 6);
  });

  it("prefers chain success for the grind goal and falls back to win rate", () => {
    const withChain = record({
      goalId: "kills_per_hull",
      chainSuccessRate: 0.42,
      chainSuccessRateCiLow: 0.4,
      chainSuccessRateCiHigh: 0.44,
    });
    expect(primaryMetricForGoal("kills_per_hull", withChain).value).toBeCloseTo(
      0.42,
      6,
    );
    expect(
      primaryMetricForGoal(
        "kills_per_hull",
        record({ goalId: "kills_per_hull" }),
      ).value,
    ).toBeCloseTo(0.9, 6);
  });
});

describe("schema migration", () => {
  const KEY = "kobayashi_loops_workspace_v1:p1";

  function writeV1(): void {
    localStorage.setItem(
      KEY,
      JSON.stringify({
        version: 1,
        records: {
          "actian|apex-49|one_round": [
            {
              id: "old",
              loopId: "actian",
              targetId: "apex-49",
              targetName: "Actian Apex",
              targetLevel: 49,
              goalId: "one_round",
              shipId: "mantis",
              shipTier: 4,
              shipLevel: 20,
              crew: { captain: "Khan", bridge: ["A", "B"], belowDecks: ["C"] },
              winRate: 0.8,
              roundOneKillRate: 0.6,
              averageHullRemaining: 0.5,
              averageDefenderHullRemaining: 0.2,
              recordedAt: "2026-07-01T00:00:00.000Z",
            },
          ],
        },
      }),
    );
  }

  beforeEach(() => {
    localStorage.clear();
  });

  it("keeps schema-1 bests instead of wiping them", () => {
    writeV1();
    const best = getLoopBestRecord("p1", "actian", "apex-49", "one_round");
    expect(best?.id).toBe("old");
    expect(best?.roundOneKillRate).toBe(0.6);
  });

  it("gives migrated records zero-width intervals and unknown sampling", () => {
    writeV1();
    const best = getLoopBestRecord("p1", "actian", "apex-49", "one_round");
    expect(best?.roundOneKillRateCiLow).toBe(0.6);
    expect(best?.roundOneKillRateCiHigh).toBe(0.6);
    // Unknown sampling must not earn the trust veto.
    expect(best?.simsPerCrew).toBeUndefined();
  });

  it("rewrites storage once so later reads need no migration", () => {
    writeV1();
    getLoopBestRecord("p1", "actian", "apex-49", "one_round");
    expect(JSON.parse(localStorage.getItem(KEY) ?? "{}").version).toBe(2);
  });

  it("falls back to empty for an unrecognised schema rather than guessing", () => {
    localStorage.setItem(KEY, JSON.stringify({ version: 99, records: {} }));
    expect(
      getLoopBestRecord("p1", "actian", "apex-49", "one_round"),
    ).toBeNull();
  });
});
