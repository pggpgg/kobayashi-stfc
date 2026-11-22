#!/usr/bin/env node
/**
 * Refresh data.stfc.space catalog JSON caches (summaries + en static translations).
 *
 * Derived from stfc.space production bundle (VITE_API_URL = https://data.stfc.space):
 * - yukiData.fetchData: GET /{entity}/summary.json for catalogs
 * - yukiTranslations.fetchStaticTranslations: GET /translations/{lang}/{category}.json
 * - ItemLoadingWrapper (ship): GET /ship/{id}.json (optional sample ship)
 *
 * Usage (repo root):
 *   node scripts/fetch_stfcspace_page_upstream.mjs
 *   node scripts/fetch_stfcspace_page_upstream.mjs --concurrency 6
 *   node scripts/fetch_stfcspace_page_upstream.mjs --ship-id 2251018025
 *   node scripts/fetch_stfcspace_page_upstream.mjs --no-sample-ship
 *   node scripts/fetch_stfcspace_page_upstream.mjs --summaries-only
 *   node scripts/fetch_stfcspace_page_upstream.mjs --json-out /tmp/catalog_fetch.json
 *
 * Output: data/upstream/data-stfc-space/ (summary-*.json, translations-*.json, ships/{id}.json)
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";
import {
  atomicWriteJson,
  canonicalJsonSha256,
  clampConcurrency,
  DEFAULT_CONCURRENCY,
  DEFAULT_PACING_MS,
  fetchJsonResilient,
  loadHttpCacheMeta,
  runConcurrentPool,
  saveHttpCacheMeta,
} from "./lib/stfcspace_http.mjs";

const ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const OUT = path.join(ROOT, "data", "upstream", "data-stfc-space");
const BASE = "https://data.stfc.space";

/** App `fetchData` summary paths (minified client passes "/ship", etc.; URL is /ship/summary.json). */
export const SUMMARY_SEGMENTS = [
  ["ship", "summary-ship.json"],
  ["officer", "summary-officer.json"],
  ["building", "summary-building.json"],
  ["research", "summary-research.json"],
  ["system", "summary-system.json"],
  ["hostile", "summary-hostile.json"],
  ["consumable", "summary-consumable.json"],
  ["forbidden_tech", "summary-forbidden_tech.json"],
  ["hazards", "summary-hazards.json"],
  ["wave_defense", "summary-wave_defense.json"],
  ["pvp_bands", "summary-pvp_bands.json"],
  ["mission", "summary-mission.json"],
  ["resource", "summary-ressource.json"],
];

/** `Ht` static translation paths (en); duplicates removed. */
export const TRANSLATION_PATHS = [
  "materials",
  "ships",
  "officers",
  "officer_names",
  "officer_buffs",
  "officer_flavor_text",
  "traits",
  "research",
  "starbase_modules",
  "factions",
  "systems",
  "ship_components",
  "blueprints",
  "consumables",
  "mission_titles",
  "navigation",
  "ship_buffs",
  "loyalty",
  "forbidden_tech",
  "event_titles",
  "player_avatars",
  "hud",
];

function parseArgs(argv) {
  const summariesOnly = argv.includes("--summaries-only");
  const noSampleShip = argv.includes("--no-sample-ship");
  const noConditional = argv.includes("--no-conditional");
  const shipIdx = argv.indexOf("--ship-id");
  const shipId =
    shipIdx !== -1 && argv[shipIdx + 1] ? String(argv[shipIdx + 1]).trim() : "2251018025";
  const concIdx = argv.indexOf("--concurrency");
  const parsedConc = concIdx !== -1 ? Number.parseInt(argv[concIdx + 1], 10) : DEFAULT_CONCURRENCY;
  const concurrency = clampConcurrency(Number.isFinite(parsedConc) ? parsedConc : DEFAULT_CONCURRENCY);
  const jsonOutIdx = argv.indexOf("--json-out");
  const jsonOut =
    jsonOutIdx !== -1 && argv[jsonOutIdx + 1] ? path.resolve(argv[jsonOutIdx + 1]) : null;
  return {
    help: argv.includes("--help") || argv.includes("-h"),
    summariesOnly,
    noSampleShip,
    noConditional,
    shipId,
    concurrency,
    jsonOut,
  };
}

function printHelp() {
  console.log(`Usage: node scripts/fetch_stfcspace_page_upstream.mjs [options]

Fetch data.stfc.space summaries + English static translations concurrently with
retries, timeouts, conditional ETag support, and atomic writes.

Options:
  --summaries-only   Fetch summary-*.json files only (skip translations & sample ship)
  --no-sample-ship   Skip fetching sample ships/{shipId}.json
  --ship-id ID       Sample ship id when sample ship fetch is enabled (default: 2251018025)
  --concurrency N    Bounded parallel workers (1..16, default: ${DEFAULT_CONCURRENCY})
  --no-conditional   Bypass ETag / If-None-Match cache headers
  --json-out PATH    Write structured fetch summary JSON
`);
}

async function readLocalJsonIfUsable(dest) {
  try {
    const raw = await fs.readFile(dest, "utf8");
    return { ok: true, data: JSON.parse(raw) };
  } catch {
    return { ok: false, data: null };
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    process.exit(0);
  }

  const startedMs = Date.now();
  const jobs = [];
  for (const [seg, filename] of SUMMARY_SEGMENTS) {
    jobs.push({
      url: `${BASE}/${seg}/summary.json`,
      relPath: filename,
      kind: "summary",
    });
  }

  if (!args.summariesOnly) {
    const lang = "en";
    for (const p of TRANSLATION_PATHS) {
      jobs.push({
        url: `${BASE}/translations/${lang}/${p}.json`,
        relPath: `translations-${p}.json`,
        kind: "translation",
      });
    }
    if (!args.noSampleShip) {
      jobs.push({
        url: `${BASE}/ship/${args.shipId}.json`,
        relPath: path.join("ships", `${args.shipId}.json`),
        kind: "sample_ship",
      });
    }
  }

  const logDir = path.join(ROOT, "data", "import_logs");
  await fs.mkdir(logDir, { recursive: true });
  const metaPath = path.join(logDir, "stfcspace-http-cache.json");
  const httpCache = args.noConditional
    ? { schema_version: 1, entries: {} }
    : await loadHttpCacheMeta(metaPath);
  let httpCacheDirty = false;

  const summary = {
    started: new Date(startedMs).toISOString(),
    source: BASE,
    concurrency: args.concurrency,
    total: jobs.length,
    fetched: 0,
    not_modified: 0,
    failed: 0,
    failures: [],
  };

  console.log(
    `Fetching ${jobs.length} catalog resources from ${BASE} (concurrency=${args.concurrency}) …`,
  );

  let completed = 0;
  await runConcurrentPool(
    jobs,
    args.concurrency,
    async (job) => {
      const dest = path.join(OUT, job.relPath);
      const local = await readLocalJsonIfUsable(dest);
      const hasLocal = local.ok;
      const cachedMeta = !args.noConditional && hasLocal ? httpCache.entries[job.url] : null;
      try {
        const res = await fetchJsonResilient(job.url, {
          userAgent: "Kobayashi-upstream-fetch/2.0",
          etag: cachedMeta?.etag ?? null,
          lastModified: cachedMeta?.lastModified ?? null,
          onRetry: ({ attempt, delayMs, error }) => {
            console.warn(
              `  [${job.relPath}] attempt ${attempt} failed (${error.message}), retrying in ${delayMs}ms…`,
            );
          },
        });
        if (res.notModified && hasLocal) {
          summary.not_modified++;
          completed++;
          process.stdout.write(`[${completed}/${jobs.length}] ${job.relPath} (304 not modified)\n`);
          return;
        }
        if (!args.noConditional && (res.etag || res.lastModified)) {
          const prev = httpCache.entries[job.url];
          const nextEtag = res.etag ?? null;
          const nextLastMod = res.lastModified ?? null;
          if (!prev || prev.etag !== nextEtag || prev.lastModified !== nextLastMod) {
            httpCache.entries[job.url] = {
              etag: nextEtag,
              lastModified: nextLastMod,
            };
            httpCacheDirty = true;
          }
        }
        // Preserve existing file bytes when upstream JSON is semantically identical
        // (avoids spurious ASCII-escape vs UTF-8 churn between Python and Node serializers).
        if (hasLocal && canonicalJsonSha256(local.data) === canonicalJsonSha256(res.data)) {
          summary.not_modified++;
          completed++;
          process.stdout.write(`[${completed}/${jobs.length}] ${job.relPath} (unchanged)\n`);
          return;
        }
        // Summaries & translations stay compact (indent=0) matching existing upstream convention.
        const indent = job.kind === "sample_ship" ? 2 : 0;
        await atomicWriteJson(dest, res.data, indent);
        summary.fetched++;
        completed++;
        process.stdout.write(`[${completed}/${jobs.length}] ${job.relPath}\n`);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        summary.failed++;
        summary.failures.push({ url: job.url, relPath: job.relPath, error: message });
        completed++;
        console.error(`[${completed}/${jobs.length}] ${job.relPath} FAILED: ${message}`);
      }
    },
    { pacingMs: DEFAULT_PACING_MS },
  );

  if (httpCacheDirty) {
    await saveHttpCacheMeta(metaPath, httpCache);
  }

  summary.finished = new Date().toISOString();
  summary.duration_ms = Date.now() - startedMs;

  if (args.jsonOut) {
    await atomicWriteJson(args.jsonOut, summary, 2);
  }

  console.log(
    `Done in ${summary.duration_ms}ms. fetched=${summary.fetched} not_modified=${summary.not_modified} failed=${summary.failed} → ${path.relative(ROOT, OUT)}`,
  );

  if (summary.failed > 0) {
    process.exit(1);
  }
}

const isMain =
  process.argv[1] && path.resolve(process.argv[1]) === path.resolve(url.fileURLToPath(import.meta.url));
if (isMain) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}
