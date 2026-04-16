/**
 * Persist last winning crews per scenario in localStorage for warm-start injection on the next optimize.
 * Schema is versioned so we can invalidate if the payload shape changes.
 */

import type { CrewRecommendation, WarmStartCrewBody } from "./api";

const SCHEMA = 1;
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
}): string {
  const mc =
    args.maxCandidates === null || args.maxCandidates <= 0
      ? "all"
      : String(args.maxCandidates);
  return stableKey([
    String(SCHEMA),
    args.profileId ?? "",
    args.shipId.trim(),
    args.scenarioId.trim(),
    String(args.shipTier),
    String(args.shipLevel),
    mc,
    args.constraintsFingerprint,
  ]);
}

export function storageKeyForWarmStart(cacheKey: string): string {
  return `${PREFIX}${cacheKey}`;
}

export function loadWarmStartCrews(cacheKey: string): WarmStartCrewPayload[] | null {
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
        below_decks: o.below_decks.filter((x): x is string => typeof x === "string"),
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
