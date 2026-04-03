import { test, expect } from "@playwright/test";

test("loads SPA and health endpoint responds", async ({ page, request }) => {
  const health = await request.get("/api/health");
  expect(health.ok()).toBeTruthy();

  await page.goto("/");
  await expect(page).toHaveTitle(/kobayashi/i);
});

