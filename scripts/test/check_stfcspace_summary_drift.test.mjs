import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  anyDrift,
  compareSummaryDirs,
  formatMarkdownReport,
  formatTextReport,
  loadDomainKeys,
} from "../lib/check_stfcspace_summary_drift.mjs";

const SHIP_DOMAIN = {
  name: "ship",
  file: "summary-ship.json",
  key: (row) => String(row.id),
};

function writeJson(dir, name, data) {
  fs.writeFileSync(path.join(dir, name), `${JSON.stringify(data)}\n`, "utf8");
}

test("loadDomainKeys extracts ship ids", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "drift-test-"));
  writeJson(dir, "summary-ship.json", [{ id: 1 }, { id: 2 }]);
  const loaded = loadDomainKeys(dir, SHIP_DOMAIN);
  assert.equal(loaded.count, 2);
  assert.deepEqual([...loaded.keys].sort(), ["1", "2"]);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("compareSummaryDirs detects added and removed hostile keys", () => {
  const before = fs.mkdtempSync(path.join(os.tmpdir(), "drift-before-"));
  const after = fs.mkdtempSync(path.join(os.tmpdir(), "drift-after-"));
  writeJson(before, "summary-ship.json", [{ id: 10 }]);
  writeJson(after, "summary-ship.json", [{ id: 10 }, { id: 11 }]);
  writeJson(before, "summary-hostile.json", [
    { id: 100, level: 1 },
    { id: 200, level: 5 },
  ]);
  writeJson(after, "summary-hostile.json", [
    { id: 100, level: 1 },
    { id: 300, level: 2 },
  ]);
  writeJson(before, "summary-research.json", [{ id: 7 }]);
  writeJson(after, "summary-research.json", [{ id: 7 }]);

  const reports = compareSummaryDirs(before, after);
  assert.equal(anyDrift(reports), true);

  const ship = reports.find((r) => r.name === "ship");
  assert.equal(ship.added.length, 1);
  assert.equal(ship.added[0], "11");
  assert.equal(ship.removed.length, 0);

  const hostile = reports.find((r) => r.name === "hostile");
  assert.deepEqual(hostile.added, ["300:2"]);
  assert.deepEqual(hostile.removed, ["200:5"]);

  const research = reports.find((r) => r.name === "research");
  assert.equal(research.drifted, false);

  const text = formatTextReport(reports);
  assert.match(text, /ship: drift/);
  assert.match(text, /hostile: drift/);
  assert.match(text, /research: no drift/);

  const md = formatMarkdownReport(reports);
  assert.match(md, /\| ship \|/);
  assert.match(md, /300:2/);

  fs.rmSync(before, { recursive: true, force: true });
  fs.rmSync(after, { recursive: true, force: true });
});

test("compareSummaryDirs reports no drift when key sets match", () => {
  const before = fs.mkdtempSync(path.join(os.tmpdir(), "drift-before-"));
  const after = fs.mkdtempSync(path.join(os.tmpdir(), "drift-after-"));
  for (const dir of [before, after]) {
    writeJson(dir, "summary-ship.json", [{ id: 1 }]);
    writeJson(dir, "summary-hostile.json", [{ id: 9, level: 3 }]);
    writeJson(dir, "summary-research.json", [{ id: 42 }]);
  }
  const reports = compareSummaryDirs(before, after);
  assert.equal(anyDrift(reports), false);
  assert.match(formatTextReport(reports), /no drift/);
  fs.rmSync(before, { recursive: true, force: true });
  fs.rmSync(after, { recursive: true, force: true });
});
