import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  computePipelineManifest,
  diffPipelineManifests,
  formatManifestDiffMarkdown,
  verifyPipelineManifest,
  writePipelineManifest,
} from "../lib/pipeline_manifest.mjs";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function writeRel(root, rel, content) {
  const dest = path.join(root, rel);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.writeFileSync(dest, content, "utf8");
}

test("computePipelineManifest is deterministic and LF/CRLF invariant", async () => {
  const rootA = fs.mkdtempSync(path.join(os.tmpdir(), "manifest-a-"));
  const rootB = fs.mkdtempSync(path.join(os.tmpdir(), "manifest-b-"));
  try {
    writeRel(rootA, "data/upstream/data-stfc-space/summary-ship.json", '[{"id":1}]\n');
    writeRel(rootB, "data/upstream/data-stfc-space/summary-ship.json", '[{"id":1}]\r\n');
    writeRel(rootA, "data/ships_extended/index.json", '{"data_version":"2026-11-21","ships":[]}\n');
    writeRel(rootB, "data/ships_extended/index.json", '{"data_version":"2026-11-21","ships":[]}\r\n');

    const mA = computePipelineManifest(rootA);
    const mB = computePipelineManifest(rootB);
    assert.equal(mA.combined_sha256, mB.combined_sha256);
    assert.deepEqual(mA, mB);
    assert.equal(mA.normalized.ships_extended.data_version, "2026-11-21");

    await writePipelineManifest(rootA);
    const okCheck = verifyPipelineManifest(rootA);
    assert.equal(okCheck.ok, true);
    assert.equal(okCheck.diffs.length, 0);

    // Modify normalized file -> verifyPipelineManifest must catch drift.
    writeRel(rootA, "data/ships_extended/uss_test.json", '{"id":"uss_test"}\n');
    const driftCheck = verifyPipelineManifest(rootA);
    assert.equal(driftCheck.ok, false);
    assert.equal(driftCheck.diffs.length, 1);
    assert.equal(driftCheck.diffs[0].key, "ships_extended");
    const md = formatManifestDiffMarkdown(driftCheck.diffs);
    assert.match(md, /ships_extended/);
    assert.match(md, /1 → 2/);
  } finally {
    fs.rmSync(rootA, { recursive: true, force: true });
    fs.rmSync(rootB, { recursive: true, force: true });
  }
});

test("diffPipelineManifests reports missing manifest cleanly", () => {
  const actual = computePipelineManifest(REPO_ROOT);
  const diffs = diffPipelineManifests(null, actual);
  assert.equal(diffs.length, 1);
  assert.equal(diffs[0].kind, "missing_committed_manifest");
});

test("committed data/pipeline_manifest.json matches on-disk catalogs", () => {
  const check = verifyPipelineManifest(REPO_ROOT);
  assert.equal(
    check.ok,
    true,
    `Committed data/pipeline_manifest.json drifted:\n${formatManifestDiffMarkdown(check.diffs)}`,
  );
});
