#!/usr/bin/env node
import { chromium } from 'playwright';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const screenshotPath = join(__dirname, '..', 'docs', 'web-ui-screenshot.png');

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });

// Navigate to Workspace (first tab, default route)
await page.goto('http://127.0.0.1:3000/', { waitUntil: 'networkidle', timeout: 15000 });

// Ensure Sandbox mode for full officer catalog
await page.getByRole('button', { name: 'Sandbox' }).click();

// Wait for officers to load (crew builder section)
await page.waitForSelector('text=BRIDGE', { timeout: 5000 });

// Select officers: Captain, Bridge 1, Bridge 2 (pick 3 distinct officers)
// Slots: Bridge 1 (nth 0), Captain (nth 1), Bridge 2 (nth 2)
const slots = await page.getByPlaceholder('Select…').all();
if (slots.length >= 3) {
  const selectOfficer = async (slotIndex, optionIndex) => {
    await slots[slotIndex].click();
    const listbox = page.getByRole('listbox');
    await listbox.waitFor({ state: 'visible', timeout: 3000 });
    const options = await listbox.getByRole('option').all();
    if (options.length > optionIndex) {
      await options[optionIndex].click();
    }
  };
  // Pick officers at indices 1, 2, 3 (skip index 0 = "— Clear —")
  await selectOfficer(1, 1); // Captain: first officer
  await selectOfficer(0, 2); // Bridge 1: second officer
  await selectOfficer(2, 3); // Bridge 2: third officer
}

// Small delay for UI to settle
await page.waitForTimeout(300);

await page.screenshot({ path: screenshotPath, fullPage: true });
await browser.close();
console.log('Screenshot saved to', screenshotPath);
