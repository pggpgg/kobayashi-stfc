const API_BASE = '';

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
  if (!res.ok) throw new Error(res.statusText);
  const data = await res.json();
  return data.officers ?? [];
}

export async function fetchShips(): Promise<ShipListItem[]> {
  const res = await fetch(`${API_BASE}/api/ships`);
  if (!res.ok) throw new Error(res.statusText);
  const data = await res.json();
  return data.ships ?? [];
}

export async function fetchHostiles(): Promise<HostileListItem[]> {
  const res = await fetch(`${API_BASE}/api/hostiles`);
  if (!res.ok) throw new Error(res.statusText);
  const data = await res.json();
  return data.hostiles ?? [];
}

export async function fetchDataVersion(): Promise<DataVersionResponse> {
  const res = await fetch(`${API_BASE}/api/data/version`);
  if (!res.ok) throw new Error(res.statusText);
  return res.json();
}

export interface SimulateCrew {
  captain: string | null;
  bridge: (string | null)[];
  below_deck: (string | null)[];
}

export interface SimulateStats {
  win_rate: number;
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
  if (!res.ok) {
    const t = await res.text();
    throw new Error(t || res.statusText);
  }
  return res.json();
}

export interface CrewRecommendation {
  captain: string;
  bridge: string;
  below_decks: string;
  win_rate: number;
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
}): Promise<OptimizeEstimate> {
  const sims = params.sims ?? 5000;
  const url = `${API_BASE}/api/optimize/estimate?ship=${encodeURIComponent(params.ship)}&hostile=${encodeURIComponent(params.hostile)}&sims=${sims}`;
  const res = await fetch(url);
  if (!res.ok) {
    const t = await res.text();
    throw new Error(t || res.statusText);
  }
  return res.json();
}

export async function optimize(params: {
  ship: string;
  hostile: string;
  sims?: number;
  seed?: number;
}): Promise<OptimizeResponse> {
  const res = await fetch(`${API_BASE}/api/optimize`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      ship: params.ship,
      hostile: params.hostile,
      sims: params.sims ?? 5000,
      seed: params.seed,
    }),
  });
  if (!res.ok) {
    const t = await res.text();
    throw new Error(t || res.statusText);
  }
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
  if (!res.ok) {
    const t = await res.text();
    throw new Error(t || res.statusText);
  }
  return res.json();
}

export interface PlayerProfile {
  bonuses: Record<string, number>;
}

export async function fetchProfile(): Promise<PlayerProfile> {
  const res = await fetch(`${API_BASE}/api/profile`);
  if (!res.ok) throw new Error(res.statusText);
  return res.json();
}

export async function updateProfile(profile: PlayerProfile): Promise<void> {
  const res = await fetch(`${API_BASE}/api/profile`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(profile),
  });
  if (!res.ok) {
    const t = await res.text();
    throw new Error(t || res.statusText);
  }
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
  if (!res.ok) throw new Error(res.statusText);
  const data = await res.json();
  return data.presets ?? [];
}

export async function fetchPreset(id: string): Promise<Preset> {
  const res = await fetch(`${API_BASE}/api/presets/${encodeURIComponent(id)}`);
  if (!res.ok) throw new Error(res.statusText);
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
  if (!res.ok) {
    const t = await res.text();
    throw new Error(t || res.statusText);
  }
  return res.json();
}
