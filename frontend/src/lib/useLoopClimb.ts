import { useCallback, useEffect, useRef, useState } from "react";
import {
  clearLoopClimbPlan,
  createLoopClimbPlan,
  DEFAULT_PER_RUNG_TIMEOUT_MS,
  type LoopClimbPlan,
  type LoopClimbRung,
  type LoopClimbSettings,
  loadLoopClimbPlan,
  runLoopClimb,
} from "./loopClimbRunner";
import type { LoopGoalId } from "./loopsCatalog";
import type { LoopRunContext } from "./loopsWorkspaceStorage";

export interface LoopClimbProgress {
  targetId: string;
  progress: number;
  phase?: string;
  etaSeconds?: number;
}

export interface StartClimbArgs {
  loopId: string;
  loopName: string;
  goalId: LoopGoalId;
  shipId: string;
  shipPolicy: LoopRunContext["shipPolicy"];
  specialtyShipIds: string[];
  shipTier: number;
  shipLevel: number;
  rungs: LoopClimbRung[];
  /** Index to resume from (the frontier), ignored when restarting from the bottom. */
  startAtIndex: number;
  settings?: Partial<LoopClimbSettings>;
}

const DEFAULT_SETTINGS: LoopClimbSettings = {
  simsPerCrew: 1000,
  maxCandidates: 200,
  strategy: "tiered",
  stopCondition: { type: "first_failure" },
  perRungTimeoutMs: DEFAULT_PER_RUNG_TIMEOUT_MS,
};

/**
 * Drives a sequential ladder climb from the Loops page.
 *
 * Deliberately does **not** live in `useWorkspace`: that hook is the single-crew
 * builder, and its loop handoff navigates to the Workspace page, which a multi-rung
 * unattended climb must never do — it would remount the page per rung and fight a
 * player who is looking at something else.
 */
export function useLoopClimb(profileId: string | null) {
  const [plan, setPlan] = useState<LoopClimbPlan | null>(null);
  const [progress, setProgress] = useState<LoopClimbProgress | null>(null);
  const cancelRef = useRef(false);
  const runningRef = useRef(false);

  // Surface an interrupted climb after a refresh. The plan is only bookkeeping —
  // every completed rung is already saved durably — so the honest move is to show
  // what was in flight and let the player restart, rather than silently resuming a
  // job whose server-side fate we no longer know.
  useEffect(() => {
    const persisted = loadLoopClimbPlan(profileId);
    if (persisted && persisted.status === "running" && !runningRef.current) {
      setPlan({ ...persisted, status: "cancelled", activeJobId: null });
      clearLoopClimbPlan(profileId);
    }
  }, [profileId]);

  const start = useCallback(
    async (args: StartClimbArgs) => {
      if (runningRef.current) return;
      const settings: LoopClimbSettings = {
        ...DEFAULT_SETTINGS,
        ...args.settings,
      };
      const fresh = createLoopClimbPlan({
        profileId,
        loopId: args.loopId,
        loopName: args.loopName,
        goalId: args.goalId,
        shipId: args.shipId,
        shipPolicy: args.shipPolicy,
        specialtyShipIds: args.specialtyShipIds,
        shipTier: args.shipTier,
        shipLevel: args.shipLevel,
        rungs: args.rungs,
        settings,
        startAtIndex: Math.max(0, args.startAtIndex),
        startedAt: new Date().toISOString(),
      });
      if (fresh.cursor >= fresh.rungs.length) {
        // Nothing left to climb; say so rather than starting an empty run.
        setPlan({ ...fresh, status: "done" });
        return;
      }
      cancelRef.current = false;
      runningRef.current = true;
      setPlan(fresh);
      setProgress(null);
      try {
        await runLoopClimb(
          fresh,
          {
            onPlan: setPlan,
            onProgress: setProgress,
          },
          () => cancelRef.current,
        );
      } finally {
        runningRef.current = false;
        clearLoopClimbPlan(profileId);
      }
    },
    [profileId],
  );

  const cancel = useCallback(() => {
    cancelRef.current = true;
  }, []);

  return {
    plan,
    progress,
    isClimbing: plan?.status === "running",
    start,
    cancel,
  };
}
