#!/usr/bin/env node
/**
 * Fetch hostile detail JSON from data.stfc.space into the upstream cache.
 *
 * Then normalize into data/hostiles/:
 *   cargo run --bin normalize_hostiles_stfc_space
 *
 * Refresh summaries first when you need the latest id list:
 *   node scripts/fetch_stfcspace_page_upstream.mjs   # or .py
 *
 * Usage:
 *   node scripts/fetch_stfcspace_hostiles.mjs [--full|--missing-only] [--limit N] [--ids 123,...]
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
const SUMMARY_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/summary-hostile.json");
const CACHE_DIR = path.join(REPO_ROOT, "data/upstream/data-stfc-space/hostiles");

function printHelp() {
  console.log(`Usage: node scripts/fetch_stfcspace_hostiles.mjs [options]

Cache https://data.stfc.space/hostile/{id}.json → data/upstream/data-stfc-space/hostiles/{id}.json

Options:
  --full           Re-fetch every hostile in summary-hostile.json
  --missing-only   Only fetch ids with no local file (default)
  --force          Same as --full
  --limit N        First N hostiles from summary order
  --ids a,b,c      Only these ids (must appear in summary)
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
    console.error("No hostile ids to fetch.");
    process.exit(1);
  }

  await runDetailFetch({
    repoRoot: REPO_ROOT,
    cacheDir: CACHE_DIR,
    segment: "hostile",
    logStem: "hostiles",
    ids,
    full: args.full,
  });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
