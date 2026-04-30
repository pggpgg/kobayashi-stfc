import { describe, expect, it } from "vitest";
import { createEmptyCrew } from "./types";
import {
  buildOptimizeConstraintsFromForm,
  buildWorkspaceOptimizeStartBody,
  buildWorkspaceSimulateParams,
  noveltyFieldsForOptimizeBody,
} from "./workspaceRequests";

describe("buildWorkspaceSimulateParams", () => {
  it("returns null without captain", () => {
    const crew = createEmptyCrew(50);
    expect(
      buildWorkspaceSimulateParams({
        shipId: "Saladin",
        scenarioId: "2918121098",
        crew,
        simsPerCrew: 1000,
        shipTier: 1,
        shipLevel: 50,
      }),
    ).toBeNull();
  });

  it("uses simsPerCrew as num_sims (not a hardcoded default)", () => {
    const crew = createEmptyCrew(50);
    crew.captain = "Kirk";
    const p = buildWorkspaceSimulateParams({
      shipId: "Enterprise",
      scenarioId: "hostile1",
      crew,
      simsPerCrew: 12_500,
      shipTier: 2,
      shipLevel: 45,
    });
    expect(p).not.toBeNull();
    expect(p).toEqual(
      expect.objectContaining({
        num_sims: 12_500,
        ship: "Enterprise",
        hostile: "hostile1",
        ship_tier: 2,
        ship_level: 45,
      }),
    );
  });

  it("serializes normalized support buffs when present", () => {
    const crew = createEmptyCrew(50);
    crew.captain = "Kirk";

    const params = buildWorkspaceSimulateParams({
      shipId: "Enterprise",
      scenarioId: "hostile1",
      crew,
      simsPerCrew: 1000,
      shipTier: 2,
      shipLevel: 45,
      supportBuffs: [
        "cerritos_support",
        "unknown_buff",
        "cerritos_support",
        "titan_a_fortification",
        "titan_a_max_fortification",
      ],
    });

    expect(params?.support_buffs).toEqual([
      "cerritos_support",
      "titan_a_max_fortification",
    ]);
  });
});

describe("buildWorkspaceOptimizeStartBody", () => {
  it("uses simsPerCrew as sims to match single-sim control", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "X",
      scenarioId: "Y",
      simsPerCrew: 50_000,
      maxCandidates: 200,
      optimizerStrategy: "genetic",
      prioritizeBelowDecksAbility: true,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
    });
    expect(body.sims).toBe(50_000);
    expect(body.strategy).toBe("genetic");
    expect(body.max_candidates).toBe(200);
    expect(body.prioritize_below_decks_ability).toBe(true);
    expect(body.below_decks_strategy).toBeUndefined();
    expect(body.below_decks_slots).toBeUndefined();
  });

  it("includes heuristics_seeds when non-empty", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "",
      scenarioId: "",
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: "exhaustive",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: ["meta"],
      heuristicsOnly: true,
      belowDecksStrategy: "exploration",
      shipTier: 1,
      shipLevel: 1,
    });
    expect(body.heuristics_seeds).toEqual(["meta"]);
    expect(body.heuristics_only).toBe(true);
    expect(body.below_decks_strategy).toBe("exploration");
  });

  it("includes fast_discovery when enabled", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: 50,
      optimizerStrategy: "tiered",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: ["meta"],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      fastDiscovery: true,
    });
    expect(body.fast_discovery).toBe(true);
  });

  it("serializes normalized support buffs for optimize/start", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: 50,
      optimizerStrategy: "tiered",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      supportBuffs: [
        "defiant_reinforce",
        "bogus",
        "titan_a_fortification",
        "titan_a_max_fortification",
      ],
    });

    expect(body.support_buffs).toEqual([
      "defiant_reinforce",
      "titan_a_max_fortification",
    ]);
  });

  it("serializes explicit below_decks_slots when provided", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: 50,
      optimizerStrategy: "tiered",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      belowDecksSlots: 6,
    });

    expect(body.below_decks_slots).toBe(6);
  });

  it("includes chain when chainGrind.enabled with kills_target", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: "exhaustive",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      chainGrind: {
        enabled: true,
        kills_target: 5,
        secondary: "max_loot_per_hull_proxy",
      },
    });
    expect(body.chain).toEqual({
      enabled: true,
      kills_target: 5,
      secondary: "max_loot_per_hull_proxy",
    });
  });

  it("includes constraints when form fields are set", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: "exhaustive",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      optimizeConstraints: {
        mustIncludeComma: "Alice, Bob",
        excludeComma: "Eve",
        captainMust: "Alice",
        bridgeMustComma: "Bob",
        belowMustComma: "Zed",
        groupsJson: '[{"officers":["X","Y"],"min_count":2}]',
      },
    });
    expect(body.constraints).toEqual({
      must_include: ["Alice", "Bob"],
      exclude: ["Eve"],
      captain_must_be: "Alice",
      bridge_must_include: ["Bob"],
      below_decks_must_include: ["Zed"],
      groups: [{ officers: ["X", "Y"], min_count: 2 }],
    });
  });

  it("joins multiple captain alternatives for captain_must_be", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: "exhaustive",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      optimizeConstraints: {
        mustIncludeComma: "",
        excludeComma: "",
        captainMust: "Alice , Bob; Carol",
        bridgeMustComma: "",
        belowMustComma: "",
        groupsJson: "",
      },
    });
    expect(body.constraints?.captain_must_be).toBe("Alice, Bob, Carol");
  });

  it("includes optimize_cache_key when non-empty", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: 10,
      optimizerStrategy: "tiered",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      optimizeCacheKey: " 3|k|v1 ",
    });
    expect(body.optimize_cache_key).toBe("3|k|v1");
  });

  it("includes enable_learned_pair_prior when disabled", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: 10,
      optimizerStrategy: "tiered",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      enableLearnedPairPrior: false,
    });
    expect(body.enable_learned_pair_prior).toBe(false);
  });

  it("includes warm_start_crews when non-empty", () => {
    const warmStartCrews = [
      { captain: "A", bridge: ["B", "C"], below_decks: ["D", "E", "F"] },
    ];
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: 10,
      optimizerStrategy: "tiered",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      warmStartCrews,
    });
    expect(body.warm_start_crews).toEqual(warmStartCrews);
  });

  it("includes tiered_scout_sims and tiered_top_k when strategy is tiered and set", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: "tiered",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      tieredScoutSims: 800,
      tieredTopK: 40,
    });
    expect(body.tiered_scout_sims).toBe(800);
    expect(body.tiered_top_k).toBe(40);
  });

  it("omits tiered fields when strategy is not tiered even if values are set", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: "exhaustive",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      tieredScoutSims: 800,
      tieredTopK: 40,
    });
    expect(body).not.toHaveProperty("tiered_scout_sims");
    expect(body).not.toHaveProperty("tiered_top_k");
  });

  it("omits tiered fields when null, zero, or negative", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: "tiered",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      tieredScoutSims: 0,
      tieredTopK: null,
    });
    expect(body).not.toHaveProperty("tiered_scout_sims");
    expect(body).not.toHaveProperty("tiered_top_k");
  });

  it("omits novelty fields when lambda text blank even if head/pool set", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: "exhaustive",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      noveltyLambdaText: "  ",
      noveltyDiverseTopText: "20",
      noveltyPoolText: "64",
    });
    expect(body).not.toHaveProperty("novelty_lambda");
    expect(body).not.toHaveProperty("novelty_diverse_top");
    expect(body).not.toHaveProperty("novelty_pool");
  });

  it("includes novelty_lambda and optional head/pool when lambda text valid", () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: "S",
      scenarioId: "H",
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: "tiered",
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 1,
      shipLevel: 50,
      noveltyLambdaText: "0.7",
      noveltyDiverseTopText: "15",
      noveltyPoolText: "100",
    });
    expect(body.novelty_lambda).toBe(0.7);
    expect(body.novelty_diverse_top).toBe(15);
    expect(body.novelty_pool).toBe(100);
  });
});

describe("noveltyFieldsForOptimizeBody", () => {
  it("returns empty when lambda blank", () => {
    expect(
      noveltyFieldsForOptimizeBody({
        noveltyLambdaText: "",
        noveltyDiverseTopText: "20",
        noveltyPoolText: "64",
      }),
    ).toEqual({});
  });

  it("returns empty when lambda out of (0, 1]", () => {
    expect(
      noveltyFieldsForOptimizeBody({
        noveltyLambdaText: "0",
        noveltyDiverseTopText: "",
        noveltyPoolText: "",
      }),
    ).toEqual({});
    expect(
      noveltyFieldsForOptimizeBody({
        noveltyLambdaText: "1.01",
        noveltyDiverseTopText: "",
        noveltyPoolText: "",
      }),
    ).toEqual({});
  });

  it("includes only lambda when head/pool invalid or blank", () => {
    expect(
      noveltyFieldsForOptimizeBody({
        noveltyLambdaText: "0.65",
        noveltyDiverseTopText: "0",
        noveltyPoolText: "xy",
      }),
    ).toEqual({ novelty_lambda: 0.65 });
  });

  it("includes novelty_history_anchors when true and lambda valid", () => {
    expect(
      noveltyFieldsForOptimizeBody({
        noveltyLambdaText: "0.6",
        noveltyDiverseTopText: "",
        noveltyPoolText: "",
        noveltyHistoryAnchors: true,
      }),
    ).toEqual({ novelty_lambda: 0.6, novelty_history_anchors: true });
  });

  it("omits novelty_history_anchors when false", () => {
    expect(
      noveltyFieldsForOptimizeBody({
        noveltyLambdaText: "0.6",
        noveltyDiverseTopText: "",
        noveltyPoolText: "",
        noveltyHistoryAnchors: false,
      }),
    ).toEqual({ novelty_lambda: 0.6 });
  });
});

describe("buildOptimizeConstraintsFromForm", () => {
  it("returns undefined when all empty", () => {
    expect(
      buildOptimizeConstraintsFromForm({
        mustIncludeComma: "",
        excludeComma: "  ",
        captainMust: "",
        bridgeMustComma: "",
        belowMustComma: "",
        groupsJson: "",
      }),
    ).toBeUndefined();
  });
});
