import type { CrewRecommendation } from "./api";
import type { LoopGoalId } from "./loopsCatalog";

/**
 * Schema 2 adds confidence intervals, the run settings that produced a record, and
 * data-version provenance. Records written by schema 1 are *migrated*, not
 * discarded — see [`readWorkspace`].
 *
 * The storage key keeps its `_v1` suffix deliberately: the version lives in the
 * `version` field inside the blob, and renaming the key would orphan every existing
 * record, which is the exact data loss this migration exists to avoid.
 */
const STORAGE_VERSION = 2;
const STORAGE_PREFIX = "kobayashi_loops_workspace_v1";
const PENDING_PREFIX = "kobayashi_loops_pending_v1";
const MAX_PRIOR_BESTS = 5;

/**
 * How much less sampling a challenger may have and still win an overlapping-CI
 * comparison. At 0.5 a 200-sim run cannot displace a 20,000-sim incumbent on a
 * hair's-breadth point-estimate lead.
 */
const OVERLAP_TRUST_RATIO = 0.5;

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

/** A measured value with its 95% confidence interval. */
export interface MetricCi {
  value: number;
  ciLow: number;
  ciHigh: number;
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
  winRateCiLow: number;
  winRateCiHigh: number;
  roundOneKillRate: number;
  roundOneKillRateCiLow: number;
  roundOneKillRateCiHigh: number;
  averageHullRemaining: number;
  averageHullRemainingCiLow: number;
  averageHullRemainingCiHigh: number;
  averageDefenderHullRemaining: number;
  averageDefenderHullRemainingCiLow: number;
  averageDefenderHullRemainingCiHigh: number;
  chainSuccessRate?: number;
  chainSuccessRateCiLow?: number;
  chainSuccessRateCiHigh?: number;
  chainHullScore?: number;
  chainHullScoreCiLow?: number;
  chainHullScoreCiHigh?: number;
  chainKillsTarget?: number;
  /**
   * Run settings behind this record. Optional because migrated schema-1 records
   * genuinely do not know them — and "unknown" must stay distinguishable from
   * "zero", since unknown sampling forfeits the trust check in
   * [`isGenuineImprovement`] rather than benefiting from it.
   */
  simsPerCrew?: number;
  strategy?: string;
  maxCandidates?: number | null;
  /** Catalog versions in force when this record was measured (cf. PresetProvenance). */
  hostileDataVersion?: string | null;
  shipDataVersion?: string | null;
  recordedAt: string;
}

/** Schema-1 record shape, retained so the migration is type-checked. */
type LoopBestRecordV1 = Omit<
  LoopBestRecord,
  | "winRateCiLow"
  | "winRateCiHigh"
  | "roundOneKillRateCiLow"
  | "roundOneKillRateCiHigh"
  | "averageHullRemainingCiLow"
  | "averageHullRemainingCiHigh"
  | "averageDefenderHullRemainingCiLow"
  | "averageDefenderHullRemainingCiHigh"
>;

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

/**
 * Lift a schema-1 record to schema 2 with zero-width confidence intervals.
 *
 * Collapsing the interval onto the point estimate is deliberate: a legacy record's
 * true uncertainty is unknowable, and inventing a width would either shield it from
 * being beaten or make it trivially beatable. Zero width means it is exactly as
 * beatable as its raw number implies, and — because `simsPerCrew` stays undefined —
 * it never earns the low-sampling veto.
 */
function migrateV1Record(v1: LoopBestRecordV1): LoopBestRecord {
  return {
    ...v1,
    winRateCiLow: v1.winRate,
    winRateCiHigh: v1.winRate,
    roundOneKillRateCiLow: v1.roundOneKillRate,
    roundOneKillRateCiHigh: v1.roundOneKillRate,
    averageHullRemainingCiLow: v1.averageHullRemaining,
    averageHullRemainingCiHigh: v1.averageHullRemaining,
    averageDefenderHullRemainingCiLow: v1.averageDefenderHullRemaining,
    averageDefenderHullRemainingCiHigh: v1.averageDefenderHullRemaining,
    ...(v1.chainSuccessRate != null
      ? {
          chainSuccessRateCiLow: v1.chainSuccessRate,
          chainSuccessRateCiHigh: v1.chainSuccessRate,
        }
      : {}),
    ...(v1.chainHullScore != null
      ? {
          chainHullScoreCiLow: v1.chainHullScore,
          chainHullScoreCiHigh: v1.chainHullScore,
        }
      : {}),
    hostileDataVersion: v1.hostileDataVersion ?? null,
    shipDataVersion: v1.shipDataVersion ?? null,
  };
}

function readWorkspace(profileId: string | null): LoopsWorkspaceFile {
  try {
    const raw = localStorage.getItem(storageKey(profileId));
    if (!raw) return emptyWorkspace();
    const parsed = JSON.parse(raw) as LoopsWorkspaceFile;
    if (!parsed.records) return emptyWorkspace();
    if (parsed.version === STORAGE_VERSION) return parsed;
    if (parsed.version === 1) {
      // These are crews the player earned by running optimizations; dropping them
      // on a schema bump (the previous behaviour) is real data loss, not cache
      // invalidation.
      const migrated: LoopsWorkspaceFile = {
        version: STORAGE_VERSION,
        records: Object.fromEntries(
          Object.entries(parsed.records).map(([key, list]) => [
            key,
            (list as unknown as LoopBestRecordV1[]).map(migrateV1Record),
          ]),
        ),
      };
      try {
        localStorage.setItem(storageKey(profileId), JSON.stringify(migrated));
      } catch {
        // Migration is still usable in memory even if it cannot be persisted.
      }
      return migrated;
    }
    // An unrecognised version is more likely corruption than a downgrade; stay
    // empty rather than risk interpreting foreign data as records.
    return emptyWorkspace();
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

function ci(value: number, low?: number, high?: number): MetricCi {
  return { value, ciLow: low ?? value, ciHigh: high ?? value };
}

/** Invert a rate and its interval (bounds swap). */
function invert(metric: MetricCi): MetricCi {
  return {
    value: 1 - metric.value,
    ciLow: 1 - metric.ciHigh,
    ciHigh: 1 - metric.ciLow,
  };
}

/**
 * The single metric a goal is judged on, with its interval.
 *
 * One definition shared by record scoring, recommendation picking, and rung status
 * — the mapping was previously duplicated per call site and drifting between them
 * would silently mean "best" and "cleared" disagreed about what the goal measures.
 */
export function primaryMetricForGoal(
  goalId: LoopGoalId,
  record: LoopBestRecord,
): MetricCi {
  switch (goalId) {
    case "one_round":
      return ci(
        record.roundOneKillRate,
        record.roundOneKillRateCiLow,
        record.roundOneKillRateCiHigh,
      );
    case "damage_dealt":
      return invert(
        ci(
          record.averageDefenderHullRemaining,
          record.averageDefenderHullRemainingCiLow,
          record.averageDefenderHullRemainingCiHigh,
        ),
      );
    case "kills_per_hull":
      return record.chainSuccessRate != null
        ? ci(
            record.chainSuccessRate,
            record.chainSuccessRateCiLow,
            record.chainSuccessRateCiHigh,
          )
        : ci(record.winRate, record.winRateCiLow, record.winRateCiHigh);
    case "no_hits":
    case "smallest_ship":
      return ci(record.winRate, record.winRateCiLow, record.winRateCiHigh);
  }
}

/**
 * Whether `candidate` should displace `incumbent` as the recorded best.
 *
 * Point estimates alone are not enough: a short, lucky run can post a higher win
 * rate than a long, rigorous one and — under the previous lexicographic comparison
 * — lock the better crew out permanently. So:
 *
 * 1. Non-overlapping intervals decide outright, in either direction.
 * 2. When intervals overlap the runs are not statistically distinguishable, so a
 *    real point-estimate lead is required *and* a challenger sampled far more
 *    thinly than the incumbent is refused.
 */
export function isGenuineImprovement(
  candidate: LoopBestRecord,
  incumbent: LoopBestRecord | null,
): boolean {
  if (!incumbent) return true;
  const c = primaryMetricForGoal(candidate.goalId, candidate);
  const i = primaryMetricForGoal(incumbent.goalId, incumbent);

  if (c.ciLow > i.ciHigh) return true;
  if (c.ciHigh < i.ciLow) return false;

  if (c.value <= i.value) return false;
  if (
    incumbent.simsPerCrew != null &&
    candidate.simsPerCrew != null &&
    candidate.simsPerCrew < incumbent.simsPerCrew * OVERLAP_TRUST_RATIO
  ) {
    return false;
  }
  return true;
}

/**
 * Ordering key for the stored alternates. The leading element always comes from
 * [`primaryMetricForGoal`] so ordering and the improvement gate agree on what the
 * goal optimizes; the remaining elements are goal-specific tie-breaks.
 */
function score(record: LoopBestRecord): readonly number[] {
  const primary = primaryMetricForGoal(record.goalId, record).value;
  switch (record.goalId) {
    case "one_round":
      return [primary, record.winRate, record.averageHullRemaining];
    case "damage_dealt":
      return [primary, record.winRate, record.averageHullRemaining];
    case "no_hits":
      return [primary, record.averageHullRemaining, record.roundOneKillRate];
    case "kills_per_hull":
      return [
        primary,
        record.chainHullScore ?? record.averageHullRemaining,
        record.winRate,
      ];
    case "smallest_ship":
      // Viability first, then the smallest hull that keeps it: this goal ranks by
      // cheapness among crews that still win, not by raw performance.
      return [
        primary >= 0.5 ? 1 : 0,
        -record.shipTier,
        -record.shipLevel,
        primary,
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

/** Why a result was not recorded, so callers can say something true about it. */
export type LoopSaveRejection =
  | "no_recommendation"
  | "off_policy_ship"
  | "not_improved"
  | "storage_failed";

export interface LoopSaveOutcome {
  saved: boolean;
  improved: boolean;
  record: LoopBestRecord | null;
  reason?: LoopSaveRejection;
}

export function saveLoopOptimizationResult(args: {
  profileId: string | null;
  context: LoopRunContext;
  shipId: string;
  shipTier: number;
  shipLevel: number;
  recommendations: readonly CrewRecommendation[];
  simsPerCrew?: number;
  strategy?: string;
  maxCandidates?: number | null;
  hostileDataVersion?: string | null;
  shipDataVersion?: string | null;
}): LoopSaveOutcome {
  const recommendation = chooseLoopRecommendation(
    args.context.goalId,
    args.recommendations,
  );
  if (!recommendation)
    return {
      saved: false,
      improved: false,
      record: null,
      reason: "no_recommendation",
    };

  // A loop that mandates a specific hull must not accept a best set on another
  // one. The ship picker is disabled in the Loops page, but the run itself happens
  // on the Workspace page where the ship dropdown is an ordinary control, so
  // without this check a post-handoff ship change is recorded silently.
  if (
    args.context.shipPolicy === "required" &&
    args.context.specialtyShipIds.length > 0 &&
    !args.context.specialtyShipIds.includes(args.shipId)
  ) {
    return {
      saved: false,
      improved: false,
      record: null,
      reason: "off_policy_ship",
    };
  }

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
    winRateCiLow: recommendation.win_rate_ci_low,
    winRateCiHigh: recommendation.win_rate_ci_high,
    roundOneKillRate: recommendation.r1_kill_rate,
    roundOneKillRateCiLow: recommendation.r1_kill_rate_ci_low,
    roundOneKillRateCiHigh: recommendation.r1_kill_rate_ci_high,
    averageHullRemaining: recommendation.avg_hull_remaining,
    averageHullRemainingCiLow: recommendation.avg_hull_remaining_ci_low,
    averageHullRemainingCiHigh: recommendation.avg_hull_remaining_ci_high,
    averageDefenderHullRemaining: recommendation.avg_defender_hull_remaining,
    averageDefenderHullRemainingCiLow:
      recommendation.avg_defender_hull_remaining_ci_low,
    averageDefenderHullRemainingCiHigh:
      recommendation.avg_defender_hull_remaining_ci_high,
    ...(recommendation.chain
      ? {
          chainSuccessRate: recommendation.chain.primary_success_rate,
          chainSuccessRateCiLow: recommendation.chain.primary_ci_low,
          chainSuccessRateCiHigh: recommendation.chain.primary_ci_high,
          chainHullScore: recommendation.chain.secondary_mean_given_primary,
          chainHullScoreCiLow: recommendation.chain.secondary_ci_low,
          chainHullScoreCiHigh: recommendation.chain.secondary_ci_high,
          chainKillsTarget: recommendation.chain.kills_target,
        }
      : {}),
    simsPerCrew: args.simsPerCrew,
    strategy: args.strategy,
    maxCandidates: args.maxCandidates ?? null,
    hostileDataVersion: args.hostileDataVersion ?? null,
    shipDataVersion: args.shipDataVersion ?? null,
    recordedAt: new Date().toISOString(),
  };

  const incumbent = [...previous].sort(compareScore)[0] ?? null;
  if (!isGenuineImprovement(record, incumbent)) {
    return {
      saved: false,
      improved: false,
      record: incumbent,
      reason: "not_improved",
    };
  }
  const ranked = [record, ...previous].sort(compareScore);

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
    // The crew *was* an improvement; only persistence failed (quota, private
    // mode). Reporting this as "not improved" — the previous behaviour — tells the
    // player the opposite of what happened.
    return {
      saved: false,
      improved: true,
      record,
      reason: "storage_failed",
    };
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
