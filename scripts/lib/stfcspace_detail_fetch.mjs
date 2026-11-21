/**
 * Shared helpers for caching per-id JSON from https://data.stfc.space/{segment}/{id}.json
 * (ships, hostiles, officers, research, forbidden_tech, …).
 *
 * v2 pipeline features:
 * - Bounded concurrent fetching (`--concurrency N`, default 6) with polite pacing
 * - Conditional HTTP (`If-None-Match` / `If-Modified-Since`) on `--full` refreshes
 * - Request timeouts, Retry-After support, jittered exponential backoff, fail-fast on 404
 * - Atomic file writes via sibling temp + rename
 * - Selective drift-driven refresh (`--since-drift <drift.json>`)
 */

import fs from "node:fs/promises";
import path from "node:path";
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
} from "./stfcspace_http.mjs";

export const BASE_URL = "https://data.stfc.space";
export const REQUEST_DELAY_MS = 150;
export const MAX_RETRIES = 3;
export const RETRY_DELAY_MS = 1000;
export const JSON_INDENT = 2;

async function readCachedDetailIfUsable(cachePath, id) {
  try {
    const detail = JSON.parse(await fs.readFile(cachePath, "utf8"));
    if (detail === null || typeof detail !== "object" || Array.isArray(detail)) {
      return { ok: false, data: null };
    }
    const idMatch = detail.id === id || (typeof detail.id === "string" && detail.id === String(id));
    return idMatch ? { ok: true, data: detail } : { ok: false, data: null };
  } catch {
    return { ok: false, data: null };
  }
}

/**
 * Parse drift report JSON produced by `check_stfcspace_summary_drift.mjs --json-out`
 * and return numeric IDs that were added or content-modified for `domainName`.
 *
 * @param {string} driftJsonPath
 * @param {string} domainName e.g. "ship", "hostile", "research", "officer", "forbidden_tech"
 * @returns {Promise<number[]>}
 */
export async function loadChangedIdsFromDriftFile(driftJsonPath, domainName) {
  const raw = await fs.readFile(driftJsonPath, "utf8");
  const parsed = JSON.parse(raw);
  const domains = Array.isArray(parsed) ? parsed : parsed?.domains;
  if (!Array.isArray(domains)) {
    throw new Error(`Invalid drift JSON at ${driftJsonPath}: expected domains array`);
  }
  const entry = domains.find((d) => d && d.name === domainName);
  if (!entry) return [];
  if (Array.isArray(entry.changedNumericIds)) {
    return [...new Set(entry.changedNumericIds.filter((n) => typeof n === "number" && Number.isFinite(n)))];
  }
  const combined = [...(entry.added ?? []), ...(entry.modified ?? [])];
  const ids = [];
  for (const key of combined) {
    const first = String(key).split(":")[0];
    const n = Number.parseInt(first, 10);
    if (Number.isFinite(n)) ids.push(n);
  }
  return [...new Set(ids)];
}

/**
 * @param {string[]} argv process.argv.slice(2)
 * @returns {{
 *   help: boolean,
 *   full: boolean,
 *   limit: number | null,
 *   ids: number[] | null,
 *   concurrency: number,
 *   sinceDrift: string | null,
 *   noConditional: boolean,
 * }}
 */
export function parseDetailFetchArgs(argv) {
  if (argv.includes("--help") || argv.includes("-h")) {
    return {
      help: true,
      full: false,
      limit: null,
      ids: null,
      concurrency: DEFAULT_CONCURRENCY,
      sinceDrift: null,
      noConditional: false,
    };
  }
  const full = argv.includes("--full") || argv.includes("--force");
  const missingOnly = argv.includes("--missing-only");
  if (full && missingOnly) {
    console.error("Error: use only one of --full (or --force) and --missing-only.");
    process.exit(2);
  }
  const limitIdx = argv.indexOf("--limit");
  const limit = limitIdx !== -1 ? Number.parseInt(argv[limitIdx + 1], 10) : null;
  const idsIdx = argv.indexOf("--ids");
  const ids =
    idsIdx !== -1 && argv[idsIdx + 1]
      ? argv[idsIdx + 1]
          .split(",")
          .map((s) => Number.parseInt(s.trim(), 10))
          .filter((n) => !Number.isNaN(n))
      : null;
  const concIdx = argv.indexOf("--concurrency");
  const parsedConc = concIdx !== -1 ? Number.parseInt(argv[concIdx + 1], 10) : DEFAULT_CONCURRENCY;
  const concurrency = clampConcurrency(Number.isFinite(parsedConc) ? parsedConc : DEFAULT_CONCURRENCY);
  const driftIdx = argv.indexOf("--since-drift");
  const sinceDrift = driftIdx !== -1 && argv[driftIdx + 1] ? path.resolve(argv[driftIdx + 1]) : null;
  const noConditional = argv.includes("--no-conditional");

  return {
    help: false,
    full,
    limit: Number.isFinite(limit) ? limit : null,
    ids,
    concurrency,
    sinceDrift,
    noConditional,
  };
}

/**
 * Backward-compatible JSON fetch with retries, timeout, and Retry-After support.
 * @param {string} url
 * @param {number} [retries]
 */
export async function fetchJsonWithRetry(url, retries = MAX_RETRIES) {
  const result = await fetchJsonResilient(url, {
    userAgent: "Kobayashi-stfcspace-fetch/2.0",
    maxRetries: retries,
    baseDelayMs: RETRY_DELAY_MS,
    onRetry: ({ attempt, delayMs, error }) => {
      console.warn(`  attempt ${attempt} failed (${error.message}), retrying in ${delayMs}ms…`);
    },
  });
  return result.data;
}

/**
 * @param {object} opts
 * @param {string} opts.repoRoot
 * @param {string} opts.cacheDir absolute cache directory for {id}.json
 * @param {string} opts.segment API path segment (e.g. ship, hostile, officer, research, forbidden_tech)
 * @param {string} opts.logStem filename stem: fetch-stfcspace-{logStem}-YYYY-MM-DD.json
 * @param {number[]} opts.ids
 * @param {boolean} opts.full when true, re-fetch even if file exists
 * @param {number[]} [opts.forceRefreshIds] optional IDs to re-fetch even in missing-only mode (e.g. from drift report)
 * @param {number} [opts.concurrency] bounded worker concurrency (default 6)
 * @param {number} [opts.pacingMs] polite pacing delay between worker requests (default 25ms)
 * @param {boolean} [opts.useConditional] send If-None-Match / If-Modified-Since on refresh (default true)
 * @param {number} [opts.retryBaseMs] override retry backoff base ms (useful in tests)
 */
export async function runDetailFetch(opts) {
  const {
    repoRoot,
    cacheDir,
    segment,
    logStem,
    ids,
    full,
    forceRefreshIds = null,
    concurrency = DEFAULT_CONCURRENCY,
    pacingMs = DEFAULT_PACING_MS,
    useConditional = true,
    retryBaseMs = RETRY_DELAY_MS,
  } = opts;

  const startedMs = Date.now();
  const logDir = path.join(repoRoot, "data", "import_logs");
  await fs.mkdir(cacheDir, { recursive: true });
  await fs.mkdir(logDir, { recursive: true });

  const metaPath = path.join(logDir, "stfcspace-http-cache.json");
  const httpCache = useConditional ? await loadHttpCacheMeta(metaPath) : { schema_version: 1, entries: {} };
  let httpCacheDirty = false;

  const forceSet = forceRefreshIds?.length ? new Set(forceRefreshIds) : null;
  const effectiveConcurrency = clampConcurrency(concurrency);
  const mode = full ? "full" : forceSet ? "missing-and-drifted" : "missing-only";

  const log = {
    started: new Date(startedMs).toISOString(),
    source: BASE_URL,
    segment,
    mode,
    concurrency: effectiveConcurrency,
    total: ids.length,
    fetched: 0,
    not_modified: 0,
    skipped: 0,
    failed: 0,
    stale_cache_used: 0,
    uncovered_failures: 0,
    failures: [],
  };

  const progressEvery = ids.length > 500 ? 250 : ids.length > 100 ? 50 : 1;
  let completed = 0;

  console.log(
    `${segment}: ${ids.length} id(s), mode=${mode}, concurrency=${effectiveConcurrency} → ${path.relative(repoRoot, cacheDir)}\n`,
  );

  await runConcurrentPool(
    ids,
    effectiveConcurrency,
    async (id) => {
      const cachePath = path.join(cacheDir, `${id}.json`);
      const mustRefresh = full || (forceSet !== null && forceSet.has(id));
      const cached = await readCachedDetailIfUsable(cachePath, id);
      const hasUsableCache = cached.ok;

      if (!mustRefresh && hasUsableCache) {
        log.skipped++;
        completed++;
        if (progressEvery === 1) {
          process.stdout.write(`[${completed}/${ids.length}] ${id} — cached (skip)\n`);
        } else if (completed % progressEvery === 0 || completed === ids.length) {
          process.stdout.write(
            `[${completed}/${ids.length}] … fetched=${log.fetched} not_modified=${log.not_modified} skipped=${log.skipped} failed=${log.failed}\n`,
          );
        }
        return;
      }

      const url = `${BASE_URL}/${segment}/${id}.json`;
      const cachedMeta = useConditional && hasUsableCache ? httpCache.entries[url] : null;
      try {
        const res = await fetchJsonResilient(url, {
          userAgent: "Kobayashi-stfcspace-fetch/2.0",
          etag: cachedMeta?.etag ?? null,
          lastModified: cachedMeta?.lastModified ?? null,
          maxRetries: MAX_RETRIES,
          baseDelayMs: retryBaseMs,
          onRetry: ({ attempt, delayMs, error }) => {
            console.warn(
              `  [${segment}/${id}] attempt ${attempt} failed (${error.message}), retrying in ${delayMs}ms…`,
            );
          },
        });

        if (res.notModified && hasUsableCache) {
          log.not_modified++;
          completed++;
          if (progressEvery === 1 || completed % progressEvery === 0 || completed === ids.length) {
            process.stdout.write(`[${completed}/${ids.length}] ${id} — 304 not modified\n`);
          }
          return;
        }

        if (useConditional && (res.etag || res.lastModified)) {
          const prev = httpCache.entries[url];
          const nextEtag = res.etag ?? null;
          const nextLastMod = res.lastModified ?? null;
          if (!prev || prev.etag !== nextEtag || prev.lastModified !== nextLastMod) {
            httpCache.entries[url] = {
              etag: nextEtag,
              lastModified: nextLastMod,
            };
            httpCacheDirty = true;
          }
        }

        if (hasUsableCache && canonicalJsonSha256(cached.data) === canonicalJsonSha256(res.data)) {
          log.not_modified++;
          completed++;
          if (progressEvery === 1 || completed % progressEvery === 0 || completed === ids.length) {
            process.stdout.write(`[${completed}/${ids.length}] ${id} — unchanged\n`);
          }
          return;
        }

        await atomicWriteJson(cachePath, res.data, JSON_INDENT);
        log.fetched++;
        completed++;
        if (progressEvery === 1) {
          process.stdout.write(`[${completed}/${ids.length}] ${id} — ok\n`);
        } else if (completed % progressEvery === 0 || completed === ids.length) {
          process.stdout.write(
            `[${completed}/${ids.length}] … fetched=${log.fetched} not_modified=${log.not_modified} skipped=${log.skipped} failed=${log.failed}\n`,
          );
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        const fallback = hasUsableCache || (await readCachedDetailIfUsable(cachePath, id)).ok;
        log.failed++;
        if (fallback) {
          log.stale_cache_used++;
        } else {
          log.uncovered_failures++;
        }
        log.failures.push({ id, error: message, stale_cache_used: fallback });
        completed++;
        process.stdout.write(
          `[${completed}/${ids.length}] ${id} — FAIL (${message})${fallback ? " — using validated stale cache" : ""}\n`,
        );
      }
    },
    { pacingMs },
  );

  if (httpCacheDirty) {
    await saveHttpCacheMeta(metaPath, httpCache);
  }

  log.finished = new Date().toISOString();
  log.duration_ms = Date.now() - startedMs;
  const dateStr = log.started.slice(0, 10);
  const logPath = path.join(logDir, `fetch-stfcspace-${logStem}-${dateStr}.json`);
  await atomicWriteJson(logPath, log, JSON_INDENT);

  console.log(
    `\nDone in ${log.duration_ms}ms. fetched=${log.fetched} not_modified=${log.not_modified} skipped=${log.skipped} failed=${log.failed} stale_cache_used=${log.stale_cache_used} uncovered_failures=${log.uncovered_failures}`,
  );
  console.log(`Log: ${path.relative(repoRoot, logPath)}`);

  if (log.failed > 0) {
    console.warn(`\nFailed IDs:`);
    log.failures.forEach((f) =>
      console.warn(
        `  ${f.id}: ${f.error}${f.stale_cache_used ? " (validated stale cache)" : " (no usable cache)"}`,
      ),
    );
    if (log.uncovered_failures > 0) process.exit(1);
  }
  return log;
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
