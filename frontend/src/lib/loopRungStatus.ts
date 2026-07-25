import type { LoopGoalId } from "./loopsCatalog";
import {
  type LoopBestRecord,
  type MetricCi,
  primaryMetricForGoal,
} from "./loopsWorkspaceStorage";

/**
 * Where a rung stands for the player.
 *
 * - `cleared` — measured, and confidently good enough to farm.
 * - `contested` — measured, but not yet convincingly clear.
 * - `untried` — reachable and never run.
 * - `locked` — the rung below is not cleared yet, so this one is likely out of
 *   reach. Advisory only; nothing stops the player running it.
 */
export type RungStatus = "cleared" | "contested" | "untried" | "locked";

export interface RungStatusInfo {
  status: RungStatus;
  /** The goal's primary metric with its interval, for display. Null when untried. */
  metric: MetricCi | null;
  best: LoopBestRecord | null;
}

/**
 * Bar a rung must clear, applied to the **lower bound** of the goal's primary
 * metric rather than its point estimate — a crew whose interval merely touches the
 * threshold has not demonstrated it can farm the rung repeatably.
 *
 * These are judgement calls, not measured constants. `no_hits` sits higher because
 * that goal is explicitly about decisive, clean wins; `smallest_ship` sits lower
 * because it asks "is this cheap hull still viable", which is a weaker claim than
 * "this rung is comfortable".
 */
export const RUNG_CLEAR_CI_LOW_THRESHOLD: Record<LoopGoalId, number> = {
  one_round: 0.6,
  damage_dealt: 0.6,
  no_hits: 0.7,
  kills_per_hull: 0.6,
  smallest_ship: 0.5,
};

export function rungStatus(
  goalId: LoopGoalId,
  best: LoopBestRecord | null,
  reachable: boolean,
): RungStatusInfo {
  if (!best) {
    return {
      status: reachable ? "untried" : "locked",
      metric: null,
      best: null,
    };
  }
  const metric = primaryMetricForGoal(goalId, best);
  const cleared = metric.ciLow >= RUNG_CLEAR_CI_LOW_THRESHOLD[goalId];
  return { status: cleared ? "cleared" : "contested", metric, best };
}

export interface LoopFrontier {
  /** Index of the highest consecutively-cleared rung in climb order; -1 if none. */
  frontierIndex: number;
  /** The rung to attack next, or null when the ladder is fully cleared. */
  nextTargetId: string | null;
  /** Status per target id. */
  statuses: Map<string, RungStatusInfo>;
  clearedCount: number;
}

/**
 * Classify a whole ladder and locate the player's frontier.
 *
 * `climbOrderTargetIds` must be **ascending** by level (see
 * `resolveLoopHostilesAscending`) — the display ladder is sorted the other way.
 *
 * Reachability is strictly sequential: a rung counts as reachable only once the one
 * below it is cleared. That is deliberately stricter than reality (a player may well
 * beat rung 5 having skipped rung 4) because its purpose is to answer "where should
 * I spend effort next", and the answer is the first rung that isn't done yet.
 */
export function loopFrontier(
  climbOrderTargetIds: readonly string[],
  bestByTargetId: ReadonlyMap<string, LoopBestRecord | null>,
  goalId: LoopGoalId,
): LoopFrontier {
  const statuses = new Map<string, RungStatusInfo>();
  let frontierIndex = -1;
  let reachable = true;
  let clearedCount = 0;

  for (const [index, targetId] of climbOrderTargetIds.entries()) {
    const info = rungStatus(
      goalId,
      bestByTargetId.get(targetId) ?? null,
      reachable,
    );
    statuses.set(targetId, info);
    if (info.status === "cleared") {
      clearedCount += 1;
      // Only advance the frontier while the run of cleared rungs is unbroken, so a
      // cleared rung above a contested gap does not overstate progress.
      if (frontierIndex === index - 1) frontierIndex = index;
    } else {
      reachable = false;
    }
  }

  const nextTargetId = climbOrderTargetIds[frontierIndex + 1] ?? null;
  return { frontierIndex, nextTargetId, statuses, clearedCount };
}
