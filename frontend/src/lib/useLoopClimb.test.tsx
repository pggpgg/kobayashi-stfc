import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "./api";
import {
  createLoopClimbPlan,
  loadLoopClimbPlan,
  saveLoopClimbPlan,
} from "./loopClimbRunner";
import { type StartClimbArgs, useLoopClimb } from "./useLoopClimb";

vi.mock("./api", async (original) => ({
  ...(await original<typeof import("./api")>()),
  optimizeStart: vi.fn(),
  getOptimizeStatus: vi.fn(),
  cancelOptimizeJob: vi.fn(),
}));

const args: StartClimbArgs = {
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
    { targetId: "t2", targetName: "High", targetLevel: 30 },
  ],
  startAtIndex: 0,
  settings: { stopCondition: { type: "full_ladder" } },
};

beforeEach(() => {
  vi.clearAllMocks();
  sessionStorage.clear();
  localStorage.clear();
  vi.mocked(api.optimizeStart).mockResolvedValue({ job_id: "job-1" });
  vi.mocked(api.cancelOptimizeJob).mockResolvedValue(undefined);
  vi.mocked(api.getOptimizeStatus).mockResolvedValue({ status: "error" });
});
afterEach(() => {
  vi.useRealTimers();
});

describe("#342 loop climb job ownership", () => {
  it("clears an old profile's live UI when cancelling it on a profile switch", async () => {
    let finish!: (status: { status: string }) => void;
    vi.mocked(api.getOptimizeStatus).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const { result, rerender } = renderHook(
      ({ profile }) => useLoopClimb(profile),
      { initialProps: { profile: "p1" } },
    );
    let run!: Promise<void>;
    act(() => {
      run = result.current.start(args);
    });
    await waitFor(() =>
      expect(api.getOptimizeStatus).toHaveBeenCalledWith("job-1"),
    );
    rerender({ profile: "p2" });
    expect(result.current.plan).toBeNull();
    expect(api.cancelOptimizeJob).toHaveBeenCalledWith("job-1");
    await act(async () => {
      finish({ status: "error" });
      await run;
    });
    expect(result.current.plan).toBeNull();
  });
  it("cancels the active server job immediately on unmount and never starts another rung", async () => {
    let finishPoll!: (status: { status: string }) => void;
    vi.mocked(api.getOptimizeStatus).mockReturnValue(
      new Promise((resolve) => {
        finishPoll = resolve;
      }),
    );
    const { result, unmount } = renderHook(() => useLoopClimb("p1"));
    let run!: Promise<void>;
    act(() => {
      run = result.current.start(args);
    });
    await waitFor(() =>
      expect(api.getOptimizeStatus).toHaveBeenCalledWith("job-1"),
    );
    unmount();
    expect(api.cancelOptimizeJob).toHaveBeenCalledWith("job-1");
    await act(async () => {
      finishPoll({ status: "error" });
      await run;
    });
    expect(api.optimizeStart).toHaveBeenCalledTimes(1);
  });

  it("cancels a job whose start response arrives after unmount", async () => {
    let finishStart!: (response: { job_id: string }) => void;
    vi.mocked(api.optimizeStart).mockReturnValue(
      new Promise((resolve) => {
        finishStart = resolve;
      }),
    );
    const { result, unmount } = renderHook(() => useLoopClimb("p1"));
    let run!: Promise<void>;
    act(() => {
      run = result.current.start(args);
    });
    unmount();
    await act(async () => {
      finishStart({ job_id: "late-job" });
      await run;
    });
    expect(api.cancelOptimizeJob).toHaveBeenCalledWith("late-job");
    expect(api.getOptimizeStatus).not.toHaveBeenCalled();
    expect(api.optimizeStart).toHaveBeenCalledTimes(1);
  });

  it("requests cancellation for an interrupted persisted job before discarding its plan", async () => {
    const plan = createLoopClimbPlan({
      ...args,
      profileId: "p1",
      settings: {
        simsPerCrew: 100,
        maxCandidates: 20,
        strategy: "tiered",
        stopCondition: { type: "full_ladder" },
        perRungTimeoutMs: 60_000,
      },
      startedAt: "2026-10-24T00:00:00Z",
    });
    saveLoopClimbPlan({ ...plan, activeJobId: "persisted-job" });
    const { result } = renderHook(() => useLoopClimb("p1"));
    await waitFor(() => expect(result.current.plan?.status).toBe("cancelled"));
    expect(api.cancelOptimizeJob).toHaveBeenCalledWith("persisted-job");
    expect(loadLoopClimbPlan("p1")).toBeNull();
  });

  it("does not let an unmounted runner erase a newly mounted climb's bookkeeping", async () => {
    let finishFirst!: (status: { status: string }) => void,
      finishSecond!: (status: { status: string }) => void;
    const firstPoll = new Promise<{ status: string }>((resolve) => {
      finishFirst = resolve;
    });
    const secondPoll = new Promise<{ status: string }>((resolve) => {
      finishSecond = resolve;
    });
    vi.mocked(api.optimizeStart)
      .mockResolvedValueOnce({ job_id: "job-1" })
      .mockResolvedValueOnce({ job_id: "job-2" });
    vi.mocked(api.getOptimizeStatus).mockImplementation((job) =>
      job === "job-1" ? firstPoll : secondPoll,
    );
    const first = renderHook(() => useLoopClimb("p1"));
    let runFirst!: Promise<void>;
    act(() => {
      runFirst = first.result.current.start(args);
    });
    await waitFor(() =>
      expect(api.getOptimizeStatus).toHaveBeenCalledWith("job-1"),
    );
    first.unmount();
    const second = renderHook(() => useLoopClimb("p1"));
    let runSecond!: Promise<void>;
    act(() => {
      runSecond = second.result.current.start(args);
    });
    await waitFor(() =>
      expect(api.getOptimizeStatus).toHaveBeenCalledWith("job-2"),
    );
    await act(async () => {
      finishFirst({ status: "error" });
      await runFirst;
    });
    expect(loadLoopClimbPlan("p1")?.activeJobId).toBe("job-2");
    second.unmount();
    await act(async () => {
      finishSecond({ status: "error" });
      await runSecond;
    });
  });
});
