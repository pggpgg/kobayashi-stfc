import type { CrewRecommendation, ParetoTag } from "./api";

export interface ParsedQdCell {
  raw: string;
  captainGroup: string;
  captainGroupLabel: string;
  speedBucket: string;
  speedLabel: string;
  survivalBucket: string;
  survivalLabel: string;
}

export interface ArchiveCellSummary {
  cell: ParsedQdCell;
  count: number;
  bestIndex: number;
  bestCrew: CrewRecommendation;
}

export interface LaneCount {
  lane: string;
  label: string;
  count: number;
}

export interface PortfolioAnalyticsSummary {
  totalRows: number;
  hasAnalytics: boolean;
  distinctCaptainGroups: number;
  archiveCells: ArchiveCellSummary[];
  laneCounts: LaneCount[];
  fidelityCounts: Record<"A" | "B" | "C" | "D", number>;
  hasFidelity: boolean;
  caveatRowCount: number;
  paretoRowCount: number;
  /** Up to 5 row indices leading distinct captain archetypes (for one-click diverse compare). */
  topDiverseIndices: number[];
}

export interface RecommendationFilters {
  lane: string | null;
  qdCell: string | null;
  captainGroup: string | null;
  fidelityGrade: "A" | "B" | "C" | "D" | null;
  paretoOnly: boolean;
  caveatsOnly: boolean;
}

export const EMPTY_RECOMMENDATION_FILTERS: RecommendationFilters = {
  lane: null,
  qdCell: null,
  captainGroup: null,
  fidelityGrade: null,
  paretoOnly: false,
  caveatsOnly: false,
};

export interface FilteredRecommendationRow {
  rec: CrewRecommendation;
  originalIndex: number;
}

export interface SeatDiffSummary {
  captainChanged: boolean;
  baselineCaptain: string;
  candidateCaptain: string;
  bridgeShared: string[];
  bridgeAdded: string[];
  bridgeRemoved: string[];
  belowShared: string[];
  belowAdded: string[];
  belowRemoved: string[];
  totalSwappedOfficers: number;
}

const SPEED_LABELS: Record<string, string> = {
  r1: "R1 burst",
  fast: "Fast",
  medium: "Medium",
  slow: "Long fight",
  stall: "Stall-prone",
};

const SURVIVAL_LABELS: Record<string, string> = {
  high: "High hull",
  high_hull: "High hull",
  mid: "Mid hull",
  medium: "Mid hull",
  mid_hull: "Mid hull",
  low: "Low hull",
  low_hull: "Low hull",
  loss: "Fragile",
};

const METHOD_LABELS: Record<string, string> = {
  beam_diversity: "Beam",
  curated_warm_start: "Curated",
  exhaustive_mc: "Exhaustive MC",
  exhaustive_two_phase: "Two-phase MC",
  genetic: "Genetic",
  heuristic_seed: "Heuristic seed",
  heuristics: "Heuristics",
  large_neighborhood_repair: "Rebuilt seats",
  linear_eval: "Linear eval",
  local_captain_swap: "Captain swap",
  local_swap: "Seat swap",
  monte_carlo: "Monte Carlo",
  portfolio: "Portfolio",
  portfolio_neighbor: "Neighbor swap",
  qd_archive: "Archive",
  random_stratified: "Stratified random",
  seeded_genetic: "Seeded GA",
  tiered_confirmed: "Tiered",
  warm_start: "Warm start",
};

export function formatDiscoveryLaneLabel(value: string | undefined): string {
  if (!value) return "";
  return METHOD_LABELS[value] ?? value.replace(/_/g, " ");
}

function humanizeToken(raw: string): string {
  const cleaned = raw.trim();
  if (!cleaned) return "Other";
  return cleaned
    .split(/[_-]+/)
    .filter(Boolean)
    .map((part) => {
      if (part.length <= 3 && /^[a-z0-9]+$/i.test(part)) {
        return part.toUpperCase();
      }
      return part[0].toUpperCase() + part.slice(1);
    })
    .join(" ");
}

/**
 * Parse a portfolio MAP-Elites cell coordinate (`captain_group|speed|survival`)
 * into structured display fields.
 */
export function parseQdCell(
  qdCell: string | null | undefined,
): ParsedQdCell | null {
  if (!qdCell?.trim()) return null;
  const parts = qdCell.split("|").map((p) => p.trim());
  const captainGroup = parts[0] || "other";
  const speedBucket = parts[1] || "unknown";
  const survivalBucket = parts[2] || "unknown";
  return {
    raw: qdCell.trim(),
    captainGroup,
    captainGroupLabel: humanizeToken(captainGroup),
    speedBucket,
    speedLabel: SPEED_LABELS[speedBucket] ?? humanizeToken(speedBucket),
    survivalBucket,
    survivalLabel:
      SURVIVAL_LABELS[survivalBucket] ?? humanizeToken(survivalBucket),
  };
}

/**
 * Return all distinct discovery lanes associated with a recommendation row.
 */
export function rowDiscoveryLanes(r: CrewRecommendation): string[] {
  const lanes: string[] = [];
  const seen = new Set<string>();
  for (const m of r.proposing_methods ?? []) {
    const k = m.trim();
    if (k && !seen.has(k)) {
      seen.add(k);
      lanes.push(k);
    }
  }
  const primary = r.method_provenance?.trim();
  if (primary && !seen.has(primary)) {
    seen.add(primary);
    lanes.push(primary);
  }
  return lanes;
}

export function rowHasCaveat(r: CrewRecommendation): boolean {
  if (r.uncertainty_reason && r.uncertainty_reason.trim().length > 0) {
    return true;
  }
  if (
    r.fidelity &&
    (r.fidelity.ignored > 0 || (r.fidelity.caveats?.length ?? 0) > 0)
  ) {
    return true;
  }
  return false;
}

export function rowHasParetoTag(
  r: CrewRecommendation,
  tag?: ParetoTag,
): boolean {
  const tags = r.pareto_tags ?? [];
  if (tag) return tags.includes(tag);
  return tags.length > 0;
}

export function hasActiveFilters(filters: RecommendationFilters): boolean {
  return (
    filters.lane !== null ||
    filters.qdCell !== null ||
    filters.captainGroup !== null ||
    filters.fidelityGrade !== null ||
    filters.paretoOnly ||
    filters.caveatsOnly
  );
}

/**
 * Aggregate portfolio diversity cells, discovery lanes, fidelity breakdown,
 * and diverse comparison picks across a recommendation result set.
 */
export function summarizePortfolioResults(
  recommendations: CrewRecommendation[],
): PortfolioAnalyticsSummary {
  const cellMap = new Map<string, ArchiveCellSummary>();
  const laneCountMap = new Map<string, number>();
  const captainGroups = new Set<string>();
  const fidelityCounts: Record<"A" | "B" | "C" | "D", number> = {
    A: 0,
    B: 0,
    C: 0,
    D: 0,
  };
  let hasFidelity = false;
  let hasMultiLane = false;
  let caveatRowCount = 0;
  let paretoRowCount = 0;

  for (let i = 0; i < recommendations.length; i++) {
    const r = recommendations[i];
    if ((r.proposing_methods?.length ?? 0) > 0) {
      hasMultiLane = true;
    }
    for (const lane of rowDiscoveryLanes(r)) {
      laneCountMap.set(lane, (laneCountMap.get(lane) ?? 0) + 1);
    }

    const parsedCell = parseQdCell(r.qd_cell);
    if (parsedCell) {
      captainGroups.add(parsedCell.captainGroup);
      const existing = cellMap.get(parsedCell.raw);
      if (existing) {
        existing.count += 1;
      } else {
        cellMap.set(parsedCell.raw, {
          cell: parsedCell,
          count: 1,
          bestIndex: i,
          bestCrew: r,
        });
      }
    }

    if (r.fidelity) {
      hasFidelity = true;
      const g = r.fidelity.grade;
      if (g === "A" || g === "B" || g === "C" || g === "D") {
        fidelityCounts[g] += 1;
      }
    }

    if (rowHasCaveat(r)) {
      caveatRowCount += 1;
    }
    if (rowHasParetoTag(r)) {
      paretoRowCount += 1;
    }
  }

  const archiveCells = Array.from(cellMap.values()).sort(
    (a, b) => a.bestIndex - b.bestIndex,
  );
  const laneCounts: LaneCount[] = Array.from(laneCountMap.entries())
    .map(([lane, count]) => ({
      lane,
      label: formatDiscoveryLaneLabel(lane),
      count,
    }))
    .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));

  // Pick up to 5 top-ranked crews representing distinct captain groups (or distinct captains
  // if qd_cell is unavailable) for one-click diverse side-by-side comparison.
  const topDiverseIndices: number[] = [];
  const seenDiversityKeys = new Set<string>();
  for (
    let i = 0;
    i < recommendations.length && topDiverseIndices.length < 5;
    i++
  ) {
    const r = recommendations[i];
    const parsed = parseQdCell(r.qd_cell);
    const key = parsed
      ? `group:${parsed.captainGroup}`
      : `cap:${r.captain.trim().toLowerCase()}`;
    if (!seenDiversityKeys.has(key)) {
      seenDiversityKeys.add(key);
      topDiverseIndices.push(i);
    }
  }

  const hasAnalytics =
    recommendations.length >= 2 &&
    (archiveCells.length > 0 ||
      hasFidelity ||
      hasMultiLane ||
      laneCounts.length > 1);

  return {
    totalRows: recommendations.length,
    hasAnalytics,
    distinctCaptainGroups: captainGroups.size,
    archiveCells,
    laneCounts,
    fidelityCounts,
    hasFidelity,
    caveatRowCount,
    paretoRowCount,
    topDiverseIndices,
  };
}

/**
 * Filter recommendation rows while preserving each row's original 0-based index
 * so table ranks (`#1`, `#2`, ...) and comparison selections never drift.
 */
export function filterRecommendations(
  recommendations: CrewRecommendation[],
  filters: RecommendationFilters,
): FilteredRecommendationRow[] {
  const out: FilteredRecommendationRow[] = [];
  for (let i = 0; i < recommendations.length; i++) {
    const rec = recommendations[i];
    if (filters.lane && !rowDiscoveryLanes(rec).includes(filters.lane)) {
      continue;
    }
    if (filters.qdCell && rec.qd_cell?.trim() !== filters.qdCell) {
      continue;
    }
    if (filters.captainGroup) {
      const parsed = parseQdCell(rec.qd_cell);
      if (!parsed || parsed.captainGroup !== filters.captainGroup) {
        continue;
      }
    }
    if (
      filters.fidelityGrade &&
      rec.fidelity?.grade !== filters.fidelityGrade
    ) {
      continue;
    }
    if (filters.paretoOnly && !rowHasParetoTag(rec)) {
      continue;
    }
    if (filters.caveatsOnly && !rowHasCaveat(rec)) {
      continue;
    }
    out.push({ rec, originalIndex: i });
  }
  return out;
}

function normalizeList(items: string | string[] | null | undefined): string[] {
  if (!items) return [];
  const arr = Array.isArray(items) ? items : items.split(",");
  return arr.map((s) => s.trim()).filter(Boolean);
}

function diffOfficerGroup(
  baseList: string[],
  candList: string[],
): { shared: string[]; added: string[]; removed: string[] } {
  const baseKeyMap = new Map<string, string>();
  for (const name of baseList) {
    baseKeyMap.set(name.toLowerCase(), name);
  }
  const candKeyMap = new Map<string, string>();
  for (const name of candList) {
    candKeyMap.set(name.toLowerCase(), name);
  }

  const shared: string[] = [];
  const added: string[] = [];
  const removed: string[] = [];

  for (const [k, name] of candKeyMap) {
    if (baseKeyMap.has(k)) {
      shared.push(name);
    } else {
      added.push(name);
    }
  }
  for (const [k, name] of baseKeyMap) {
    if (!candKeyMap.has(k)) {
      removed.push(name);
    }
  }
  return { shared, added, removed };
}

/**
 * Compute seat-by-seat officer changes between a baseline recommendation and a
 * candidate recommendation for side-by-side comparison cards.
 */
export function diffCrewSeats(
  baseline: CrewRecommendation,
  candidate: CrewRecommendation,
): SeatDiffSummary {
  const baseCap = baseline.captain.trim();
  const candCap = candidate.captain.trim();
  const captainChanged = baseCap.toLowerCase() !== candCap.toLowerCase();

  const bridge = diffOfficerGroup(
    normalizeList(baseline.bridge),
    normalizeList(candidate.bridge),
  );
  const below = diffOfficerGroup(
    normalizeList(baseline.below_decks),
    normalizeList(candidate.below_decks),
  );

  const totalSwappedOfficers =
    (captainChanged ? 1 : 0) +
    Math.max(bridge.added.length, bridge.removed.length) +
    Math.max(below.added.length, below.removed.length);

  return {
    captainChanged,
    baselineCaptain: baseCap,
    candidateCaptain: candCap,
    bridgeShared: bridge.shared,
    bridgeAdded: bridge.added,
    bridgeRemoved: bridge.removed,
    belowShared: below.shared,
    belowAdded: below.added,
    belowRemoved: below.removed,
    totalSwappedOfficers,
  };
}
