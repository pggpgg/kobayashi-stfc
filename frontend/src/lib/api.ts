import type {
  DataVersionResponse,
  HostileListItem,
  OfficerListItem,
  OptimizeEstimate,
  OptimizeStartResponse,
  ProfileEntry,
  ProfilesResponse,
  ShipListItem,
  ShipTiersLevels,
  SimulateCrew,
  SimulateResponse,
} from "./api/schema";

export type {
  DataVersionResponse,
  HostileListItem,
  OfficerListItem,
  OptimizeEstimate,
  OptimizeStartResponse,
  ProfileEntry,
  ProfilesResponse,
  ShipListItem,
  ShipTiersLevels,
  SimulateCrew,
  SimulateResponse,
  SimulateStats,
} from "./api/schema";

/**
 * Base URL for API requests. Empty string = same origin.
 * Set at build time via VITE_API_BASE (e.g. for deployment behind a proxy).
 */
export const API_BASE =
  typeof import.meta !== "undefined" && import.meta.env?.VITE_API_BASE != null
    ? String(import.meta.env.VITE_API_BASE).replace(/\/$/, "")
    : "";

/** Categorical code when the server returns HTTP 503 `code: cpu_busy` (CPU admission). */
export const API_ERROR_CPU_BUSY = "CPU_BUSY";

/** Structured error from the API (status code + server message when available). */
export class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly code: string,
    /** From `retry_after_ms` JSON or `Retry-After` header when `code` is {@link API_ERROR_CPU_BUSY}. */
    public readonly retryAfterMs?: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

function codeFromStatus(status: number): string {
  if (status >= 500) return "SERVER_ERROR";
  if (status === 404) return "NOT_FOUND";
  if (status === 400 || status === 422) return "VALIDATION";
  if (status === 401 || status === 403) return "AUTH";
  return "ERROR";
}

function retryAfterMsFromRetryAfterHeader(res: Response): number | undefined {
  const raw = res.headers.get("Retry-After");
  if (raw == null || raw === "") return undefined;
  const secs = Number.parseInt(raw, 10);
  if (!Number.isFinite(secs) || secs < 1) return undefined;
  return secs * 1000;
}

/** Parse error response body; returns an ApiError with server message when JSON has status/message. */
export async function parseApiError(
  res: Response,
  bodyText: string,
): Promise<ApiError> {
  let message = bodyText || res.statusText;
  let code = codeFromStatus(res.status);
  let retryAfterMs: number | undefined;

  try {
    const json = JSON.parse(bodyText) as {
      message?: string;
      status?: string;
      code?: string;
      retry_after_ms?: unknown;
    };
    if (typeof json.message === "string" && json.message.trim()) {
      message = json.message.trim();
    }
    if (typeof json.code === "string" && json.code.trim() === "cpu_busy") {
      code = API_ERROR_CPU_BUSY;
      const raw = json.retry_after_ms;
      if (typeof raw === "number" && Number.isFinite(raw) && raw > 0) {
        retryAfterMs = raw;
      } else if (
        typeof raw === "string" &&
        Number.isFinite(Number.parseFloat(raw)) &&
        Number.parseFloat(raw) > 0
      ) {
        retryAfterMs = Number.parseFloat(raw);
      }
      if (retryAfterMs == null) {
        retryAfterMs = retryAfterMsFromRetryAfterHeader(res);
      }
    }
  } catch {
    // keep message as bodyText or statusText
  }

  if (code === API_ERROR_CPU_BUSY && retryAfterMs == null) {
    retryAfterMs = retryAfterMsFromRetryAfterHeader(res);
  }

  return new ApiError(message, res.status, code, retryAfterMs);
}

/** Max single sleep when backing off after `cpu_busy` (per server hint, capped). */
const MAX_CPU_BUSY_PER_WAIT_MS = 120_000;
/** Max cumulative sleep across all `cpu_busy` rounds for one logical request. */
const MAX_CPU_BUSY_TOTAL_WAIT_MS = 300_000;
/** Number of `cpu_busy` responses tolerated before failing (each followed by a wait, then re-fetch). */
const MAX_CPU_BUSY_ROUNDS = 7;
/** When the server omits `retry_after_ms` / `Retry-After`, wait this long before retrying. */
const CPU_BUSY_DEFAULT_BACKOFF_MS = 1_500;

/** Passed to {@link fetchWithCpuBusyRetries} when the client waits for a CPU slot. */
export type CpuBusyWaitInfo = { waitMs: number; attempt: number };

export type FetchCpuBusyOptions = {
  onCpuBusyWait?: (info: CpuBusyWaitInfo) => void;
};

function sleepMs(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

/**
 * Repeated `fetch` with bounded waits when the server returns 503 `cpu_busy`
 * (CPU admission). Caps per-wait, total wait, and number of rounds.
 */
export async function fetchWithCpuBusyRetries(
  url: string,
  init: RequestInit,
  options?: FetchCpuBusyOptions,
): Promise<Response> {
  let totalWaitedMs = 0;
  let cpuBusyRound = 0;

  while (true) {
    const res = await fetch(url, init);
    if (res.ok) return res;

    const bodyText = await res.text();
    const clone = new Response(bodyText, {
      status: res.status,
      statusText: res.statusText,
      headers: res.headers,
    });

    if (res.status !== 503) {
      throw await parseApiError(clone, bodyText);
    }

    const err = await parseApiError(clone, bodyText);
    if (err.code !== API_ERROR_CPU_BUSY) {
      throw err;
    }

    if (cpuBusyRound >= MAX_CPU_BUSY_ROUNDS) {
      throw err;
    }

    let waitMs =
      err.retryAfterMs != null && err.retryAfterMs > 0
        ? Math.min(err.retryAfterMs, MAX_CPU_BUSY_PER_WAIT_MS)
        : CPU_BUSY_DEFAULT_BACKOFF_MS;

    const remaining = MAX_CPU_BUSY_TOTAL_WAIT_MS - totalWaitedMs;
    if (remaining <= 0) {
      throw err;
    }
    waitMs = Math.min(waitMs, remaining);
    if (waitMs <= 0) {
      throw err;
    }

    options?.onCpuBusyWait?.({
      waitMs,
      attempt: cpuBusyRound + 1,
    });

    await sleepMs(waitMs);
    totalWaitedMs += waitMs;
    cpuBusyRound += 1;
  }
}

/** Format any thrown value for user display; adds retry hint for server errors. */
export function formatApiError(e: unknown): string {
  const message = e instanceof Error ? e.message : String(e);
  if (e instanceof ApiError && e.code === API_ERROR_CPU_BUSY) {
    const capDisplaySec = 300;
    if (
      e.retryAfterMs != null &&
      Number.isFinite(e.retryAfterMs) &&
      e.retryAfterMs > 0
    ) {
      const displayMs = Math.min(e.retryAfterMs, capDisplaySec * 1000);
      const sec = Math.max(1, Math.round(displayMs / 1000));
      const suffix =
        e.retryAfterMs > capDisplaySec * 1000
          ? ` Try again in about ${sec}s (server suggested a longer wait).`
          : ` Try again in about ${sec}s.`;
      return `${message}${suffix}`;
    }
    return `${message} The server is busy with another simulation or optimization; try again in a few seconds.`;
  }
  if (e instanceof ApiError && e.code === "SERVER_ERROR") {
    return `${message} Try again later.`;
  }
  return message;
}

async function checkOk(res: Response): Promise<void> {
  if (res.ok) return;
  const text = await res.text();
  throw await parseApiError(res, text);
}

/** Build headers with X-Profile-Id when profileId is provided. */
function profileHeaders(profileId?: string | null): Record<string, string> {
  if (!profileId) return {};
  return { "X-Profile-Id": profileId };
}

export async function fetchProfiles(): Promise<ProfilesResponse> {
  const res = await fetch(`${API_BASE}/api/profiles`);
  await checkOk(res);
  return res.json();
}

export async function createProfile(params: {
  id?: string;
  name: string;
}): Promise<ProfileEntry> {
  const res = await fetch(`${API_BASE}/api/profiles`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(params),
  });
  await checkOk(res);
  return res.json();
}

export async function deleteProfile(id: string): Promise<void> {
  const res = await fetch(
    `${API_BASE}/api/profiles/${encodeURIComponent(id)}`,
    {
      method: "DELETE",
    },
  );
  await checkOk(res);
}

/** Download a zip of the entire `profiles/` tree (backup). */
export async function exportProfilesBackup(): Promise<Blob> {
  const res = await fetch(`${API_BASE}/api/profiles/export`);
  await checkOk(res);
  return res.blob();
}

/** Replace `profiles/` on the server with the contents of a Kobayashi export zip. Destructive. */
export async function importProfilesBackup(zip: Blob): Promise<void> {
  const res = await fetch(`${API_BASE}/api/profiles/import`, {
    method: "POST",
    headers: { "Content-Type": "application/zip" },
    body: zip,
  });
  await checkOk(res);
}

export async function getShipTiersLevels(
  shipId: string,
): Promise<ShipTiersLevels> {
  const res = await fetch(
    `${API_BASE}/api/ships/${encodeURIComponent(shipId)}/tiers-levels`,
  );
  await checkOk(res);
  return res.json();
}

/** Primary sort key for hostiles (display name when present). */
export function hostileSortLabel(h: HostileListItem): string {
  return h.display_name ?? h.hostile_name;
}

function compareHostiles(a: HostileListItem, b: HostileListItem): number {
  const byName = hostileSortLabel(a).localeCompare(
    hostileSortLabel(b),
    undefined,
    { sensitivity: "base" },
  );
  if (byName !== 0) return byName;
  if (a.level !== b.level) return a.level - b.level;
  return a.id.localeCompare(b.id);
}

export async function fetchOfficers(
  ownedOnly = false,
  profileId?: string | null,
): Promise<OfficerListItem[]> {
  const url = ownedOnly
    ? `${API_BASE}/api/officers?owned_only=1`
    : `${API_BASE}/api/officers`;
  const res = await fetch(url, { headers: profileHeaders(profileId) });
  await checkOk(res);
  const data = await res.json();
  return data.officers ?? [];
}

function compareShips(a: ShipListItem, b: ShipListItem): number {
  const byName = a.ship_name.localeCompare(b.ship_name, undefined, {
    sensitivity: "base",
  });
  if (byName !== 0) return byName;
  return a.id.localeCompare(b.id);
}

export async function fetchShips(
  ownedOnly = false,
  profileId?: string | null,
): Promise<ShipListItem[]> {
  const url = ownedOnly
    ? `${API_BASE}/api/ships?owned_only=1`
    : `${API_BASE}/api/ships`;
  const res = await fetch(url, { headers: profileHeaders(profileId) });
  await checkOk(res);
  const data = await res.json();
  const list = (data.ships ?? []) as ShipListItem[];
  return [...list].sort(compareShips);
}

export async function fetchHostiles(): Promise<HostileListItem[]> {
  const res = await fetch(`${API_BASE}/api/hostiles`);
  await checkOk(res);
  const data = await res.json();
  const list = (data.hostiles ?? []) as HostileListItem[];
  return [...list].sort(compareHostiles);
}

export async function fetchDataVersion(): Promise<DataVersionResponse> {
  const res = await fetch(`${API_BASE}/api/data/version`);
  await checkOk(res);
  return res.json();
}

export async function simulate(
  params: {
    ship: string;
    hostile: string;
    crew: SimulateCrew;
    num_sims?: number;
    ship_tier?: number | null;
    ship_level?: number | null;
    support_buffs?: string[];
  },
  profileId?: string | null,
): Promise<SimulateResponse> {
  const body: Record<string, unknown> = {
    ship: params.ship,
    hostile: params.hostile,
    crew: params.crew,
    num_sims: params.num_sims ?? 5000,
  };
  if (params.ship_tier != null && params.ship_tier > 0) {
    body.ship_tier = params.ship_tier;
  }
  if (params.ship_level != null && params.ship_level > 0) {
    body.ship_level = params.ship_level;
  }
  if (params.support_buffs && params.support_buffs.length > 0) {
    body.support_buffs = params.support_buffs;
  }
  const res = await fetchWithCpuBusyRetries(`${API_BASE}/api/simulate`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...profileHeaders(profileId),
    },
    body: JSON.stringify(body),
  });
  return res.json();
}

/** Map optimize recommendation row to simulate/compare crew JSON (officer names). */
export function crewRecommendationToSimulateCrew(
  r: CrewRecommendation,
  belowDecksSlots: number,
): {
  captain: string;
  bridge: (string | null)[];
  below_deck: (string | null)[];
} {
  const br = Array.isArray(r.bridge) ? r.bridge : [r.bridge];
  const bdRaw = Array.isArray(r.below_decks) ? r.below_decks : [r.below_decks];
  const bridge: (string | null)[] = [br[0] ?? null, br[1] ?? null];
  const below_deck = Array.from({ length: belowDecksSlots }, (_, i) => {
    const v = bdRaw[i];
    return v != null && String(v).trim() !== "" ? String(v) : null;
  });
  return { captain: r.captain, bridge, below_deck };
}

export interface CompareCrewDistribution {
  captain: string;
  trials: number;
  wins: number;
  stalls: number;
  losses: number;
  rounds_histogram: [number, number][];
  hull_remaining_bins: number[];
  proc_rates?: Record<string, number>;
}

export interface CompareCrewsResponse {
  status: string;
  seed: number;
  crews: CompareCrewDistribution[];
  using_placeholder_combatants: boolean;
  warnings?: string[];
}

export async function compareCrewsDistributions(
  params: {
    ship: string;
    hostile: string;
    crews: {
      captain: string;
      bridge: (string | null)[];
      below_deck: (string | null)[];
    }[];
    num_sims?: number;
    seed?: number;
    ship_tier?: number | null;
    ship_level?: number | null;
    below_decks_slots?: number | null;
    proc_sample_trials?: number;
    /** Same ids as simulate/optimize (`data/support_buffs.json`). */
    support_buffs?: string[];
  },
  profileId?: string | null,
): Promise<CompareCrewsResponse> {
  const body: Record<string, unknown> = {
    ship: params.ship,
    hostile: params.hostile,
    crews: params.crews,
    num_sims: params.num_sims ?? 3000,
    seed: params.seed ?? 0,
  };
  if (params.ship_tier != null && params.ship_tier > 0)
    body.ship_tier = params.ship_tier;
  if (params.ship_level != null && params.ship_level > 0)
    body.ship_level = params.ship_level;
  if (params.below_decks_slots != null && params.below_decks_slots >= 0) {
    body.below_decks_slots = params.below_decks_slots;
  }
  if (params.proc_sample_trials != null && params.proc_sample_trials > 0) {
    body.proc_sample_trials = params.proc_sample_trials;
  }
  if (params.support_buffs && params.support_buffs.length > 0) {
    body.support_buffs = params.support_buffs;
  }
  const res = await fetchWithCpuBusyRetries(`${API_BASE}/api/compare/crews`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...profileHeaders(profileId),
    },
    body: JSON.stringify(body),
  });
  return res.json();
}

/** Chain grind Monte Carlo summary (POST /api optimize when `chain.enabled`). */
export interface ChainSimulationSummary {
  kills_target: number;
  secondary_objective: "min_hull_damage" | "max_loot_per_hull_proxy";
  primary_success_rate: number;
  primary_ci_low: number;
  primary_ci_high: number;
  secondary_mean_given_primary: number;
  secondary_ci_low: number;
  secondary_ci_high: number;
  n_primary_successes: number;
}

export interface CrewRecommendation {
  captain: string;
  /** API returns string[]; we accept string for backward compatibility. */
  bridge: string | string[];
  /** API returns string[]; we accept string for backward compatibility. */
  below_decks: string | string[];
  win_rate: number;
  win_rate_ci_low: number;
  win_rate_ci_high: number;
  stall_rate: number;
  stall_rate_ci_low: number;
  stall_rate_ci_high: number;
  loss_rate: number;
  loss_rate_ci_low: number;
  loss_rate_ci_high: number;
  /** Win on round 1 (not a round-limit stall). */
  r1_kill_rate: number;
  r1_kill_rate_ci_low: number;
  r1_kill_rate_ci_high: number;
  avg_hull_remaining: number;
  avg_hull_remaining_ci_low: number;
  avg_hull_remaining_ci_high: number;
  avg_defender_hull_remaining: number;
  avg_defender_hull_remaining_ci_low: number;
  avg_defender_hull_remaining_ci_high: number;
  /** Present when optimization used chain grind mode. */
  chain?: ChainSimulationSummary;
  /** Closed-form expected hull damage when strategy was linear_eval. */
  expected_hull_damage?: number;
}

export interface ChainGrindRequestBody {
  enabled: boolean;
  kills_target?: number;
  secondary?: "min_hull_damage" | "max_loot_per_hull_proxy";
}

/** Matches server `WarmStartCrewDto`; sent as `warm_start_crews` on optimize. */
export type WarmStartCrewBody = {
  captain: string;
  bridge: string[];
  below_decks: string[];
};

export interface OptimizeResponse {
  status: string;
  engine?: string;
  scenario: {
    ship: string;
    hostile: string;
    sims: number;
    seed: number;
    /** Resolved below-decks slot count used for candidate generation. */
    below_decks_slots: number;
    /** Present when optimize constraints were applied (counts only). */
    optimize_constraints?: {
      must_include: number;
      exclude: number;
      groups: number;
      captain_must_be: boolean;
      bridge_must_include: number;
      below_decks_must_include: number;
    };
    analytical_prefilter_keep?: number;
    analytical_prefilter_from?: number;
    analytical_prefilter_kept?: number;
    /** Echo of optimize request chain settings when present. */
    chain?: ChainGrindRequestBody;
    effective_strategy: string;
    strategy_auto: boolean;
    requested_strategy?: string | null;
    /** Maximal marginal relevance blend when client requested novelty-aware ordering. */
    novelty_lambda?: number | null;
    novelty_diverse_top?: number | null;
    novelty_pool?: number | null;
    /** True when the server merged heuristic seeds into warm-start (fast discovery pipeline). */
    fast_discovery?: boolean | null;
    /** Tiered: crews that reused persisted confirmation stats from profile `optimize_history.json`. */
    optimize_history_confirm_hits?: number | null;
    /** True when the server wrote an entry to `optimize_history.json` for this run. */
    optimize_history_wrote?: boolean | null;
  };
  recommendations: CrewRecommendation[];
  duration_ms?: number;
  notes?: string[];
  approximate_notes?: string[];
  warnings?: string[];
}

export async function getOptimizeEstimate(
  params: {
    ship: string;
    hostile: string;
    sims?: number;
    max_candidates?: number | null;
    below_decks_pool_mode?: import("./optimizeWarmStart").BelowDecksPoolMode;
    ship_tier?: number | null;
    ship_level?: number | null;
    below_decks_slots?: number | null;
  },
  profileId?: string | null,
): Promise<OptimizeEstimate> {
  const sims = params.sims ?? 5000;
  const search = new URLSearchParams({
    ship: params.ship,
    hostile: params.hostile,
    sims: String(sims),
  });
  if (params.max_candidates != null && params.max_candidates > 0) {
    search.set("max_candidates", String(params.max_candidates));
  }
  if (
    params.below_decks_pool_mode &&
    params.below_decks_pool_mode !== "strict"
  ) {
    search.set("below_decks_pool_mode", params.below_decks_pool_mode);
  }
  if (params.ship_tier != null && params.ship_tier > 0) {
    search.set("ship_tier", String(params.ship_tier));
  }
  if (params.ship_level != null && params.ship_level > 0) {
    search.set("ship_level", String(params.ship_level));
  }
  if (params.below_decks_slots != null && params.below_decks_slots >= 0) {
    search.set("below_decks_slots", String(params.below_decks_slots));
  }
  if (profileId) search.set("profile", profileId);
  const url = `${API_BASE}/api/optimize/estimate?${search.toString()}`;
  const res = await fetch(url);
  await checkOk(res);
  return res.json();
}

export async function optimize(
  params: {
    ship: string;
    hostile: string;
    sims?: number;
    seed?: number;
    max_candidates?: number | null;
    ship_tier?: number | null;
    ship_level?: number | null;
    below_decks_slots?: number | null;
  },
  profileId?: string | null,
): Promise<OptimizeResponse> {
  const body: Record<string, unknown> = {
    ship: params.ship,
    hostile: params.hostile,
    sims: params.sims ?? 5000,
    seed: params.seed,
  };
  if (params.max_candidates != null && params.max_candidates > 0) {
    body.max_candidates = params.max_candidates;
  }
  if (params.ship_tier != null && params.ship_tier > 0) {
    body.ship_tier = params.ship_tier;
  }
  if (params.ship_level != null && params.ship_level > 0) {
    body.ship_level = params.ship_level;
  }
  if (params.below_decks_slots != null && params.below_decks_slots >= 0) {
    body.below_decks_slots = params.below_decks_slots;
  }
  const res = await fetchWithCpuBusyRetries(`${API_BASE}/api/optimize`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...profileHeaders(profileId),
    },
    body: JSON.stringify(body),
  });
  return res.json();
}

export interface OptimizeStatusResponse {
  status: string;
  progress?: number;
  crews_done?: number;
  total_crews?: number;
  /** Server phase: heuristics, monte_carlo, genetic, tiered_scout, tiered_confirm */
  phase?: string;
  throughput_crews_per_sec?: number;
  eta_seconds?: number;
  progress_preview?: CrewRecommendation[];
  result?: OptimizeResponse;
  error?: string;
}

/** Human-readable label for optimize job `phase` (status / SSE). */
export function formatOptimizePhaseLabel(
  phase: string | null | undefined,
): string {
  if (!phase) return "";
  const map: Record<string, string> = {
    heuristics: "Heuristics",
    monte_carlo: "Monte Carlo",
    genetic: "Genetic search",
    tiered_scout: "Tiered (scout)",
    tiered_confirm: "Tiered (confirm)",
    linear_eval: "Linear eval",
  };
  return map[phase] ?? phase.replace(/_/g, " ");
}

export async function fetchHeuristics(): Promise<string[]> {
  const res = await fetch(`${API_BASE}/api/heuristics`);
  await checkOk(res);
  const data = await res.json();
  return data.seeds ?? [];
}

export type OptimizerStrategyType =
  | "exhaustive"
  | "genetic"
  | "tiered"
  | "linear_eval";

/** Optional hooks for `optimizeStart` (CPU admission may queue the request). */
export type OptimizeStartOptions = {
  onCpuBusyWait?: (info: CpuBusyWaitInfo) => void;
};

/** Sub-object for POST /api/optimize/start `constraints` (matches server OptimizeConstraintsDto). */
export interface OptimizeCrewConstraintsBody {
  must_include?: string[];
  exclude?: string[];
  groups?: { officers: string[]; min_count: number }[];
  captain_must_be?: string;
  bridge_must_include?: string[];
  below_decks_must_include?: string[];
}

export async function optimizeStart(
  params: {
    ship: string;
    hostile: string;
    sims?: number;
    seed?: number;
    max_candidates?: number | null;
    strategy?: OptimizerStrategyType;
    below_decks_pool_mode?: import("./optimizeWarmStart").BelowDecksPoolMode;
    heuristics_seeds?: string[];
    heuristics_only?: boolean;
    below_decks_strategy?: "ordered" | "exploration";
    ship_tier?: number | null;
    ship_level?: number | null;
    below_decks_slots?: number | null;
    constraints?: OptimizeCrewConstraintsBody;
    support_buffs?: string[];
    chain?: ChainGrindRequestBody;
    warm_start_crews?: WarmStartCrewBody[];
    tiered_scout_sims?: number;
    tiered_top_k?: number;
    fast_discovery?: boolean;
    enable_learned_pair_prior?: boolean;
    novelty_lambda?: number;
    novelty_diverse_top?: number;
    novelty_pool?: number;
  },
  profileId?: string | null,
  options?: OptimizeStartOptions,
): Promise<OptimizeStartResponse> {
  const body: Record<string, unknown> = {
    ship: params.ship,
    hostile: params.hostile,
    sims: params.sims ?? 5000,
    seed: params.seed,
  };
  if (params.max_candidates != null && params.max_candidates > 0) {
    body.max_candidates = params.max_candidates;
  }
  if (params.strategy && params.strategy !== "exhaustive") {
    body.strategy = params.strategy;
  }
  if (
    params.below_decks_pool_mode &&
    params.below_decks_pool_mode !== "strict"
  ) {
    body.below_decks_pool_mode = params.below_decks_pool_mode;
  }
  if (params.heuristics_seeds && params.heuristics_seeds.length > 0) {
    body.heuristics_seeds = params.heuristics_seeds;
  }
  if (params.heuristics_only === true) {
    body.heuristics_only = true;
  }
  if (
    params.below_decks_strategy &&
    params.below_decks_strategy !== "ordered"
  ) {
    body.below_decks_strategy = params.below_decks_strategy;
  }
  if (params.ship_tier != null && params.ship_tier > 0) {
    body.ship_tier = params.ship_tier;
  }
  if (params.ship_level != null && params.ship_level > 0) {
    body.ship_level = params.ship_level;
  }
  if (params.below_decks_slots != null && params.below_decks_slots >= 0) {
    body.below_decks_slots = params.below_decks_slots;
  }
  if (params.constraints && Object.keys(params.constraints).length > 0) {
    body.constraints = params.constraints;
  }
  if (params.support_buffs && params.support_buffs.length > 0) {
    body.support_buffs = params.support_buffs;
  }
  if (params.chain?.enabled) {
    body.chain = {
      enabled: true,
      kills_target: params.chain.kills_target,
      ...(params.chain.secondary && params.chain.secondary !== "min_hull_damage"
        ? { secondary: params.chain.secondary }
        : {}),
    };
  }
  if (params.warm_start_crews && params.warm_start_crews.length > 0) {
    body.warm_start_crews = params.warm_start_crews;
  }
  if (params.tiered_scout_sims != null && params.tiered_scout_sims > 0) {
    body.tiered_scout_sims = params.tiered_scout_sims;
  }
  if (params.tiered_top_k != null && params.tiered_top_k > 0) {
    body.tiered_top_k = params.tiered_top_k;
  }
  if (params.fast_discovery === true) {
    body.fast_discovery = true;
  }
  if (params.enable_learned_pair_prior === false) {
    body.enable_learned_pair_prior = false;
  }
  if (
    params.novelty_lambda != null &&
    params.novelty_lambda > 0 &&
    params.novelty_lambda <= 1
  ) {
    body.novelty_lambda = params.novelty_lambda;
  }
  if (params.novelty_diverse_top != null && params.novelty_diverse_top >= 1) {
    body.novelty_diverse_top = params.novelty_diverse_top;
  }
  if (params.novelty_pool != null && params.novelty_pool >= 2) {
    body.novelty_pool = params.novelty_pool;
  }
  const res = await fetchWithCpuBusyRetries(
    `${API_BASE}/api/optimize/start`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...profileHeaders(profileId),
      },
      body: JSON.stringify(body),
    },
    { onCpuBusyWait: options?.onCpuBusyWait },
  );
  return res.json();
}

const OPTIMIZE_STATUS_POLL_MAX_ATTEMPTS = 5;
const OPTIMIZE_STATUS_POLL_BASE_MS = 300;
const OPTIMIZE_STATUS_POLL_CAP_MS = 3_000;

/** Transient HTTP errors while polling job status (long-running optimize). */
function optimizeStatusFailureIsRetriable(err: ApiError): boolean {
  return err.status === 502 || err.status === 503 || err.status === 504;
}

/** Poll async optimize job status. Jobs are keyed only by `job_id` (profile affects the start request body/headers, not this URL). */
export async function getOptimizeStatus(
  jobId: string,
): Promise<OptimizeStatusResponse> {
  const url = `${API_BASE}/api/optimize/status/${encodeURIComponent(jobId)}`;
  let lastErr: unknown;

  for (
    let attempt = 0;
    attempt < OPTIMIZE_STATUS_POLL_MAX_ATTEMPTS;
    attempt++
  ) {
    try {
      const res = await fetch(url);
      if (res.ok) {
        return (await res.json()) as OptimizeStatusResponse;
      }
      const text = await res.text();
      const err = await parseApiError(
        new Response(text, {
          status: res.status,
          statusText: res.statusText,
          headers: res.headers,
        }),
        text,
      );
      if (err.status === 404) {
        throw err;
      }
      if (
        optimizeStatusFailureIsRetriable(err) &&
        attempt < OPTIMIZE_STATUS_POLL_MAX_ATTEMPTS - 1
      ) {
        lastErr = err;
        await sleepMs(
          Math.min(
            OPTIMIZE_STATUS_POLL_CAP_MS,
            OPTIMIZE_STATUS_POLL_BASE_MS * 2 ** attempt,
          ),
        );
        continue;
      }
      throw err;
    } catch (e) {
      if (e instanceof ApiError) {
        throw e;
      }
      if (attempt < OPTIMIZE_STATUS_POLL_MAX_ATTEMPTS - 1) {
        lastErr = e;
        await sleepMs(
          Math.min(
            OPTIMIZE_STATUS_POLL_CAP_MS,
            OPTIMIZE_STATUS_POLL_BASE_MS * 2 ** attempt,
          ),
        );
        continue;
      }
      throw e;
    }
  }

  throw lastErr instanceof Error
    ? lastErr
    : new Error("getOptimizeStatus: exhausted retries");
}

/** URL for SSE stream of optimize job progress (GET). Use with EventSource for live updates. */
export function getOptimizeStreamUrl(jobId: string): string {
  return `${API_BASE}/api/optimize/jobs/${encodeURIComponent(jobId)}/stream`;
}

/** Request cancellation of a running optimize job. */
export async function cancelOptimizeJob(jobId: string): Promise<void> {
  const res = await fetch(
    `${API_BASE}/api/optimize/jobs/${encodeURIComponent(jobId)}/cancel`,
    {
      method: "POST",
    },
  );
  await checkOk(res);
}

export interface ImportUnresolvedRow {
  record_index: number;
  input_name: string;
  normalized_name?: string;
  reason: string;
  suggested_matches?: string[];
  hint?: string | null;
}

export interface RosterImportDiagnostic {
  record_index: number;
  input_name: string;
  severity: string;
  code: string;
  message: string;
  hint?: string | null;
}

export interface ImportReport {
  source_path: string;
  output_path: string;
  total_records: number;
  matched_records: number;
  unmatched_records: number;
  ambiguous_records?: number;
  duplicate_records?: number;
  conflict_records?: number;
  critical_failures?: number;
  roster_entries_written: number;
  unresolved?: ImportUnresolvedRow[];
  duplicates?: { canonical_officer_id: string; record_indices: number[] }[];
  conflicts?: unknown[];
  diagnostics?: RosterImportDiagnostic[];
}

export async function importRoster(
  body: string,
  profileId?: string | null,
): Promise<ImportReport> {
  const res = await fetch(`${API_BASE}/api/officers/import`, {
    method: "POST",
    headers: { "Content-Type": "text/plain", ...profileHeaders(profileId) },
    body: body.trim(),
  });
  await checkOk(res);
  return res.json();
}

export interface ForbiddenTechBonusEntry {
  stat: string;
  value: number;
  operator?: string;
}

export interface ForbiddenTechCatalogItem {
  fid?: number | null;
  name: string;
  tech_type?: string;
  tier?: number | null;
  bonuses: ForbiddenTechBonusEntry[];
}

export interface ForbiddenTechCatalogResponse {
  items: ForbiddenTechCatalogItem[];
}

export async function fetchForbiddenTech(): Promise<
  ForbiddenTechCatalogItem[]
> {
  const res = await fetch(`${API_BASE}/api/forbidden-tech`);
  await checkOk(res);
  const data: ForbiddenTechCatalogResponse = await res.json();
  return data.items ?? [];
}

/** Rows from `profiles/.../forbidden_tech.imported.json` (mod sync inventory). */
export interface ForbiddenTechImportedEntry {
  fid: number;
  tier: number;
  level: number;
  shard_count: number;
}

export interface ForbiddenTechImportedResponse {
  profile_id: string;
  forbidden_tech: ForbiddenTechImportedEntry[];
}

export async function fetchForbiddenTechImported(
  profileId?: string | null,
): Promise<ForbiddenTechImportedResponse> {
  const q = profileId ? `?profile=${encodeURIComponent(profileId)}` : "";
  const res = await fetch(
    `${API_BASE}/api/profile/forbidden-tech-imported${q}`,
    { headers: { ...profileHeaders(profileId) } },
  );
  await checkOk(res);
  return res.json();
}

export interface PlayerProfile {
  bonuses: Record<string, number>;
  /** @deprecated Legacy field; ignored for combat — use equipped_* slots. */
  forbidden_tech_override?: number[] | null;
  /** @deprecated Legacy field; ignored for combat — use equipped_* slots. */
  chaos_tech_override?: number[] | null;
  /** STFC forbidden-tech slot (one fid or empty). Omitted/null = no forbidden tech bonuses. */
  equipped_forbidden_fid?: number | null;
  /** STFC chaos-tech slot (one fid or empty). Omitted/null = no chaos tech bonuses. */
  equipped_chaos_fid?: number | null;
}

/** Community mod persist timestamp (RFC3339) from `profiles/{id}/last_mod_sync.json`; null if never synced via mod. */
export interface ModSyncStatus {
  profile_id: string;
  last_mod_sync_utc: string | null;
}

export async function fetchModSyncStatus(
  profileId?: string | null,
): Promise<ModSyncStatus> {
  const url = profileId
    ? `${API_BASE}/api/sync/status?profile=${encodeURIComponent(profileId)}`
    : `${API_BASE}/api/sync/status`;
  const res = await fetch(url);
  await checkOk(res);
  const data = (await res.json()) as Record<string, unknown>;
  return {
    profile_id: typeof data.profile_id === "string" ? data.profile_id : "",
    last_mod_sync_utc:
      typeof data.last_mod_sync_utc === "string"
        ? data.last_mod_sync_utc
        : null,
  };
}

export async function fetchProfile(
  profileId?: string | null,
): Promise<PlayerProfile> {
  const url = profileId
    ? `${API_BASE}/api/profile?profile=${encodeURIComponent(profileId)}`
    : `${API_BASE}/api/profile`;
  const res = await fetch(url);
  await checkOk(res);
  return res.json();
}

export async function updateProfile(
  profile: PlayerProfile,
  profileId?: string | null,
): Promise<void> {
  const res = await fetch(`${API_BASE}/api/profile`, {
    method: "PUT",
    headers: {
      "Content-Type": "application/json",
      ...profileHeaders(profileId),
    },
    body: JSON.stringify(profile),
  });
  await checkOk(res);
}

export interface BuildingSummaryRow {
  bid: number;
  level: number;
  kobayashi_building_id?: string | null;
  building_name?: string | null;
  catalog_record_present: boolean;
}

/** Synced starbase modules → effective ship-combat bonuses from buildings only. */
export interface BuildingCombatSummary {
  profile_id: string;
  error?: string | null;
  ops_level_profile_override?: number | null;
  ops_level_inferred_from_sync?: number | null;
  ops_level_effective?: number | null;
  synced_building_count: number;
  buildings: BuildingSummaryRow[];
  unmapped_bids: number[];
  combat_bonuses_from_buildings?: Record<string, number>;
}

export async function fetchBuildingCombatSummary(
  profileId?: string | null,
): Promise<BuildingCombatSummary> {
  const q = profileId ? `?profile=${encodeURIComponent(profileId)}` : "";
  const res = await fetch(`${API_BASE}/api/profile/buildings-summary${q}`, {
    headers: { ...profileHeaders(profileId) },
  });
  await checkOk(res);
  return res.json();
}

export interface ResearchConditionalBonusLine {
  stat: string;
  value: number;
  requires_runtime_state: boolean;
  condition_label?: string | null;
  defender_ship_class?: string | null;
  defender_faction?: string | null;
  attacker_faction?: string | null;
  attacker_factions?: string[];
  requires_morale?: boolean;
  requires_defender_burning?: boolean;
  requires_defender_hull_breach?: boolean;
}

export interface UnmappedResearchEntry {
  rid: number;
  level: number;
}

export interface ResearchSummaryScenarioContext {
  ship_id: string;
  hostile_id: string;
  ship_faction?: string | null;
  defender_faction: string;
  defender_ship_class: string;
}

export interface ResearchSummaryRow {
  rid: number;
  level: number;
  research_name?: string | null;
  catalog_record_present: boolean;
  /** unmapped | non_combat | flat | owner_faction | conditional | mixed | support_buff_gated */
  combat_kind: string;
  combat_bonuses_from_row?: Record<string, number>;
  /** Owner-hull faction → stat → value for this synced row (e.g. Modulated Federation). */
  combat_owner_faction_bonuses_from_row?: Record<
    string,
    Record<string, number>
  >;
  combat_conditional_bonuses_from_row?: ResearchConditionalBonusLine[];
}

/** Synced research → effective ship-combat bonuses from research only (same rules as simulate/optimize). */
export interface ResearchCombatSummary {
  profile_id: string;
  error?: string | null;
  synced_research_count: number;
  unmapped_rids: number[];
  unmapped_research?: UnmappedResearchEntry[];
  combat_bonuses_from_research?: Record<string, number>;
  /** Cumulative owner-faction-gated research (faction slug → stat → value). */
  combat_owner_faction_bonuses_from_research?: Record<
    string,
    Record<string, number>
  >;
  combat_conditional_bonuses_from_research?: ResearchConditionalBonusLine[];
  scenario_context?: ResearchSummaryScenarioContext | null;
  combat_bonuses_scenario_effective?: Record<string, number>;
  combat_conditional_scenario_active?: ResearchConditionalBonusLine[];
  research: ResearchSummaryRow[];
}

function researchSummaryQuery(
  profileId?: string | null,
  shipId?: string | null,
  hostileId?: string | null,
): string {
  const params = new URLSearchParams();
  if (profileId) params.set("profile", profileId);
  if (shipId?.trim()) params.set("ship_id", shipId.trim());
  if (hostileId?.trim()) params.set("hostile_id", hostileId.trim());
  const q = params.toString();
  return q ? `?${q}` : "";
}

export async function fetchResearchCombatSummary(
  profileId?: string | null,
  options?: { shipId?: string | null; hostileId?: string | null },
): Promise<ResearchCombatSummary> {
  const q = researchSummaryQuery(
    profileId,
    options?.shipId,
    options?.hostileId,
  );
  const res = await fetch(`${API_BASE}/api/profile/research-summary${q}`, {
    headers: { ...profileHeaders(profileId) },
  });
  await checkOk(res);
  return res.json();
}

export interface PresetCrew {
  captain?: string | null;
  bridge?: (string | null)[];
  below_deck?: (string | null)[];
}

/** Embedded when saving a preset. */
export interface PresetProvenance {
  saved_at: string;
  kobayashi_version: string;
  hostile_data_version?: string | null;
  ship_data_version?: string | null;
  source?: string | null;
}

export interface Preset {
  id: string;
  name: string;
  ship: string;
  scenario: string;
  crew: PresetCrew;
  schema_version: number;
  provenance: PresetProvenance;
}

export interface PresetSummary {
  id: string;
  name: string;
  ship: string;
  scenario: string;
  schema_version: number;
}

export async function fetchPresets(
  profileId?: string | null,
): Promise<PresetSummary[]> {
  const url = profileId
    ? `${API_BASE}/api/presets?profile=${encodeURIComponent(profileId)}`
    : `${API_BASE}/api/presets`;
  const res = await fetch(url);
  await checkOk(res);
  const data = await res.json();
  return data.presets ?? [];
}

export async function fetchPreset(
  id: string,
  profileId?: string | null,
): Promise<Preset> {
  const url = profileId
    ? `${API_BASE}/api/presets/${encodeURIComponent(id)}?profile=${encodeURIComponent(profileId)}`
    : `${API_BASE}/api/presets/${encodeURIComponent(id)}`;
  const res = await fetch(url);
  await checkOk(res);
  return res.json();
}

export async function savePreset(
  preset: {
    name?: string;
    ship: string;
    scenario: string;
    crew: PresetCrew;
  },
  profileId?: string | null,
): Promise<Preset> {
  const c = preset.crew;
  const bridge = (c.bridge ?? []).filter((x): x is string => x != null);
  const below_deck = (c.below_deck ?? []).filter((x): x is string => x != null);
  const crewBody: Record<string, unknown> = {};
  if (c.captain != null && c.captain !== "") crewBody.captain = c.captain;
  if (bridge.length > 0) crewBody.bridge = bridge;
  if (below_deck.length > 0) crewBody.below_deck = below_deck;

  const res = await fetch(`${API_BASE}/api/presets`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...profileHeaders(profileId),
    },
    body: JSON.stringify({
      name: preset.name ?? "Unnamed",
      ship: preset.ship,
      scenario: preset.scenario,
      crew: crewBody,
    }),
  });
  await checkOk(res);
  return res.json();
}
