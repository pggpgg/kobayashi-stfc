import type {
  ChainGrindRequestBody,
  OptimizeCrewConstraintsBody,
  OptimizerStrategyType,
  WarmStartCrewBody,
} from "./api";
import { normalizeSupportBuffSelection } from "./supportBuffs";
import type { CrewState } from "./types";

/** Params for POST /api/simulate from workspace UI (single-crew Monte Carlo). */
export function buildWorkspaceSimulateParams(args: {
  shipId: string;
  scenarioId: string;
  crew: CrewState;
  simsPerCrew: number;
  shipTier: number;
  shipLevel: number;
  supportBuffs?: readonly string[];
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
  support_buffs?: string[];
} | null {
  if (!args.crew.captain) return null;
  const support_buffs = normalizeSupportBuffSelection(args.supportBuffs).ids;
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
    ...(support_buffs.length > 0 ? { support_buffs } : {}),
  };
}

export function splitOfficerList(s: string | undefined): string[] {
  if (!s?.trim()) return [];
  return s
    .split(/[,;]+/)
    .map((x) => x.trim())
    .filter(Boolean);
}

/** Join officer display names for comma-separated optimize form fields / API `captain_must_be`. */
export function joinOfficerList(names: string[]): string {
  return names
    .map((x) => x.trim())
    .filter(Boolean)
    .join(", ");
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
  const captainList = splitOfficerList(args.captainMust);
  const captain_must_be =
    captainList.length > 0 ? joinOfficerList(captainList) : undefined;
  const groups = parseGroupsJson(args.groupsJson);
  if (
    !must_include.length &&
    !exclude.length &&
    !bridge_must_include.length &&
    !below_decks_must_include.length &&
    captainList.length === 0 &&
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

/** Match server `MAX_NOVELTY_DIVERSE_TOP` / `MAX_NOVELTY_POOL` (`src/server/api/requests.rs`). */
const MAX_NOVELTY_DIVERSE_TOP_UI = 500;
const MAX_NOVELTY_POOL_UI = 10_000;

/** Optional MMR / novelty fields for `POST /api/optimize/start` (omit when lambda blank). */
export function noveltyFieldsForOptimizeBody(args: {
  noveltyLambdaText: string;
  noveltyDiverseTopText: string;
  noveltyPoolText: string;
}): {
  novelty_lambda?: number;
  novelty_diverse_top?: number;
  novelty_pool?: number;
} {
  const lamStr = args.noveltyLambdaText.trim();
  if (!lamStr) return {};
  const novelty_lambda = Number(lamStr);
  if (
    !Number.isFinite(novelty_lambda) ||
    novelty_lambda <= 0 ||
    novelty_lambda > 1
  ) {
    return {};
  }
  const out: {
    novelty_lambda: number;
    novelty_diverse_top?: number;
    novelty_pool?: number;
  } = { novelty_lambda };

  const dStr = args.noveltyDiverseTopText.trim();
  if (dStr) {
    const d = parseInt(dStr, 10);
    if (Number.isFinite(d) && d >= 1 && d <= MAX_NOVELTY_DIVERSE_TOP_UI) {
      out.novelty_diverse_top = d;
    }
  }
  const pStr = args.noveltyPoolText.trim();
  if (pStr) {
    const p = parseInt(pStr, 10);
    if (Number.isFinite(p) && p >= 2 && p <= MAX_NOVELTY_POOL_UI) {
      out.novelty_pool = p;
    }
  }
  return out;
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
  supportBuffs?: readonly string[];
  optimizeConstraints?: {
    mustIncludeComma: string;
    excludeComma: string;
    captainMust: string;
    bridgeMustComma: string;
    belowMustComma: string;
    groupsJson: string;
  };
  /** Chain grind: sequential fights, hull carry-over, full shields each link. */
  chainGrind?: ChainGrindRequestBody;
  /** Deduped crews prepended before generated candidates (localStorage warm-start). */
  warmStartCrews?: WarmStartCrewBody[];
  /** Tiered only: scout sims per crew (omit for server default 500). */
  tieredScoutSims?: number | null;
  /** Tiered only: crews promoted to full confirmation (omit for server default 20). */
  tieredTopK?: number | null;
  /** Merge heuristic seeds into main optimize warm-start (requires non-empty selected seeds). */
  fastDiscovery?: boolean;
  /** MMR blend in (0, 1]; blank = omit (pure strength ordering). */
  noveltyLambdaText?: string;
  noveltyDiverseTopText?: string;
  noveltyPoolText?: string;
  /** Opaque key for server-side optimize history (`profiles/{id}/optimize_history.json`). */
  optimizeCacheKey?: string | null;
  /** Analytical prefilter: learned pair co-occurrence prior toggle (default true). */
  enableLearnedPairPrior?: boolean;
}) {
  const constraints = args.optimizeConstraints
    ? buildOptimizeConstraintsFromForm(args.optimizeConstraints)
    : undefined;

  const support_buffs = normalizeSupportBuffSelection(args.supportBuffs).ids;

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
    ...(support_buffs.length > 0 ? { support_buffs } : {}),
    ...(constraints ? { constraints } : {}),
    ...(args.chainGrind?.enabled
      ? {
          chain: {
            enabled: true,
            kills_target: args.chainGrind.kills_target,
            ...(args.chainGrind.secondary &&
            args.chainGrind.secondary !== "min_hull_damage"
              ? { secondary: args.chainGrind.secondary }
              : {}),
          } satisfies ChainGrindRequestBody,
        }
      : {}),
    ...(args.warmStartCrews && args.warmStartCrews.length > 0
      ? { warm_start_crews: args.warmStartCrews }
      : {}),
    ...(args.optimizerStrategy === "tiered" &&
    args.tieredScoutSims != null &&
    args.tieredScoutSims > 0
      ? { tiered_scout_sims: args.tieredScoutSims }
      : {}),
    ...(args.optimizerStrategy === "tiered" &&
    args.tieredTopK != null &&
    args.tieredTopK > 0
      ? { tiered_top_k: args.tieredTopK }
      : {}),
    ...(args.fastDiscovery === true ? { fast_discovery: true } : {}),
    ...noveltyFieldsForOptimizeBody({
      noveltyLambdaText: args.noveltyLambdaText ?? "",
      noveltyDiverseTopText: args.noveltyDiverseTopText ?? "",
      noveltyPoolText: args.noveltyPoolText ?? "",
    }),
    ...(args.optimizeCacheKey != null && args.optimizeCacheKey.trim() !== ""
      ? { optimize_cache_key: args.optimizeCacheKey.trim() }
      : {}),
    ...(args.enableLearnedPairPrior === false
      ? { enable_learned_pair_prior: false }
      : {}),
  };
}
