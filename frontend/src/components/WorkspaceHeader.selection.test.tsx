import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { type ComponentProps, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider, useProfile } from "../contexts/ProfileContext";
import {
  useWorkspaceMode,
  WorkspaceModeProvider,
} from "../contexts/WorkspaceModeContext";
import * as api from "../lib/api";
import WorkspaceHeader from "./WorkspaceHeader";

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  fetchShips: vi.fn(),
  fetchProfiles: vi.fn(),
  fetchHostiles: vi.fn(),
  getShipTiersLevels: vi.fn(),
  getShipComponentOverrides: vi.fn(),
}));
const headerProps: ComponentProps<typeof WorkspaceHeader> = {
  shipId: "sandbox",
  scenarioId: "h1",
  onShipIdChange: () => {},
  onScenarioIdChange: () => {},
  enemyType: "",
  onEnemyTypeChange: () => {},
  shipTier: 9,
  onShipTierChange: () => {},
  shipLevel: 60,
  onShipLevelChange: () => {},
  onBelowDeckUnlockLevelsChange: () => {},
  crew: { captain: "Kirk", bridge: [null, null], belowDeck: [] },
  simsPerCrew: 100,
  onSimsPerCrewChange: () => {},
  estimate: null,
  lastOptimizeDurationMs: null,
  onRunSim: () => {},
  onRunOptimize: () => {},
  onCancelOptimize: () => {},
  onSavePreset: () => {},
  loadingSim: false,
  loadingOptimize: false,
  optimizeProgress: null,
  optimizeCrewsDone: null,
  optimizeTotalCrews: null,
  selectedSupportBuffs: [],
  onSelectedSupportBuffsChange: () => {},
};
function Harness() {
  const { setMode } = useWorkspaceMode();
  const { activeProfileId, setActiveProfileId } = useProfile();
  const [shipId, setShipId] = useState("sandbox"),
    [tier, setTier] = useState(9),
    [level, setLevel] = useState(60);
  const run = () =>
    void api.simulate(
      {
        ship: shipId,
        hostile: "h1",
        ship_tier: tier,
        ship_level: level,
        crew: headerProps.crew,
        num_sims: 10,
      },
      activeProfileId,
    );
  return (
    <>
      <button type="button" onClick={() => setMode("roster")}>
        Use roster
      </button>
      <button type="button" onClick={() => setActiveProfileId("p2")}>
        Use second profile
      </button>
      <WorkspaceHeader
        {...headerProps}
        shipId={shipId}
        onShipIdChange={setShipId}
        shipTier={tier}
        onShipTierChange={setTier}
        shipLevel={level}
        onShipLevelChange={setLevel}
        onRunSim={run}
      />
    </>
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  localStorage.setItem("kobayashi_workspace_mode", "sandbox");
  localStorage.setItem("kobayashi_active_profile", "p1");
  vi.mocked(api.fetchProfiles).mockResolvedValue({
    profiles: [
      { id: "p1", name: "First", sync_token: "t1" },
      { id: "p2", name: "Second", sync_token: "t2" },
    ],
    default_id: "p1",
  });
  vi.mocked(api.fetchShips).mockImplementation(async (owned, profile) =>
    owned
      ? [
          {
            id: profile === "p2" ? "second" : "owned",
            ship_name: "Owned",
            ship_class: "explorer",
            tier: 4,
            level: 20,
          },
        ]
      : [{ id: "sandbox", ship_name: "Sandbox", ship_class: "explorer" }],
  );
  vi.mocked(api.fetchHostiles).mockResolvedValue([
    { id: "h1", hostile_name: "Bandit", level: 26, ship_class: "explorer" },
  ]);
  vi.mocked(api.getShipTiersLevels).mockResolvedValue({
    tiers: [1, 4, 9],
    levels: [1, 20, 60],
    crew_slots: [],
  });
  vi.mocked(api.getShipComponentOverrides).mockResolvedValue({
    applied: false,
    deltas: null,
  });
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
describe("#339 Workspace selection matches the request", () => {
  it.each([
    false,
    true,
  ])("reselects an available owned ship after a roster/profile switch (profile switch=%s)", async (switchProfile) => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ stats: {}, seed: 1 }), {
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(
      <ProfileProvider>
        <WorkspaceModeProvider>
          <Harness />
        </WorkspaceModeProvider>
      </ProfileProvider>,
    );
    await waitFor(() =>
      expect(
        (screen.getByRole("combobox", { name: "Ship" }) as HTMLSelectElement)
          .value,
      ).toBe("sandbox"),
    );
    fireEvent.click(screen.getByRole("button", { name: "Use roster" }));
    if (switchProfile)
      fireEvent.click(
        screen.getByRole("button", { name: "Use second profile" }),
      );
    const expected = switchProfile ? "second" : "owned";
    await waitFor(() =>
      expect(
        (screen.getByRole("combobox", { name: "Ship" }) as HTMLSelectElement)
          .value,
      ).toBe(expected),
    );
    fireEvent.click(screen.getByRole("button", { name: "Run simulation" }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toMatchObject({
      ship: expected,
      ship_tier: 4,
      ship_level: 20,
    });
  });
});
