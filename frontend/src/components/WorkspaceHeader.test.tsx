import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import { WorkspaceModeProvider } from "../contexts/WorkspaceModeContext";
import type { CrewState } from "../lib/types";
import WorkspaceHeader from "./WorkspaceHeader";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    fetchShips: vi.fn(),
    fetchHostiles: vi.fn(),
    fetchProfiles: vi.fn(),
    getShipTiersLevels: vi.fn(),
  };
});

import * as api from "../lib/api";

const emptyCrew: CrewState = {
  captain: null,
  bridge: [null, null],
  belowDeck: [null, null, null],
};

describe("WorkspaceHeader", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(api.fetchProfiles).mockResolvedValue({
      profiles: [{ id: "p1", name: "Test", sync_token: "t" }],
      default_id: "p1",
    });
    vi.mocked(api.fetchShips).mockResolvedValue([
      { id: "ship1", ship_name: "Enterprise", ship_class: "explorer" },
    ]);
    vi.mocked(api.fetchHostiles).mockResolvedValue([
      { id: "h1", hostile_name: "Borg", level: 30, ship_class: "interceptor" },
    ]);
    vi.mocked(api.getShipTiersLevels).mockResolvedValue({
      tiers: [1],
      levels: [1, 50],
      crew_slots: [],
    });
  });

  it("renders fight controls after ships load", async () => {
    render(
      <ProfileProvider>
        <WorkspaceModeProvider>
          <WorkspaceHeader
            shipId="ship1"
            scenarioId="h1"
            onShipIdChange={vi.fn()}
            onScenarioIdChange={vi.fn()}
            shipTier={1}
            onShipTierChange={vi.fn()}
            shipLevel={50}
            onShipLevelChange={vi.fn()}
            onBelowDeckUnlockLevelsChange={vi.fn()}
            crew={emptyCrew}
            simsPerCrew={1000}
            onSimsPerCrewChange={vi.fn()}
            estimate={null}
            lastOptimizeDurationMs={null}
            onRunSim={vi.fn()}
            onRunOptimize={vi.fn()}
            onCancelOptimize={vi.fn()}
            onSavePreset={vi.fn()}
            loadingSim={false}
            loadingOptimize={false}
            optimizeProgress={null}
            optimizeCrewsDone={null}
            optimizeTotalCrews={null}
            selectedSupportBuffs={[]}
            onSelectedSupportBuffsChange={vi.fn()}
          />
        </WorkspaceModeProvider>
      </ProfileProvider>,
    );
    await waitFor(() => expect(api.fetchShips).toHaveBeenCalled());
    expect(screen.getByText(/Fight iterations/i)).toBeTruthy();
  });
});
