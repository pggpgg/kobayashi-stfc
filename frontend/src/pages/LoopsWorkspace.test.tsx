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
      {
        id: "3294438884",
        hostile_name: "Xindi-Aquatic Cruiser",
        level: 62,
        ship_class: "interceptor",
        system_names: [],
        warp: 0,
        has_loot: false,
        primary_resource_name: null,
      },
      {
        id: "3919830203",
        hostile_name: "Xindi-Aquatic Cruiser",
        level: 62,
        ship_class: "interceptor",
        system_names: ["Yoton"],
        warp: 1000,
        has_loot: true,
        primary_resource_name: "Tech Module",
      },
      {
        id: "565037258",
        hostile_name: "Hijacked D7 Class",
        level: 40,
        ship_class: "interceptor",
        system_names: ["Philo"],
        warp: 70,
        has_loot: true,
        primary_resource_name: "Common Hijacked Splicer",
      },
      {
        id: "2399975665",
        hostile_name: "Hijacked D3 Class",
        level: 40,
        ship_class: "interceptor",
        system_names: ["Sandweiss"],
        warp: 70,
        has_loot: true,
        primary_resource_name: "Common Hijacked Splicer",
      },
    ]),
    fetchShips: vi.fn().mockResolvedValue([
      { id: "mantis", ship_name: "MANTIS", ship_class: "battleship" },
      { id: "saladin", ship_name: "SALADIN", ship_class: "interceptor" },
      {
        id: "uss_dauntless",
        ship_name: "USS Dauntless",
        ship_class: "interceptor",
        tier: 6,
        level: 30,
      },
      {
        id: "enterprise_nx_01",
        ship_name: "Enterprise NX-01",
        ship_class: "explorer",
      },
      {
        id: "uss_reliant",
        ship_name: "U.S.S. RELIANT",
        ship_class: "explorer",
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
    loopRun?: {
      loopId: string;
      targetId: string;
      shipId: string;
      shipTier?: number;
      shipLevel?: number;
    };
  } | null;
  return (
    <div
      data-ship={state?.loopRun?.shipId}
      data-tier={state?.loopRun?.shipTier ?? ""}
      data-level={state?.loopRun?.shipLevel ?? ""}
    >
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
    expect(handoff.getAttribute("data-tier")).toBe("6");
    expect(handoff.getAttribute("data-level")).toBe("30");
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

  it("#414/#415 resolves the Yoton L62 Xindi cruiser with system metadata and displays saved farming loot", async () => {
    localStorage.setItem(
      loopWorkspaceStorageKey("p1"),
      JSON.stringify({
        version: 2,
        records: {
          "xindi-aquatic|3919830203|farming_loot": [
            {
              id: "yoton-loot",
              loopId: "xindi-aquatic",
              targetId: "3919830203",
              targetName: "Xindi-Aquatic Cruiser",
              targetLevel: 62,
              goalId: "farming_loot",
              shipId: "enterprise_nx_01",
              shipTier: 12,
              shipLevel: 60,
              crew: {
                captain: "Jonathan Archer",
                bridge: ["Spock", "Bones"],
                belowDecks: ["Ent-E Data"],
              },
              winRate: 1,
              winRateCiLow: 0.99,
              winRateCiHigh: 1,
              roundOneKillRate: 0.5,
              roundOneKillRateCiLow: 0.45,
              roundOneKillRateCiHigh: 0.55,
              averageHullRemaining: 0.8,
              averageHullRemainingCiLow: 0.78,
              averageHullRemainingCiHigh: 0.82,
              averageDefenderHullRemaining: 0,
              averageDefenderHullRemainingCiLow: 0,
              averageDefenderHullRemainingCiHigh: 0,
              chainSuccessRate: 1,
              chainSuccessRateCiLow: 0.99,
              chainSuccessRateCiHigh: 1,
              chainKillsTarget: 4,
              chainSecondaryObjective: "max_expected_loot",
              chainEndingHull: 0.8,
              chainExpectedLoot: 17_736_376,
              chainLootResourceName: "Tech Module",
              chainCargoCapped: false,
              recordedAt: "2026-11-23T00:00:00Z",
            },
          ],
        },
      }),
    );

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

    fireEvent.click(
      await screen.findByRole("button", { name: /Xindi-Aquatic.*1 saved/ }),
    );
    await screen.findByText("Yoton · W1000 · Tech Module");
    expect(screen.getByText(/3919830203/)).toBeTruthy();
    expect(screen.queryByText(/3294438884/)).toBeNull();

    fireEvent.change(screen.getByLabelText("Rank crews by"), {
      target: { value: "farming_loot" },
    });
    expect(await screen.findByText(/17\.74M Tech Module/)).toBeTruthy();
    expect(screen.getByText(/100% P\(4-kill\)/)).toBeTruthy();
    expect(screen.getByText(/80% hull/)).toBeTruthy();
    expect(screen.getByText(/Below decks:/)).toBeTruthy();
    expect(screen.getByText(/Ent-E Data/)).toBeTruthy();
  });

  it("displays saved below-decks officers on each ladder rung and omits the row when empty", async () => {
    localStorage.setItem(
      loopWorkspaceStorageKey("p1"),
      JSON.stringify({
        version: 2,
        records: {
          "actian|actian-49|kills_per_hull": [
            {
              id: "with-bd",
              loopId: "actian",
              targetId: "actian-49",
              targetName: "Actian Apex",
              targetLevel: 49,
              goalId: "kills_per_hull",
              shipId: "mantis",
              shipTier: 6,
              shipLevel: 30,
              crew: {
                captain: "SNW M'Benga",
                bridge: ["SNW James Kirk", "Moreau"],
                belowDecks: ["SNW Nurse Chapel", "Hugh", "B'Elanna Torres"],
              },
              winRate: 1,
              winRateCiLow: 0.99,
              winRateCiHigh: 1,
              roundOneKillRate: 0.93,
              roundOneKillRateCiLow: 0.9,
              roundOneKillRateCiHigh: 0.95,
              averageHullRemaining: 0.72,
              averageHullRemainingCiLow: 0.7,
              averageHullRemainingCiHigh: 0.74,
              averageDefenderHullRemaining: 0,
              averageDefenderHullRemainingCiLow: 0,
              averageDefenderHullRemainingCiHigh: 0,
              chainSuccessRate: 1,
              chainSuccessRateCiLow: 0.99,
              chainSuccessRateCiHigh: 1,
              chainKillsTarget: 5,
              chainEndingHull: 0.72,
              recordedAt: "2026-11-27T00:00:00Z",
            },
          ],
          "actian|actian-40|kills_per_hull": [
            {
              id: "without-bd",
              loopId: "actian",
              targetId: "actian-40",
              targetName: "Actian Instigator",
              targetLevel: 40,
              goalId: "kills_per_hull",
              shipId: "mantis",
              shipTier: 6,
              shipLevel: 30,
              crew: {
                captain: "Khan",
                bridge: ["Spock", "Kirk"],
                belowDecks: [],
              },
              winRate: 1,
              winRateCiLow: 0.99,
              winRateCiHigh: 1,
              roundOneKillRate: 1,
              roundOneKillRateCiLow: 0.99,
              roundOneKillRateCiHigh: 1,
              averageHullRemaining: 0.95,
              averageHullRemainingCiLow: 0.94,
              averageHullRemainingCiHigh: 0.96,
              averageDefenderHullRemaining: 0,
              averageDefenderHullRemainingCiLow: 0,
              averageDefenderHullRemainingCiHigh: 0,
              chainSuccessRate: 1,
              chainSuccessRateCiLow: 0.99,
              chainSuccessRateCiHigh: 1,
              chainKillsTarget: 5,
              chainEndingHull: 0.95,
              recordedAt: "2026-11-27T00:00:00Z",
            },
          ],
        },
      }),
    );

    render(
      <MemoryRouter initialEntries={["/loops"]}>
        <ProfileProvider>
          <LoopsWorkspace />
        </ProfileProvider>
      </MemoryRouter>,
    );

    fireEvent.click(
      await screen.findByRole("button", { name: /Actian.*2 saved/ }),
    );
    await screen.findByText("SNW M'Benga · SNW James Kirk · Moreau");
    expect(
      screen.getByText(/SNW Nurse Chapel · Hugh · B'Elanna Torres/),
    ).toBeTruthy();
    expect(screen.getAllByText("Below decks:")).toHaveLength(1);
  });

  it("defaults Augment Exiles to Hijacked D7 Class and lets the player switch hostile variants", async () => {
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

    fireEvent.click(screen.getByRole("button", { name: "Augment Exiles" }));
    await screen.findByText("Hijacked D7 Class");
    expect(screen.queryByText("Hijacked D3 Class")).toBeNull();

    const targetSelect =
      screen.getByLabelText<HTMLSelectElement>("Target hostile");
    expect(targetSelect.value).toBe("hijacked-d7");

    fireEvent.change(targetSelect, { target: { value: "hijacked-regular" } });
    await screen.findByText("Hijacked D3 Class");
    expect(screen.queryByText("Hijacked D7 Class")).toBeNull();

    fireEvent.change(targetSelect, { target: { value: "all" } });
    await screen.findByText("Hijacked D7 Class");
    expect(screen.getByText("Hijacked D3 Class")).toBeTruthy();
  });
});
