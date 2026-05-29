import type { ChainGrindRequestBody, OptimizerStrategyType } from "./api";
import { normalizeSupportBuffSelection } from "./supportBuffs";
import type { CrewState } from "./types";

function crewToApiBody(crew: CrewState) {
  return {
    captain: crew.captain,
    bridge: crew.bridge,
    below_deck: crew.belowDeck,
  };
}

function optionalSupportBuffField(ids: readonly string[] | undefined) {
  const normalized = normalizeSupportBuffSelection(ids).ids;
  return normalized.length > 0 ? normalized : undefined;
}

/** Fingerprint defender setup for optimize warm-start / history keys. */
export function buildPvpDefenderFingerprint(args: {
  defenderShipId: string;
  defenderShipTier: number;
  defenderShipLevel: number;
  opponentProfileId: string;
  defenderCrew: CrewState;
  attackerSupportBuffs?: readonly string[];
  defenderSupportBuffs?: readonly string[];
  defenderAllianceDebuffs?: readonly string[];
}): string {
  const c = args.defenderCrew;
  const buffKey = [
    optionalSupportBuffField(args.attackerSupportBuffs)?.join("+") ?? "",
    optionalSupportBuffField(args.defenderSupportBuffs)?.join("+") ?? "",
    optionalSupportBuffField(args.defenderAllianceDebuffs)?.join("+") ?? "",
  ].join(";");
  return [
    args.defenderShipId.trim(),
    String(args.defenderShipTier),
    String(args.defenderShipLevel),
    args.opponentProfileId.trim(),
    c.captain,
    ...c.bridge.map((x) => x ?? ""),
    ...c.belowDeck.map((x) => x ?? ""),
    buffKey,
  ].join("|");
}

export function buildPvpSimulateParams(args: {
  attackerShipId: string;
  attackerShipTier: number;
  attackerShipLevel: number;
  attackerCrew: CrewState;
  defenderShipId: string;
  defenderShipTier: number;
  defenderShipLevel: number;
  defenderCrew: CrewState;
  opponentProfileId: string;
  simsPerCrew: number;
  attackerSupportBuffs?: readonly string[];
  defenderSupportBuffs?: readonly string[];
  defenderAllianceDebuffs?: readonly string[];
}) {
  if (!args.attackerCrew.captain) return null;
  if (!args.opponentProfileId.trim()) return null;
  if (!args.defenderShipId.trim()) return null;
  const support_buffs = optionalSupportBuffField(args.attackerSupportBuffs);
  const defender_support_buffs = optionalSupportBuffField(
    args.defenderSupportBuffs,
  );
  const defender_alliance_debuffs = optionalSupportBuffField(
    args.defenderAllianceDebuffs,
  );
  return {
    ship: args.attackerShipId,
    hostile: "",
    defender_ship: args.defenderShipId,
    defender_ship_tier: args.defenderShipTier >= 1 ? args.defenderShipTier : 1,
    defender_ship_level:
      args.defenderShipLevel >= 1 ? args.defenderShipLevel : 1,
    defender_profile_id: args.opponentProfileId.trim(),
    defender_opponent: "player",
    crew: crewToApiBody(args.attackerCrew),
    defender_crew: crewToApiBody(args.defenderCrew),
    num_sims: args.simsPerCrew,
    ship_tier: args.attackerShipTier,
    ship_level: args.attackerShipLevel,
    ...(support_buffs ? { support_buffs } : {}),
    ...(defender_support_buffs ? { defender_support_buffs } : {}),
    ...(defender_alliance_debuffs ? { defender_alliance_debuffs } : {}),
  };
}

export function buildPvpOptimizeStartBody(args: {
  attackerShipId: string;
  attackerShipTier: number;
  attackerShipLevel: number;
  defenderShipId: string;
  defenderShipTier: number;
  defenderShipLevel: number;
  opponentProfileId: string;
  defenderCrew: CrewState;
  simsPerCrew: number;
  maxCandidates: number | null;
  optimizerStrategy: OptimizerStrategyType;
  belowDecksSlots: number;
  attackerSupportBuffs?: readonly string[];
  defenderSupportBuffs?: readonly string[];
  defenderAllianceDebuffs?: readonly string[];
  optimizeCacheKey?: string | null;
  chainGrind?: ChainGrindRequestBody;
}) {
  const support_buffs = optionalSupportBuffField(args.attackerSupportBuffs);
  const defender_support_buffs = optionalSupportBuffField(
    args.defenderSupportBuffs,
  );
  const defender_alliance_debuffs = optionalSupportBuffField(
    args.defenderAllianceDebuffs,
  );
  return {
    ship: args.attackerShipId,
    hostile: "",
    defender_ship: args.defenderShipId,
    defender_ship_tier: args.defenderShipTier,
    defender_ship_level: args.defenderShipLevel,
    defender_profile_id: args.opponentProfileId.trim(),
    defender_opponent: "player",
    defender_crew: crewToApiBody(args.defenderCrew),
    sims: args.simsPerCrew,
    max_candidates: args.maxCandidates ?? undefined,
    strategy: args.optimizerStrategy,
    ship_tier: args.attackerShipTier,
    ship_level: args.attackerShipLevel,
    below_decks_slots: args.belowDecksSlots,
    ...(support_buffs ? { support_buffs } : {}),
    ...(defender_support_buffs ? { defender_support_buffs } : {}),
    ...(defender_alliance_debuffs ? { defender_alliance_debuffs } : {}),
    ...(args.chainGrind ? { chain: args.chainGrind } : {}),
    ...(args.optimizeCacheKey?.trim()
      ? { optimize_cache_key: args.optimizeCacheKey.trim() }
      : {}),
  };
}
