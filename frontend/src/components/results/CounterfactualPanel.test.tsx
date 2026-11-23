import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CrewCounterfactualResponse } from "../../lib/api";
import CounterfactualPanel from "./CounterfactualPanel";

const sampleResponse: CrewCounterfactualResponse = {
  status: "ok",
  seed: 42,
  trials_per_variant: 400,
  captain: "Cadet Uhura",
  bridge: ["Badgey", "Nesmith"],
  below_decks: ["Odo", "Harry Kim"],
  variants_evaluated: 28,
  using_placeholder_combatants: false,
  unavailable_officers: ["Cadet Uhura"],
  baseline: {
    score: 0.84,
    win_rate: 0.93,
    win_rate_ci_low: 0.9,
    win_rate_ci_high: 0.95,
    stall_rate: 0.0,
    loss_rate: 0.07,
    r1_kill_rate: 0.93,
    avg_hull_remaining: 0.48,
    avg_hull_remaining_ci_low: 0.45,
    avg_hull_remaining_ci_high: 0.51,
    avg_defender_hull_remaining: 0.01,
    trials_run: 400,
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
  seat_ablations: [
    {
      slot_group: "captain",
      officer: "Cadet Uhura",
      unavailable: true,
      marginal_contribution: 0.145,
      trigger_dependencies: ["Reduces incoming hostile weapon damage"],
      leave_one_out: {
        score: 0.695,
        win_rate: 0.78,
        win_rate_ci_low: 0.74,
        win_rate_ci_high: 0.82,
        stall_rate: 0.0,
        loss_rate: 0.22,
        r1_kill_rate: 0.78,
        avg_hull_remaining: 0.35,
        avg_hull_remaining_ci_low: 0.32,
        avg_hull_remaining_ci_high: 0.38,
        avg_defender_hull_remaining: 0.04,
        trials_run: 400,
        fidelity: {
          implemented: 4,
          partial: 0,
          ignored: 0,
          total_combat_effects: 4,
          coverage_pct: 100,
          grade: "A",
          caveats: [],
        },
      },
      drop_delta: {
        delta_score: -0.145,
        delta_score_ci_low: -0.16,
        delta_score_ci_high: -0.13,
        delta_win_rate: -0.15,
        delta_avg_hull_remaining: -0.13,
        delta_r1_kill_rate: -0.15,
        statistically_significant: true,
      },
      best_replacements: [
        {
          officer: "TOS Sulu",
          synergy_match: false,
          preserves_triggers: [],
          metrics: {
            score: 0.79,
            win_rate: 0.87,
            win_rate_ci_low: 0.84,
            win_rate_ci_high: 0.9,
            stall_rate: 0.0,
            loss_rate: 0.13,
            r1_kill_rate: 0.87,
            avg_hull_remaining: 0.47,
            avg_hull_remaining_ci_low: 0.44,
            avg_hull_remaining_ci_high: 0.5,
            avg_defender_hull_remaining: 0.02,
            trials_run: 400,
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
          delta: {
            delta_score: -0.05,
            delta_score_ci_low: -0.06,
            delta_score_ci_high: -0.04,
            delta_win_rate: -0.06,
            delta_avg_hull_remaining: -0.01,
            delta_r1_kill_rate: -0.06,
            statistically_significant: true,
          },
        },
      ],
    },
  ],
  substitutes: [
    {
      captain: "TOS Sulu",
      bridge: ["Badgey", "Nesmith"],
      below_decks: ["Odo", "Harry Kim"],
      changed_slots: [
        {
          slot_group: "captain",
          from: "Cadet Uhura",
          to: "TOS Sulu",
        },
      ],
      retention_pct: 94.0,
      summary:
        "Retains 94.0% of baseline (-5.00% ± 1.00%) replacing 1 unavailable seat",
      metrics: {
        score: 0.79,
        win_rate: 0.87,
        win_rate_ci_low: 0.84,
        win_rate_ci_high: 0.9,
        stall_rate: 0.0,
        loss_rate: 0.13,
        r1_kill_rate: 0.87,
        avg_hull_remaining: 0.47,
        avg_hull_remaining_ci_low: 0.44,
        avg_hull_remaining_ci_high: 0.5,
        avg_defender_hull_remaining: 0.02,
        trials_run: 400,
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
      delta: {
        delta_score: -0.05,
        delta_score_ci_low: -0.06,
        delta_score_ci_high: -0.04,
        delta_win_rate: -0.06,
        delta_avg_hull_remaining: -0.01,
        delta_r1_kill_rate: -0.06,
        statistically_significant: true,
      },
    },
  ],
};

describe("CounterfactualPanel", () => {
  it("renders marginal impact, trigger dependencies, and loads substitute crews", () => {
    const onToggle = vi.fn();
    const onRun = vi.fn();
    const onLoad = vi.fn();
    const onClose = vi.fn();

    render(
      <CounterfactualPanel
        data={sampleResponse}
        loading={false}
        error={null}
        unavailableOfficers={["Cadet Uhura"]}
        onToggleUnavailable={onToggle}
        onClearUnavailable={vi.fn()}
        onRunWithUnavailable={onRun}
        onClose={onClose}
        onLoadCrew={onLoad}
      />,
    );

    expect(screen.getByText("+14.50% impact")).toBeTruthy();
    expect(screen.getByText("95% sig")).toBeTruthy();
    expect(
      screen.getByText("Reduces incoming hostile weapon damage"),
    ).toBeTruthy();
    expect(screen.getByText("94.0% retained")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Load Crew" }));
    expect(onLoad).toHaveBeenCalledWith({
      captain: "TOS Sulu",
      bridge: ["Badgey", "Nesmith"],
      below_decks: ["Odo", "Harry Kim"],
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Find Substitute Crews" }),
    );
    expect(onRun).toHaveBeenCalledWith(["Cadet Uhura"]);
  });
});
