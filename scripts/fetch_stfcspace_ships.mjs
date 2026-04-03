#!/usr/bin/env node
/**
 * Fetch ship detail JSON files from data.stfc.space and cache them locally.
 *
 * Refresh summaries first when you need the latest id list:
 *   node scripts/fetch_stfcspace_page_upstream.mjs   # or .py
 *
 * Usage:
 *   node scripts/fetch_stfcspace_ships.mjs [--full|--missing-only] [--limit N] [--ids 123,456,...]
 *
 * Options:
 *   --full          Re-download every ship in the summary (overwrite cache)
 *   --missing-only  Only download ids with no local file (default)
 *   --force         Same as --full (backward compatible)
 *   --limit N       Only consider the first N ships from the summary order
 *   --ids ...       Comma-separated ids (only those that appear in summary are fetched)
 *
 * Output:
 *   data/upstream/data-stfc-space/ships/{id}.json
 *   data/import_logs/fetch-stfcspace-ships-{date}.json
 *
 * Run from repo root.
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import {
  parseDetailFetchArgs,
  runDetailFetch,
  idsFromSummaryArray,
} from "./lib/stfcspace_detail_fetch.mjs";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const SUMMARY_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/summary-ship.json");
const CACHE_DIR = path.join(REPO_ROOT, "data/upstream/data-stfc-space/ships");

function printHelp() {
  console.log(`Usage: node scripts/fetch_stfcspace_ships.mjs [options]

Cache per-ship JSON from https://data.stfc.space/ship/{id}.json into:
  data/upstream/data-stfc-space/ships/{id}.json

Refresh summary-ship.json first (e.g. fetch_stfcspace_page_upstream) before relying on new ships.

Options:
  --full           Re-fetch all ships in summary (overwrite local files)
  --missing-only   Only fetch ids missing locally (default)
  --force          Same as --full
  --limit N        First N ships from summary order
  --ids a,b,c      Restrict to these numeric ids (must exist in summary)
`);
}

async function main() {
  const argv = process.argv.slice(2);
  const args = parseDetailFetchArgs(argv);
  if (args.help) {
    printHelp();
    process.exit(0);
  }

  let summary;
  try {
    summary = JSON.parse(await fs.readFile(SUMMARY_PATH, "utf8"));
  } catch (err) {
    console.error(`Failed to read summary: ${SUMMARY_PATH}\n${err.message}`);
    process.exit(1);
  }

  const idFilter = args.ids?.length ? new Set(args.ids) : null;
  const ids = idsFromSummaryArray(summary, { limit: args.limit, idFilter });

  if (ids.length === 0) {
    console.error("No ship ids to fetch (check --ids / summary).");
    process.exit(1);
  }

  await runDetailFetch({
    repoRoot: REPO_ROOT,
    cacheDir: CACHE_DIR,
    segment: "ship",
    logStem: "ships",
    ids,
    full: args.full,
  });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
