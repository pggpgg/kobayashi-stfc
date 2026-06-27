import { expect, test, type Page } from "@playwright/test";

/** Max wait for tiered optimize (scout + confirm) on cold CI. */
const OPTIMIZE_DONE_MS = 240_000;

async function pickCaptainAnnorax(page: Page) {
  const bridgeSection = page
    .locator("section")
    .filter({ has: page.locator("h2", { hasText: "BRIDGE" }) })
    .first();
  const captainCombo = bridgeSection.getByRole("combobox").nth(1);

  const selectAnnorax = async () => {
    await captainCombo.click();
    await captainCombo.fill("Annorax");
    await page
      .getByRole("option", { name: /^Annorax$/i })
      .first()
      .waitFor({ state: "visible", timeout: 15_000 });
    await page.getByRole("option", { name: /^Annorax$/i }).first().click();
  };

  try {
    await selectAnnorax();
  } catch {
    await page.getByRole("button", { name: "Sandbox", exact: true }).click();
    await selectAnnorax();
  }
}

test.describe("workspace smoke flow", () => {
  test.describe.configure({ timeout: 300_000 });

  test("workspace → sim → optimize → preset → results library", async ({
    page,
  }) => {
    await page.goto("/");
    await expect(page).toHaveTitle(/kobayashi/i);
    await expect(
      page.getByRole("button", { name: "Run simulation" }),
    ).toBeVisible();

    await expect(
      page.getByRole("heading", { name: "BRIDGE", exact: true }).first(),
    ).toBeVisible();

    await pickCaptainAnnorax(page);

    await page
      .getByRole("button", { name: /Set fight iterations to 1,?000/i })
      .click();
    await page.getByRole("button", { name: "Run simulation" }).click();
    await expect(page.getByText(/Win rate:/)).toBeVisible({ timeout: 120_000 });

    // "Max crews" now lives in the collapsed "Search scope" advanced section.
    await page.locator("summary").filter({ hasText: "Search scope" }).click();
    const maxCrews = page.getByLabel("Max crews (optional)");
    await maxCrews.scrollIntoViewIfNeeded();
    await maxCrews.fill("12");

    await page.getByRole("button", { name: "Run optimization" }).click();

    await expect(
      page.getByRole("button", { name: "Run optimization" }),
    ).toBeEnabled({ timeout: OPTIMIZE_DONE_MS });

    await expect(page.getByText(/Select 2–5 rows to compare/)).toBeVisible({
      timeout: 30_000,
    });

    const presetName = `e2e-${Date.now()}`;
    await page.getByRole("button", { name: "Save as Preset" }).click();
    await expect(
      page.getByRole("heading", { name: "Save preset" }),
    ).toBeVisible();
    await page
      .getByRole("dialog", { name: "Save preset" })
      .getByLabel("Preset name")
      .fill(presetName);
    await page
      .getByRole("dialog", { name: "Save preset" })
      .getByRole("button", { name: "Save", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: "Save preset" }),
    ).toBeHidden({ timeout: 30_000 });

    await page.getByRole("link", { name: "Results Library" }).click();
    await expect(page.getByRole("heading", { name: "Results Library" })).toBeVisible();
    await expect(page.getByText(presetName)).toBeVisible();

    await page
      .getByRole("listitem")
      .filter({ hasText: presetName })
      .getByRole("button", { name: "Load", exact: true })
      .click();

    await expect(page).toHaveURL(/http:\/\/[^/]+\/$/, { timeout: 15_000 });
    await expect(
      page.getByRole("button", { name: "Run simulation" }),
    ).toBeVisible();
  });
});
