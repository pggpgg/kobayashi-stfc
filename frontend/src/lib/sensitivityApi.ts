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
