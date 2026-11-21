/**
 * Shared resilient HTTP, bounded concurrency, atomic file write, and conditional
 * ETag/Last-Modified cache helpers for data.stfc.space fetchers.
 */

import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";

export const DEFAULT_TIMEOUT_MS = 30_000;
export const DEFAULT_MAX_RETRIES = 3;
export const DEFAULT_RETRY_BASE_MS = 500;
export const DEFAULT_RETRY_MAX_MS = 15_000;
export const DEFAULT_CONCURRENCY = 6;
export const MAX_CONCURRENCY = 16;
export const DEFAULT_PACING_MS = 25;

export function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Normalize line endings to LF before hashing so Windows CRLF and Linux LF
 * checkouts produce identical content digests.
 * @param {string} text
 * @returns {string}
 */
export function sha256NormalizedText(text) {
  return crypto
    .createHash("sha256")
    .update(text.replace(/\r\n/g, "\n"), "utf8")
    .digest("hex");
}

/**
 * Compute deterministic SHA-256 hex digest of a JSON value with recursively
 * sorted object keys.
 * @param {unknown} value
 * @returns {string}
 */
export function canonicalJsonString(value) {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map((item) => canonicalJsonString(item)).join(",")}]`;
  }
  const keys = Object.keys(value).sort();
  const entries = keys
    .filter((k) => value[k] !== undefined)
    .map((k) => `${JSON.stringify(k)}:${canonicalJsonString(value[k])}`);
  return `{${entries.join(",")}}`;
}

/**
 * @param {unknown} value
 * @returns {string}
 */
export function canonicalJsonSha256(value) {
  return crypto
    .createHash("sha256")
    .update(canonicalJsonString(value), "utf8")
    .digest("hex");
}

/**
 * Write UTF-8 text atomically using a sibling temporary file and rename so an
 * interrupted process never leaves truncated or corrupt JSON on disk.
 * @param {string} destPath
 * @param {string} content
 */
export async function atomicWriteFile(destPath, content) {
  const dir = path.dirname(destPath);
  await fs.mkdir(dir, { recursive: true });
  const suffix = `${process.pid}.${crypto.randomBytes(6).toString("hex")}`;
  const tmpPath = `${destPath}.tmp.${suffix}`;
  try {
    await fs.writeFile(tmpPath, content, "utf8");
    await fs.rename(tmpPath, destPath);
  } catch (err) {
    try {
      await fs.unlink(tmpPath);
    } catch {
      // Ignore cleanup failure if temp file was never created or already moved.
    }
    throw err;
  }
}

/**
 * @param {string} destPath
 * @param {unknown} data
 * @param {number} [indent]
 */
export async function atomicWriteJson(destPath, data, indent = 2) {
  const serialized =
    indent > 0 ? `${JSON.stringify(data, null, indent)}\n` : `${JSON.stringify(data)}\n`;
  await atomicWriteFile(destPath, serialized);
}

/**
 * Parse an HTTP `Retry-After` header (delta-seconds or HTTP-date) into milliseconds.
 * Returns `null` when absent or invalid.
 * @param {string | null | undefined} value
 * @param {number} [nowMs]
 * @returns {number | null}
 */
export function parseRetryAfterMs(value, nowMs = Date.now()) {
  if (!value || typeof value !== "string") return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (/^\d+$/.test(trimmed)) {
    const secs = Number.parseInt(trimmed, 10);
    return Number.isFinite(secs) && secs >= 0 ? secs * 1000 : null;
  }
  const parsedDate = Date.parse(trimmed);
  if (!Number.isNaN(parsedDate)) {
    return Math.max(0, parsedDate - nowMs);
  }
  return null;
}

/**
 * True for HTTP status codes where retrying with backoff makes sense.
 * Deterministic 4xx (400, 401, 403, 404, 410) fail fast without wasting retries.
 * @param {number} status
 */
export function isRetryableStatus(status) {
  return status === 408 || status === 425 || status === 429 || (status >= 500 && status <= 599);
}

/**
 * Fetch JSON with timeout, conditional headers (`If-None-Match` / `If-Modified-Since`),
 * `Retry-After` handling, and jittered exponential backoff.
 *
 * @param {string} url
 * @param {object} [opts]
 * @param {string} [opts.userAgent]
 * @param {string | null} [opts.etag]
 * @param {string | null} [opts.lastModified]
 * @param {number} [opts.timeoutMs]
 * @param {number} [opts.maxRetries]
 * @param {number} [opts.baseDelayMs]
 * @param {number} [opts.maxDelayMs]
 * @param {number} [opts.jitterFactor]
 * @param {typeof fetch} [opts.fetchImpl]
 * @param {(ms: number) => Promise<void>} [opts.sleepImpl]
 * @param {() => number} [opts.randomImpl]
 * @param {(info: { attempt: number, maxRetries: number, delayMs: number, error: Error }) => void} [opts.onRetry]
 * @returns {Promise<{ status: number, notModified: boolean, data: unknown, etag: string | null, lastModified: string | null, attempts: number }>}
 */
export async function fetchJsonResilient(url, opts = {}) {
  const {
    userAgent = "Kobayashi-stfcspace-fetch/2.0",
    etag = null,
    lastModified = null,
    timeoutMs = DEFAULT_TIMEOUT_MS,
    maxRetries = DEFAULT_MAX_RETRIES,
    baseDelayMs = DEFAULT_RETRY_BASE_MS,
    maxDelayMs = DEFAULT_RETRY_MAX_MS,
    jitterFactor = 0.2,
    fetchImpl = globalThis.fetch,
    sleepImpl = sleep,
    randomImpl = Math.random,
    onRetry = null,
  } = opts;

  const totalAttempts = Math.max(1, maxRetries);
  const headers = {
    Accept: "application/json",
    "User-Agent": userAgent,
  };
  if (etag) headers["If-None-Match"] = etag;
  if (lastModified) headers["If-Modified-Since"] = lastModified;

  for (let attempt = 1; attempt <= totalAttempts; attempt++) {
    let retryAfterMs = null;
    try {
      const fetchInit = { headers };
      if (timeoutMs > 0 && typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
        fetchInit.signal = AbortSignal.timeout(timeoutMs);
      }
      const res = await fetchImpl(url, fetchInit);
      const status = typeof res.status === "number" ? res.status : res.ok ? 200 : 500;
      const respHeaders = res.headers;
      const respEtag = respHeaders && typeof respHeaders.get === "function" ? respHeaders.get("etag") : null;
      const respLastMod =
        respHeaders && typeof respHeaders.get === "function" ? respHeaders.get("last-modified") : null;

      if (status === 304) {
        return {
          status: 304,
          notModified: true,
          data: undefined,
          etag: respEtag ?? etag,
          lastModified: respLastMod ?? lastModified,
          attempts: attempt,
        };
      }

      if (!res.ok) {
        const raRaw =
          respHeaders && typeof respHeaders.get === "function" ? respHeaders.get("retry-after") : null;
        retryAfterMs = parseRetryAfterMs(raRaw);
        const err = new Error(`HTTP ${status} ${res.statusText ?? ""}`.trim());
        err.status = status;
        err.retryable = isRetryableStatus(status);
        throw err;
      }

      const data = await res.json();
      return {
        status,
        notModified: false,
        data,
        etag: respEtag,
        lastModified: respLastMod,
        attempts: attempt,
      };
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      const retryable = error.retryable !== false;
      if (!retryable || attempt >= totalAttempts) {
        throw error;
      }
      const expDelay = Math.min(maxDelayMs, baseDelayMs * 2 ** (attempt - 1));
      const jitter = expDelay * jitterFactor * randomImpl();
      const delayMs =
        retryAfterMs !== null
          ? Math.min(maxDelayMs * 2, retryAfterMs)
          : Math.round(expDelay + jitter);
      if (onRetry) {
        onRetry({ attempt, maxRetries: totalAttempts, delayMs, error });
      }
      if (delayMs > 0) {
        await sleepImpl(delayMs);
      }
    }
  }
  throw new Error(`Exhausted retries fetching ${url}`);
}

/**
 * Clamp user-specified concurrency into safe bounds.
 * @param {number | null | undefined} value
 * @returns {number}
 */
export function clampConcurrency(value) {
  if (typeof value !== "number" || !Number.isFinite(value)) return DEFAULT_CONCURRENCY;
  return Math.max(1, Math.min(MAX_CONCURRENCY, Math.trunc(value)));
}

/**
 * Process `items` through `worker(item, index)` with bounded concurrency and
 * optional polite launch pacing. Preserves input ordering in the returned array.
 *
 * @template T, R
 * @param {T[]} items
 * @param {number} concurrency
 * @param {(item: T, index: number) => Promise<R>} worker
 * @param {{ pacingMs?: number, sleepImpl?: (ms: number) => Promise<void> }} [opts]
 * @returns {Promise<R[]>}
 */
export async function runConcurrentPool(items, concurrency, worker, opts = {}) {
  const { pacingMs = 0, sleepImpl = sleep } = opts;
  const limit = Math.min(clampConcurrency(concurrency), Math.max(1, items.length));
  const results = new Array(items.length);
  let nextIndex = 0;

  async function runWorker(workerIdx) {
    if (pacingMs > 0 && workerIdx > 0) {
      await sleepImpl(pacingMs * workerIdx);
    }
    while (true) {
      const idx = nextIndex++;
      if (idx >= items.length) return;
      results[idx] = await worker(items[idx], idx);
      if (pacingMs > 0 && nextIndex < items.length) {
        await sleepImpl(pacingMs);
      }
    }
  }

  const workers = [];
  for (let w = 0; w < limit; w++) {
    workers.push(runWorker(w));
  }
  await Promise.all(workers);
  return results;
}

/**
 * Load conditional-HTTP cache metadata (`url -> { etag, lastModified, updatedAt }`).
 * Missing or corrupt files safely return an empty store.
 * @param {string} metaPath
 * @returns {Promise<{ schema_version: number, entries: Record<string, { etag?: string | null, lastModified?: string | null, updatedAt?: string }> }>}
 */
export async function loadHttpCacheMeta(metaPath) {
  try {
    const raw = await fs.readFile(metaPath, "utf8");
    const parsed = JSON.parse(raw);
    if (
      parsed &&
      typeof parsed === "object" &&
      parsed.entries &&
      typeof parsed.entries === "object" &&
      !Array.isArray(parsed.entries)
    ) {
      return { schema_version: 1, entries: parsed.entries };
    }
  } catch {
    // Missing or unreadable cache meta is treated as an empty cache.
  }
  return { schema_version: 1, entries: {} };
}

/**
 * Persist conditional-HTTP cache metadata atomically with sorted keys.
 * @param {string} metaPath
 * @param {{ schema_version: number, entries: Record<string, { etag?: string | null, lastModified?: string | null, updatedAt?: string }> }} store
 */
export async function saveHttpCacheMeta(metaPath, store) {
  const sortedEntries = {};
  for (const key of Object.keys(store.entries ?? {}).sort()) {
    sortedEntries[key] = store.entries[key];
  }
  await atomicWriteJson(metaPath, { schema_version: 1, entries: sortedEntries }, 2);
}
