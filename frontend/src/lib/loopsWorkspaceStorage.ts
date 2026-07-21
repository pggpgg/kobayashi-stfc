import type { CrewRecommendation } from "./api";
import type { LoopGoalId } from "./loopsCatalog";

const STORAGE_VERSION = 1;
const STORAGE_PREFIX = "kobayashi_loops_workspace_v1";
const PENDING_PREFIX = "kobayashi_loops_pending_v1";
const MAX_PRIOR_BESTS = 5;

export interface LoopRunContext {
  loopId: string;
  loopName: string;
  targetId: string;
  targetName: string;
  targetLevel: number;
  goalId: LoopGoalId;
  shipId: string;
  shipPolicy: "required" | "recommended" | "open";
  specialtyShipIds: string[];
}

export interface LoopCrewSnapshot {
  captain: string;
  bridge: string[];
  belowDecks: string[];
}

export interface LoopBestRecord {
  id: string;
  loopId: string;
  targetId: string;
  targetName: string;
  targetLevel: number;
  goalId: LoopGoalId;
  shipId: string;
  shipTier: number;
  shipLevel: number;
  crew: LoopCrewSnapshot;
  winRate: number;
  roundOneKillRate: number;
  averageHullRemaining: number;
  averageDefenderHullRemaining: number;
  chainSuccessRate?: number;
  chainHullScore?: number;
  recordedAt: string;
}

interface LoopsWorkspaceFile {
  version: number;
  records: Record<string, LoopBestRecord[]>;
}

function profileKey(profileId: string | null): string {
  return profileId?.trim() || "default";
}

function storageKey(profileId: string | null): string {
  return `${STORAGE_PREFIX}:${profileKey(profileId)}`;
}

function pendingKey(profileId: string | null): string {
  return `${PENDING_PREFIX}:${profileKey(profileId)}`;
}

function emptyWorkspace(): LoopsWorkspaceFile {
  return { version: STORAGE_VERSION, records: {} };
}

function readWorkspace(profileId: string | null): LoopsWorkspaceFile {
  try {
    const raw = localStorage.getItem(storageKey(profileId));
    if (!raw) return emptyWorkspace();
    const parsed = JSON.parse(raw) as LoopsWorkspaceFile;
    if (parsed.version !== STORAGE_VERSION || !parsed.records) {
      return emptyWorkspace();
    }
    return parsed;
  } catch {
    return emptyWorkspace();
  }
}

function recordKey(
  loopId: string,
  targetId: string,
  goalId: LoopGoalId,
): string {
  return `${loopId}|${targetId}|${goalId}`;
}

function asStringArray(value: string | string[]): string[] {
  return Array.isArray(value) ? [...value] : value ? [value] : [];
}

function score(record: LoopBestRecord): readonly number[] {
  switch (record.goalId) {
    case "one_round":
      return [
        record.roundOneKillRate,
        record.winRate,
        record.averageHullRemaining,
      ];
    case "damage_dealt":
      return [
        1 - record.averageDefenderHullRemaining,
        record.winRate,
        record.averageHullRemaining,
      ];
    case "no_hits":
      return [
        record.winRate,
        record.averageHullRemaining,
        record.roundOneKillRate,
      ];
    case "kills_per_hull":
      return [
        record.chainSuccessRate ?? record.winRate,
        record.chainHullScore ?? record.averageHullRemaining,
        record.winRate,
      ];
    case "smallest_ship":
      return [
        record.winRate >= 0.5 ? 1 : 0,
        -record.shipTier,
        -record.shipLevel,
        record.winRate,
      ];
  }
}

function compareScore(a: LoopBestRecord, b: LoopBestRecord): number {
  const aScore = score(a);
  const bScore = score(b);
  for (let i = 0; i < Math.max(aScore.length, bScore.length); i += 1) {
    const delta = (bScore[i] ?? 0) - (aScore[i] ?? 0);
    if (Math.abs(delta) > 1e-12) return delta;
  }
  return b.recordedAt.localeCompare(a.recordedAt);
}

function recommendationScore(
  goalId: LoopGoalId,
  recommendation: CrewRecommendation,
): readonly number[] {
  switch (goalId) {
    case "one_round":
      return [
        recommendation.r1_kill_rate,
        recommendation.win_rate,
        recommendation.avg_hull_remaining,
      ];
    case "damage_dealt":
      return [
        1 - recommendation.avg_defender_hull_remaining,
        recommendation.win_rate,
        recommendation.avg_hull_remaining,
      ];
    case "no_hits":
      return [
        recommendation.win_rate,
        recommendation.avg_hull_remaining,
        recommendation.r1_kill_rate,
      ];
    case "kills_per_hull":
      return [
        recommendation.chain?.primary_success_rate ?? recommendation.win_rate,
        recommendation.chain?.secondary_mean_given_primary ??
          recommendation.avg_hull_remaining,
        recommendation.win_rate,
      ];
    case "smallest_ship":
      return [recommendation.win_rate, recommendation.avg_hull_remaining];
  }
}

export function chooseLoopRecommendation(
  goalId: LoopGoalId,
  recommendations: readonly CrewRecommendation[],
): CrewRecommendation | null {
  let best: CrewRecommendation | null = null;
  for (const candidate of recommendations) {
    if (!best) {
      best = candidate;
      continue;
    }
    const candidateScore = recommendationScore(goalId, candidate);
    const bestScore = recommendationScore(goalId, best);
    for (
      let i = 0;
      i < Math.max(candidateScore.length, bestScore.length);
      i += 1
    ) {
      const delta = (candidateScore[i] ?? 0) - (bestScore[i] ?? 0);
      if (Math.abs(delta) <= 1e-12) continue;
      if (delta > 0) best = candidate;
      break;
    }
  }
  return best;
}

export function listLoopRecords(profileId: string | null): LoopBestRecord[] {
  return Object.values(readWorkspace(profileId).records)
    .flat()
    .sort((a, b) => b.recordedAt.localeCompare(a.recordedAt));
}

export function getLoopBestRecord(
  profileId: string | null,
  loopId: string,
  targetId: string,
  goalId: LoopGoalId,
): LoopBestRecord | null {
  const records =
    readWorkspace(profileId).records[recordKey(loopId, targetId, goalId)];
  if (!records?.length) return null;
  return [...records].sort(compareScore)[0] ?? null;
}

export function saveLoopOptimizationResult(args: {
  profileId: string | null;
  context: LoopRunContext;
  shipId: string;
  shipTier: number;
  shipLevel: number;
  recommendations: readonly CrewRecommendation[];
}): { saved: boolean; improved: boolean; record: LoopBestRecord | null } {
  const recommendation = chooseLoopRecommendation(
    args.context.goalId,
    args.recommendations,
  );
  if (!recommendation) return { saved: false, improved: false, record: null };

  const workspace = readWorkspace(args.profileId);
  const key = recordKey(
    args.context.loopId,
    args.context.targetId,
    args.context.goalId,
  );
  const previous = workspace.records[key] ?? [];
  const record: LoopBestRecord = {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    loopId: args.context.loopId,
    targetId: args.context.targetId,
    targetName: args.context.targetName,
    targetLevel: args.context.targetLevel,
    goalId: args.context.goalId,
    shipId: args.shipId,
    shipTier: args.shipTier,
    shipLevel: args.shipLevel,
    crew: {
      captain: recommendation.captain,
      bridge: asStringArray(recommendation.bridge),
      belowDecks: asStringArray(recommendation.below_decks),
    },
    winRate: recommendation.win_rate,
    roundOneKillRate: recommendation.r1_kill_rate,
    averageHullRemaining: recommendation.avg_hull_remaining,
    averageDefenderHullRemaining: recommendation.avg_defender_hull_remaining,
    ...(recommendation.chain
      ? {
          chainSuccessRate: recommendation.chain.primary_success_rate,
          chainHullScore: recommendation.chain.secondary_mean_given_primary,
        }
      : {}),
    recordedAt: new Date().toISOString(),
  };
  const ranked = [record, ...previous].sort(compareScore);
  const improved = ranked[0]?.id === record.id;
  if (!improved)
    return { saved: false, improved: false, record: ranked[0] ?? null };

  const deduped = ranked.filter((candidate, index, all) => {
    const signature = JSON.stringify([
      candidate.shipId,
      candidate.shipTier,
      candidate.shipLevel,
      candidate.crew,
    ]);
    return (
      all.findIndex(
        (other) =>
          JSON.stringify([
            other.shipId,
            other.shipTier,
            other.shipLevel,
            other.crew,
          ]) === signature,
      ) === index
    );
  });
  workspace.records[key] = deduped.slice(0, MAX_PRIOR_BESTS);
  try {
    localStorage.setItem(storageKey(args.profileId), JSON.stringify(workspace));
    return { saved: true, improved: true, record };
  } catch {
    return { saved: false, improved: false, record: previous[0] ?? null };
  }
}

export function savePendingLoopRun(
  profileId: string | null,
  context: LoopRunContext,
): void {
  try {
    sessionStorage.setItem(pendingKey(profileId), JSON.stringify(context));
  } catch {}
}

export function loadPendingLoopRun(
  profileId: string | null,
): LoopRunContext | null {
  try {
    const raw = sessionStorage.getItem(pendingKey(profileId));
    if (!raw) return null;
    return JSON.parse(raw) as LoopRunContext;
  } catch {
    return null;
  }
}

export function clearPendingLoopRun(profileId: string | null): void {
  try {
    sessionStorage.removeItem(pendingKey(profileId));
  } catch {}
}
