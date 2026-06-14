import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SensitivityRow } from "../lib/sensitivityApi";
import SensitivityResults from "./SensitivityResults";

const rows: SensitivityRow[] = [
  {
    stat: "weapon_damage",
    delta_applied: 0.05,
    mean_diff: 120,
    mean_diff_relative: 0.02,
    ci95_low: 80,
    ci95_high: 160,
    significant: true,
  },
  {
    stat: "shield_hp",
    delta_applied: 0.1,
    mean_diff: 5,
    mean_diff_relative: 0.5,
    ci95_low: -2,
    ci95_high: 12,
    significant: false,
  },
];

describe("SensitivityResults", () => {
  it("shows empty state when there are no rows", () => {
    render(
      <SensitivityResults
        rows={[]}
        baselineMean={0}
        metric="hull_remaining"
        numSims={100}
      />,
    );

    expect(
      screen.getByText(
        /No rows yet. Configure the scenario and run a sensitivity analysis./,
      ),
    ).toBeTruthy();
  });

  it("sorts rows by absolute relative delta and marks significance", () => {
    render(
      <SensitivityResults
        rows={rows}
        baselineMean={1000}
        metric="hull_remaining"
        numSims={200}
      />,
    );

    expect(screen.getByText(/hull_remaining/)).toBeTruthy();
    expect(screen.getByText(/baseline mean =/)).toBeTruthy();

    const statCells = screen
      .getAllByRole("row")
      .slice(1)
      .map((row) => row.children[0]?.textContent);
    expect(statCells[0]).toBe("shield_hp");
    expect(statCells[1]).toBe("weapon_damage");

    expect(screen.getByText("✓")).toBeTruthy();
    expect(screen.getByText("·")).toBeTruthy();
  });
});
