import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import type { Preset } from "./api";
import { useWorkspace } from "./useWorkspace";

const { apiMocks, doneOptimizeStatus } = vi.hoisted(() => {
  const doneOptimizeStatus = {
    status: "done" as const,
    result: {
      status: "ok",
      scenario: {
        ship: "saladin",
        hostile: "2918121098",
        sims: 5000,
        seed: 1,
        below_decks_slots: 3,
      },
      recommendations: [] as unknown[],
      duration_ms: 0,
    },
  };
  const apiMocks = {
    mockSimulate: vi.fn().mockResolvedValue({
      stats: {
        win_rate: 0.5,
        stall_rate: 0.1,
        loss_rate: 0.4,
        avg_hull_remaining: 0.3,
        avg_defender_hull_remaining: 0.15,
        n: 100,
      },
      seed: 42,
    }),
    mockSavePreset: vi.fn().mockResolvedValue(undefined),
    mockGetEstimate: vi.fn().mockResolvedValue({
      estimated_candidates: 10,
      sims_per_crew: 5000,
      estimated_seconds: 2,
    }),
    mockFetchHeuristics: vi.fn().mockResolvedValue([]),
    mockOptimizeStart: vi
      .fn()
      .mockRejectedValue(new Error("skip optimize in test")),
    mockGetOptimizeStatus: vi.fn().mockResolvedValue(doneOptimizeStatus),
  };
  return { apiMocks, doneOptimizeStatus };
});

vi.mock("./api", () => ({
  fetchProfiles: vi.fn().mockResolvedValue({
    profiles: [{ id: "p1", name: "Main", sync_token: "tok" }],
    default_id: "p1",
  }),
  simulate: apiMocks.mockSimulate,
  savePreset: apiMocks.mockSavePreset,
  getOptimizeEstimate: apiMocks.mockGetEstimate,
  fetchHeuristics: apiMocks.mockFetchHeuristics,
  optimizeStart: apiMocks.mockOptimizeStart,
  getOptimizeStatus: apiMocks.mockGetOptimizeStatus,
  getOptimizeStreamUrl: vi.fn(() => "http://test/stream"),
  cancelOptimizeJob: vi.fn(),
}));

function wrapper({ children }: { children: ReactNode }) {
  return (
    <MemoryRouter>
      <ProfileProvider>{children}</ProfileProvider>
    </MemoryRouter>
  );
}

describe("useWorkspace", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sessionStorage.clear();
    apiMocks.mockGetOptimizeStatus.mockResolvedValue(doneOptimizeStatus);
  });

  it("sets error and does not call simulate when captain is missing", async () => {
    const { result } = renderHook(() => useWorkspace(), { wrapper });

    await act(async () => {
      await result.current.handleRunSim();
    });

    expect(result.current.error).toBe("Select a captain first");
    expect(result.current.errorSeverity).toBe("error");
    expect(apiMocks.mockSimulate).not.toHaveBeenCalled();
  });

  it("calls simulate when captain is set", async () => {
    const { result } = renderHook(() => useWorkspace(), { wrapper });

    act(() => {
      result.current.setShipId("saladin");
      result.current.setScenarioId("2918121098");
      result.current.setCrew({
        captain: "officer-1",
        bridge: [null, null],
        belowDeck: [null, null, null],
      });
    });

    await act(async () => {
      await result.current.handleRunSim();
    });

    expect(apiMocks.mockSimulate).toHaveBeenCalled();
    expect(result.current.error).toBeNull();
    expect(result.current.errorSeverity).toBeNull();
    expect(result.current.simResult).not.toBeNull();
    expect(result.current.simResult?.n).toBe(100);
  });

  it("calls savePreset when saving a preset", async () => {
    const { result } = renderHook(() => useWorkspace(), { wrapper });

    await waitFor(() => {
      expect(result.current.activeProfileId).toBe("p1");
    });

    act(() => {
      result.current.setShipId("enterprise");
      result.current.setScenarioId("hostile-x");
      result.current.setSavePresetName("My crew");
      result.current.setCrew({
        captain: "c1",
        bridge: ["b1", null],
        belowDeck: [null, null, null],
      });
    });

    await act(async () => {
      await result.current.handleSavePreset();
    });

    expect(apiMocks.mockSavePreset).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "My crew",
        ship: "enterprise",
        scenario: "hostile-x",
        crew: expect.objectContaining({ captain: "c1" }),
      }),
      "p1",
    );
    expect(result.current.error).toBeNull();
  });

  it("hydrates ship, scenario, and crew from location state preset", async () => {
    const preset: Preset = {
      id: "preset-1",
      name: "Saved",
      ship: "mayflower",
      scenario: "999",
      crew: {
        captain: "cap-99",
        bridge: ["br1", "br2"],
        below_deck: [null, null, null, null],
      },
    };

    const { result } = renderHook(() => useWorkspace(), {
      wrapper: ({ children }) => (
        <MemoryRouter initialEntries={[{ pathname: "/", state: { preset } }]}>
          <ProfileProvider>{children}</ProfileProvider>
        </MemoryRouter>
      ),
    });

    await waitFor(() => {
      expect(result.current.shipId).toBe("mayflower");
      expect(result.current.scenarioId).toBe("999");
      expect(result.current.crew.captain).toBe("cap-99");
      expect(result.current.crew.bridge).toEqual(["br1", "br2"]);
    });
  });

  it("resumes persisted job: fetches status and clears storage when job is already done", async () => {
    sessionStorage.setItem(
      "kobayashi_active_optimize_job_v1",
      JSON.stringify({ jobId: "job-resume-1", profileId: "p1" }),
    );

    const { result } = renderHook(() => useWorkspace(), { wrapper });

    await waitFor(() => {
      expect(apiMocks.mockGetOptimizeStatus).toHaveBeenCalledWith(
        "job-resume-1",
      );
    });
    expect(
      sessionStorage.getItem("kobayashi_active_optimize_job_v1"),
    ).toBeNull();
    expect(result.current.loadingOptimize).toBe(false);
  });

  it("trims below-deck slots when ship level decreases", async () => {
    const { result } = renderHook(() => useWorkspace(), { wrapper });

    await waitFor(() => {
      // Level 50: unlocks at 5,10,20,30,40,45 → 6 BD slots (default STFC schedule).
      expect(result.current.crew.belowDeck.length).toBe(6);
    });

    await act(async () => {
      result.current.setShipLevel(34);
    });

    await waitFor(() => {
      // Level 34: first four unlocks only → 4 slots.
      expect(result.current.crew.belowDeck.length).toBe(4);
    });
  });
});
