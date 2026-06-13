import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { MorrisRow } from "../lib/sensitivityApi";
import MorrisResults from "./MorrisResults";

const rows: MorrisRow[] = [
  {
    stat: "weapon_damage",
    delta_applied: 0.05,
    mu_star: 0.4,
    mu: 0.2,
    sigma: 0.1,
    n_samples: 8,
    mu_star_ci95_low: 0.3,
    mu_star_ci95_high: 0.5,
  },
  {
    stat: "shield_hp",
    delta_applied: 0.1,
    mu_star: 0.9,
    mu: -0.1,
    sigma: 0.6,
    n_samples: 8,
    mu_star_ci95_low: 0.7,
    mu_star_ci95_high: 1.1,
  },
];

describe("MorrisResults", () => {
  it("shows empty state when there are no rows", () => {
    render(
      <MorrisResults
        rows={[]}
        metric="win_rate"
        rTrajectories={4}
        numSimsPerPoint={50}
        totalSims={200}
        baseSeed={42}
      />,
    );

    expect(
      screen.getByText(/No rows yet. Configure the scenario and run Morris screening./),
    ).toBeTruthy();
  });

  it("sorts by μ* by default and flags interaction dots", () => {
    render(
      <MorrisResults
        rows={rows}
        metric="win_rate"
        rTrajectories={4}
        numSimsPerPoint={50}
        totalSims={200}
        baseSeed={42}
      />,
    );

    const statCells = screen.getAllByRole("row").slice(1).map((row) => row.children[0]?.textContent);
    expect(statCells[0]).toBe("shield_hp");
    expect(statCells[1]).toBe("weapon_damage");
    expect(screen.getByText("•")).toBeTruthy();
  });

  it("re-sorts when the user picks a different sort key", () => {
    render(
      <MorrisResults
        rows={rows}
        metric="win_rate"
        rTrajectories={4}
        numSimsPerPoint={50}
        totalSims={200}
        baseSeed={42}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "σ (interaction)" }));

    const statCells = screen.getAllByRole("row").slice(1).map((row) => row.children[0]?.textContent);
    expect(statCells[0]).toBe("shield_hp");
    expect(statCells[1]).toBe("weapon_damage");
  });
});
