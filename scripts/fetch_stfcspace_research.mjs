#!/usr/bin/env node
/**
 * Fetch research detail JSON files from data.stfc.space and cache them locally.
 *
 * Run before import (import reads **only** these files for per-rid detail):
 *   node scripts/fetch_stfcspace_research.mjs
 *   node scripts/import_stfcspace_research.mjs --from-upstream --limit 0
 *
 * Refresh summaries first when you need the latest id list:
 *   node scripts/fetch_stfcspace_page_upstream.mjs   # or .py
 *
 * Usage:
 *   node scripts/fetch_stfcspace_research.mjs [--full|--missing-only] [--limit N] [--ids 123,456,...]
 *
 * Options:
 *   --full          Re-download every rid in the summary (overwrite cache)
 *   --missing-only  Only download rids with no local file (default)
 *   --force         Same as --full (backward compatible)
 *   --limit N       Only fetch the first N rids from summary order (ignored if --ids is set)
 *   --ids ...       Comma-separated numeric rids (uses API directly; no summary filter)
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

import {
  parseDetailFetchArgs,
  runDetailFetch,
  idsFromSummaryArray,
} from "./lib/stfcspace_detail_fetch.mjs";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const SUMMARY_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/summary-research.json");
const CACHE_DIR = path.join(REPO_ROOT, "data/upstream/data-stfc-space/research");

function printHelp() {
  console.log(`Usage: node scripts/fetch_stfcspace_research.mjs [options]

Cache per-rid JSON from https://data.stfc.space/research/{id}.json into:
  data/upstream/data-stfc-space/research/{id}.json

Options:
  --full           Re-fetch all rids in summary-research.json
  --missing-only   Only fetch rids missing locally (default)
  --force          Same as --full
  --limit N        First N rids from summary (ignored when --ids is set)
  --ids a,b,c      Fetch only these rids (does not read summary for the list)
`);
}

async function main() {
  const argv = process.argv.slice(2);
  const args = parseDetailFetchArgs(argv);
  if (args.help) {
    printHelp();
    process.exit(0);
  }

  let ids;
  if (args.ids?.length) {
    ids = args.ids;
    if (args.limit != null && args.limit > 0) {
      ids = ids.slice(0, args.limit);
    }
  } else {
    const raw = await fs.readFile(SUMMARY_PATH, "utf8");
    const summary = JSON.parse(raw);
    ids = idsFromSummaryArray(summary, { limit: args.limit, idFilter: null });
  }

  if (ids.length === 0) {
    console.error("No research ids to fetch.");
    process.exit(1);
  }

  await runDetailFetch({
    repoRoot: REPO_ROOT,
    cacheDir: CACHE_DIR,
    segment: "research",
    logStem: "research",
    ids,
    full: args.full,
  });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
