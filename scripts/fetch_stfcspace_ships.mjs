#!/usr/bin/env node
/**
 * Fetch ship detail JSON files from data.stfc.space and cache them locally.
 *
 * Stage 1 of 2: fetching only — no normalization.
 * Run this first, then run normalize_stfcspace_ships.mjs to produce canonical output.
 *
 * Usage:
 *   node scripts/fetch_stfcspace_ships.mjs [--force] [--limit N] [--ids 123,456,...]
 *
 * Options:
 *   --force       Re-fetch even if cached file already exists
 *   --limit N     Only fetch the first N ships (useful for testing)
 *   --ids ...     Comma-separated list of numeric ship IDs to fetch (overrides summary)
 *
 * Output:
 *   data/upstream/data-stfc-space/ships/{id}.json   — raw detail JSON per ship
 *   data/import_logs/fetch-stfcspace-ships-{date}.json — fetch log
 *
 * Run from repo root.
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const SUMMARY_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/summary-ship.json");
const CACHE_DIR = path.join(REPO_ROOT, "data/upstream/data-stfc-space/ships");
const LOG_DIR = path.join(REPO_ROOT, "data/import_logs");
const BASE_URL = "https://data.stfc.space";

// Rate limiting: delay between requests to avoid hammering the API.
const REQUEST_DELAY_MS = 150;
// Retry on transient failures.
const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 1000;

// ── CLI args ──────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const force = args.includes("--force");
const limitIdx = args.indexOf("--limit");
const limit = limitIdx !== -1 ? parseInt(args[limitIdx + 1], 10) : null;
const idsIdx = args.indexOf("--ids");
const filterIds = idsIdx !== -1
  ? new Set(args[idsIdx + 1].split(",").map((s) => parseInt(s.trim(), 10)))
  : null;

// ── Helpers ───────────────────────────────────────────────────────────────────
function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function fetchWithRetry(url, retries = MAX_RETRIES) {
  for (let attempt = 1; attempt <= retries; attempt++) {
    try {
      const res = await fetch(url);
      if (!res.ok) {
        throw new Error(`HTTP ${res.status} ${res.statusText}`);
      }
      return await res.json();
    } catch (err) {
      if (attempt === retries) throw err;
      console.warn(`  ⚠ Attempt ${attempt} failed (${err.message}), retrying in ${RETRY_DELAY_MS}ms…`);
      await sleep(RETRY_DELAY_MS * attempt);
    }
  }
}

// ── Main ──────────────────────────────────────────────────────────────────────
async function main() {
  await fs.mkdir(CACHE_DIR, { recursive: true });
  await fs.mkdir(LOG_DIR, { recursive: true });

  // Load summary to get all ship IDs.
  let summary;
  try {
    summary = JSON.parse(await fs.readFile(SUMMARY_PATH, "utf8"));
  } catch (err) {
    console.error(`Failed to read summary: ${SUMMARY_PATH}\n${err.message}`);
    process.exit(1);
  }

  // Filter by explicit IDs or limit.
  let ships = filterIds
    ? summary.filter((s) => filterIds.has(s.id))
    : summary;
  if (limit !== null) ships = ships.slice(0, limit);

  console.log(`Fetching detail for ${ships.length} ships (force=${force})…\n`);

  const log = {
    timestamp: new Date().toISOString(),
    source: BASE_URL,
    total: ships.length,
    fetched: 0,
    cached: 0,
    failed: 0,
    failures: [],
  };

  for (let i = 0; i < ships.length; i++) {
    const ship = ships[i];
    const cachePath = path.join(CACHE_DIR, `${ship.id}.json`);

    // Check cache.
    if (!force) {
      try {
        await fs.access(cachePath);
        process.stdout.write(`[${i + 1}/${ships.length}] ${ship.id} — cached\n`);
        log.cached++;
        continue;
      } catch {
        // Not cached, proceed to fetch.
      }
    }

    // Fetch.
    const endpoint = `${BASE_URL}/ship/${ship.id}.json`;
    try {
      process.stdout.write(`[${i + 1}/${ships.length}] ${ship.id} — fetching…`);
      const detail = await fetchWithRetry(endpoint);
      await fs.writeFile(cachePath, JSON.stringify(detail, null, 2));
      process.stdout.write(` ✓\n`);
      log.fetched++;
    } catch (err) {
      process.stdout.write(` ✗ (${err.message})\n`);
      log.failed++;
      log.failures.push({ id: ship.id, error: err.message });
    }

    // Rate limit.
    if (i < ships.length - 1) await sleep(REQUEST_DELAY_MS);
  }

  // Write log.
  const dateStr = new Date().toISOString().slice(0, 10);
  const logPath = path.join(LOG_DIR, `fetch-stfcspace-ships-${dateStr}.json`);
  await fs.writeFile(logPath, JSON.stringify(log, null, 2));

  console.log(`\nDone. fetched=${log.fetched} cached=${log.cached} failed=${log.failed}`);
  console.log(`Log: ${logPath}`);
  if (log.failed > 0) {
    console.warn(`\nFailed IDs:`);
    log.failures.forEach((f) => console.warn(`  ${f.id}: ${f.error}`));
    process.exit(1);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
