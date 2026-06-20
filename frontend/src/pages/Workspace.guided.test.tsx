import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Workspace from "./Workspace";

const { workspaceState } = vi.hoisted(() => ({
  workspaceState: {
    shipId: "ship-1",
    scenarioId: "hostile-1",
    shipTier: 9,
    shipLevel: 45,
    belowDeckUnlockLevels: [5, 10],
    activeProfileId: "p1",
    simsPerCrew: 5000,
    selectedSupportBuffs: [] as string[],
    crew: {
      captain: null,
      bridge: [null, null],
      belowDeck: [null, null],
    },
    pins: { captain: false, bridge: [false, false], belowDeck: [false, false] },
    simResult: null,
    recommendations: [] as unknown[],
    resultWarnings: [] as string[],
    unresolvedOfficers: [] as string[],
    loadingSim: false,
    loadingOptimize: false,
    workspaceInfo: null,
    error: null,
    errorSeverity: null,
    showSavePreset: false,
    savePresetName: "",
    savingPreset: false,
    handleRunSim: vi.fn(),
    handleRunOptimize: vi.fn(),
    handleCancelOptimize: vi.fn(),
    resetResults: vi.fn(),
    setShowSavePreset: vi.fn(),
    handleSavePreset: vi.fn(),
    setWorkspaceInfo: vi.fn(),
    setShipId: vi.fn(),
    setScenarioId: vi.fn(),
    setShipTier: vi.fn(),
    setShipLevel: vi.fn(),
    setBelowDeckUnlockLevels: vi.fn(),
    setSimsPerCrew: vi.fn(),
    setSelectedSupportBuffs: vi.fn(),
    setCrew: vi.fn(),
    setPins: vi.fn(),
  },
}));

vi.mock("../lib/useWorkspace", () => ({
  useWorkspace: () => workspaceState,
}));
vi.mock("../lib/api", () => ({
  fetchOfficers: vi.fn().mockResolvedValue([]),
}));
vi.mock("../contexts/ProfileContext", () => ({
  useProfile: () => ({ activeProfileId: "p1" }),
}));
vi.mock("../contexts/WorkspaceModeContext", () => ({
  useWorkspaceMode: () => ({
    mode: "guided",
    ownedOnly: true,
    setMode: vi.fn(),
  }),
}));
vi.mock("../components/WorkspaceHeader", () => ({
  default: () => <div>Scenario controls</div>,
}));
vi.mock("../components/CrewBuilder", () => ({
  default: () => <div>Crew controls</div>,
}));
vi.mock("../components/SimResults", () => ({
  default: () => <div>Results controls</div>,
}));
vi.mock("../components/OptimizePanel", () => ({
  default: () => <div>Advanced optimizer controls</div>,
}));
vi.mock("../components/SavePresetModal", () => ({
  default: () => null,
}));

describe("Workspace guided progression", () => {
  it("mounts only the controls for the active step", () => {
    render(<Workspace />);

    expect(screen.getByText("Scenario controls")).toBeTruthy();
    expect(screen.queryByText("Crew controls")).toBeNull();
    expect(screen.queryByText("Results controls")).toBeNull();
    expect(screen.queryByText("Advanced optimizer controls")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Continue to crew" }));
    expect(screen.queryByText("Scenario controls")).toBeNull();
    expect(screen.getByText("Crew controls")).toBeTruthy();
    expect(screen.queryByText("Results controls")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Find a crew for me" }));
    expect(screen.queryByText("Crew controls")).toBeNull();
    expect(screen.getByRole("button", { name: "Find best crew" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Find best crew" }));
    expect(workspaceState.handleRunOptimize).toHaveBeenCalledOnce();
    expect(screen.getByText("Results controls")).toBeTruthy();
    expect(screen.queryByText("Scenario controls")).toBeNull();
    expect(screen.queryByText("Crew controls")).toBeNull();

    fireEvent.click(
      screen.getByRole("button", { name: "Start another mission" }),
    );
    expect(workspaceState.resetResults).toHaveBeenCalledOnce();
    expect(screen.getByText("Scenario controls")).toBeTruthy();
  });
});
