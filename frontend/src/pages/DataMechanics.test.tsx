import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import DataMechanics from "./DataMechanics";

vi.mock("../lib/api", () => ({
  fetchDataVersion: vi.fn().mockResolvedValue({
    officer_version: "canonical",
    hostile_version: "hostiles-2026-06-20",
    ship_version: "ships-2026-06-20",
    mechanics: [],
  }),
  fetchMechanicsCoverage: vi.fn().mockResolvedValue({
    status: "ok",
    lcars_officers_files: 286,
    lcars_effects: { implemented: 447, partial: 1, ignored: 126 },
    ship_hull_abilities: { implemented: 79, partial: 65, ignored: 0 },
    ships_with_abilities_scanned: 114,
    hostile_catalog_entries: { implemented: 255, partial: 721, ignored: 0 },
    hostile_catalog_entry_count: 976,
    hostile_upstream_unique_ability_ids: 976,
    hostile_catalog_modeled_count: 255,
    hostile_catalog_noop_count: 721,
    hostile_upstream_ids_missing_from_catalog: 0,
    lcars_by_effect_type: {},
    lcars_ignored_samples: [],
    fidelity_backlog: [
      {
        rank: 1,
        area: "hostile_ability_catalog",
        key: "_aggregate",
        ignored: 0,
        partial: 721,
        implemented: 255,
        summary: "Hostile ability catalog coverage gap",
      },
    ],
    notes: [],
  }),
  formatApiError: vi.fn((error: unknown) => String(error)),
}));

describe("DataMechanics", () => {
  it("renders live coverage counts and fidelity backlog", async () => {
    render(<DataMechanics />);

    expect(await screen.findByText("Live mechanics coverage")).toBeTruthy();
    expect(screen.getByText("286 officer files scanned")).toBeTruthy();
    expect(screen.getByText(/976 catalog entries/)).toBeTruthy();
    expect(
      screen.getByText("Hostile ability catalog coverage gap"),
    ).toBeTruthy();
    expect(screen.queryByText("Isolytic")).toBeNull();
  });
});
