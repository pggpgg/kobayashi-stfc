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
