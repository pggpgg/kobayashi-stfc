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

/** Build headers with X-Profile-Id when profileId is provided. */
function profileHeaders(profileId?: string | null): Record<string, string> {
  if (!profileId) return {};
  return { 'X-Profile-Id': profileId };
}

export interface ProfileEntry {
  id: string;
  name: string;
  sync_token: string;
}

export interface ProfilesResponse {
  profiles: ProfileEntry[];
  default_id?: string;
}

export async function fetchProfiles(): Promise<ProfilesResponse> {
  const res = await fetch(`${API_BASE}/api/profiles`);
  await checkOk(res);
  return res.json();
}

export async function createProfile(params: { id?: string; name: string }): Promise<ProfileEntry> {
  const res = await fetch(`${API_BASE}/api/profiles`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(params),
  });
  await checkOk(res);
  return res.json();
}

export async function deleteProfile(id: string): Promise<void> {
  const res = await fetch(`${API_BASE}/api/profiles/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  });
  await checkOk(res);
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
  /** From roster when owned_only: tier of first roster entry for this ship. */
  tier?: number;
  /** From roster when owned_only: level of first roster entry for this ship. */
  level?: number;
}

export interface ShipTiersLevels {
  tiers: number[];
  levels: number[];
}

export async function getShipTiersLevels(shipId: string): Promise<ShipTiersLevels> {
  const res = await fetch(`${API_BASE}/api/ships/${encodeURIComponent(shipId)}/tiers-levels`);
  await checkOk(res);
  return res.json();
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

export async function fetchOfficers(
  ownedOnly = false,
  profileId?: string | null,
): Promise<OfficerListItem[]> {
  const url = ownedOnly ? `${API_BASE}/api/officers?owned_only=1` : `${API_BASE}/api/officers`;
  const res = await fetch(url, { headers: profileHeaders(profileId) });
  await checkOk(res);
  const data = await res.json();
  return data.officers ?? [];
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

export async function simulate(
  params: {
    ship: string;
    hostile: string;
    crew: SimulateCrew;
    num_sims?: number;
    ship_tier?: number | null;
    ship_level?: number | null;
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
  const res = await fetch(`${API_BASE}/api/simulate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...profileHeaders(profileId) },
    body: JSON.stringify(body),
  });
  await checkOk(res);
  return res.json();
}

export interface CrewRecommendation {
  captain: string;
  /** API returns string[]; we accept string for backward compatibility. */
  bridge: string | string[];
  /** API returns string[]; we accept string for backward compatibility. */
  below_decks: string | string[];
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

export async function getOptimizeEstimate(
  params: {
    ship: string;
    hostile: string;
    sims?: number;
    max_candidates?: number | null;
    prioritize_below_decks_ability?: boolean;
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
    search.set('max_candidates', String(params.max_candidates));
  }
  if (params.prioritize_below_decks_ability === true) {
    search.set('prioritize_below_decks_ability', 'true');
  }
  if (profileId) search.set('profile', profileId);
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
  const res = await fetch(`${API_BASE}/api/optimize`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...profileHeaders(profileId) },
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

export type OptimizerStrategyType = 'exhaustive' | 'genetic' | 'tiered';

export async function optimizeStart(
  params: {
    ship: string;
    hostile: string;
    sims?: number;
    seed?: number;
    max_candidates?: number | null;
    strategy?: OptimizerStrategyType;
    prioritize_below_decks_ability?: boolean;
    heuristics_seeds?: string[];
    heuristics_only?: boolean;
    below_decks_strategy?: 'ordered' | 'exploration';
    ship_tier?: number | null;
    ship_level?: number | null;
  },
  profileId?: string | null,
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
  if (params.strategy && params.strategy !== 'exhaustive') {
    body.strategy = params.strategy;
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
  if (params.ship_tier != null && params.ship_tier > 0) {
    body.ship_tier = params.ship_tier;
  }
  if (params.ship_level != null && params.ship_level > 0) {
    body.ship_level = params.ship_level;
  }
  const res = await fetch(`${API_BASE}/api/optimize/start`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...profileHeaders(profileId) },
    body: JSON.stringify(body),
  });
  await checkOk(res);
  return res.json();
}

export async function getOptimizeStatus(
  jobId: string,
  profileId?: string | null,
): Promise<OptimizeStatusResponse> {
  const url = profileId
    ? `${API_BASE}/api/optimize/status/${encodeURIComponent(jobId)}?profile=${encodeURIComponent(profileId)}`
    : `${API_BASE}/api/optimize/status/${encodeURIComponent(jobId)}`;
  const res = await fetch(url);
  await checkOk(res);
  return res.json();
}

/** URL for SSE stream of optimize job progress (GET). Use with EventSource for live updates. */
export function getOptimizeStreamUrl(jobId: string): string {
  return `${API_BASE}/api/optimize/jobs/${encodeURIComponent(jobId)}/stream`;
}

/** Request cancellation of a running optimize job. */
export async function cancelOptimizeJob(jobId: string): Promise<void> {
  const res = await fetch(`${API_BASE}/api/optimize/jobs/${encodeURIComponent(jobId)}/cancel`, {
    method: 'POST',
  });
  await checkOk(res);
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

export async function importRoster(
  body: string,
  profileId?: string | null,
): Promise<ImportReport> {
  const res = await fetch(`${API_BASE}/api/officers/import`, {
    method: 'POST',
    headers: { 'Content-Type': 'text/plain', ...profileHeaders(profileId) },
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

export async function fetchForbiddenTech(): Promise<ForbiddenTechCatalogItem[]> {
  const res = await fetch(`${API_BASE}/api/forbidden-tech`);
  await checkOk(res);
  const data: ForbiddenTechCatalogResponse = await res.json();
  return data.items ?? [];
}

export interface PlayerProfile {
  bonuses: Record<string, number>;
  /** When undefined/null: use synced forbidden_tech.imported.json. When []: no FT. When number[]: use these fids. */
  forbidden_tech_override?: number[] | null;
  /** When undefined/null: use synced chaos tech from forbidden_tech.imported.json. When []: none. When number[]: use these fids. */
  chaos_tech_override?: number[] | null;
}

export async function fetchProfile(profileId?: string | null): Promise<PlayerProfile> {
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
    method: 'PUT',
    headers: { 'Content-Type': 'application/json', ...profileHeaders(profileId) },
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
  const q = profileId ? `?profile=${encodeURIComponent(profileId)}` : '';
  const res = await fetch(`${API_BASE}/api/profile/buildings-summary${q}`, {
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

export async function fetchPresets(profileId?: string | null): Promise<PresetSummary[]> {
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
  const res = await fetch(`${API_BASE}/api/presets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...profileHeaders(profileId) },
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
