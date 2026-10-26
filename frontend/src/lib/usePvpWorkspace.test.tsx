import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import { WorkspaceModeProvider } from "../contexts/WorkspaceModeContext";
import { usePvpWorkspace } from "./usePvpWorkspace";

const mocks = vi.hoisted(() => ({
  simulate: vi.fn(),
  optimizeStart: vi.fn(),
  getOptimizeStatus: vi.fn(),
  cancelOptimizeJob: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("./api", () => ({
  fetchProfiles: vi.fn().mockResolvedValue({
    profiles: [{ id: "demo", name: "Demo", sync_token: "tok" }],
    default_id: "demo",
  }),
  formatApiError: vi.fn((e: unknown) =>
    e instanceof Error ? e.message : String(e),
  ),
  ...mocks,
}));

function wrapper({ children }: { children: ReactNode }) {
  return (
    <WorkspaceModeProvider>
      <ProfileProvider>{children}</ProfileProvider>
    </WorkspaceModeProvider>
  );
}

describe("usePvpWorkspace opponent profile", () => {
  it("auto-selects the only profile when it is also active", async () => {
    const { result } = renderHook(() => usePvpWorkspace(), { wrapper });

    await waitFor(() => {
      expect(result.current.opponentProfileId).toBe("demo");
    });
  });

  it("ignores a simulation result after the matchup changes", async () => {
    let resolveSim!: (value: unknown) => void;
    mocks.simulate.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveSim = resolve;
      }),
    );
    const { result } = renderHook(() => usePvpWorkspace(), { wrapper });
    await waitFor(() => expect(result.current.opponentProfileId).toBe("demo"));
    act(() => {
      result.current.setAttackerShipId("saladin");
      result.current.setDefenderShipId("enterprise");
      result.current.setAttackerCrew({
        captain: "officer-1",
        bridge: [null, null],
        belowDeck: [],
      });
    });
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.handleRunSim();
    });
    expect(mocks.simulate).toHaveBeenCalled();
    act(() => result.current.setDefenderShipId("augur"));
    await act(async () => {
      resolveSim({ stats: { n: 1 }, warnings: [] });
      await pending;
    });
    expect(result.current.simResult).toBeNull();
    expect(result.current.loadingSim).toBe(false);
  });

  it("cancels an accepted optimization after unmount", async () => {
    let resolveStart!: (value: { job_id: string }) => void;
    mocks.optimizeStart.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveStart = resolve;
      }),
    );
    const { result, unmount } = renderHook(() => usePvpWorkspace(), {
      wrapper,
    });
    await waitFor(() => expect(result.current.opponentProfileId).toBe("demo"));
    act(() => {
      result.current.setAttackerShipId("saladin");
      result.current.setDefenderShipId("enterprise");
      result.current.setAttackerCrew({
        captain: "officer-1",
        bridge: [null, null],
        belowDeck: [],
      });
    });
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.handleRunOptimize();
    });
    expect(mocks.optimizeStart).toHaveBeenCalled();
    unmount();
    await act(async () => {
      resolveStart({ job_id: "late-pvp" });
      await pending;
    });
    expect(mocks.cancelOptimizeJob).toHaveBeenCalledWith("late-pvp");
    expect(mocks.getOptimizeStatus).not.toHaveBeenCalled();
  });
});
