#!/usr/bin/env node
/**
 * Fetch research detail JSON files from data.stfc.space and cache them locally.
 *
 * Run before import (import reads **only** these files for per-rid detail; it does not HTTP-fetch research/{id}.json):
 *   node scripts/fetch_stfcspace_research.mjs
 *   node scripts/import_stfcspace_research.mjs --from-upstream --limit 0
 *
 * Usage:
 *   node scripts/fetch_stfcspace_research.mjs [--force] [--limit N] [--ids 123,456,...]
 *
 * Options:
 *   --force       Re-fetch even if cached file already exists
 *   --limit N     Only fetch the first N research ids from summary (testing)
 *   --ids ...     Comma-separated list of numeric research ids (overrides summary order)
 *
 * Output:
 *   data/upstream/data-stfc-space/research/{id}.json
 *   data/import_logs/fetch-stfcspace-research-{date}.json
 *
 * Run from repo root.
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const SUMMARY_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/summary-research.json");
const CACHE_DIR = path.join(REPO_ROOT, "data/upstream/data-stfc-space/research");
const LOG_DIR = path.join(REPO_ROOT, "data/import_logs");
const BASE_URL = "https://data.stfc.space";

const REQUEST_DELAY_MS = 150;
const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 1000;

const args = process.argv.slice(2);
if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: node scripts/fetch_stfcspace_research.mjs [--force] [--limit N] [--ids 123,456,...]

Cache per-rid JSON from ${BASE_URL}/research/{id}.json into:
  data/upstream/data-stfc-space/research/{id}.json

Options:
  --force   Re-fetch even if cached file already exists
  --limit   Only fetch the first N ids from summary-research.json (testing)
  --ids     Comma-separated rids (overrides summary order)
`);
  process.exit(0);
}
const force = args.includes("--force");
const limitIdx = args.indexOf("--limit");
const limit = limitIdx !== -1 ? parseInt(args[limitIdx + 1], 10) : null;
const idsIdx = args.indexOf("--ids");
const filterIds = idsIdx !== -1
  ? args[idsIdx + 1].split(",").map((s) => parseInt(s.trim(), 10)).filter((n) => !Number.isNaN(n))
  : null;

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
      console.warn(`  attempt ${attempt} failed (${err.message}), retrying in ${RETRY_DELAY_MS * attempt}ms…`);
      await sleep(RETRY_DELAY_MS * attempt);
    }
  }
}

async function main() {
  await fs.mkdir(CACHE_DIR, { recursive: true });
  await fs.mkdir(LOG_DIR, { recursive: true });

  let ids;
  if (filterIds && filterIds.length > 0) {
    ids = filterIds;
  } else {
    const raw = await fs.readFile(SUMMARY_PATH, "utf8");
    const summary = JSON.parse(raw);
    if (!Array.isArray(summary)) throw new Error("summary-research.json: expected array");
    ids = summary.map((e) => e && e.id).filter((id) => typeof id === "number");
    if (limit != null && limit > 0) {
      ids = ids.slice(0, limit);
    }
  }

  const log = {
    started: new Date().toISOString(),
    count: ids.length,
    ok: [],
    failed: [],
  };

  console.log(`Fetching ${ids.length} research detail JSON files into ${path.relative(REPO_ROOT, CACHE_DIR)} …`);

  for (let i = 0; i < ids.length; i++) {
    const id = ids[i];
    const outPath = path.join(CACHE_DIR, `${id}.json`);
    if (!force) {
      try {
        await fs.access(outPath);
        log.ok.push({ id, cached: true });
        if ((i + 1) % 500 === 0) console.log(`  ${i + 1}/${ids.length} …`);
        continue;
      } catch (_) {
        /* fetch */
      }
    }

    const url = `${BASE_URL}/research/${id}.json`;
    try {
      const json = await fetchWithRetry(url);
      await fs.writeFile(outPath, JSON.stringify(json, null, 2), "utf8");
      log.ok.push({ id, cached: false });
    } catch (e) {
      log.failed.push({ id, error: String(e.message || e) });
      console.warn(`  rid ${id}: ${e.message || e}`);
    }

    await sleep(REQUEST_DELAY_MS);
    if ((i + 1) % 100 === 0) console.log(`  ${i + 1}/${ids.length} …`);
  }

  log.finished = new Date().toISOString();
  const logPath = path.join(LOG_DIR, `fetch-stfcspace-research-${log.started.slice(0, 10)}.json`);
  await fs.writeFile(logPath, JSON.stringify(log, null, 2), "utf8");
  console.log(`Done. Log: ${path.relative(REPO_ROOT, logPath)}`);
  console.log(`  ok: ${log.ok.length}, failed: ${log.failed.length}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
