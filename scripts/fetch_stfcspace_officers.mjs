#!/usr/bin/env node
/**
 * Cache per-officer JSON from data.stfc.space (reference / tooling only).
 * Kobayashi combat officers remain LCARS (officers.lcars.yaml); this does not replace that pipeline.
 *
 * Refresh summaries first when you need the latest id list:
 *   node scripts/fetch_stfcspace_page_upstream.mjs   # or .py
 *
 * Usage:
 *   node scripts/fetch_stfcspace_officers.mjs [--full|--missing-only] [--limit N] [--ids 123,...]
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
const SUMMARY_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/summary-officer.json");
const CACHE_DIR = path.join(REPO_ROOT, "data/upstream/data-stfc-space/officers");

function printHelp() {
  console.log(`Usage: node scripts/fetch_stfcspace_officers.mjs [options]

Cache https://data.stfc.space/officer/{id}.json → data/upstream/data-stfc-space/officers/{id}.json

Reference only — abilities for the simulator stay in LCARS.

Options:
  --full           Re-fetch every officer in summary-officer.json
  --missing-only   Only fetch ids with no local file (default)
  --force          Same as --full
  --limit N        First N officers from summary order
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
    console.error("No officer ids to fetch.");
    process.exit(1);
  }

  await runDetailFetch({
    repoRoot: REPO_ROOT,
    cacheDir: CACHE_DIR,
    segment: "officer",
    logStem: "officers",
    ids,
    full: args.full,
  });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
