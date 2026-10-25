import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import { WorkspaceModeProvider } from "../contexts/WorkspaceModeContext";
import * as api from "../lib/api";
import * as workspace from "../lib/usePvpWorkspace";
import PvpWorkspace from "./PvpWorkspace";

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  fetchProfiles: vi.fn(),
  fetchShips: vi.fn(),
  getShipTiersLevels: vi.fn(),
}));
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  localStorage.setItem("kobayashi_workspace_mode", "sandbox");
  vi.mocked(api.fetchProfiles).mockResolvedValue({
    profiles: [{ id: "p1", name: "Main", sync_token: "t" }],
    default_id: "p1",
  });
  vi.mocked(api.fetchShips).mockResolvedValue([
    {
      id: "small",
      name: "Small ship",
      ship_name: "Small ship",
      ship_class: "explorer",
    },
    {
      id: "large",
      name: "Large ship",
      ship_name: "Large ship",
      ship_class: "battleship",
    },
  ]);
  vi.mocked(api.getShipTiersLevels).mockImplementation(async (id) =>
    id === "small"
      ? { tiers: [1, 2], levels: [1, 20], crew_slots: [{ unlock_level: 10 }] }
      : {
          tiers: [1, 2, 3, 4, 5],
          levels: [1, 20, 50],
          crew_slots: [
            { unlock_level: 1 },
            { unlock_level: 10 },
            { unlock_level: 20 },
            { unlock_level: 30 },
          ],
        },
  );
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
function renderPvp() {
  return render(
    <ProfileProvider>
      <WorkspaceModeProvider>
        <PvpWorkspace />
      </WorkspaceModeProvider>
    </ProfileProvider>,
  );
}
function side(name: string) {
  return screen
    .getByRole("heading", { name })
    .closest("section") as HTMLElement;
}

describe("PvP ship selectors", () => {
  it("#365 reconciles both sides' tier/level state with the displayed ship before serialization", async () => {
    const hook = vi.spyOn(workspace, "usePvpWorkspace");
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ status: "ok", stats: {}, seed: 1 }), {
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    renderPvp();
    await screen.findAllByRole("option", { name: "Small ship" });
    const attacker = within(side("Attacker (your profile)")),
      defender = within(side("Defender (opponent)"));
    fireEvent.change(attacker.getAllByRole("combobox")[0], {
      target: { value: "large" },
    });
    fireEvent.change(defender.getAllByRole("combobox")[0], {
      target: { value: "large" },
    });
    await waitFor(() =>
      expect(
        attacker.getByLabelText("Level").querySelector('option[value="50"]'),
      ).toBeTruthy(),
    );
    fireEvent.change(attacker.getByLabelText("Tier"), {
      target: { value: "5" },
    });
    fireEvent.change(defender.getByLabelText("Tier"), {
      target: { value: "5" },
    });
    fireEvent.change(attacker.getAllByRole("combobox")[0], {
      target: { value: "small" },
    });
    fireEvent.change(defender.getAllByRole("combobox")[0], {
      target: { value: "small" },
    });
    await waitFor(() =>
      expect(
        attacker.getByLabelText("Level").querySelector('option[value="20"]'),
      ).toBeTruthy(),
    );
    const state = () =>
      hook.mock.results[hook.mock.results.length - 1]?.value as ReturnType<
        typeof workspace.usePvpWorkspace
      >;
    expect([state().attackerShipTier, state().attackerShipLevel]).toEqual([
      1, 1,
    ]);
    expect([state().defenderShipTier, state().defenderShipLevel]).toEqual([
      1, 1,
    ]);
    act(() =>
      state().setAttackerCrew({
        captain: "Kirk",
        bridge: [null, null],
        belowDeck: [],
      }),
    );
    await act(async () => state().handleRunSim());
    const body = JSON.parse(fetchMock.mock.calls[0][1].body);
    expect([
      body.ship_tier,
      body.ship_level,
      body.defender_ship_tier,
      body.defender_ship_level,
    ]).toEqual([1, 1, 1, 1]);
  });

  it("#344 gives the defender its own below-decks unlock schedule", async () => {
    renderPvp();
    await screen.findAllByRole("option", { name: "Small ship" });
    const attacker = within(side("Attacker (your profile)")),
      defender = within(side("Defender (opponent)"));
    fireEvent.change(attacker.getAllByRole("combobox")[0], {
      target: { value: "small" },
    });
    fireEvent.change(defender.getAllByRole("combobox")[0], {
      target: { value: "large" },
    });
    await waitFor(() =>
      expect(
        defender.getByLabelText("Level").querySelector('option[value="50"]'),
      ).toBeTruthy(),
    );
    fireEvent.change(attacker.getByLabelText("Level"), {
      target: { value: "20" },
    });
    fireEvent.change(defender.getByLabelText("Level"), {
      target: { value: "50" },
    });
    expect(
      attacker
        .getAllByRole("combobox")
        .filter((node) => node.tagName === "INPUT"),
    ).toHaveLength(4);
    expect(
      defender
        .getAllByRole("combobox")
        .filter((node) => node.tagName === "INPUT"),
    ).toHaveLength(7);
  });
});
