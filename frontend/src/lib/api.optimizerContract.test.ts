import { afterEach, describe, expect, it, vi } from "vitest";
import { optimize, optimizeStart } from "./api";

afterEach(() => vi.unstubAllGlobals());

describe("generated optimizer request contract", () => {
  it("preserves advanced options on synchronous optimize requests", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    await optimize({
      ship: "realta",
      hostile: "123",
      strategy: "tiered",
      local_refinement: true,
      constraints: { must_include: ["Tom Paris"] },
      tiered_top_k: 12,
    });
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toMatchObject({
      strategy: "tiered",
      local_refinement: true,
      constraints: { must_include: ["Tom Paris"] },
      tiered_top_k: 12,
    });
  });

  it("preserves the supported budget options on asynchronous requests", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    await optimizeStart({
      ship: "realta",
      hostile: "123",
      analytical_prefilter_keep: 30,
      local_refinement_seeds: 4,
      local_refinement_rounds: 2,
      exhaustive_scout_sims: 20,
      exhaustive_scout_top_keep: 10,
      tiered_scout_uniform: false,
      tiered_confirm_budget_cap_mult: 1.5,
    });
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toMatchObject({
      analytical_prefilter_keep: 30,
      local_refinement_seeds: 4,
      local_refinement_rounds: 2,
      exhaustive_scout_sims: 20,
      exhaustive_scout_top_keep: 10,
      tiered_scout_uniform: false,
      tiered_confirm_budget_cap_mult: 1.5,
    });
  });
});
