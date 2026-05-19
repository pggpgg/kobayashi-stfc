import { describe, expect, it } from "vitest";
import { buildPvpDefenderFingerprint, buildPvpSimulateParams } from "./pvpRequests";
import { createEmptyCrew } from "./types";

describe("buildPvpSimulateParams", () => {
  it("returns null without opponent profile", () => {
    const crew = createEmptyCrew(50, [10, 20, 30, 40, 50]);
    crew.captain = "Picard";
    expect(
      buildPvpSimulateParams({
        attackerShipId: "uss_enterprise_d",
        attackerShipTier: 5,
        attackerShipLevel: 50,
        attackerCrew: crew,
        defenderShipId: "rotarran",
        defenderShipTier: 5,
        defenderShipLevel: 50,
        defenderCrew: crew,
        opponentProfileId: "",
        simsPerCrew: 1000,
      }),
    ).toBeNull();
  });

  it("includes defender fields when opponent profile set", () => {
    const crew = createEmptyCrew(50, [10, 20, 30, 40, 50]);
    crew.captain = "Picard";
    const body = buildPvpSimulateParams({
      attackerShipId: "uss_enterprise_d",
      attackerShipTier: 5,
      attackerShipLevel: 50,
      attackerCrew: crew,
      defenderShipId: "rotarran",
      defenderShipTier: 3,
      defenderShipLevel: 40,
      defenderCrew: crew,
      opponentProfileId: "demo-opponent",
      simsPerCrew: 500,
    });
    expect(body).not.toBeNull();
    expect(body?.defender_ship).toBe("rotarran");
    expect(body?.defender_profile_id).toBe("demo-opponent");
    expect(body?.defender_opponent).toBe("player");
  });
});

describe("buildPvpDefenderFingerprint", () => {
  it("changes when defender ship changes", () => {
    const crew = createEmptyCrew(50, [10, 20, 30, 40, 50]);
    const a = buildPvpDefenderFingerprint({
      defenderShipId: "rotarran",
      defenderShipTier: 5,
      defenderShipLevel: 50,
      opponentProfileId: "p2",
      defenderCrew: crew,
    });
    const b = buildPvpDefenderFingerprint({
      defenderShipId: "uss_enterprise_d",
      defenderShipTier: 5,
      defenderShipLevel: 50,
      opponentProfileId: "p2",
      defenderCrew: crew,
    });
    expect(a).not.toBe(b);
  });
});
