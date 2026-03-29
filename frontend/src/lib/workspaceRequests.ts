import type { OptimizeCrewConstraintsBody, OptimizerStrategyType } from "./api";
import type { CrewState } from "./types";

/** Params for POST /api/simulate from workspace UI (single-crew Monte Carlo). */
export function buildWorkspaceSimulateParams(args: {
  shipId: string;
  scenarioId: string;
  crew: CrewState;
  simsPerCrew: number;
  shipTier: number;
  shipLevel: number;
}): {
  ship: string;
  hostile: string;
  crew: {
    captain: string;
    bridge: (string | null)[];
    below_deck: (string | null)[];
  };
  num_sims: number;
  ship_tier: number;
  ship_level: number;
} | null {
  if (!args.crew.captain) return null;
  return {
    ship: args.shipId || "Saladin",
    hostile: args.scenarioId || "2918121098",
    crew: {
      captain: args.crew.captain,
      bridge: args.crew.bridge,
      below_deck: args.crew.belowDeck,
    },
    num_sims: args.simsPerCrew,
    ship_tier: args.shipTier,
    ship_level: args.shipLevel,
  };
}

function splitOfficerList(s: string | undefined): string[] {
  if (!s?.trim()) return [];
  return s
    .split(/[,;]+/)
    .map((x) => x.trim())
    .filter(Boolean);
}

function parseGroupsJson(
  raw: string | undefined,
): OptimizeCrewConstraintsBody["groups"] {
  const t = raw?.trim();
  if (!t) return undefined;
  try {
    const v = JSON.parse(t) as unknown;
    if (!Array.isArray(v)) return undefined;
    const out: { officers: string[]; min_count: number }[] = [];
    for (const item of v) {
      if (!item || typeof item !== "object") continue;
      const o = item as Record<string, unknown>;
      if (!Array.isArray(o.officers) || typeof o.min_count !== "number")
        continue;
      const officers = o.officers
        .filter((x): x is string => typeof x === "string")
        .map((x) => x.trim())
        .filter(Boolean);
      const min_count = Math.floor(o.min_count);
      if (officers.length > 0 && min_count >= 1) {
        out.push({ officers, min_count });
      }
    }
    return out.length ? out : undefined;
  } catch {
    return undefined;
  }
}

/** Build API `constraints` object from Workspace form strings (omit when empty). */
export function buildOptimizeConstraintsFromForm(args: {
  mustIncludeComma: string;
  excludeComma: string;
  captainMust: string;
  bridgeMustComma: string;
  belowMustComma: string;
  groupsJson: string;
}): OptimizeCrewConstraintsBody | undefined {
  const must_include = splitOfficerList(args.mustIncludeComma);
  const exclude = splitOfficerList(args.excludeComma);
  const bridge_must_include = splitOfficerList(args.bridgeMustComma);
  const below_decks_must_include = splitOfficerList(args.belowMustComma);
  const captain_raw = args.captainMust?.trim();
  const captain_must_be = captain_raw || undefined;
  const groups = parseGroupsJson(args.groupsJson);
  if (
    !must_include.length &&
    !exclude.length &&
    !bridge_must_include.length &&
    !below_decks_must_include.length &&
    !captain_must_be &&
    !groups?.length
  ) {
    return undefined;
  }
  const body: OptimizeCrewConstraintsBody = {};
  if (must_include.length) body.must_include = must_include;
  if (exclude.length) body.exclude = exclude;
  if (bridge_must_include.length)
    body.bridge_must_include = bridge_must_include;
  if (below_decks_must_include.length)
    body.below_decks_must_include = below_decks_must_include;
  if (captain_must_be) body.captain_must_be = captain_must_be;
  if (groups?.length) body.groups = groups;
  return body;
}

/** Body for POST /api/optimize/start from workspace UI (mirrors handleRunOptimize). */
export function buildWorkspaceOptimizeStartBody(args: {
  shipId: string;
  scenarioId: string;
  simsPerCrew: number;
  maxCandidates: number | null;
  optimizerStrategy: OptimizerStrategyType;
  prioritizeBelowDecksAbility: boolean;
  selectedSeeds: string[];
  heuristicsOnly: boolean;
  belowDecksStrategy: "ordered" | "exploration";
  shipTier: number;
  shipLevel: number;
  optimizeConstraints?: {
    mustIncludeComma: string;
    excludeComma: string;
    captainMust: string;
    bridgeMustComma: string;
    belowMustComma: string;
    groupsJson: string;
  };
}) {
  const constraints = args.optimizeConstraints
    ? buildOptimizeConstraintsFromForm(args.optimizeConstraints)
    : undefined;

  return {
    ship: args.shipId || "Saladin",
    hostile: args.scenarioId || "2918121098",
    sims: args.simsPerCrew,
    max_candidates: args.maxCandidates ?? undefined,
    strategy: args.optimizerStrategy,
    prioritize_below_decks_ability:
      args.prioritizeBelowDecksAbility || undefined,
    heuristics_seeds:
      args.selectedSeeds.length > 0 ? args.selectedSeeds : undefined,
    heuristics_only: args.heuristicsOnly || undefined,
    below_decks_strategy:
      args.belowDecksStrategy !== "ordered"
        ? args.belowDecksStrategy
        : undefined,
    ship_tier: args.shipTier,
    ship_level: args.shipLevel,
    ...(constraints ? { constraints } : {}),
  };
}
