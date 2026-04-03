/**
 * Shared helpers for caching per-id JSON from https://data.stfc.space/{segment}/{id}.json
 * (ships, hostiles, officers, research, forbidden_tech, …).
 */

import fs from "node:fs/promises";
import path from "node:path";

export const BASE_URL = "https://data.stfc.space";
export const REQUEST_DELAY_MS = 150;
export const MAX_RETRIES = 3;
export const RETRY_DELAY_MS = 1000;
export const JSON_INDENT = 2;

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

/**
 * @param {string[]} argv process.argv.slice(2)
 * @returns {{ help: boolean, full: boolean, limit: number | null, ids: number[] | null }}
 */
export function parseDetailFetchArgs(argv) {
  if (argv.includes("--help") || argv.includes("-h")) {
    return { help: true, full: false, limit: null, ids: null };
  }
  const full = argv.includes("--full") || argv.includes("--force");
  const missingOnly = argv.includes("--missing-only");
  if (full && missingOnly) {
    console.error("Error: use only one of --full (or --force) and --missing-only.");
    process.exit(2);
  }
  const limitIdx = argv.indexOf("--limit");
  const limit = limitIdx !== -1 ? parseInt(argv[limitIdx + 1], 10) : null;
  const idsIdx = argv.indexOf("--ids");
  const ids =
    idsIdx !== -1
      ? argv[idsIdx + 1]
          .split(",")
          .map((s) => parseInt(s.trim(), 10))
          .filter((n) => !Number.isNaN(n))
      : null;
  return { help: false, full, limit: Number.isFinite(limit) ? limit : null, ids };
}

/**
 * @param {string} url
 * @param {number} [retries]
 */
export async function fetchJsonWithRetry(url, retries = MAX_RETRIES) {
  for (let attempt = 1; attempt <= retries; attempt++) {
    try {
      const res = await fetch(url, {
        headers: { Accept: "application/json", "User-Agent": "Kobayashi-stfcspace-fetch/1.0" },
      });
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

/**
 * @param {object} opts
 * @param {string} opts.repoRoot
 * @param {string} opts.cacheDir absolute cache directory for {id}.json
 * @param {string} opts.segment API path segment (e.g. ship, hostile, officer, research, forbidden_tech)
 * @param {string} opts.logStem filename stem: fetch-stfcspace-{logStem}-YYYY-MM-DD.json
 * @param {number[]} opts.ids
 * @param {boolean} opts.full when true, re-fetch even if file exists
 */
export async function runDetailFetch(opts) {
  const { repoRoot, cacheDir, segment, logStem, ids, full } = opts;
  const logDir = path.join(repoRoot, "data", "import_logs");
  await fs.mkdir(cacheDir, { recursive: true });
  await fs.mkdir(logDir, { recursive: true });

  const mode = full ? "full" : "missing-only";
  const log = {
    started: new Date().toISOString(),
    source: BASE_URL,
    segment,
    mode,
    total: ids.length,
    fetched: 0,
    skipped: 0,
    failed: 0,
    failures: [],
  };

  const progressEvery = ids.length > 500 ? 250 : 1;

  console.log(
    `${segment}: ${ids.length} id(s), mode=${mode} → ${path.relative(repoRoot, cacheDir)}\n`,
  );

  for (let i = 0; i < ids.length; i++) {
    const id = ids[i];
    const cachePath = path.join(cacheDir, `${id}.json`);

    if (!full) {
      try {
        await fs.access(cachePath);
        log.skipped++;
        if (progressEvery === 1) {
          process.stdout.write(`[${i + 1}/${ids.length}] ${id} — cached (skip)\n`);
        } else if ((i + 1) % progressEvery === 0 || i === ids.length - 1) {
          process.stdout.write(`[${i + 1}/${ids.length}] … skipped so far ${log.skipped}\n`);
        }
        continue;
      } catch {
        /* fetch */
      }
    }

    const url = `${BASE_URL}/${segment}/${id}.json`;
    try {
      process.stdout.write(`[${i + 1}/${ids.length}] ${id} — fetching…`);
      const detail = await fetchJsonWithRetry(url);
      await fs.writeFile(cachePath, `${JSON.stringify(detail, null, JSON_INDENT)}\n`, "utf8");
      process.stdout.write(` ok\n`);
      log.fetched++;
    } catch (err) {
      process.stdout.write(` FAIL (${err.message})\n`);
      log.failed++;
      log.failures.push({ id, error: err.message });
    }

    if (i < ids.length - 1) await sleep(REQUEST_DELAY_MS);
  }

  log.finished = new Date().toISOString();
  const dateStr = log.started.slice(0, 10);
  const logPath = path.join(logDir, `fetch-stfcspace-${logStem}-${dateStr}.json`);
  await fs.writeFile(logPath, `${JSON.stringify(log, null, JSON_INDENT)}\n`, "utf8");

  console.log(`\nDone. fetched=${log.fetched} skipped=${log.skipped} failed=${log.failed}`);
  console.log(`Log: ${path.relative(repoRoot, logPath)}`);

  if (log.failed > 0) {
    console.warn(`\nFailed IDs:`);
    log.failures.forEach((f) => console.warn(`  ${f.id}: ${f.error}`));
    process.exit(1);
  }
}

/**
 * Load summary JSON array and return numeric ids in file order, with optional limit and filter.
 * @param {unknown} summary
 * @param {{ limit: number | null, idFilter: Set<number> | null }} opt
 * @returns {number[]}
 */
export function idsFromSummaryArray(summary, opt) {
  if (!Array.isArray(summary)) {
    throw new Error("summary: expected JSON array");
  }
  let ids = summary.map((e) => e && e.id).filter((id) => typeof id === "number");
  if (opt.idFilter) {
    ids = ids.filter((id) => opt.idFilter.has(id));
  }
  if (opt.limit != null && opt.limit > 0) {
    ids = ids.slice(0, opt.limit);
  }
  return [...new Set(ids)];
}
