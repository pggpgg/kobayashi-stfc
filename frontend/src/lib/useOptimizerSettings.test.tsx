import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useOptimizerSettings } from "./useOptimizerSettings";

describe("#301 grouped optimizer settings", () => {
  it("preserves existing workspace defaults", () => {
    const { result } = renderHook(useOptimizerSettings);
    expect(result.current.settings).toMatchObject({
      maxCandidates: 100,
      optimizerStrategy: "tiered",
      belowDecksPoolMode: "strict",
      enableLearnedPairPrior: true,
      tieredScoutSims: null,
      tieredTopK: null,
      tieredRandomExplorationPct: null,
      localRefinement: false,
      noveltyLambdaText: "",
      noveltyHistoryAnchors: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      fastDiscovery: false,
      belowDecksStrategy: "ordered",
      chainGrindEnabled: false,
      chainKillsTarget: 3,
      chainSecondary: "min_hull_damage",
      optimizeGroupsJson: "",
    });
  });
  it("merges batched partial changes across sections without resetting other controls", () => {
    const { result } = renderHook(useOptimizerSettings);
    const update = result.current.update;
    act(() => {
      result.current.update({ selectedSeeds: ["borg"], fastDiscovery: true });
      result.current.update({ tieredScoutSims: 25, localRefinement: true });
      result.current.update({
        noveltyLambdaText: "0.6",
        noveltyHistoryAnchors: true,
      });
      result.current.update({
        optimizeGroupsJson: '[{"officers":["A","B"],"min_count":2}]',
      });
    });
    expect(result.current.settings).toMatchObject({
      selectedSeeds: ["borg"],
      fastDiscovery: true,
      tieredScoutSims: 25,
      localRefinement: true,
      noveltyLambdaText: "0.6",
      noveltyHistoryAnchors: true,
      optimizeGroupsJson: '[{"officers":["A","B"],"min_count":2}]',
      maxCandidates: 100,
    });
    expect(result.current.update).toBe(update);
  });
  it("keeps legacy field setters stable and preserves functional updates", () => {
    const { result } = renderHook(useOptimizerSettings);
    const setter = result.current.setters.chainKillsTarget;
    act(() => {
      result.current.setters.chainKillsTarget((previous) => previous + 1);
      result.current.setters.chainKillsTarget((previous) => previous + 1);
      result.current.update({ optimizerStrategy: "exhaustive" });
    });
    expect(result.current.settings.chainKillsTarget).toBe(5);
    expect(result.current.settings.optimizerStrategy).toBe("exhaustive");
    expect(result.current.setters.chainKillsTarget).toBe(setter);
  });
});
