#!/usr/bin/env node
/**
 * Run one or more detail fetchers (ships, hostiles, officers, research, forbidden_tech)
 * with the same flags passed through to each selected kind.
 *
 * Required:
 *   --entities ships,hostiles,officers,research,forbidden_tech   (comma-separated subset)
 *
 * Common flags (see per-entity scripts for full help):
 *   --full | --missing-only (default) | --force
 *   --limit N
 *   --ids a,b,c   (semantics match each script: ships/hostiles/officers/ft filter summary;
 *                  research uses ids directly without summary)
 *
 * Examples:
 *   node scripts/fetch_stfcspace_details.mjs --entities ships --limit 3
 *   node scripts/fetch_stfcspace_details.mjs --entities ships,hostiles --full
 *
 * Refresh catalog JSON first when you need new ids:
 *   node scripts/fetch_stfcspace_page_upstream.mjs
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
const UPSTREAM = path.join(REPO_ROOT, "data/upstream/data-stfc-space");

/** @type {Record<string, { summaryRel: string, segment: string, cacheRel: string, logStem: string, idMode: 'summary_filter' | 'research' }>} */
const ENTITY = {
  ships: {
    summaryRel: "summary-ship.json",
    segment: "ship",
    cacheRel: "ships",
    logStem: "ships",
    idMode: "summary_filter",
  },
  hostiles: {
    summaryRel: "summary-hostile.json",
    segment: "hostile",
    cacheRel: "hostiles",
    logStem: "hostiles",
    idMode: "summary_filter",
  },
  officers: {
    summaryRel: "summary-officer.json",
    segment: "officer",
    cacheRel: "officers",
    logStem: "officers",
    idMode: "summary_filter",
  },
  research: {
    summaryRel: "summary-research.json",
    segment: "research",
    cacheRel: "research",
    logStem: "research",
    idMode: "research",
  },
  forbidden_tech: {
    summaryRel: "summary-forbidden_tech.json",
    segment: "forbidden_tech",
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
  --full           Re-download all ids from the summary (or --ids list for research)
  --missing-only   Only download missing local files (default)
  --force          Same as --full
  --limit N
  --ids a,b,c      When multiple KINDs are selected, the same --ids list is applied to each;
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
    console.log(`\n========== ${kind} (${ids.length} ids) ==========\n`);
    await runDetailFetch({
      repoRoot: REPO_ROOT,
      cacheDir: path.join(UPSTREAM, def.cacheRel),
      segment: def.segment,
      logStem: def.logStem,
      ids,
      full: args.full,
    });
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
