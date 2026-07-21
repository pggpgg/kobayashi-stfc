import { beforeEach, describe, expect, it } from "vitest";
import type { CrewRecommendation } from "./api";
import {
  getLoopBestRecord,
  type LoopRunContext,
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
});
