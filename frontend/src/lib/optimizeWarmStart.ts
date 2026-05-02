/**
 * Persist last winning crews per scenario in localStorage for warm-start injection on the next optimize.
 * Schema is versioned so we can invalidate if the payload shape changes.
 */

import type { CrewRecommendation, WarmStartCrewBody } from "./api";

const SCHEMA = 4;
const PREFIX = "kobayashi_opt_warm_v";

export type WarmStartCrewPayload = WarmStartCrewBody;

function stableKey(parts: string[]): string {
  return parts.join("|");
}

/** Build a cache key from scenario + coarse constraint fingerprint. */
export function buildOptimizeWarmStartKey(args: {
  profileId: string | null;
  shipId: string;
  scenarioId: string;
  shipTier: number;
  shipLevel: number;
  maxCandidates: number | null;
  constraintsFingerprint: string;
  /** Matches API `defender_opponent` when the UI exposes it; default hostile NPC. */
  defenderOpponent?: string;
  /** Support buff ids applied to the optimize scenario (sorted for stability). */
  supportBuffIds?: readonly string[];
  chainGrindEnabled?: boolean;
  chainKillsTarget?: number;
  chainSecondary?: string;
  allowBelowDecksWithoutCombatAbility?: boolean;
  /** Resolved below-decks slot count used for candidate generation. */
  belowDecksSlots?: number;
  /** Fast discovery merges heuristic seeds into warm-start; affects which persisted wins apply. */
  fastDiscovery?: boolean;
  /** Learned pair prior toggle affects analytical prefilter ranking. */
  enableLearnedPairPrior?: boolean;
}): string {
  const mc =
    args.maxCandidates === null || args.maxCandidates <= 0
      ? "all"
      : String(args.maxCandidates);
  const defender = (args.defenderOpponent ?? "Hostile").trim() || "Hostile";
  const buffs = [...(args.supportBuffIds ?? [])]
    .map((s) => s.trim())
    .filter(Boolean)
    .sort()
    .join(",");
  const chain =
    args.chainGrindEnabled === true
      ? `1:${args.chainKillsTarget ?? ""}:${(args.chainSecondary ?? "").trim()}`
      : "0";
  const abd = args.allowBelowDecksWithoutCombatAbility === true ? "1" : "0";
  const bdSlots =
    args.belowDecksSlots != null && args.belowDecksSlots >= 0
      ? String(args.belowDecksSlots)
      : "";
  const fd = args.fastDiscovery === true ? "1" : "0";
  const lpp = args.enableLearnedPairPrior === false ? "0" : "1";
  return stableKey([
    String(SCHEMA),
    args.profileId ?? "",
    args.shipId.trim(),
    args.scenarioId.trim(),
    String(args.shipTier),
    String(args.shipLevel),
    mc,
    args.constraintsFingerprint,
    defender,
    buffs,
    chain,
    abd,
    bdSlots,
    fd,
    lpp,
  ]);
}

export function storageKeyForWarmStart(cacheKey: string): string {
  return `${PREFIX}${cacheKey}`;
}

export function loadWarmStartCrews(
  cacheKey: string,
): WarmStartCrewPayload[] | null {
  try {
    const raw = localStorage.getItem(storageKeyForWarmStart(cacheKey));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as { schema?: number; crews?: unknown };
    if (parsed.schema !== SCHEMA || !Array.isArray(parsed.crews)) return null;
    const out: WarmStartCrewPayload[] = [];
    for (const row of parsed.crews) {
      const o = row as Record<string, unknown>;
      if (
        typeof o.captain !== "string" ||
        !Array.isArray(o.bridge) ||
        !Array.isArray(o.below_decks)
      ) {
        return null;
      }
      out.push({
        captain: o.captain,
        bridge: o.bridge.filter((x): x is string => typeof x === "string"),
        below_decks: o.below_decks.filter(
          (x): x is string => typeof x === "string",
        ),
      });
    }
    return out.length ? out : null;
  } catch {
    return null;
  }
}

const MAX_STORED_CREWS = 12;
const MAX_JSON_BYTES = 48_000;

export function saveWarmStartFromRecommendations(
  cacheKey: string,
  recommendations: CrewRecommendation[],
): void {
  const crews: WarmStartCrewPayload[] = recommendations
    .slice(0, MAX_STORED_CREWS)
    .map((r) => ({
      captain: r.captain,
      bridge: [...r.bridge],
      below_decks: [...r.below_decks],
    }));
  if (!crews.length) return;
  const payload = JSON.stringify({ schema: SCHEMA, crews });
  if (payload.length > MAX_JSON_BYTES) return;
  try {
    localStorage.setItem(storageKeyForWarmStart(cacheKey), payload);
  } catch {
    /* quota / private mode */
  }
}
