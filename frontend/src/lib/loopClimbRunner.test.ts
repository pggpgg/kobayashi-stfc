import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LoopClimbPlan } from "./loopClimbRunner";

const optimizeStart = vi.fn();
const getOptimizeStatus = vi.fn();
const cancelOptimizeJob = vi.fn();

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    optimizeStart: (...args: unknown[]) => optimizeStart(...args),
    getOptimizeStatus: (...args: unknown[]) => getOptimizeStatus(...args),
    cancelOptimizeJob: (...args: unknown[]) => cancelOptimizeJob(...args),
  };
});

const {
  clearLoopClimbPlan,
  createLoopClimbPlan,
  loadLoopClimbPlan,
  resolveClimbStrategy,
  runLoopClimb,
} = await import("./loopClimbRunner");

function recommendation(overrides: Record<string, number> = {}) {
  return {
    captain: "Khan",
    bridge: ["A", "B"],
    below_decks: ["C"],
    win_rate: 0.99,
    win_rate_ci_low: 0.98,
    win_rate_ci_high: 1,
    stall_rate: 0,
    stall_rate_ci_low: 0,
    stall_rate_ci_high: 0,
    loss_rate: 0.01,
    loss_rate_ci_low: 0,
    loss_rate_ci_high: 0.02,
    r1_kill_rate: 0.9,
    r1_kill_rate_ci_low: 0.88,
    r1_kill_rate_ci_high: 0.92,
    avg_hull_remaining: 0.9,
    avg_hull_remaining_ci_low: 0.88,
    avg_hull_remaining_ci_high: 0.92,
    avg_defender_hull_remaining: 0,
    avg_defender_hull_remaining_ci_low: 0,
    avg_defender_hull_remaining_ci_high: 0,
    ...overrides,
  };
}

function doneStatus(overrides: Record<string, unknown> = {}) {
  return {
    status: "done",
    progress: 100,
    result: {
      status: "ok",
      scenario: { ship: "mantis", hostile: "t1", sims: 100, seed: 1 },
      recommendations: [recommendation()],
    },
    ...overrides,
  };
}

function plan(overrides: Partial<LoopClimbPlan> = {}): LoopClimbPlan {
  return createLoopClimbPlan({
    profileId: "p1",
    loopId: "actian",
    loopName: "Actian",
    goalId: "no_hits",
    shipId: "mantis",
    shipPolicy: "recommended",
    specialtyShipIds: ["mantis"],
    shipTier: 5,
    shipLevel: 25,
    rungs: [
      { targetId: "t1", targetName: "Low", targetLevel: 20 },
      { targetId: "t2", targetName: "Mid", targetLevel: 30 },
      { targetId: "t3", targetName: "High", targetLevel: 40 },
    ],
    settings: {
      simsPerCrew: 100,
      maxCandidates: 20,
      strategy: "tiered",
      stopCondition: { type: "full_ladder" },
      perRungTimeoutMs: 60_000,
    },
    startAtIndex: 0,
    startedAt: "2026-07-24T00:00:00.000Z",
    ...overrides,
  });
}

const deps = {
  now: () => 0,
  sleep: async () => {},
};

describe("runLoopClimb", () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    optimizeStart.mockReset();
    getOptimizeStatus.mockReset();
    cancelOptimizeJob.mockReset();
    cancelOptimizeJob.mockResolvedValue(undefined);
  });

  it("never has two optimize jobs in flight at once", async () => {
    // The server takes its single CPU permit *before* returning a job id and holds
    // it for the job's whole life, so a second start issued while one runs does not
    // queue — the HTTP request hangs. This is the invariant the whole runner design
    // exists to guarantee, so assert the actual event ordering rather than just a
    // call count: every start must be followed by that job reaching a terminal
    // status before the next start is issued.
    const events: string[] = [];
    let jobSeq = 0;
    optimizeStart.mockImplementation(async () => {
      jobSeq += 1;
      events.push(`start:${jobSeq}`);
      return { job_id: `job-${jobSeq}` };
    });
    // Two polls per rung: one still running, then terminal — so an incorrectly
    // parallel runner would have room to interleave a second start.
    const pollCounts = new Map<string, number>();
    getOptimizeStatus.mockImplementation(async (jobId: string) => {
      const seen = (pollCounts.get(jobId) ?? 0) + 1;
      pollCounts.set(jobId, seen);
      if (seen < 2) {
        events.push(`running:${jobId}`);
        return { status: "running", progress: 50 };
      }
      events.push(`done:${jobId}`);
      return doneStatus();
    });

    await runLoopClimb(plan(), { onPlan: () => {} }, () => false, deps);

    expect(optimizeStart).toHaveBeenCalledTimes(3);
    // Walk the log: no start may appear while a previous job is unfinished.
    let open: string | null = null;
    for (const event of events) {
      const [kind, id] = event.split(":");
      if (kind === "start") {
        expect(open).toBeNull();
        open = `job-${id}`;
      } else if (kind === "done") {
        expect(open).toBe(id);
        open = null;
      }
    }
    expect(open).toBeNull();
    expect(events).toEqual([
      "start:1",
      "running:job-1",
      "done:job-1",
      "start:2",
      "running:job-2",
      "done:job-2",
      "start:3",
      "running:job-3",
      "done:job-3",
    ]);
  });

  it("warm-starts each rung from the rung below's saved crew", async () => {
    optimizeStart.mockResolvedValue({ job_id: "job" });
    getOptimizeStatus.mockResolvedValue(doneStatus());

    await runLoopClimb(plan(), { onPlan: () => {} }, () => false, deps);

    const first = optimizeStart.mock.calls[0][0] as {
      warm_start_crews?: unknown[];
    };
    const second = optimizeStart.mock.calls[1][0] as {
      warm_start_crews?: { captain: string }[];
    };
    // Nothing saved yet when the first rung runs.
    expect(first.warm_start_crews).toBeUndefined();
    // The second rung inherits the crew the first rung just recorded.
    expect(second.warm_start_crews?.[0]?.captain).toBe("Khan");
  });

  it("stops at the first rung it cannot clear under first_failure", async () => {
    optimizeStart.mockResolvedValue({ job_id: "job" });
    getOptimizeStatus.mockResolvedValueOnce(doneStatus()).mockResolvedValue(
      doneStatus({
        result: {
          status: "ok",
          scenario: { ship: "mantis", hostile: "t2", sims: 100, seed: 1 },
          // Well below the no_hits bar, so the rung is contested, not cleared.
          recommendations: [
            recommendation({
              win_rate: 0.2,
              win_rate_ci_low: 0.1,
              win_rate_ci_high: 0.3,
            }),
          ],
        },
      }),
    );

    const final = await runLoopClimb(
      plan({
        settings: {
          ...plan().settings,
          stopCondition: { type: "first_failure" },
        },
      }),
      { onPlan: () => {} },
      () => false,
      deps,
    );

    expect(optimizeStart).toHaveBeenCalledTimes(2);
    expect(final.results.map((r) => r.outcome)).toEqual([
      "cleared",
      "contested",
    ]);
    expect(final.status).toBe("done");
  });

  it("continues past a contested rung under full_ladder", async () => {
    optimizeStart.mockResolvedValue({ job_id: "job" });
    getOptimizeStatus.mockResolvedValue(
      doneStatus({
        result: {
          status: "ok",
          scenario: { ship: "mantis", hostile: "t", sims: 100, seed: 1 },
          recommendations: [
            recommendation({
              win_rate: 0.2,
              win_rate_ci_low: 0.1,
              win_rate_ci_high: 0.3,
            }),
          ],
        },
      }),
    );

    const final = await runLoopClimb(
      plan(),
      { onPlan: () => {} },
      () => false,
      deps,
    );
    expect(optimizeStart).toHaveBeenCalledTimes(3);
    expect(final.results.every((r) => r.outcome === "contested")).toBe(true);
  });

  it("stops after the requested rung under reach_target", async () => {
    optimizeStart.mockResolvedValue({ job_id: "job" });
    getOptimizeStatus.mockResolvedValue(doneStatus());

    const final = await runLoopClimb(
      plan({
        settings: {
          ...plan().settings,
          stopCondition: { type: "reach_target", targetId: "t2" },
        },
      }),
      { onPlan: () => {} },
      () => false,
      deps,
    );

    expect(optimizeStart).toHaveBeenCalledTimes(2);
    expect(final.results[final.results.length - 1]?.targetId).toBe("t2");
  });

  it("cancels the running job and issues no further starts", async () => {
    optimizeStart.mockResolvedValue({ job_id: "job-1" });
    getOptimizeStatus.mockResolvedValue({ status: "running", progress: 10 });

    let cancelled = false;
    const climb = runLoopClimb(
      plan(),
      {
        onPlan: () => {
          cancelled = true;
        },
      },
      () => cancelled,
      deps,
    );

    const final = await climb;
    expect(cancelOptimizeJob).toHaveBeenCalledWith("job-1");
    expect(optimizeStart).toHaveBeenCalledTimes(1);
    expect(final.status).toBe("cancelled");
  });

  it("marks a rung interrupted when its job has vanished rather than skipping it", async () => {
    optimizeStart.mockResolvedValue({ job_id: "job-1" });
    getOptimizeStatus.mockRejectedValue(new Error("404 Not Found"));

    const final = await runLoopClimb(
      plan(),
      { onPlan: () => {} },
      () => false,
      deps,
    );

    // A vanished job leaves the outcome genuinely unknown, so the climb halts
    // instead of treating the rung as attempted.
    expect(final.results[0]?.outcome).toBe("interrupted");
    expect(optimizeStart).toHaveBeenCalledTimes(1);
  });

  it("times a rung out and cancels rather than polling forever", async () => {
    optimizeStart.mockResolvedValue({ job_id: "job-1" });
    getOptimizeStatus.mockResolvedValue({ status: "running", progress: 5 });

    let clock = 0;
    const final = await runLoopClimb(
      plan({
        settings: { ...plan().settings, perRungTimeoutMs: 1000 },
      }),
      { onPlan: () => {} },
      () => false,
      {
        now: () => {
          clock += 600;
          return clock;
        },
        sleep: async () => {},
      },
    );

    expect(final.results[0]?.outcome).toBe("timed_out");
    expect(cancelOptimizeJob).toHaveBeenCalledWith("job-1");
  });

  it("records an error outcome when the server reports one", async () => {
    optimizeStart.mockResolvedValue({ job_id: "job-1" });
    getOptimizeStatus.mockResolvedValue({
      status: "error",
      error: "boom",
    });

    const final = await runLoopClimb(
      plan({
        settings: {
          ...plan().settings,
          stopCondition: { type: "first_failure" },
        },
      }),
      { onPlan: () => {} },
      () => false,
      deps,
    );

    expect(final.results[0]?.outcome).toBe("error");
    expect(final.results[0]?.error).toBe("boom");
  });

  it("reports a cancellation point when the server gives no error message", async () => {
    optimizeStart.mockResolvedValue({ job_id: "job-1" });
    getOptimizeStatus.mockResolvedValue({
      status: "error",
      cancellation_point: "tiered_scout",
    });

    const final = await runLoopClimb(
      plan({
        settings: {
          ...plan().settings,
          stopCondition: { type: "first_failure" },
        },
      }),
      { onPlan: () => {} },
      () => false,
      deps,
    );

    // The server has no "cancelled" status — it ends as error plus this field.
    expect(final.results[0]?.error).toContain("tiered_scout");
  });

  it("resumes at the frontier unless asked to restart from the bottom", () => {
    const resumed = createLoopClimbPlan({
      profileId: "p1",
      loopId: "actian",
      loopName: "Actian",
      goalId: "no_hits",
      shipId: "mantis",
      shipPolicy: "recommended",
      specialtyShipIds: [],
      shipTier: 5,
      shipLevel: 25,
      rungs: [
        { targetId: "t1", targetName: "Low", targetLevel: 20 },
        { targetId: "t2", targetName: "Mid", targetLevel: 30 },
        { targetId: "t3", targetName: "High", targetLevel: 40 },
      ],
      settings: {
        simsPerCrew: 100,
        maxCandidates: null,
        strategy: "tiered",
        stopCondition: { type: "full_ladder" },
        perRungTimeoutMs: 1000,
      },
      startAtIndex: 2,
      startedAt: "2026-07-24T00:00:00.000Z",
    });
    expect(resumed.cursor).toBe(2);
    const restarted = createLoopClimbPlan({
      profileId: "p1",
      loopId: "actian",
      loopName: "Actian",
      goalId: "no_hits",
      shipId: "mantis",
      shipPolicy: "recommended",
      specialtyShipIds: [],
      shipTier: 5,
      shipLevel: 25,
      rungs: [{ targetId: "t1", targetName: "Low", targetLevel: 20 }],
      settings: {
        simsPerCrew: 100,
        maxCandidates: null,
        strategy: "tiered",
        stopCondition: { type: "full_ladder" },
        perRungTimeoutMs: 1000,
        restartFromBottom: true,
      },
      startAtIndex: 2,
      startedAt: "2026-07-24T00:00:00.000Z",
    });
    expect(restarted.cursor).toBe(0);
  });
});

describe("resolveClimbStrategy", () => {
  const withGoalAndCandidates = (
    goalId: LoopClimbPlan["goalId"],
    maxCandidates: number | null,
  ) => {
    const base = plan();
    return {
      ...base,
      goalId,
      settings: { ...base.settings, maxCandidates },
    };
  };

  it("upgrades to exhaustive for goals the default pruning would undermine", () => {
    // one_round ranks on round-one kills, but genetic/tiered prune toward a
    // win-rate blend, so the crew this goal wants can be discarded before it is
    // ever simulated. Exhaustive does no such pruning.
    expect(resolveClimbStrategy(withGoalAndCandidates("one_round", 500))).toBe(
      "exhaustive",
    );
    expect(
      resolveClimbStrategy(withGoalAndCandidates("damage_dealt", 500)),
    ).toBe("exhaustive");
  });

  it("keeps the configured strategy for goals that match the default fitness", () => {
    expect(resolveClimbStrategy(withGoalAndCandidates("no_hits", 500))).toBe(
      "tiered",
    );
    expect(
      resolveClimbStrategy(withGoalAndCandidates("kills_per_hull", 500)),
    ).toBe("tiered");
  });

  it("refuses exhaustive on an unbounded or large pool during an unattended climb", () => {
    // Nobody is watching a climb to notice a rung that never finishes.
    expect(resolveClimbStrategy(withGoalAndCandidates("one_round", null))).toBe(
      "tiered",
    );
    expect(
      resolveClimbStrategy(withGoalAndCandidates("one_round", 500_000)),
    ).toBe("tiered");
  });
});

describe("climb plan persistence", () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it("round-trips a plan so a refresh mid-climb can resume", () => {
    const saved = plan();
    sessionStorage.setItem("kobayashi_loop_climb_v1:p1", JSON.stringify(saved));
    expect(loadLoopClimbPlan("p1")?.loopId).toBe("actian");
  });

  it("ignores a plan written by a different schema", () => {
    sessionStorage.setItem(
      "kobayashi_loop_climb_v1:p1",
      JSON.stringify({ ...plan(), version: 99 }),
    );
    expect(loadLoopClimbPlan("p1")).toBeNull();
  });

  it("uses its own key so the workspace resume effects cannot adopt a climb job", () => {
    const saved = plan();
    sessionStorage.setItem("kobayashi_loop_climb_v1:p1", JSON.stringify(saved));
    // The single-run handoff and active-job keys must stay untouched.
    expect(sessionStorage.getItem("kobayashi_loops_pending_v1:p1")).toBeNull();
    expect(
      sessionStorage.getItem("kobayashi_active_optimize_job_v1"),
    ).toBeNull();
    clearLoopClimbPlan("p1");
    expect(loadLoopClimbPlan("p1")).toBeNull();
  });
});
