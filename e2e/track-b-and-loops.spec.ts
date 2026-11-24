import { expect, test } from "@playwright/test";

/** Max wait for optimize + counterfactual / fleet requests on cold CI. */
const OPTIMIZE_DONE_MS = 240_000;

test.describe("Track B & Loops E2E flows", () => {
  test.describe.configure({ timeout: 300_000 });

  test("workspace optimize → counterfactual ablations → substitute crew planner → load crew", async ({
    page,
  }) => {
    const pageErrors: string[] = [];
    page.on("pageerror", (err) => pageErrors.push(err.message));

    await page.goto("/");
    await expect(
      page.getByRole("button", { name: "Run simulation" }),
    ).toBeVisible();

    // Keep candidate count small so optimize finishes quickly on cold CI.
    await page.locator("summary").filter({ hasText: "Search scope" }).click();
    const maxCrews = page.getByLabel("Max crews (optional)");
    await maxCrews.scrollIntoViewIfNeeded();
    await maxCrews.fill("12");

    const started = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname === "/api/optimize/start",
    );
    await page.getByRole("button", { name: "Run optimization" }).click();
    const startResponse = await started;
    expect(startResponse.status(), await startResponse.text()).toBe(200);

    await expect(
      page.getByRole("button", { name: "Run optimization" }),
    ).toBeEnabled({ timeout: OPTIMIZE_DONE_MS });

    const inspectBtn = page.getByRole("button", {
      name: /Inspect #1 & Substitutes/,
    });
    await expect(inspectBtn).toBeVisible({ timeout: 30_000 });

    const initialCf = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname === "/api/crew/counterfactual",
    );
    await inspectBtn.click();
    const initialCfResp = await initialCf;
    expect(initialCfResp.status(), await initialCfResp.text()).toBe(200);

    const panel = page.locator(
      'section[aria-label="Counterfactual ablation and substitute crew planner"]',
    );
    await expect(panel).toBeVisible({ timeout: 30_000 });
    await expect(
      panel.getByText("1. Seat Marginal Impact (Leave-One-Out CRN)"),
    ).toBeVisible();

    // Mark the first seat (Captain) unavailable and synthesize legal substitute crews.
    const firstUnavailableCheckbox = panel.getByRole("checkbox").first();
    const ariaLabel =
      (await firstUnavailableCheckbox.getAttribute("aria-label")) ?? "";
    const excludedOfficer = ariaLabel
      .replace(/^Mark\s+/i, "")
      .replace(/\s+unavailable$/i, "")
      .trim();
    expect(excludedOfficer.length).toBeGreaterThan(0);

    await firstUnavailableCheckbox.check();

    const rerunCf = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname === "/api/crew/counterfactual",
    );
    await panel
      .getByRole("button", { name: "Find Substitute Crews" })
      .click();
    const rerunCfResp = await rerunCf;
    const rerunBodyText = await rerunCfResp.text();
    expect(rerunCfResp.status(), rerunBodyText).toBe(200);
    const rerunJson = JSON.parse(rerunBodyText) as {
      substitutes: Array<{
        captain: string;
        bridge: string[];
        below_decks: string[];
      }>;
    };
    expect(rerunJson.substitutes.length).toBeGreaterThan(0);
    for (const sub of rerunJson.substitutes) {
      const names = [sub.captain, ...sub.bridge, ...sub.below_decks].map((n) =>
        n.trim().toLowerCase(),
      );
      expect(names).not.toContain(excludedOfficer.toLowerCase());
    }

    await expect(
      panel.getByText(/2\. Substitute Crew Planner \(Excluding:/),
    ).toBeVisible();

    const loadCrewBtn = panel
      .getByRole("button", { name: "Load Crew" })
      .first();
    await expect(loadCrewBtn).toBeVisible();
    await loadCrewBtn.click();

    await expect(
      page.getByText(/Loaded .+ into Crew Builder for simulation or tweaking\./),
    ).toBeVisible({ timeout: 15_000 });
    expect(pageErrors).toEqual([]);
  });

  test("multi-ship fleet allocator solves 2 docks with zero officer conflicts", async ({
    page,
  }) => {
    const pageErrors: string[] = [];
    page.on("pageerror", (err) => pageErrors.push(err.message));

    await page.goto("/");
    await page.getByRole("link", { name: "Fleet Allocator" }).click();
    await expect(page).toHaveURL(/\/fleet$/);
    await expect(
      page.getByRole("heading", { name: "Multi-Ship Fleet Allocator" }),
    ).toBeVisible();

    const optimizeBtn = page.getByRole("button", { name: "Optimize Fleet" });
    await expect(optimizeBtn).toBeEnabled({ timeout: 30_000 });

    await page.getByLabel("Fleet Objective").selectOption("min_regret");
    await page.getByLabel("Sims per crew").selectOption("150");
    await page.getByLabel("Portfolio budget per dock").selectOption("48");

    const allocateRespPromise = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname === "/api/fleet/allocate",
      { timeout: OPTIMIZE_DONE_MS },
    );
    await optimizeBtn.click();
    const allocateResp = await allocateRespPromise;
    const allocateText = await allocateResp.text();
    expect(allocateResp.status(), allocateText).toBe(200);

    const allocateJson = JSON.parse(allocateText) as {
      conflict_free: boolean;
      fleet_efficiency_pct: number;
      slots: Array<{
        assigned_crew: {
          captain: string;
          bridge: string[];
          below_decks: string[];
        } | null;
      }>;
    };
    expect(allocateJson.conflict_free).toBe(true);
    expect(allocateJson.slots.length).toBe(2);
    expect(allocateJson.fleet_efficiency_pct).toBeGreaterThan(0);

    const seen = new Set<string>();
    for (const slot of allocateJson.slots) {
      const crew = slot.assigned_crew;
      expect(crew).not.toBeNull();
      if (!crew) continue;
      for (const name of [crew.captain, ...crew.bridge, ...crew.below_decks]) {
        const key = name.trim().toLowerCase();
        expect(seen.has(key), `duplicate officer across docks: ${key}`).toBe(
          false,
        );
        seen.add(key);
      }
    }

    const summary = page.locator(
      'section[aria-label="Fleet allocation summary"]',
    );
    await expect(summary).toBeVisible({ timeout: 30_000 });
    await expect(summary.getByText("Zero Officer Conflicts")).toBeVisible();
    await expect(summary.getByText("FLEET EFFICIENCY")).toBeVisible();
    await expect(summary.getByText("Min-Regret")).toBeVisible();
    expect(pageErrors).toEqual([]);
  });

  test("loops workspace exposes farming loot goal and disambiguated Xindi ladders", async ({
    page,
  }) => {
    const pageErrors: string[] = [];
    page.on("pageerror", (err) => pageErrors.push(err.message));

    await page.goto("/");
    await page.getByRole("link", { name: "Loops workspace" }).click();
    await expect(page).toHaveURL(/\/loops$/);
    await expect(
      page.getByRole("heading", { name: "Loops workspace" }),
    ).toBeVisible();

    const sidebar = page.locator('aside[aria-label="Gameplay loops"]');
    await expect(sidebar).toBeVisible();

    await sidebar
      .getByRole("button", { name: /^Xindi-Aquatic(\s+\d+\s+saved)?$/ })
      .click();
    await expect(
      page.getByRole("heading", { level: 2, name: "Xindi-Aquatic" }),
    ).toBeVisible();

    const rankSelect = page.getByLabel("Rank crews by");
    await rankSelect.selectOption("farming_loot");
    await expect(
      page.getByText(/expected primary resource payout/i),
    ).toBeVisible();

    // Switch to the disambiguated Enhanced Cruisers ladder and confirm it renders cleanly.
    await sidebar
      .getByRole("button", { name: /Xindi-Aquatic Enhanced Cruisers/ })
      .click();
    await expect(
      page.getByRole("heading", {
        level: 2,
        name: "Xindi-Aquatic Enhanced Cruisers",
      }),
    ).toBeVisible();

    expect(pageErrors).toEqual([]);
  });
});
