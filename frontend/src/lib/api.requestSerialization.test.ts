import { afterEach, describe, expect, it, vi } from "vitest";
import { optimize, optimizeStart, simulate } from "./api";
import {
  buildPvpOptimizeStartBody,
  buildPvpSimulateParams,
} from "./pvpRequests";
import { createEmptyCrew } from "./types";
import { buildWorkspaceOptimizeStartBody } from "./workspaceRequests";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function captureRequest() {
  const fetchMock = vi.fn().mockResolvedValue(
    new Response(JSON.stringify({ status: "ok", job_id: "job-1" }), {
      headers: { "Content-Type": "application/json" },
    }),
  );
  vi.stubGlobal("fetch", fetchMock);
  return () => {
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, options] = fetchMock.mock.calls[0] as [string, RequestInit];
    return { url, options, body: JSON.parse(String(options.body)) };
  };
}

function pvpSetup() {
  const crew = createEmptyCrew(40, [10, 20, 30, 40]);
  crew.captain = "Picard";
  crew.bridge = ["Data", "Riker"];
  const defenderCrew = createEmptyCrew(30, [10, 20, 30]);
  defenderCrew.captain = "Kirk";
  return {
    attackerShipId: "enterprise",
    attackerShipTier: 5,
    attackerShipLevel: 40,
    attackerCrew: crew,
    defenderShipId: "rotarran",
    defenderShipTier: 3,
    defenderShipLevel: 30,
    defenderCrew,
    opponentProfileId: "opponent-profile",
    simsPerCrew: 500,
    attackerSupportBuffs: ["cerritos_support"],
    defenderSupportBuffs: ["titan_a_fortification"],
    defenderAllianceDebuffs: ["mantis_sting"],
  };
}

function expectedDefender() {
  const { defenderCrew: _defenderCrew, ...setup } = pvpSetup();
  return {
    defender_ship: setup.defenderShipId,
    defender_ship_tier: setup.defenderShipTier,
    defender_ship_level: setup.defenderShipLevel,
    defender_profile_id: setup.opponentProfileId,
    defender_opponent: "player",
    defender_crew: expect.objectContaining({ captain: "Kirk" }),
    defender_support_buffs: setup.defenderSupportBuffs,
    defender_alliance_debuffs: setup.defenderAllianceDebuffs,
  };
}

describe("real API request serialization", () => {
  it("#318 sends all PvP defender fields from the simulate builder to fetch", async () => {
    const request = captureRequest();
    const params = buildPvpSimulateParams(pvpSetup());
    if (!params) throw new Error("valid PvP setup must build");
    await simulate(params, "attacker-profile");
    const { url, options, body } = request();
    expect(url).toMatch(/\/api\/simulate$/);
    expect(body).toMatchObject({ hostile: "", ...expectedDefender() });
    expect(options.headers).toMatchObject({
      "X-Profile-Id": "attacker-profile",
    });
  });

  it("#318 sends all PvP defender fields from the optimize builder to fetch", async () => {
    const request = captureRequest();
    const params = buildPvpOptimizeStartBody({
      ...pvpSetup(),
      maxCandidates: 25,
      optimizerStrategy: "tiered",
      belowDecksSlots: 3,
      optimizeCacheKey: "pvp-history",
    });
    await optimizeStart(params, "attacker-profile");
    const { url, body } = request();
    expect(url).toMatch(/\/api\/optimize\/start$/);
    expect(body).toMatchObject({ hostile: "", ...expectedDefender() });
    expect(body.optimize_cache_key).toBe("pvp-history");
  });

  it("#318 preserves the same defender side on synchronous optimization", async () => {
    const request = captureRequest();
    const params = buildPvpOptimizeStartBody({
      ...pvpSetup(),
      maxCandidates: 25,
      optimizerStrategy: "exhaustive",
      belowDecksSlots: 3,
    });
    await optimize(params, "attacker-profile");
    expect(request().body).toMatchObject(expectedDefender());
  });

  it("#340 forwards workspace refinement, history key and history anchors", async () => {
    const request = captureRequest();
    const params = buildWorkspaceOptimizeStartBody({
      shipId: "enterprise",
      scenarioId: "hostile-1",
      simsPerCrew: 500,
      maxCandidates: 25,
      optimizerStrategy: "tiered",
      belowDecksPoolMode: "strict",
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: 5,
      shipLevel: 40,
      localRefinement: true,
      optimizeCacheKey: " scenario-history ",
      noveltyLambdaText: "0.25",
      noveltyHistoryAnchors: true,
    });
    await optimizeStart(params, "attacker-profile");
    expect(request().body).toMatchObject({
      local_refinement: true,
      optimize_cache_key: "scenario-history",
      novelty_history_anchors: true,
      novelty_lambda: 0.25,
    });
  });

  it("#341/#330 sends an explicit exhaustive strategy", async () => {
    const request = captureRequest();
    await optimizeStart({
      ship: "enterprise",
      hostile: "hostile-1",
      strategy: "exhaustive",
    });
    expect(request().body.strategy).toBe("exhaustive");
  });

  it("keeps omitted strategy and disabled optional features absent for server defaults", async () => {
    const request = captureRequest();
    await optimizeStart({ ship: "enterprise", hostile: "hostile-1" });
    const { body } = request();
    for (const key of [
      "strategy",
      "local_refinement",
      "optimize_cache_key",
      "novelty_history_anchors",
      "defender_ship",
    ]) {
      expect(body).not.toHaveProperty(key);
    }
  });
});
