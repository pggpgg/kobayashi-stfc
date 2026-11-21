#!/usr/bin/env node
/**
 * Run one or more detail fetchers (ships, hostiles, officers, research, forbidden_tech)
 * with bounded concurrency, conditional ETag caching, and optional drift-targeted refresh.
 *
 * Required:
 *   --entities ships,hostiles,officers,research,forbidden_tech   (comma-separated subset)
 *
 * Common flags:
 *   --full | --missing-only (default) | --force
 *   --concurrency N        Bounded parallel workers (1..16, default 6)
 *   --since-drift PATH     Also force-refresh IDs added or modified in drift JSON (missing-only still fetches missing IDs)
 *   --no-conditional       Bypass ETag / If-None-Match cache headers
 *   --limit N
 *   --ids a,b,c            Explicit numeric IDs (research uses directly; other kinds filter summary)
 *
 * Examples:
 *   node scripts/fetch_stfcspace_details.mjs --entities ships --limit 3
 *   node scripts/fetch_stfcspace_details.mjs --entities ships,hostiles --full --concurrency 8
 *   node scripts/fetch_stfcspace_details.mjs --entities hostiles,research --since-drift /tmp/drift.json
 *
 * Refresh catalog JSON first when you need new ids:
 *   node scripts/fetch_stfcspace_page_upstream.mjs
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import {
  idsFromSummaryArray,
  loadChangedIdsFromDriftFile,
  parseDetailFetchArgs,
  runDetailFetch,
} from "./lib/stfcspace_detail_fetch.mjs";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const UPSTREAM = path.join(REPO_ROOT, "data/upstream/data-stfc-space");

/** @type {Record<string, { summaryRel: string, segment: string, domainName: string, cacheRel: string, logStem: string, idMode: 'summary_filter' | 'research' }>} */
const ENTITY = {
  ships: {
    summaryRel: "summary-ship.json",
    segment: "ship",
    domainName: "ship",
    cacheRel: "ships",
    logStem: "ships",
    idMode: "summary_filter",
  },
  hostiles: {
    summaryRel: "summary-hostile.json",
    segment: "hostile",
    domainName: "hostile",
    cacheRel: "hostiles",
    logStem: "hostiles",
    idMode: "summary_filter",
  },
  officers: {
    summaryRel: "summary-officer.json",
    segment: "officer",
    domainName: "officer",
    cacheRel: "officers",
    logStem: "officers",
    idMode: "summary_filter",
  },
  research: {
    summaryRel: "summary-research.json",
    segment: "research",
    domainName: "research",
    cacheRel: "research",
    logStem: "research",
    idMode: "research",
  },
  forbidden_tech: {
    summaryRel: "summary-forbidden_tech.json",
    segment: "forbidden_tech",
    domainName: "forbidden_tech",
    cacheRel: "forbidden_tech",
    logStem: "forbidden_tech",
    idMode: "summary_filter",
  },
};

function stripEntitiesArgv(argv) {
  const out = argv.slice();
  const i = out.indexOf("--entities");
  if (i !== -1) {
    out.splice(i, 2);
  }
  return out;
}

function printHelp() {
  console.log(`Usage: node scripts/fetch_stfcspace_details.mjs --entities KIND[,KIND...] [fetch options]

KIND: ships | hostiles | officers | research | forbidden_tech

Fetch options (passed to each selected fetcher):
  --full               Re-download all ids from the summary (uses ETag conditional cache unless --no-conditional)
  --missing-only       Only download missing local files (default)
  --force              Same as --full
  --since-drift PATH   Force-refresh IDs added or modified in check_stfcspace_summary_drift --json-out
  --concurrency N      Bounded parallel workers (1..16, default 6)
  --no-conditional     Disable ETag / If-None-Match cache headers
  --limit N
  --ids a,b,c          When multiple KINDs are selected, the same --ids list is applied to each;
                       research treats --ids as explicit rids; other kinds intersect with their summary.

Run catalog refresh first when summaries may be stale:
  node scripts/fetch_stfcspace_page_upstream.mjs
`);
}

async function resolveIds(kind, def, args) {
  if (def.idMode === "research") {
    if (args.ids?.length) {
      let ids = args.ids;
      if (args.limit != null && args.limit > 0) ids = ids.slice(0, args.limit);
      return ids;
    }
    const raw = await fs.readFile(path.join(UPSTREAM, def.summaryRel), "utf8");
    const summary = JSON.parse(raw);
    return idsFromSummaryArray(summary, { limit: args.limit, idFilter: null });
  }

  const raw = await fs.readFile(path.join(UPSTREAM, def.summaryRel), "utf8");
  const summary = JSON.parse(raw);
  const idFilter = args.ids?.length ? new Set(args.ids) : null;
  return idsFromSummaryArray(summary, { limit: args.limit, idFilter });
}

async function main() {
  const argv = process.argv.slice(2);
  if (argv.includes("--help") || argv.includes("-h")) {
    printHelp();
    process.exit(0);
  }

  const entitiesIdx = argv.indexOf("--entities");
  if (entitiesIdx === -1 || !argv[entitiesIdx + 1]) {
    console.error("Missing required --entities ships,hostiles,…\n");
    printHelp();
    process.exit(2);
  }

  const kinds = argv[entitiesIdx + 1]
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  const unknown = kinds.filter((k) => !ENTITY[k]);
  if (unknown.length) {
    console.error(`Unknown --entities: ${unknown.join(", ")}`);
    process.exit(2);
  }

  const passthrough = stripEntitiesArgv(argv);
  const args = parseDetailFetchArgs(passthrough);
  if (args.help) {
    printHelp();
    process.exit(0);
  }

  for (const kind of kinds) {
    const def = ENTITY[kind];
    const ids = await resolveIds(kind, def, args);
    if (ids.length === 0) {
      console.error(`[${kind}] No ids to fetch; skipping.`);
      continue;
    }
    let forceRefreshIds = null;
    if (args.sinceDrift && !args.full) {
      forceRefreshIds = await loadChangedIdsFromDriftFile(args.sinceDrift, def.domainName);
      console.log(
        `[${kind}] drift-targeted refresh: ${forceRefreshIds.length} added/modified id(s) from ${args.sinceDrift}`,
      );
    }
    console.log(`\n========== ${kind} (${ids.length} ids) ==========\n`);
    await runDetailFetch({
      repoRoot: REPO_ROOT,
      cacheDir: path.join(UPSTREAM, def.cacheRel),
      segment: def.segment,
      logStem: def.logStem,
      ids,
      full: args.full,
      forceRefreshIds,
      concurrency: args.concurrency,
      useConditional: !args.noConditional,
    });
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
