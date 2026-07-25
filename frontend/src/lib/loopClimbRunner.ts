import {
  cancelOptimizeJob,
  getOptimizeStatus,
  type OptimizerStrategyType,
  optimizeStart,
} from "./api";
import { rungStatus } from "./loopRungStatus";
import type { LoopGoalId } from "./loopsCatalog";
import { loopGoal } from "./loopsCatalog";
import {
  getLoopBestRecord,
  type LoopBestRecord,
  type LoopRunContext,
  saveLoopOptimizationResult,
} from "./loopsWorkspaceStorage";
import { buildWorkspaceOptimizeStartBody } from "./workspaceRequests";

const CLIMB_PREFIX = "kobayashi_loop_climb_v1";
const PLAN_VERSION = 1;

/** Status poll interval. A multi-minute unattended climb does not need finer. */
const POLL_INTERVAL_MS = 500;

/** Default ceiling per rung before the job is cancelled and the rung marked timed out. */
export const DEFAULT_PER_RUNG_TIMEOUT_MS = 10 * 60 * 1000;

/**
 * Largest candidate pool an unattended climb will search exhaustively.
 *
 * Goals that rank on something other than win rate want exhaustive search, because
 * the optimizer's own pruning discards candidates they care about. But nobody is
 * watching a climb to notice a runaway rung, and `/api/optimize/estimate` ignores
 * chain-grind cost, so the affordability test here is the bounded candidate count
 * rather than a predicted runtime.
 */
const MAX_EXHAUSTIVE_CANDIDATES = 2000;

export type LoopClimbStopCondition =
  | { type: "first_failure" }
  | { type: "reach_target"; targetId: string }
  | { type: "full_ladder" };

export interface LoopClimbSettings {
  simsPerCrew: number;
  maxCandidates: number | null;
  strategy: OptimizerStrategyType;
  stopCondition: LoopClimbStopCondition;
  perRungTimeoutMs: number;
  /** Start from the bottom rather than resuming at the frontier. */
  restartFromBottom?: boolean;
}

export type LoopClimbRungOutcome =
  | "cleared"
  | "contested"
  | "error"
  | "timed_out"
  | "interrupted";

export interface LoopClimbRungResult {
  targetId: string;
  targetName: string;
  targetLevel: number;
  outcome: LoopClimbRungOutcome;
  record: LoopBestRecord | null;
  error?: string;
}

export interface LoopClimbRung {
  targetId: string;
  targetName: string;
  targetLevel: number;
}

export interface LoopClimbPlan {
  version: number;
  profileId: string | null;
  loopId: string;
  loopName: string;
  goalId: LoopGoalId;
  shipId: string;
  shipPolicy: LoopRunContext["shipPolicy"];
  specialtyShipIds: string[];
  shipTier: number;
  shipLevel: number;
  settings: LoopClimbSettings;
  /** Frozen at plan creation, ascending by level. */
  rungs: LoopClimbRung[];
  cursor: number;
  results: LoopClimbRungResult[];
  activeJobId: string | null;
  startedAt: string;
  status: "running" | "cancelled" | "done";
}

function climbKey(profileId: string | null): string {
  return `${CLIMB_PREFIX}:${profileId?.trim() || "default"}`;
}

/**
 * The plan is resumption bookkeeping, not results.
 *
 * `sessionStorage` is sufficient — and its own key, deliberately not the pending-run
 * or active-job keys, because `useWorkspace`'s resume effects watch those and would
 * otherwise adopt a climb's job as a manual single run. Every rung's result is
 * written durably by `saveLoopOptimizationResult` the moment it completes, so losing
 * the plan costs nothing but a re-click: the frontier is recomputed from the durable
 * records.
 */
export function saveLoopClimbPlan(plan: LoopClimbPlan): void {
  try {
    sessionStorage.setItem(climbKey(plan.profileId), JSON.stringify(plan));
  } catch {}
}

export function loadLoopClimbPlan(
  profileId: string | null,
): LoopClimbPlan | null {
  try {
    const raw = sessionStorage.getItem(climbKey(profileId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as LoopClimbPlan;
    if (parsed.version !== PLAN_VERSION || !Array.isArray(parsed.rungs)) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

export function clearLoopClimbPlan(profileId: string | null): void {
  try {
    sessionStorage.removeItem(climbKey(profileId));
  } catch {}
}

export interface LoopClimbCallbacks {
  /** Called whenever the plan changes so the UI can re-render. */
  onPlan: (plan: LoopClimbPlan) => void;
  /** Live per-rung progress from the job status stream. */
  onProgress?: (info: {
    targetId: string;
    progress: number;
    phase?: string;
    etaSeconds?: number;
  }) => void;
}

interface RunnerDeps {
  now: () => number;
  sleep: (ms: number) => Promise<void>;
}

const defaultDeps: RunnerDeps = {
  now: () => Date.now(),
  sleep: (ms) =>
    new Promise((resolve) => {
      setTimeout(resolve, ms);
    }),
};

function isTerminal(status: string): boolean {
  return status === "done" || status === "error";
}

/**
 * Strategy for one rung of an unattended climb.
 *
 * Upgrades to exhaustive for goals whose ranking the optimizer's win-rate-first
 * pruning would undermine — but only with a bounded, small candidate pool, since an
 * unattended run has nobody to notice a rung that never finishes.
 */
export function resolveClimbStrategy(
  plan: LoopClimbPlan,
): OptimizerStrategyType {
  const wantsExhaustive =
    loopGoal(plan.goalId).requestsExhaustiveSearch === true;
  const bounded =
    plan.settings.maxCandidates != null &&
    plan.settings.maxCandidates > 0 &&
    plan.settings.maxCandidates <= MAX_EXHAUSTIVE_CANDIDATES;
  return wantsExhaustive && bounded ? "exhaustive" : plan.settings.strategy;
}

function stopAfter(
  plan: LoopClimbPlan,
  outcome: LoopClimbRungOutcome,
  targetId: string,
): boolean {
  switch (plan.settings.stopCondition.type) {
    case "first_failure":
      return outcome !== "cleared";
    case "reach_target":
      return plan.settings.stopCondition.targetId === targetId;
    case "full_ladder":
      // Still stop on conditions that mean we cannot trust continuing.
      return outcome === "interrupted";
  }
}

/**
 * Climb a ladder one rung at a time.
 *
 * **Strictly sequential by construction, and that is not a stylistic choice.** The
 * server takes its CPU permit *inside* the optimize/start handler before returning a
 * job id, and holds it for the job's whole life, with a default of one permit. A
 * second start issued while one is running does not queue — the HTTP request hangs.
 * So the next rung is only ever launched from the previous rung's terminal status.
 *
 * Polling rather than SSE: resuming after a refresh is a single status call, and an
 * unattended background climb has no use for sub-second progress fidelity.
 */
export async function runLoopClimb(
  initial: LoopClimbPlan,
  callbacks: LoopClimbCallbacks,
  isCancelled: () => boolean,
  deps: RunnerDeps = defaultDeps,
): Promise<LoopClimbPlan> {
  let plan = initial;

  const publish = () => {
    saveLoopClimbPlan(plan);
    callbacks.onPlan(plan);
  };

  while (plan.status === "running" && plan.cursor < plan.rungs.length) {
    if (isCancelled()) {
      plan = { ...plan, status: "cancelled", activeJobId: null };
      publish();
      return plan;
    }

    const rung = plan.rungs[plan.cursor];

    // Adjacent rungs want similar crews, so seed from the rung below plus this
    // rung's own prior best when re-climbing. Both are only seeds — the optimizer
    // still decides what wins.
    const warmStartCrews = [
      plan.cursor > 0
        ? getLoopBestRecord(
            plan.profileId,
            plan.loopId,
            plan.rungs[plan.cursor - 1].targetId,
            plan.goalId,
          )
        : null,
      getLoopBestRecord(
        plan.profileId,
        plan.loopId,
        rung.targetId,
        plan.goalId,
      ),
    ]
      .filter((record): record is LoopBestRecord => record != null)
      .map((record) => ({
        captain: record.crew.captain,
        bridge: record.crew.bridge,
        below_decks: record.crew.belowDecks,
      }));

    const body = buildWorkspaceOptimizeStartBody({
      shipId: plan.shipId,
      scenarioId: rung.targetId,
      simsPerCrew: plan.settings.simsPerCrew,
      maxCandidates: plan.settings.maxCandidates,
      optimizerStrategy: resolveClimbStrategy(plan),
      belowDecksPoolMode: "strict",
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: "ordered",
      shipTier: plan.shipTier,
      shipLevel: plan.shipLevel,
      warmStartCrews: warmStartCrews.length > 0 ? warmStartCrews : undefined,
      chainGrind:
        plan.goalId === "kills_per_hull"
          ? {
              enabled: true,
              kills_target: 5,
              secondary: "max_loot_per_hull_proxy",
            }
          : undefined,
    });

    let jobId: string;
    try {
      const started = await optimizeStart(body, plan.profileId);
      jobId = started.job_id;
    } catch (cause) {
      plan = {
        ...plan,
        status: "done",
        activeJobId: null,
        results: [
          ...plan.results,
          {
            targetId: rung.targetId,
            targetName: rung.targetName,
            targetLevel: rung.targetLevel,
            outcome: "error",
            record: null,
            error: cause instanceof Error ? cause.message : String(cause),
          },
        ],
      };
      publish();
      return plan;
    }

    plan = { ...plan, activeJobId: jobId };
    publish();

    const result = await awaitRung(
      plan,
      jobId,
      rung,
      callbacks,
      isCancelled,
      deps,
    );
    plan = { ...plan, activeJobId: null, results: [...plan.results, result] };

    if (result.outcome === "interrupted" && isCancelled()) {
      plan = { ...plan, status: "cancelled" };
      publish();
      return plan;
    }

    if (stopAfter(plan, result.outcome, rung.targetId)) {
      plan = { ...plan, status: "done" };
      publish();
      return plan;
    }

    plan = { ...plan, cursor: plan.cursor + 1 };
    publish();
  }

  plan = { ...plan, status: "done", activeJobId: null };
  publish();
  return plan;
}

/**
 * Poll one rung's job to completion and record its result.
 *
 * Note the server has no "cancelled" status: a cancelled job reports
 * `status: "error"` with `cancellation_point` set.
 */
async function awaitRung(
  plan: LoopClimbPlan,
  jobId: string,
  rung: LoopClimbRung,
  callbacks: LoopClimbCallbacks,
  isCancelled: () => boolean,
  deps: RunnerDeps,
): Promise<LoopClimbRungResult> {
  const startedAt = deps.now();
  const base = {
    targetId: rung.targetId,
    targetName: rung.targetName,
    targetLevel: rung.targetLevel,
  };

  for (;;) {
    if (isCancelled()) {
      await cancelOptimizeJob(jobId).catch(() => {});
      return { ...base, outcome: "interrupted", record: null };
    }
    if (deps.now() - startedAt > plan.settings.perRungTimeoutMs) {
      await cancelOptimizeJob(jobId).catch(() => {});
      return {
        ...base,
        outcome: "timed_out",
        record: null,
        error: "Rung exceeded its time budget",
      };
    }

    let status: Awaited<ReturnType<typeof getOptimizeStatus>>;
    try {
      status = await getOptimizeStatus(jobId);
    } catch (cause) {
      // A vanished job (server restart) leaves this rung's outcome genuinely
      // unknown. Say so and stop rather than skipping it as if it had been tried.
      return {
        ...base,
        outcome: "interrupted",
        record: null,
        error: cause instanceof Error ? cause.message : String(cause),
      };
    }

    callbacks.onProgress?.({
      targetId: rung.targetId,
      progress: status.progress ?? 0,
      phase: status.phase,
      etaSeconds: status.eta_seconds,
    });

    if (!isTerminal(status.status)) {
      await deps.sleep(POLL_INTERVAL_MS);
      continue;
    }

    if (status.status === "error" || !status.result) {
      return {
        ...base,
        outcome: "error",
        record: null,
        error:
          status.error ??
          (status.cancellation_point
            ? `Cancelled during ${status.cancellation_point}`
            : "Optimization failed"),
      };
    }

    const context: LoopRunContext = {
      loopId: plan.loopId,
      loopName: plan.loopName,
      targetId: rung.targetId,
      targetName: rung.targetName,
      targetLevel: rung.targetLevel,
      goalId: plan.goalId,
      shipId: plan.shipId,
      shipPolicy: plan.shipPolicy,
      specialtyShipIds: plan.specialtyShipIds,
    };
    const saved = saveLoopOptimizationResult({
      profileId: plan.profileId,
      context,
      shipId: plan.shipId,
      shipTier: plan.shipTier,
      shipLevel: plan.shipLevel,
      recommendations: status.result.recommendations ?? [],
      simsPerCrew: plan.settings.simsPerCrew,
      strategy: plan.settings.strategy,
      maxCandidates: plan.settings.maxCandidates,
    });

    // Judge the rung on whatever now stands as its best — a run that failed to
    // improve on an already-cleared rung is still a cleared rung.
    const best =
      saved.record ??
      getLoopBestRecord(
        plan.profileId,
        plan.loopId,
        rung.targetId,
        plan.goalId,
      );
    const status_ = rungStatus(plan.goalId, best, true);
    return {
      ...base,
      outcome: status_.status === "cleared" ? "cleared" : "contested",
      record: best,
    };
  }
}

/** Build a fresh plan. `rungs` must be ascending by level. */
export function createLoopClimbPlan(args: {
  profileId: string | null;
  loopId: string;
  loopName: string;
  goalId: LoopGoalId;
  shipId: string;
  shipPolicy: LoopRunContext["shipPolicy"];
  specialtyShipIds: string[];
  shipTier: number;
  shipLevel: number;
  rungs: LoopClimbRung[];
  settings: LoopClimbSettings;
  startAtIndex: number;
  startedAt: string;
}): LoopClimbPlan {
  return {
    version: PLAN_VERSION,
    profileId: args.profileId,
    loopId: args.loopId,
    loopName: args.loopName,
    goalId: args.goalId,
    shipId: args.shipId,
    shipPolicy: args.shipPolicy,
    specialtyShipIds: args.specialtyShipIds,
    shipTier: args.shipTier,
    shipLevel: args.shipLevel,
    settings: args.settings,
    rungs: args.rungs,
    cursor: args.settings.restartFromBottom ? 0 : args.startAtIndex,
    results: [],
    activeJobId: null,
    startedAt: args.startedAt,
    status: "running",
  };
}
