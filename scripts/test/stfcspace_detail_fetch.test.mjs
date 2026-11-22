import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const helperUrl = pathToFileURL(path.join(repoRoot, "scripts/lib/stfcspace_detail_fetch.mjs"));

test("a full refresh uses a valid cached detail when one upstream id fails and continues", async () => {
  const scratchRoot = path.join(repoRoot, "target/test-tmp");
  await fs.mkdir(scratchRoot, { recursive: true });
  const fixture = await fs.mkdtemp(path.join(scratchRoot, "detail-fetch-test-"));
  try {
    const cacheDir = path.join(fixture, "cache");
    await fs.mkdir(cacheDir);
    await fs.writeFile(path.join(cacheDir, "101.json"), '{"id":101,"cached":true}\n');
    const script = `
      import { runDetailFetch } from ${JSON.stringify(helperUrl.href)};
      globalThis.fetch = async (url) => {
        if (url.endsWith('/101.json')) throw new Error('upstream unavailable');
        return { ok: true, json: async () => ({ id: 102, fresh: true }) };
      };
      await runDetailFetch({ repoRoot: ${JSON.stringify(fixture)}, cacheDir: ${JSON.stringify(cacheDir)}, segment: 'hostile', logStem: 'hostiles', ids: [101, 102], full: true });
      console.log('downstream-ran');
    `;
    const run = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      cwd: repoRoot,
      encoding: "utf8",
      timeout: 20_000,
    });
    assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
    assert.match(run.stdout, /downstream-ran/);
    assert.deepEqual(JSON.parse(await fs.readFile(path.join(cacheDir, "101.json"), "utf8")), {
      id: 101,
      cached: true,
    });
    assert.deepEqual(JSON.parse(await fs.readFile(path.join(cacheDir, "102.json"), "utf8")), {
      id: 102,
      fresh: true,
    });
    const [logName] = await fs.readdir(path.join(fixture, "data/import_logs"));
    const log = JSON.parse(await fs.readFile(path.join(fixture, "data/import_logs", logName)));
    assert.equal(log.failed, 1);
    assert.equal(log.stale_cache_used, 1);
  } finally {
    assert.ok(fixture.startsWith(`${scratchRoot}${path.sep}`));
    await fs.rm(fixture, { recursive: true, force: true });
  }
});

test("a failed fetch with corrupt cache still blocks downstream processing", async () => {
  const scratchRoot = path.join(repoRoot, "target/test-tmp");
  await fs.mkdir(scratchRoot, { recursive: true });
  const fixture = await fs.mkdtemp(path.join(scratchRoot, "detail-fetch-test-"));
  try {
    const cacheDir = path.join(fixture, "cache");
    await fs.mkdir(cacheDir);
    await fs.writeFile(path.join(cacheDir, "101.json"), "{broken json");
    const script = `
      import { runDetailFetch } from ${JSON.stringify(helperUrl.href)};
      globalThis.fetch = async () => { throw new Error('upstream unavailable'); };
      await runDetailFetch({ repoRoot: ${JSON.stringify(fixture)}, cacheDir: ${JSON.stringify(cacheDir)}, segment: 'hostile', logStem: 'hostiles', ids: [101], full: true });
      console.log('downstream-ran');
    `;
    const run = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      cwd: repoRoot,
      encoding: "utf8",
      timeout: 20_000,
    });
    assert.equal(run.status, 1, `${run.stdout}\n${run.stderr}`);
    assert.doesNotMatch(run.stdout, /downstream-ran/);
    const [logName] = await fs.readdir(path.join(fixture, "data/import_logs"));
    const log = JSON.parse(await fs.readFile(path.join(fixture, "data/import_logs", logName)));
    assert.equal(log.stale_cache_used, 0);
    assert.equal(log.uncovered_failures, 1);
  } finally {
    assert.ok(fixture.startsWith(`${scratchRoot}${path.sep}`));
    await fs.rm(fixture, { recursive: true, force: true });
  }
});

test("drift-targeted refresh re-fetches modified ids and honors 304 Not Modified on ETag match", async () => {
  const scratchRoot = path.join(repoRoot, "target/test-tmp");
  await fs.mkdir(scratchRoot, { recursive: true });
  const fixture = await fs.mkdtemp(path.join(scratchRoot, "detail-fetch-v2-"));
  try {
    const cacheDir = path.join(fixture, "cache");
    await fs.mkdir(cacheDir);
    await fs.writeFile(path.join(cacheDir, "201.json"), '{"id":201,"ver":1}\n');
    await fs.writeFile(path.join(cacheDir, "202.json"), '{"id":202,"ver":1}\n');
    const driftPath = path.join(fixture, "drift.json");
    await fs.writeFile(
      driftPath,
      JSON.stringify({
        domains: [{ name: "ship", added: ["203"], modified: ["202"], changedNumericIds: [202, 203] }],
      }),
    );

    const script = `
      import { loadChangedIdsFromDriftFile, runDetailFetch } from ${JSON.stringify(helperUrl.href)};
      const changed = await loadChangedIdsFromDriftFile(${JSON.stringify(driftPath)}, "ship");
      const fetchedUrls = [];
      globalThis.fetch = async (url, init) => {
        fetchedUrls.push({ url, ifNoneMatch: init?.headers?.["If-None-Match"] ?? null });
        if (url.endsWith("/202.json") && init?.headers?.["If-None-Match"] === '"etag-202"') {
          return { ok: false, status: 304, statusText: "Not Modified", headers: new Headers({ etag: '"etag-202"' }) };
        }
        if (url.endsWith("/202.json")) {
          return { ok: true, status: 200, headers: new Headers({ etag: '"etag-202"' }), json: async () => ({ id: 202, ver: 2 }) };
        }
        return { ok: true, status: 200, headers: new Headers({ etag: '"etag-203"' }), json: async () => ({ id: 203, ver: 1 }) };
      };
      const first = await runDetailFetch({
        repoRoot: ${JSON.stringify(fixture)},
        cacheDir: ${JSON.stringify(cacheDir)},
        segment: "ship",
        logStem: "ships",
        ids: [201, 202, 203],
        full: false,
        forceRefreshIds: changed,
        concurrency: 3,
        pacingMs: 0,
        retryBaseMs: 1,
      });
      const second = await runDetailFetch({
        repoRoot: ${JSON.stringify(fixture)},
        cacheDir: ${JSON.stringify(cacheDir)},
        segment: "ship",
        logStem: "ships",
        ids: [202],
        full: true,
        concurrency: 2,
        pacingMs: 0,
        retryBaseMs: 1,
      });
      console.log(JSON.stringify({ first, second, fetchedUrls }));
    `;
    const run = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      cwd: repoRoot,
      encoding: "utf8",
      timeout: 20_000,
    });
    assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
    const lastLine = run.stdout.trim().split("\n").pop();
    const out = JSON.parse(lastLine);
    assert.equal(out.first.skipped, 1, "201 should be skipped");
    assert.equal(out.first.fetched, 2, "202 (modified) and 203 (missing) should be fetched");
    assert.equal(out.second.not_modified, 1, "second full fetch of 202 should hit 304 via ETag");
    assert.equal(out.second.fetched, 0);
    assert.deepEqual(JSON.parse(await fs.readFile(path.join(cacheDir, "202.json"), "utf8")), {
      id: 202,
      ver: 2,
    });
  } finally {
    assert.ok(fixture.startsWith(`${scratchRoot}${path.sep}`));
    await fs.rm(fixture, { recursive: true, force: true });
  }
});

test("full refresh preserves original file bytes when fetched JSON is semantically identical", async () => {
  const scratchRoot = path.join(repoRoot, "target/test-tmp");
  await fs.mkdir(scratchRoot, { recursive: true });
  const fixture = await fs.mkdtemp(path.join(scratchRoot, "detail-fetch-noop-"));
  try {
    const cacheDir = path.join(fixture, "cache");
    await fs.mkdir(cacheDir);
    // Original file uses Python-style \\u00e9 escaping and reversed key order.
    const originalBytes = '{"name":"Caf\\u00e9","id":301}\n';
    await fs.writeFile(path.join(cacheDir, "301.json"), originalBytes, "utf8");
    const script = `
      import { runDetailFetch } from ${JSON.stringify(helperUrl.href)};
      globalThis.fetch = async () => ({
        ok: true,
        status: 200,
        headers: new Headers({ etag: '"e-301"' }),
        json: async () => ({ id: 301, name: "Café" }),
      });
      const log = await runDetailFetch({
        repoRoot: ${JSON.stringify(fixture)},
        cacheDir: ${JSON.stringify(cacheDir)},
        segment: "ship",
        logStem: "ships",
        ids: [301],
        full: true,
        concurrency: 2,
        pacingMs: 0,
        retryBaseMs: 1,
      });
      console.log(JSON.stringify(log));
    `;
    const run = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      cwd: repoRoot,
      encoding: "utf8",
      timeout: 20_000,
    });
    assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
    const out = JSON.parse(run.stdout.trim().split("\n").pop());
    assert.equal(out.not_modified, 1);
    assert.equal(out.fetched, 0);
    assert.equal(await fs.readFile(path.join(cacheDir, "301.json"), "utf8"), originalBytes);
  } finally {
    assert.ok(fixture.startsWith(`${scratchRoot}${path.sep}`));
    await fs.rm(fixture, { recursive: true, force: true });
  }
});
