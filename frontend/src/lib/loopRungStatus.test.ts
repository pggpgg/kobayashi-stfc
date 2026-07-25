import { describe, expect, it } from "vitest";
import {
  loopFrontier,
  RUNG_CLEAR_CI_LOW_THRESHOLD,
  rungStatus,
} from "./loopRungStatus";
import type { LoopBestRecord } from "./loopsWorkspaceStorage";

function record(overrides: Partial<LoopBestRecord> = {}): LoopBestRecord {
  return {
    id: "r1",
    loopId: "actian",
    targetId: "t1",
    targetName: "Actian Apex",
    targetLevel: 40,
    goalId: "no_hits",
    shipId: "mantis",
    shipTier: 5,
    shipLevel: 25,
    crew: { captain: "Khan", bridge: ["A", "B"], belowDecks: ["C"] },
    winRate: 0.95,
    winRateCiLow: 0.93,
    winRateCiHigh: 0.97,
    roundOneKillRate: 0.8,
    roundOneKillRateCiLow: 0.78,
    roundOneKillRateCiHigh: 0.82,
    averageHullRemaining: 0.9,
    averageHullRemainingCiLow: 0.88,
    averageHullRemainingCiHigh: 0.92,
    averageDefenderHullRemaining: 0.0,
    averageDefenderHullRemainingCiLow: 0.0,
    averageDefenderHullRemainingCiHigh: 0.0,
    recordedAt: "2026-07-24T00:00:00.000Z",
    ...overrides,
  };
}

describe("rungStatus", () => {
  it("reports an unmeasured but reachable rung as untried", () => {
    const info = rungStatus("no_hits", null, true);
    expect(info.status).toBe("untried");
    expect(info.metric).toBeNull();
  });

  it("reports an unmeasured unreachable rung as locked", () => {
    expect(rungStatus("no_hits", null, false).status).toBe("locked");
  });

  it("clears a rung whose interval floor beats the goal threshold", () => {
    expect(rungStatus("no_hits", record(), true).status).toBe("cleared");
  });

  it("judges the interval floor, not the point estimate", () => {
    // Point estimate is well above the 0.7 bar, but the interval reaches below it,
    // so the crew has not shown it can farm the rung repeatably.
    const wide = record({
      winRate: 0.85,
      winRateCiLow: 0.55,
      winRateCiHigh: 0.99,
    });
    expect(rungStatus("no_hits", wide, true).status).toBe("contested");
  });

  it("uses each goal's own metric and threshold", () => {
    const r1Only = record({
      goalId: "one_round",
      roundOneKillRate: 0.7,
      roundOneKillRateCiLow: 0.65,
      roundOneKillRateCiHigh: 0.75,
      winRate: 0.1,
      winRateCiLow: 0.05,
      winRateCiHigh: 0.15,
    });
    // Cleared on round-one kills (0.65 >= 0.6) despite a poor win rate.
    expect(rungStatus("one_round", r1Only, true).status).toBe("cleared");
    // The same record judged as no_hits fails, since that reads win rate vs 0.7.
    expect(rungStatus("no_hits", r1Only, true).status).toBe("contested");
  });

  it("holds smallest_ship to a viability bar rather than a comfort bar", () => {
    expect(RUNG_CLEAR_CI_LOW_THRESHOLD.smallest_ship).toBeLessThan(
      RUNG_CLEAR_CI_LOW_THRESHOLD.no_hits,
    );
    const marginal = record({
      goalId: "smallest_ship",
      winRate: 0.6,
      winRateCiLow: 0.55,
      winRateCiHigh: 0.65,
    });
    expect(rungStatus("smallest_ship", marginal, true).status).toBe("cleared");
    expect(rungStatus("no_hits", marginal, true).status).toBe("contested");
  });
});

describe("loopFrontier", () => {
  const cleared = (id: string) => record({ id, targetId: id });
  const contested = (id: string) =>
    record({
      id,
      targetId: id,
      winRate: 0.5,
      winRateCiLow: 0.4,
      winRateCiHigh: 0.6,
    });

  it("points at the first rung when nothing is measured", () => {
    const frontier = loopFrontier(["a", "b", "c"], new Map(), "no_hits");
    expect(frontier.frontierIndex).toBe(-1);
    expect(frontier.nextTargetId).toBe("a");
    expect(frontier.statuses.get("a")?.status).toBe("untried");
    // Only the first rung is reachable until it is cleared.
    expect(frontier.statuses.get("b")?.status).toBe("locked");
  });

  it("advances the frontier through consecutively cleared rungs", () => {
    const frontier = loopFrontier(
      ["a", "b", "c"],
      new Map([
        ["a", cleared("a")],
        ["b", cleared("b")],
      ]),
      "no_hits",
    );
    expect(frontier.frontierIndex).toBe(1);
    expect(frontier.nextTargetId).toBe("c");
    expect(frontier.statuses.get("c")?.status).toBe("untried");
  });

  it("stops the frontier at a contested rung and locks everything above it", () => {
    const frontier = loopFrontier(
      ["a", "b", "c"],
      new Map([
        ["a", cleared("a")],
        ["b", contested("b")],
      ]),
      "no_hits",
    );
    expect(frontier.frontierIndex).toBe(0);
    expect(frontier.nextTargetId).toBe("b");
    expect(frontier.statuses.get("b")?.status).toBe("contested");
    expect(frontier.statuses.get("c")?.status).toBe("locked");
  });

  it("does not let a cleared rung above a gap overstate progress", () => {
    const frontier = loopFrontier(
      ["a", "b", "c"],
      new Map([
        ["a", contested("a")],
        ["c", cleared("c")],
      ]),
      "no_hits",
    );
    // c is cleared, but the frontier is still below a — the run is broken.
    expect(frontier.frontierIndex).toBe(-1);
    expect(frontier.nextTargetId).toBe("a");
    expect(frontier.clearedCount).toBe(1);
  });

  it("reports a fully cleared ladder with no next target", () => {
    const frontier = loopFrontier(
      ["a", "b"],
      new Map([
        ["a", cleared("a")],
        ["b", cleared("b")],
      ]),
      "no_hits",
    );
    expect(frontier.frontierIndex).toBe(1);
    expect(frontier.nextTargetId).toBeNull();
    expect(frontier.clearedCount).toBe(2);
  });
});
