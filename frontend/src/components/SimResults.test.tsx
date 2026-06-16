import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CrewRecommendation, SimulateStats } from "../lib/api";
import SimResults from "./SimResults";

/** Minimal valid optimize row for tests (CI bounds bracket the point estimate). */
function crewRec(p: {
  captain: string;
  bridge: string | string[];
  below_decks: string | string[];
  win_rate: number;
  stall_rate: number;
  loss_rate: number;
  avg_hull_remaining: number;
  avg_defender_hull_remaining?: number;
  r1_kill_rate?: number;
}): CrewRecommendation {
  const w = p.win_rate;
  const s = p.stall_rate;
  const l = p.loss_rate;
  const h = p.avg_hull_remaining;
  const dh = p.avg_defender_hull_remaining ?? 0.25;
  const r1 = p.r1_kill_rate ?? 0.05;
  return {
    captain: p.captain,
    bridge: p.bridge,
    below_decks: p.below_decks,
    win_rate: w,
    win_rate_ci_low: Math.max(0, w - 0.02),
    win_rate_ci_high: Math.min(1, w + 0.02),
    stall_rate: s,
    stall_rate_ci_low: Math.max(0, s - 0.02),
    stall_rate_ci_high: Math.min(1, s + 0.02),
    loss_rate: l,
    loss_rate_ci_low: Math.max(0, l - 0.02),
    loss_rate_ci_high: Math.min(1, l + 0.02),
    r1_kill_rate: r1,
    r1_kill_rate_ci_low: Math.max(0, r1 - 0.02),
    r1_kill_rate_ci_high: Math.min(1, r1 + 0.02),
    avg_hull_remaining: h,
    avg_hull_remaining_ci_low: Math.max(0, h - 0.02),
    avg_hull_remaining_ci_high: Math.min(1, h + 0.02),
    avg_defender_hull_remaining: dh,
    avg_defender_hull_remaining_ci_low: Math.max(0, dh - 0.02),
    avg_defender_hull_remaining_ci_high: Math.min(1, dh + 0.02),
  };
}

const baseProps = {
  simResult: null as SimulateStats | null,
  recommendations: [] as CrewRecommendation[],
  loadingSim: false,
  loadingOptimize: false,
  optimizeProgress: null as number | null,
  optimizeCrewsDone: null as number | null,
  optimizeTotalCrews: null as number | null,
};

describe("SimResults", () => {
  it("renders empty state message when no results", () => {
    render(<SimResults {...baseProps} />);
    expect(screen.getByText(/Run Sim for current crew/)).toBeTruthy();
  });

  it('shows "Running..." when loadingSim is true', () => {
    render(<SimResults {...baseProps} loadingSim={true} />);
    expect(screen.getByText("Running\u2026")).toBeTruthy();
  });

  it("shows optimization progress bar when loadingOptimize", () => {
    render(
      <SimResults
        {...baseProps}
        loadingOptimize={true}
        optimizeProgress={45}
        optimizeCrewsDone={90}
        optimizeTotalCrews={200}
      />,
    );
    expect(screen.getByText(/90 \/ 200 units/)).toBeTruthy();
    expect(screen.getByText(/45%/)).toBeTruthy();
  });

  it("displays sim result stats", () => {
    const simResult: SimulateStats = {
      win_rate: 0.85,
      stall_rate: 0.1,
      loss_rate: 0.05,
      avg_hull_remaining: 0.42,
      avg_defender_hull_remaining: 0.18,
      n: 5000,
    };
    render(<SimResults {...baseProps} simResult={simResult} />);
    expect(screen.getByText("Win rate: 85.00%")).toBeTruthy();
    expect(screen.getByText("Stall rate: 10.00%")).toBeTruthy();
    expect(screen.getByText("Loss rate: 5.00%")).toBeTruthy();
    expect(screen.getByText("Your hull remaining (wins): 42.00%")).toBeTruthy();
    expect(screen.getByText("Enemy hull remaining (avg): 18.00%")).toBeTruthy();
    expect(screen.getByText("(n=5000)")).toBeTruthy();
  });

  it("displays 95% CI when present", () => {
    const simResult: SimulateStats = {
      win_rate: 0.85,
      stall_rate: 0.1,
      loss_rate: 0.05,
      avg_hull_remaining: 0.42,
      avg_defender_hull_remaining: 0.2,
      n: 5000,
      win_rate_95_ci: [0.83, 0.87],
    };
    render(<SimResults {...baseProps} simResult={simResult} />);
    expect(screen.getByText(/0\.830/)).toBeTruthy();
    expect(screen.getByText(/0\.870/)).toBeTruthy();
  });

  it("renders recommendation rows with array bridge/below_decks (API shape) as comma-separated", () => {
    const recs: CrewRecommendation[] = [
      crewRec({
        captain: "Janeway",
        bridge: ["Ent-E Data", "Tuvok"],
        below_decks: ["Seven", "Neelix", "Chakotay"],
        win_rate: 0.88,
        stall_rate: 0.06,
        loss_rate: 0.06,
        avg_hull_remaining: 0.5,
      }),
    ];
    render(<SimResults {...baseProps} recommendations={recs} />);
    expect(screen.getByText("Janeway")).toBeTruthy();
    expect(screen.getByText("Ent-E Data, Tuvok")).toBeTruthy();
    expect(screen.getByText("Seven, Neelix, Chakotay")).toBeTruthy();
  });

  it("renders recommendation rows", () => {
    const recs: CrewRecommendation[] = [
      crewRec({
        captain: "Kirk",
        bridge: "Spock, Uhura",
        below_decks: "Scotty, McCoy, Sulu",
        win_rate: 0.95,
        stall_rate: 0.03,
        loss_rate: 0.02,
        avg_hull_remaining: 0.6,
      }),
      crewRec({
        captain: "Picard",
        bridge: "Riker, Data",
        below_decks: "Worf, Crusher, LaForge",
        win_rate: 0.9,
        stall_rate: 0.05,
        loss_rate: 0.05,
        avg_hull_remaining: 0.55,
      }),
    ];
    render(<SimResults {...baseProps} recommendations={recs} />);
    expect(screen.getByText("Kirk")).toBeTruthy();
    expect(screen.getByText("Picard")).toBeTruthy();
    expect(screen.getByText("Spock, Uhura")).toBeTruthy();
    expect(screen.getByText(/Select 2\u20135 rows to compare/)).toBeTruthy();
  });

  it("shows compare section when 2+ rows selected", () => {
    const recs: CrewRecommendation[] = [
      crewRec({
        captain: "Kirk",
        bridge: "Spock, Uhura",
        below_decks: "Scotty, McCoy, Sulu",
        win_rate: 0.95,
        stall_rate: 0.03,
        loss_rate: 0.02,
        avg_hull_remaining: 0.6,
      }),
      crewRec({
        captain: "Picard",
        bridge: "Riker, Data",
        below_decks: "Worf, Crusher, LaForge",
        win_rate: 0.9,
        stall_rate: 0.05,
        loss_rate: 0.05,
        avg_hull_remaining: 0.55,
      }),
    ];
    render(<SimResults {...baseProps} recommendations={recs} />);

    // Select both rows
    const checkboxes = screen.getAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    expect(screen.getByText("Compare (delta)")).toBeTruthy();
  });

  it("shows chain grind column headers when recommendations include chain", () => {
    const recs: CrewRecommendation[] = [
      {
        ...crewRec({
          captain: "Kirk",
          bridge: "Spock",
          below_decks: "McCoy",
          win_rate: 0.4,
          stall_rate: 0.1,
          loss_rate: 0.5,
          avg_hull_remaining: 0.7,
        }),
        chain: {
          kills_target: 4,
          secondary_objective: "min_hull_damage",
          primary_success_rate: 0.4,
          primary_ci_low: 0.35,
          primary_ci_high: 0.45,
          secondary_mean_given_primary: 0.7,
          secondary_ci_low: 0.65,
          secondary_ci_high: 0.75,
          n_primary_successes: 100,
        },
      },
    ];
    render(<SimResults {...baseProps} recommendations={recs} />);
    expect(screen.getByText("P(4-kill)")).toBeTruthy();
    expect(screen.getByText("Hull %*|hit")).toBeTruthy();
  });

  it("limits selection to 5 rows", () => {
    const recs: CrewRecommendation[] = Array.from({ length: 7 }, (_, i) =>
      crewRec({
        captain: `Cap${i}`,
        bridge: `B${i}`,
        below_decks: `BD${i}`,
        win_rate: 0.9 - i * 0.05,
        stall_rate: 0.05,
        loss_rate: 0.05,
        avg_hull_remaining: 0.5,
      }),
    );
    render(<SimResults {...baseProps} recommendations={recs} />);

    const checkboxes = screen.getAllByRole("checkbox");
    // Select first 5
    for (let i = 0; i < 5; i++) {
      fireEvent.click(checkboxes[i]);
    }
    // 6th click should not add (cap at 5)
    fireEvent.click(checkboxes[5]);
    // The 6th checkbox should not be checked
    expect((checkboxes[5] as HTMLInputElement).checked).toBe(false);
  });

  it("linear eval mode shows expected hull damage column only", () => {
    const recs: CrewRecommendation[] = [
      {
        ...crewRec({
          captain: "Kirk",
          bridge: ["Spock", "McCoy"],
          below_decks: ["Scotty", "Sulu", "Uhura"],
          win_rate: 0,
          stall_rate: 0,
          loss_rate: 0,
          avg_hull_remaining: 0,
        }),
        expected_hull_damage: 125_000,
      },
    ];
    render(
      <SimResults
        {...baseProps}
        recommendations={recs}
        optimizeEffectiveStrategy="linear_eval"
      />,
    );
    expect(screen.getByText("Expected hull damage")).toBeTruthy();
    expect(screen.queryByText("Win %")).toBeNull();
    expect(screen.getByText("125,000")).toBeTruthy();
    expect(
      screen.getByText(/Approximate ranking by expected hull damage/i),
    ).toBeTruthy();
  });
});
