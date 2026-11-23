import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProfileProvider } from "../contexts/ProfileContext";
import { WorkspaceModeProvider } from "../contexts/WorkspaceModeContext";
import type { FleetAllocateResponse } from "../lib/api";
import FleetWorkspace from "./FleetWorkspace";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    fetchProfiles: vi.fn(),
    fetchShips: vi.fn(),
    fetchHostiles: vi.fn(),
    fetchOfficers: vi.fn(),
    allocateFleet: vi.fn(),
  };
});

import * as api from "../lib/api";

const sampleFleetResponse: FleetAllocateResponse = {
  status: "ok",
  objective: "min_regret",
  seed: 42,
  sims_per_crew: 250,
  candidates_per_slot: 64,
  total_fleet_score: 1.72,
  standalone_upper_bound: 1.76,
  fleet_efficiency_pct: 97.7,
  min_dock_retention_pct: 95.5,
  conflict_free: true,
  solver_mode: "exact_branch_and_bound",
  contested_officers: [
    {
      officer: "Cadet Uhura",
      wanted_by_docks: ["Dock 1", "Dock 2"],
      assigned_to_dock: "Dock 1",
      reason: "Assigned to Dock 1 (100.0% retention); Dock 2 retained 95.5%",
    },
  ],
  slots: [
    {
      slot_index: 0,
      label: "Dock 1",
      ship: "uss_saladin",
      hostile: "1140710508",
      priority_weight: 1.0,
      candidates_evaluated: 28,
      retention_pct: 100.0,
      score_loss_vs_standalone: 0.0,
      assigned_crew: {
        captain: "Cadet Uhura",
        bridge: ["Badgey", "Nesmith"],
        below_decks: ["Odo", "Harry Kim"],
        score: 0.85,
        win_rate: 0.94,
        win_rate_ci_low: 0.91,
        win_rate_ci_high: 0.96,
        stall_rate: 0,
        loss_rate: 0.06,
        r1_kill_rate: 0.94,
        avg_hull_remaining: 0.48,
        avg_hull_remaining_ci_low: 0.45,
        avg_hull_remaining_ci_high: 0.51,
        trials_run: 250,
        fidelity: {
          implemented: 5,
          partial: 0,
          ignored: 0,
          total_combat_effects: 5,
          coverage_pct: 100,
          grade: "A",
          caveats: [],
        },
      },
      standalone_best: {
        captain: "Cadet Uhura",
        bridge: ["Badgey", "Nesmith"],
        below_decks: ["Odo", "Harry Kim"],
        score: 0.85,
        win_rate: 0.94,
        win_rate_ci_low: 0.91,
        win_rate_ci_high: 0.96,
        stall_rate: 0,
        loss_rate: 0.06,
        r1_kill_rate: 0.94,
        avg_hull_remaining: 0.48,
        avg_hull_remaining_ci_low: 0.45,
        avg_hull_remaining_ci_high: 0.51,
        trials_run: 250,
        fidelity: {
          implemented: 5,
          partial: 0,
          ignored: 0,
          total_combat_effects: 5,
          coverage_pct: 100,
          grade: "A",
          caveats: [],
        },
      },
    },
    {
      slot_index: 1,
      label: "Dock 2",
      ship: "enterprise_nx_01",
      hostile: "1120713907",
      priority_weight: 1.0,
      candidates_evaluated: 28,
      retention_pct: 95.5,
      score_loss_vs_standalone: 0.04,
      yielded_officers: ["Cadet Uhura"],
      assigned_crew: {
        captain: "TOS Sulu",
        bridge: ["Khan", "Klaa"],
        below_decks: ["Hugh", "Zeph"],
        score: 0.87,
        win_rate: 0.96,
        win_rate_ci_low: 0.93,
        win_rate_ci_high: 0.98,
        stall_rate: 0,
        loss_rate: 0.04,
        r1_kill_rate: 0.96,
        avg_hull_remaining: 0.51,
        avg_hull_remaining_ci_low: 0.48,
        avg_hull_remaining_ci_high: 0.54,
        trials_run: 250,
        fidelity: {
          implemented: 5,
          partial: 0,
          ignored: 0,
          total_combat_effects: 5,
          coverage_pct: 100,
          grade: "A",
          caveats: [],
        },
      },
      standalone_best: {
        captain: "Cadet Uhura",
        bridge: ["Khan", "Klaa"],
        below_decks: ["Hugh", "Zeph"],
        score: 0.91,
        win_rate: 0.99,
        win_rate_ci_low: 0.97,
        win_rate_ci_high: 1.0,
        stall_rate: 0,
        loss_rate: 0.01,
        r1_kill_rate: 0.99,
        avg_hull_remaining: 0.59,
        avg_hull_remaining_ci_low: 0.56,
        avg_hull_remaining_ci_high: 0.62,
        trials_run: 250,
        fidelity: {
          implemented: 5,
          partial: 0,
          ignored: 0,
          total_combat_effects: 5,
          coverage_pct: 100,
          grade: "A",
          caveats: [],
        },
      },
    },
  ],
};

describe("FleetWorkspace", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(api.fetchProfiles).mockResolvedValue({
      profiles: [{ id: "demo", name: "Demo", sync_token: "t" }],
      default_id: "demo",
    });
    vi.mocked(api.fetchShips).mockResolvedValue([
      {
        id: "uss_saladin",
        ship_name: "USS Saladin",
        ship_class: "interceptor",
      },
      {
        id: "enterprise_nx_01",
        ship_name: "Enterprise NX-01",
        ship_class: "explorer",
      },
    ]);
    vi.mocked(api.fetchHostiles).mockResolvedValue([
      {
        id: "1140710508",
        hostile_name: "Corvus Battleship",
        level: 42,
        ship_class: "battleship",
      },
      {
        id: "1120713907",
        hostile_name: "Xindi Interceptor",
        level: 58,
        ship_class: "interceptor",
      },
    ]);
    vi.mocked(api.fetchOfficers).mockResolvedValue([
      { id: "cadet-uhura", name: "Cadet Uhura", slot: "captain" },
      { id: "tos-sulu", name: "TOS Sulu", slot: "captain" },
    ]);
    vi.mocked(api.allocateFleet).mockResolvedValue(sampleFleetResponse);
  });

  it("allocates a conflict-free multi-ship fleet and shows contested officers", async () => {
    render(
      <MemoryRouter initialEntries={["/fleet"]}>
        <ProfileProvider>
          <WorkspaceModeProvider>
            <FleetWorkspace />
          </WorkspaceModeProvider>
        </ProfileProvider>
      </MemoryRouter>,
    );

    const runBtn = (await screen.findByRole("button", {
      name: "Optimize Fleet",
    })) as HTMLButtonElement;
    await waitFor(() => expect(runBtn.disabled).toBe(false));

    fireEvent.change(screen.getByLabelText("Fleet Objective"), {
      target: { value: "min_regret" },
    });
    fireEvent.click(runBtn);

    await waitFor(() =>
      expect(screen.getByText("Zero Officer Conflicts")).toBeTruthy(),
    );
    expect(screen.getByText("97.7%")).toBeTruthy();
    expect(screen.getByText("Contested Officers Resolved (1)")).toBeTruthy();
    expect(screen.getByText("Exact Branch & Bound")).toBeTruthy();

    expect(api.allocateFleet).toHaveBeenCalledWith(
      expect.objectContaining({
        objective: "min_regret",
        slots: expect.arrayContaining([
          expect.objectContaining({
            ship: "uss_saladin",
            hostile: "1140710508",
          }),
          expect.objectContaining({
            ship: "enterprise_nx_01",
            hostile: "1120713907",
          }),
        ]),
      }),
      "demo",
      expect.any(AbortSignal),
    );
  });
});
