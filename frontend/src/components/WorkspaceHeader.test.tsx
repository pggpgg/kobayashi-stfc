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
    getShipComponentOverrides: vi.fn(),
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
    vi.mocked(api.getShipComponentOverrides).mockResolvedValue({
      applied: false,
      deltas: null,
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
            enemyType=""
            onEnemyTypeChange={vi.fn()}
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

  it("shows only scenario controls in guided mode", async () => {
    localStorage.setItem("kobayashi_workspace_mode", "guided");
    render(
      <ProfileProvider>
        <WorkspaceModeProvider>
          <WorkspaceHeader
            shipId="ship1"
            scenarioId="h1"
            onShipIdChange={vi.fn()}
            onScenarioIdChange={vi.fn()}
            enemyType=""
            onEnemyTypeChange={vi.fn()}
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
    expect(screen.getByRole("combobox", { name: "Ship" })).toBeTruthy();
    expect(screen.queryByRole("spinbutton")).toBeNull();
    expect(screen.queryByRole("button", { name: "Run simulation" })).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Run optimization" }),
    ).toBeNull();
  });

  it("shows the component-upgrade chip in roster mode when components beat the hull tier", async () => {
    localStorage.setItem("kobayashi_workspace_mode", "roster");
    vi.mocked(api.getShipComponentOverrides).mockResolvedValue({
      applied: true,
      deltas: {
        armor_piercing: 0,
        shield_piercing: 0,
        accuracy: 0,
        armor: 0,
        shield_deflection: 0,
        dodge: 0,
        attack: 60,
        crit_chance: 0,
        crit_damage: 0,
        hull_health: 50,
        shield_health: 0,
      },
    });
    render(
      <ProfileProvider>
        <WorkspaceModeProvider>
          <WorkspaceHeader
            shipId="ship1"
            scenarioId="h1"
            onShipIdChange={vi.fn()}
            onScenarioIdChange={vi.fn()}
            enemyType=""
            onEnemyTypeChange={vi.fn()}
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
    await waitFor(() =>
      expect(api.getShipComponentOverrides).toHaveBeenCalled(),
    );
    const chip = await screen.findByTitle(
      /Synced component upgrades above hull tier/,
    );
    expect(chip.textContent).toContain("atk +60");
    expect(chip.textContent).toContain("hull +50");
  });

  describe("cost estimate qualifiers", () => {
    function renderWithEstimate(estimate: api.OptimizeEstimate) {
      return render(
        <ProfileProvider>
          <WorkspaceModeProvider>
            <WorkspaceHeader
              shipId="ship1"
              scenarioId="h1"
              onShipIdChange={vi.fn()}
              onScenarioIdChange={vi.fn()}
              enemyType=""
              onEnemyTypeChange={vi.fn()}
              shipTier={1}
              onShipTierChange={vi.fn()}
              shipLevel={50}
              onShipLevelChange={vi.fn()}
              onBelowDeckUnlockLevelsChange={vi.fn()}
              crew={emptyCrew}
              simsPerCrew={1000}
              onSimsPerCrewChange={vi.fn()}
              estimate={estimate}
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
    }

    it("reports the chain multiplier that drove the estimate", async () => {
      renderWithEstimate({
        estimated_candidates: 1240,
        sims_per_crew: 1000,
        estimated_seconds: 14.8,
        chain_kills_target: 3,
        chain_fights_per_trial_upper_bound: 3,
      });
      const label = await screen.findByText(/Est\. ~14\.8 s/);
      expect(label.textContent).toContain("1,240 crews");
      expect(label.textContent).toContain("×3 chain");
      // The scaling is an upper bound, and the UI has to say so.
      expect(label.getAttribute("title")).toMatch(/Worst case/);
    });

    it("still reports the chain multiplier when the crew count is unavailable", async () => {
      // A chain multiplier explains the *time*, so a 0 candidate count must not hide it.
      renderWithEstimate({
        estimated_candidates: 0,
        sims_per_crew: 1000,
        estimated_seconds: 0.1,
        chain_kills_target: 5,
        chain_fights_per_trial_upper_bound: 5,
      });
      const label = await screen.findByText(/Est\. ~/);
      expect(label.textContent).toContain("×5 chain");
      expect(label.textContent).not.toContain("crews");
    });

    it("adds no qualifiers for a single-fight run with no crew count", async () => {
      renderWithEstimate({
        estimated_candidates: 0,
        sims_per_crew: 1000,
        estimated_seconds: 0.1,
      });
      const label = await screen.findByText(/Est\. ~/);
      expect(label.textContent).not.toContain("(");
      expect(label.getAttribute("title")).toBeNull();
    });
  });
});
