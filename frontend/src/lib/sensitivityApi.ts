import { API_BASE, parseApiError } from "./api";

export type OutcomeMetric =
  | "hull_remaining"
  | "win_rate"
  | "rounds_to_kill"
  | "defender_hull_remaining";

export interface SensitivityRequest {
  ship: string;
  hostile: string;
  ship_tier?: number;
  ship_level?: number;
  captain?: string;
  bridge: string[];
  below_decks?: string[];
  support_buffs?: string[];
  profile_id?: string;
  num_sims?: number;
  seed?: number;
  rounds?: number;
  metric?: OutcomeMetric;
  /** Per-stat δ overrides keyed by stat name. A `0` value skips the stat. */
  deltas?: Record<string, number>;
}

export interface SensitivityRow {
  stat: string;
  delta_applied: number;
  mean_diff: number;
  mean_diff_relative: number | null;
  ci95_low: number;
  ci95_high: number;
  significant: boolean;
}

export interface SensitivityResponse {
  metric: string;
  baseline_mean: number;
  num_sims: number;
  base_seed: number;
  rows: SensitivityRow[];
}

export interface SensitivityDefaultRow {
  stat: string;
  delta: number;
  multiplicative: boolean;
}

export interface SensitivityDefaultsResponse {
  deltas: SensitivityDefaultRow[];
}

function profileHeaders(profileId?: string | null): Record<string, string> {
  const h: Record<string, string> = { "Content-Type": "application/json" };
  if (profileId) h["X-Profile-Id"] = profileId;
  return h;
}

export async function fetchSensitivityDefaults(): Promise<SensitivityDefaultsResponse> {
  const res = await fetch(`${API_BASE}/api/sensitivity/defaults`);
  if (!res.ok) {
    const body = await res.text();
    throw await parseApiError(res, body);
  }
  return (await res.json()) as SensitivityDefaultsResponse;
}

export async function runSensitivity(
  request: SensitivityRequest,
  profileId?: string | null,
): Promise<SensitivityResponse> {
  const res = await fetch(`${API_BASE}/api/sensitivity`, {
    method: "POST",
    headers: profileHeaders(profileId),
    body: JSON.stringify(request),
  });
  if (!res.ok) {
    const body = await res.text();
    throw await parseApiError(res, body);
  }
  return (await res.json()) as SensitivityResponse;
}

// ---------------------------------------------------------------------------
// Morris-method screening (`POST /api/sensitivity/morris`)
// ---------------------------------------------------------------------------

export interface MorrisRequest {
  ship: string;
  hostile: string;
  ship_tier?: number;
  ship_level?: number;
  captain?: string;
  bridge: string[];
  below_decks?: string[];
  support_buffs?: string[];
  profile_id?: string;
  num_sims?: number;
  r_trajectories?: number;
  seed?: number;
  rounds?: number;
  metric?: OutcomeMetric;
  deltas?: Record<string, number>;
}

export interface MorrisRow {
  stat: string;
  delta_applied: number;
  /** μ* — mean of |EE| across trajectories. Importance. */
  mu_star: number;
  /** μ — mean signed EE. Direction. */
  mu: number;
  /** σ — std of EE across trajectories. Interaction signal. */
  sigma: number;
  n_samples: number;
  mu_star_ci95_low: number;
  mu_star_ci95_high: number;
}

export interface MorrisResponse {
  metric: string;
  num_sims_per_point: number;
  r_trajectories: number;
  k_stats: number;
  base_seed: number;
  total_sims: number;
  rows: MorrisRow[];
}

export interface MorrisDefaultsResponse {
  deltas: SensitivityDefaultRow[];
  r_trajectories_default: number;
  r_trajectories_max: number;
  num_sims_default: number;
  num_sims_max: number;
}

export async function fetchMorrisDefaults(): Promise<MorrisDefaultsResponse> {
  const res = await fetch(`${API_BASE}/api/sensitivity/morris/defaults`);
  if (!res.ok) {
    const body = await res.text();
    throw await parseApiError(res, body);
  }
  return (await res.json()) as MorrisDefaultsResponse;
}

export async function runMorris(
  request: MorrisRequest,
  profileId?: string | null,
): Promise<MorrisResponse> {
  const res = await fetch(`${API_BASE}/api/sensitivity/morris`, {
    method: "POST",
    headers: profileHeaders(profileId),
    body: JSON.stringify(request),
  });
  if (!res.ok) {
    const body = await res.text();
    throw await parseApiError(res, body);
  }
  return (await res.json()) as MorrisResponse;
}

// ---------------------------------------------------------------------------
// Sobol variance-based sensitivity (`POST /api/sensitivity/sobol`)
// ---------------------------------------------------------------------------

export interface SobolRequest {
  ship: string;
  hostile: string;
  ship_tier?: number;
  ship_level?: number;
  captain?: string;
  bridge: string[];
  below_decks?: string[];
  support_buffs?: string[];
  profile_id?: string;
  n_samples?: number;
  seed?: number;
  rounds?: number;
  metric?: OutcomeMetric;
  deltas?: Record<string, number>;
}

export interface SobolRow {
  stat: string;
  base_delta: number;
  /** First-order Sobol index — main effect alone. */
  s1: number;
  /** Total-order Sobol index — main + all interactions involving this stat. */
  st: number;
  /** S_T_i − S_i: fraction of variance from interactions. */
  interaction: number;
  s1_ci95_low: number;
  s1_ci95_high: number;
  st_ci95_low: number;
  st_ci95_high: number;
}

export interface SobolResponse {
  metric: string;
  n_samples: number;
  k_stats: number;
  base_seed: number;
  total_sims: number;
  output_variance: number;
  rows: SobolRow[];
}

export interface SobolDefaultsResponse {
  deltas: SensitivityDefaultRow[];
  n_samples_default: number;
  n_samples_max: number;
}

export async function fetchSobolDefaults(): Promise<SobolDefaultsResponse> {
  const res = await fetch(`${API_BASE}/api/sensitivity/sobol/defaults`);
  if (!res.ok) {
    const body = await res.text();
    throw await parseApiError(res, body);
  }
  return (await res.json()) as SobolDefaultsResponse;
}

export async function runSobol(
  request: SobolRequest,
  profileId?: string | null,
): Promise<SobolResponse> {
  const res = await fetch(`${API_BASE}/api/sensitivity/sobol`, {
    method: "POST",
    headers: profileHeaders(profileId),
    body: JSON.stringify(request),
  });
  if (!res.ok) {
    const body = await res.text();
    throw await parseApiError(res, body);
  }
  return (await res.json()) as SobolResponse;
}
