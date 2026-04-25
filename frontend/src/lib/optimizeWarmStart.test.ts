import { describe, expect, it } from "vitest";
import { buildOptimizeWarmStartKey } from "./optimizeWarmStart";

const baseArgs = {
  profileId: "p1",
  shipId: "saladin",
  scenarioId: "2918121098",
  shipTier: 5,
  shipLevel: 40,
  maxCandidates: 100 as number | null,
  constraintsFingerprint: "a\u001fb",
};

describe("buildOptimizeWarmStartKey", () => {
  it("changes when support buff selection changes", () => {
    const a = buildOptimizeWarmStartKey({
      ...baseArgs,
      supportBuffIds: ["cerritos_support"],
    });
    const b = buildOptimizeWarmStartKey({
      ...baseArgs,
      supportBuffIds: ["cerritos_support", "defiant_reinforce"],
    });
    expect(a).not.toBe(b);
  });

  it("is stable for same support buff ids in different order", () => {
    const a = buildOptimizeWarmStartKey({
      ...baseArgs,
      supportBuffIds: ["defiant_reinforce", "cerritos_support"],
    });
    const b = buildOptimizeWarmStartKey({
      ...baseArgs,
      supportBuffIds: ["cerritos_support", "defiant_reinforce"],
    });
    expect(a).toBe(b);
  });

  it("changes when chain grind toggles", () => {
    const off = buildOptimizeWarmStartKey({
      ...baseArgs,
      chainGrindEnabled: false,
    });
    const on = buildOptimizeWarmStartKey({
      ...baseArgs,
      chainGrindEnabled: true,
      chainKillsTarget: 3,
      chainSecondary: "min_hull_damage",
    });
    expect(off).not.toBe(on);
  });

  it("changes when below-decks slot count changes", () => {
    const three = buildOptimizeWarmStartKey({
      ...baseArgs,
      belowDecksSlots: 3,
    });
    const four = buildOptimizeWarmStartKey({
      ...baseArgs,
      belowDecksSlots: 4,
    });
    expect(three).not.toBe(four);
  });
});
