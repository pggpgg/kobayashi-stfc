import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  atomicWriteJson,
  canonicalJsonSha256,
  fetchJsonResilient,
  loadHttpCacheMeta,
  parseRetryAfterMs,
  runConcurrentPool,
  saveHttpCacheMeta,
  sha256NormalizedText,
} from "../lib/stfcspace_http.mjs";

test("sha256NormalizedText produces identical hashes for LF and CRLF", () => {
  const lf = '{"a":1}\n{"b":2}\n';
  const crlf = '{"a":1}\r\n{"b":2}\r\n';
  assert.equal(sha256NormalizedText(lf), sha256NormalizedText(crlf));
});

test("canonicalJsonSha256 ignores object key order recursively", () => {
  const a = { z: 1, nested: { b: 2, a: 1 }, arr: [{ y: 2, x: 1 }] };
  const b = { arr: [{ x: 1, y: 2 }], nested: { a: 1, b: 2 }, z: 1 };
  assert.equal(canonicalJsonSha256(a), canonicalJsonSha256(b));
});

test("parseRetryAfterMs parses delta seconds and HTTP dates", () => {
  assert.equal(parseRetryAfterMs("3"), 3000);
  assert.equal(parseRetryAfterMs("0"), 0);
  const now = Date.parse("2026-06-01T12:00:00Z");
  assert.equal(parseRetryAfterMs("Mon, 01 Jun 2026 12:00:05 GMT", now), 5000);
  assert.equal(parseRetryAfterMs("not-a-date", now), null);
});

test("fetchJsonResilient retries 429 honoring Retry-After and fails fast on 404", async () => {
  const slept = [];
  let calls = 0;
  const res = await fetchJsonResilient("https://example.test/item.json", {
    maxRetries: 3,
    baseDelayMs: 10,
    randomImpl: () => 0,
    sleepImpl: async (ms) => {
      slept.push(ms);
    },
    fetchImpl: async () => {
      calls++;
      if (calls === 1) {
        return {
          ok: false,
          status: 429,
          statusText: "Too Many Requests",
          headers: new Headers({ "retry-after": "2" }),
        };
      }
      return {
        ok: true,
        status: 200,
        headers: new Headers({ etag: '"v1"' }),
        json: async () => ({ id: 42 }),
      };
    },
  });
  assert.equal(calls, 2);
  assert.deepEqual(slept, [2000]);
  assert.deepEqual(res.data, { id: 42 });
  assert.equal(res.etag, '"v1"');

  // 404 must fail immediately without retries.
  let notFoundCalls = 0;
  await assert.rejects(
    () =>
      fetchJsonResilient("https://example.test/missing.json", {
        maxRetries: 4,
        sleepImpl: async () => {},
        fetchImpl: async () => {
          notFoundCalls++;
          return { ok: false, status: 404, statusText: "Not Found", headers: new Headers() };
        },
      }),
    /HTTP 404/,
  );
  assert.equal(notFoundCalls, 1);
});

test("fetchJsonResilient sends If-None-Match and returns notModified on 304", async () => {
  let seenHeader = null;
  const res = await fetchJsonResilient("https://example.test/ship/1.json", {
    etag: '"abc123"',
    fetchImpl: async (_url, init) => {
      seenHeader = init.headers["If-None-Match"];
      return {
        ok: false,
        status: 304,
        statusText: "Not Modified",
        headers: new Headers({ etag: '"abc123"' }),
      };
    },
  });
  assert.equal(seenHeader, '"abc123"');
  assert.equal(res.status, 304);
  assert.equal(res.notModified, true);
  assert.equal(res.etag, '"abc123"');
});

test("runConcurrentPool caps in-flight concurrency and preserves input order", async () => {
  let inFlight = 0;
  let maxInFlight = 0;
  const items = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
  const results = await runConcurrentPool(items, 3, async (x) => {
    inFlight++;
    maxInFlight = Math.max(maxInFlight, inFlight);
    await new Promise((r) => setTimeout(r, (11 - x) * 2));
    inFlight--;
    return x * 10;
  });
  assert.ok(maxInFlight <= 3, `expected max in-flight <= 3, got ${maxInFlight}`);
  assert.ok(maxInFlight >= 2, `expected concurrent execution, got maxInFlight=${maxInFlight}`);
  assert.deepEqual(
    results,
    items.map((x) => x * 10),
  );
});

test("atomicWriteJson and HTTP cache metadata roundtrip without temp leftovers", async () => {
  const tmp = await fs.mkdtemp(path.join(os.tmpdir(), "stfc-http-test-"));
  try {
    const dest = path.join(tmp, "sub", "data.json");
    await atomicWriteJson(dest, { ok: true, n: 7 });
    const files = await fs.readdir(path.join(tmp, "sub"));
    assert.deepEqual(files, ["data.json"]);
    assert.deepEqual(JSON.parse(await fs.readFile(dest, "utf8")), { ok: true, n: 7 });

    const metaPath = path.join(tmp, "cache-meta.json");
    await saveHttpCacheMeta(metaPath, {
      schema_version: 1,
      entries: {
        "https://b.test": { etag: '"b"', lastModified: null },
        "https://a.test": { etag: '"a"', lastModified: null },
      },
    });
    const loaded = await loadHttpCacheMeta(metaPath);
    assert.deepEqual(Object.keys(loaded.entries), ["https://a.test", "https://b.test"]);
  } finally {
    await fs.rm(tmp, { recursive: true, force: true });
  }
});
