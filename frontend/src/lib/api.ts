/**
 * Base URL for API requests. Empty string = same origin.
 * Set at build time via VITE_API_BASE (e.g. for deployment behind a proxy).
 */
export const API_BASE = typeof import.meta !== 'undefined' && import.meta.env?.VITE_API_BASE != null
  ? String(import.meta.env.VITE_API_BASE).replace(/\/$/, '')
  : '';

/** Structured error from the API (status code + server message when available). */
export class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly code: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

function codeFromStatus(status: number): string {
  if (status >= 500) return 'SERVER_ERROR';
  if (status === 404) return 'NOT_FOUND';
  if (status === 400 || status === 422) return 'VALIDATION';
  if (status === 401 || status === 403) return 'AUTH';
  return 'ERROR';
}

/** Parse error response body; returns an ApiError with server message when JSON has status/message. */
export async function parseApiError(res: Response, bodyText: string): Promise<ApiError> {
  let message = bodyText || res.statusText;
  const code = codeFromStatus(res.status);
  try {
    const json = JSON.parse(bodyText) as { message?: string; status?: string };
    if (typeof json.message === 'string' && json.message.trim()) {
      message = json.message.trim();
    }
  } catch {
    // keep message as bodyText or statusText
  }
  return new ApiError(message, res.status, code);
}

/** Format any thrown value for user display; adds retry hint for server errors. */
export function formatApiError(e: unknown): string {
  const message = e instanceof Error ? e.message : String(e);
  if (e instanceof ApiError && e.code === 'SERVER_ERROR') {
    return `${message} Try again later.`;
  }
  return message;
}

async function checkOk(res: Response): Promise<void> {
  if (res.ok) return;
  const text = await res.text();
  throw await parseApiError(res, text);
}

export interface OfficerListItem {
  id: string;
  name: string;
  slot?: string;
}

export interface ShipListItem {
  id: string;
  ship_name: string;
  ship_class: string;
}

export interface HostileListItem {
  id: string;
  hostile_name: string;
  level: number;
  ship_class: string;
}

export interface MechanicStatus {
  name: string;
  status: string;
}

export interface DataVersionResponse {
  officer_version?: string;
  hostile_version?: string;
  ship_version?: string;
  mechanics: MechanicStatus[];
}

export async function fetchOfficers(ownedOnly = false): Promise<OfficerListItem[]> {
  const url = ownedOnly ? `${API_BASE}/api/officers?owned_only=1` : `${API_BASE}/api/officers`;
  const res = await fetch(url);
  await checkOk(res);
  const data = await res.json();
  return data.officers ?? [];
}

export async function fetchShips(): Promise<ShipListItem[]> {
  const res = await fetch(`${API_BASE}/api/ships`);
  await checkOk(res);
  const data = await res.json();
  return data.ships ?? [];
}

export async function fetchHostiles(): Promise<HostileListItem[]> {
  const res = await fetch(`${API_BASE}/api/hostiles`);
  await checkOk(res);
  const data = await res.json();
  return data.hostiles ?? [];
}

export async function fetchDataVersion(): Promise<DataVersionResponse> {
  const res = await fetch(`${API_BASE}/api/data/version`);
  await checkOk(res);
  return res.json();
}

export interface SimulateCrew {
  captain: string | null;
  bridge: (string | null)[];
  below_deck: (string | null)[];
}

export interface SimulateStats {
  win_rate: number;
  stall_rate: number;
  loss_rate: number;
  avg_hull_remaining: number;
  n: number;
  win_rate_95_ci?: [number, number];
}

export interface SimulateResponse {
  status: string;
  stats: SimulateStats;
  seed: number;
}

export async function simulate(params: {
  ship: string;
  hostile: string;
  crew: SimulateCrew;
  num_sims?: number;
}): Promise<SimulateResponse> {
  const res = await fetch(`${API_BASE}/api/simulate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      ship: params.ship,
      hostile: params.hostile,
      crew: params.crew,
      num_sims: params.num_sims ?? 5000,
    }),
  });
  await checkOk(res);
  return res.json();
}

export interface CrewRecommendation {
  captain: string;
  bridge: string;
  below_decks: string;
  win_rate: number;
  stall_rate: number;
  loss_rate: number;
  avg_hull_remaining: number;
}

export interface OptimizeResponse {
  status: string;
  scenario: { ship: string; hostile: string; sims: number; seed: number };
  recommendations: CrewRecommendation[];
  duration_ms?: number;
}

export interface OptimizeEstimate {
  estimated_candidates: number;
  sims_per_crew: number;
  estimated_seconds: number;
}

export async function getOptimizeEstimate(params: {
  ship: string;
  hostile: string;
  sims?: number;
  max_candidates?: number | null;
  prioritize_below_decks_ability?: boolean;
}): Promise<OptimizeEstimate> {
  const sims = params.sims ?? 5000;
  const search = new URLSearchParams({
    ship: params.ship,
    hostile: params.hostile,
    sims: String(sims),
  });
  if (params.max_candidates != null && params.max_candidates > 0) {
    search.set('max_candidates', String(params.max_candidates));
  }
  if (params.prioritize_below_decks_ability === true) {
    search.set('prioritize_below_decks_ability', 'true');
  }
  const url = `${API_BASE}/api/optimize/estimate?${search.toString()}`;
  const res = await fetch(url);
  await checkOk(res);
  return res.json();
}

export async function optimize(params: {
  ship: string;
  hostile: string;
  sims?: number;
  seed?: number;
  max_candidates?: number | null;
}): Promise<OptimizeResponse> {
  const body: Record<string, unknown> = {
    ship: params.ship,
    hostile: params.hostile,
    sims: params.sims ?? 5000,
    seed: params.seed,
  };
  if (params.max_candidates != null && params.max_candidates > 0) {
    body.max_candidates = params.max_candidates;
  }
  const res = await fetch(`${API_BASE}/api/optimize`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  await checkOk(res);
  return res.json();
}

export interface OptimizeStartResponse {
  job_id: string;
}

export interface OptimizeStatusResponse {
  status: string;
  progress?: number;
  crews_done?: number;
  total_crews?: number;
  result?: OptimizeResponse;
  error?: string;
}

export async function fetchHeuristics(): Promise<string[]> {
  const res = await fetch(`${API_BASE}/api/heuristics`);
  await checkOk(res);
  const data = await res.json();
  return data.seeds ?? [];
}

export async function optimizeStart(params: {
  ship: string;
  hostile: string;
  sims?: number;
  seed?: number;
  max_candidates?: number | null;
  prioritize_below_decks_ability?: boolean;
  heuristics_seeds?: string[];
  heuristics_only?: boolean;
  below_decks_strategy?: 'ordered' | 'exploration';
}): Promise<OptimizeStartResponse> {
  const body: Record<string, unknown> = {
    ship: params.ship,
    hostile: params.hostile,
    sims: params.sims ?? 5000,
    seed: params.seed,
  };
  if (params.max_candidates != null && params.max_candidates > 0) {
    body.max_candidates = params.max_candidates;
  }
  if (params.prioritize_below_decks_ability === true) {
    body.prioritize_below_decks_ability = true;
  }
  if (params.heuristics_seeds && params.heuristics_seeds.length > 0) {
    body.heuristics_seeds = params.heuristics_seeds;
  }
  if (params.heuristics_only === true) {
    body.heuristics_only = true;
  }
  if (params.below_decks_strategy && params.below_decks_strategy !== 'ordered') {
    body.below_decks_strategy = params.below_decks_strategy;
  }
  const res = await fetch(`${API_BASE}/api/optimize/start`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  await checkOk(res);
  return res.json();
}

export async function getOptimizeStatus(jobId: string): Promise<OptimizeStatusResponse> {
  const res = await fetch(`${API_BASE}/api/optimize/status/${encodeURIComponent(jobId)}`);
  await checkOk(res);
  return res.json();
}

export interface ImportReport {
  source_path: string;
  output_path: string;
  total_records: number;
  matched_records: number;
  unmatched_records: number;
  roster_entries_written: number;
  unresolved?: { record_index: number; input_name: string; reason: string }[];
}

export async function importRoster(body: string): Promise<ImportReport> {
  const res = await fetch(`${API_BASE}/api/officers/import`, {
    method: 'POST',
    headers: { 'Content-Type': 'text/plain' },
    body: body.trim(),
  });
  await checkOk(res);
  return res.json();
}

export interface PlayerProfile {
  bonuses: Record<string, number>;
}

export async function fetchProfile(): Promise<PlayerProfile> {
  const res = await fetch(`${API_BASE}/api/profile`);
  await checkOk(res);
  return res.json();
}

export async function updateProfile(profile: PlayerProfile): Promise<void> {
  const res = await fetch(`${API_BASE}/api/profile`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(profile),
  });
  await checkOk(res);
}

export interface PresetCrew {
  captain?: string | null;
  bridge?: (string | null)[];
  below_deck?: (string | null)[];
}

export interface Preset {
  id: string;
  name: string;
  ship: string;
  scenario: string;
  crew: PresetCrew;
}

export interface PresetSummary {
  id: string;
  name: string;
  ship: string;
  scenario: string;
}

export async function fetchPresets(): Promise<PresetSummary[]> {
  const res = await fetch(`${API_BASE}/api/presets`);
  await checkOk(res);
  const data = await res.json();
  return data.presets ?? [];
}

export async function fetchPreset(id: string): Promise<Preset> {
  const res = await fetch(`${API_BASE}/api/presets/${encodeURIComponent(id)}`);
  await checkOk(res);
  return res.json();
}

export async function savePreset(preset: {
  name?: string;
  ship: string;
  scenario: string;
  crew: PresetCrew;
}): Promise<Preset> {
  const res = await fetch(`${API_BASE}/api/presets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      name: preset.name ?? 'Unnamed',
      ship: preset.ship,
      scenario: preset.scenario,
      crew: preset.crew,
    }),
  });
  await checkOk(res);
  return res.json();
}
