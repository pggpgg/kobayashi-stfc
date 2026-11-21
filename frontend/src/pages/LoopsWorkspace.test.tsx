import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import { fetchProfiles, optimizeStart } from "../lib/api";
import { loopWorkspaceStorageKey } from "../lib/loopsWorkspaceStorage";
import LoopsWorkspace from "./LoopsWorkspace";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    fetchProfiles: vi.fn().mockResolvedValue({
      profiles: [{ id: "p1", name: "Main", sync_token: "token" }],
      default_id: "p1",
    }),
    fetchHostiles: vi.fn().mockResolvedValue([
      {
        id: "actian-40",
        hostile_name: "Actian Instigator",
        level: 40,
        ship_class: "explorer",
      },
      {
        id: "actian-49",
        hostile_name: "Actian Apex",
        level: 49,
        ship_class: "interceptor",
      },
      {
        id: "607623533",
        hostile_name: "Imperial Starfleet Planet Harvester",
        level: 51,
        ship_class: "survey",
      },
    ]),
    fetchShips: vi.fn().mockResolvedValue([
      { id: "mantis", ship_name: "MANTIS", ship_class: "battleship" },
      { id: "saladin", ship_name: "SALADIN", ship_class: "interceptor" },
      {
        id: "uss_dauntless",
        ship_name: "USS Dauntless",
        ship_class: "interceptor",
      },
    ]),
    // Never settles, so a started climb stays in flight for assertions about the
    // in-flight UI. Without this the real call fails immediately against no server
    // and the climb is over before it can be observed.
    optimizeStart: vi.fn(() => new Promise(() => {})),
    getOptimizeStatus: vi.fn(() => new Promise(() => {})),
    cancelOptimizeJob: vi.fn().mockResolvedValue(undefined),
  };
});

function WorkspaceStateProbe() {
  const location = useLocation();
  const state = location.state as {
    loopRun?: { loopId: string; targetId: string; shipId: string };
  } | null;
  return (
    <div data-ship={state?.loopRun?.shipId}>
      handoff:{state?.loopRun?.loopId}:{state?.loopRun?.targetId}
    </div>
  );
}

describe("LoopsWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
  });

  it("renders a high-to-low ladder and hands a rung to the optimizer", async () => {
    render(
      <MemoryRouter initialEntries={["/loops"]}>
        <ProfileProvider>
          <Routes>
            <Route path="/loops" element={<LoopsWorkspace />} />
            <Route path="/" element={<WorkspaceStateProbe />} />
          </Routes>
        </ProfileProvider>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Actian" }));
    await screen.findByText("Actian Apex");
    const levels = screen
      .getAllByText(/^(49|40)$/)
      .map((node) => node.textContent);
    expect(levels).toEqual(["49", "40"]);

    const optimizeButtons = screen.getAllByRole("button", { name: "Optimize" });
    fireEvent.click(optimizeButtons[0]);
    await waitFor(() => {
      expect(screen.getByText("handoff:actian:actian-49")).toBeTruthy();
    });
  });

  it("shows progress status per rung and points at the next target", async () => {
    render(
      <MemoryRouter initialEntries={["/loops"]}>
        <ProfileProvider>
          <Routes>
            <Route path="/loops" element={<LoopsWorkspace />} />
            <Route path="/" element={<WorkspaceStateProbe />} />
          </Routes>
        </ProfileProvider>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Actian" }));
    await screen.findByText("Actian Apex");

    // Nothing measured yet: the lowest rung is the one to attack, and the rung
    // above it is not reachable.
    expect(screen.getAllByText("· untried")).toHaveLength(1);
    expect(screen.getAllByText("🔒 locked")).toHaveLength(1);
    expect(screen.getByLabelText("Next rung to attack")).toBeTruthy();
    expect(screen.getByText(/0 cleared/)).toBeTruthy();
  });

  it("locks MU Rep Grind to the Dauntless and links each target's system", async () => {
    render(
      <MemoryRouter initialEntries={["/loops"]}>
        <ProfileProvider>
          <Routes>
            <Route path="/loops" element={<LoopsWorkspace />} />
            <Route path="/" element={<WorkspaceStateProbe />} />
          </Routes>
        </ProfileProvider>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: "MU Rep Grind" }));
    const system = await screen.findByRole("link", {
      name: "Mirror P'Jem [Extract]",
    });
    expect(system.getAttribute("href")).toBe(
      "https://stfc.space/systems/538197195",
    );
    const ship = screen.getByLabelText<HTMLSelectElement>("Ship to test");
    expect(ship.disabled).toBe(true);
    expect(ship.value).toBe("uss_dauntless");

    fireEvent.click(screen.getByRole("button", { name: "Optimize" }));
    const handoff = await screen.findByText("handoff:mu-rep-grind:607623533");
    expect(handoff.getAttribute("data-ship")).toBe("uss_dauntless");
  });

  it("asks before resetting the selected ladder and returns it to the first rung", async () => {
    localStorage.setItem(
      loopWorkspaceStorageKey("p1"),
      JSON.stringify({
        version: 1,
        records: {
          "actian|actian-40|kills_per_hull": [
            {
              id: "result",
              loopId: "actian",
              targetId: "actian-40",
              targetName: "Actian Instigator",
              targetLevel: 40,
              goalId: "kills_per_hull",
              shipId: "mantis",
              shipTier: 5,
              shipLevel: 25,
              crew: { captain: "Khan", bridge: [], belowDecks: [] },
              winRate: 1,
              chainSuccessRate: 1,
              chainKillsTarget: 5,
              roundOneKillRate: 1,
              averageHullRemaining: 0.9,
              averageDefenderHullRemaining: 0,
              recordedAt: "2026-11-20T00:00:00Z",
            },
          ],
        },
      }),
    );
    const confirm = vi
      .spyOn(window, "confirm")
      .mockReturnValueOnce(false)
      .mockReturnValue(true);
    try {
      render(
        <MemoryRouter initialEntries={["/loops"]}>
          <ProfileProvider>
            <LoopsWorkspace />
          </ProfileProvider>
        </MemoryRouter>,
      );

      await screen.findByRole("button", { name: /Actian.*1 saved/ });
      fireEvent.click(screen.getByRole("button", { name: /Actian.*1 saved/ }));
      await screen.findByText("Saved best");
      const reset = screen.getByRole("button", { name: "Reset ladder" });
      fireEvent.click(reset);
      expect(confirm).toHaveBeenCalledWith(
        expect.stringContaining("Reset the Actian ladder"),
      );
      expect(screen.getByText("Saved best")).toBeTruthy();

      fireEvent.click(reset);
      expect(screen.queryByText("Saved best")).toBeNull();
      expect(screen.getByText("Actian ladder reset.")).toBeTruthy();
      expect(screen.getByText("0 cleared · 0 / 2 recorded")).toBeTruthy();
      expect(screen.getByLabelText("Next rung to attack")).toBeTruthy();
      expect((reset as HTMLButtonElement).disabled).toBe(true);
    } finally {
      confirm.mockRestore();
    }
  });

  it("keeps the climb disabled until the active profile resolves", async () => {
    vi.mocked(fetchProfiles).mockImplementationOnce(
      () => new Promise(() => {}),
    );
    render(
      <MemoryRouter initialEntries={["/loops"]}>
        <ProfileProvider>
          <LoopsWorkspace />
        </ProfileProvider>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Actian" }));
    await screen.findByText("Actian Apex");
    expect(
      screen
        .getByRole("button", { name: "Climb this loop" })
        .hasAttribute("disabled"),
    ).toBe(true);
  });

  it("explains an optimizer start failure beside the ladder controls", async () => {
    vi.mocked(optimizeStart).mockRejectedValueOnce(
      new Error("No legal crews for this profile"),
    );
    render(
      <MemoryRouter initialEntries={["/loops"]}>
        <ProfileProvider>
          <LoopsWorkspace />
        </ProfileProvider>
      </MemoryRouter>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Actian" }));
    const climbButton = await screen.findByRole("button", {
      name: "Climb this loop",
    });
    await waitFor(() =>
      expect(climbButton.hasAttribute("disabled")).toBe(false),
    );
    fireEvent.click(climbButton);
    expect((await screen.findByRole("alert")).textContent).toContain(
      "Stopped at level 40. No legal crews for this profile",
    );
  });

  it("offers a whole-ladder climb and locks manual runs while it is in flight", async () => {
    render(
      <MemoryRouter initialEntries={["/loops"]}>
        <ProfileProvider>
          <Routes>
            <Route path="/loops" element={<LoopsWorkspace />} />
            <Route path="/" element={<WorkspaceStateProbe />} />
          </Routes>
        </ProfileProvider>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Actian" }));
    await screen.findByText("Actian Apex");

    const climbButton = screen.getByRole("button", { name: "Climb this loop" });
    await waitFor(() =>
      expect(climbButton.hasAttribute("disabled")).toBe(false),
    );
    fireEvent.click(climbButton);
    expect(screen.getByRole("status").textContent).toContain(
      "Starting optimizer",
    );

    // The server holds a single CPU permit for a running job, so a manual run
    // started mid-climb would hang rather than queue.
    await waitFor(() => {
      const optimizeButtons = screen.getAllByRole("button", {
        name: /Optimize/,
      });
      expect(
        optimizeButtons.every((button) => button.hasAttribute("disabled")),
      ).toBe(true);
    });
    expect(screen.getByRole("button", { name: "Cancel climb" })).toBeTruthy();
  });
});
