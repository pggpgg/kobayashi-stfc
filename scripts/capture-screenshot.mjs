#!/usr/bin/env node
import { chromium } from 'playwright';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const screenshotPath = join(__dirname, '..', 'docs', 'web-ui-screenshot.png');

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.goto('http://127.0.0.1:3000/', { waitUntil: 'networkidle', timeout: 10000 });
await page.click('text=Roster & Profile');
await page.waitForSelector('text=Forbidden tech', { timeout: 5000 });
await page.screenshot({ path: screenshotPath, fullPage: true });
await browser.close();
console.log('Screenshot saved to', screenshotPath);
