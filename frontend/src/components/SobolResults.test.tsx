import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SobolPairRow, SobolRow } from "../lib/sensitivityApi";
import SobolResults from "./SobolResults";

const rows: SobolRow[] = [
  {
    stat: "weapon_damage",
    base_delta: 0.05,
    s1: 0.2,
    st: 0.35,
    interaction: 0.15,
    s1_ci95_low: 0.1,
    s1_ci95_high: 0.3,
    st_ci95_low: 0.2,
    st_ci95_high: 0.5,
  },
  {
    stat: "shield_hp",
    base_delta: 0.1,
    s1: 0.05,
    st: 0.6,
    interaction: 0.55,
    s1_ci95_low: 0.01,
    s1_ci95_high: 0.09,
    st_ci95_low: 0.4,
    st_ci95_high: 0.8,
  },
];

const pairs: SobolPairRow[] = [
  {
    stat_a: "weapon_damage",
    stat_b: "shield_hp",
    base_delta_a: 0.05,
    base_delta_b: 0.1,
    s_ij: 0.12,
    s_ij_ci95_low: 0.08,
    s_ij_ci95_high: 0.16,
  },
];

describe("SobolResults", () => {
  it("shows empty state when there are no rows", () => {
    render(
      <SobolResults
        rows={[]}
        metric="hull_remaining"
        nSamples={64}
        totalSims={640}
        outputVariance={0.02}
        baseSeed={7}
      />,
    );

    expect(
      screen.getByText(/No rows yet. Configure the scenario and run Sobol analysis./),
    ).toBeTruthy();
  });

  it("sorts by total impact by default", () => {
    render(
      <SobolResults
        rows={rows}
        metric="hull_remaining"
        nSamples={64}
        totalSims={640}
        outputVariance={0.02}
        baseSeed={7}
      />,
    );

    const statCells = screen.getAllByRole("row").slice(1).map((row) => row.children[0]?.textContent);
    expect(statCells[0]).toBe("shield_hp");
    expect(statCells[1]).toBe("weapon_damage");
  });

  it("re-sorts on solo impact and renders pairwise section when pairs exist", () => {
    render(
      <SobolResults
        rows={rows}
        metric="hull_remaining"
        nSamples={64}
        totalSims={640}
        outputVariance={0.02}
        baseSeed={7}
        pairs={pairs}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Solo impact" }));

    const statCells = screen.getAllByRole("row").slice(1).map((row) => row.children[0]?.textContent);
    expect(statCells[0]).toBe("weapon_damage");
    expect(statCells[1]).toBe("shield_hp");

    expect(screen.getByText(/Pairwise interactions/)).toBeTruthy();
    expect(screen.getAllByText("weapon_damage").length).toBeGreaterThan(0);
    expect(screen.getAllByText("shield_hp").length).toBeGreaterThan(0);
  });
});
