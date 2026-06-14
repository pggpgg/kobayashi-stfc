#!/usr/bin/env node
/**
 * Compare data.stfc.space summary catalogs (ship, hostile, research) for upstream drift.
 *
 * Usage (repo root):
 *   node scripts/check_stfcspace_summary_drift.mjs --check
 *   node scripts/check_stfcspace_summary_drift.mjs --compare-dir /tmp/before --markdown-out /tmp/report.md
 *
 * Modes:
 *   --check (default)  Fetch live summaries and compare to committed data/upstream/data-stfc-space/
 *   --compare-dir DIR  Compare DIR (before) to committed upstream dir (after)
 *   --markdown-out P   Write Markdown report to path
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  anyDrift,
  compareSummaryDirs,
  DRIFT_REMEDIATION,
  fetchLiveSummaries,
  formatMarkdownReport,
  formatTextReport,
} from "./lib/check_stfcspace_summary_drift.mjs";

const ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const COMMITTED_DIR = path.join(ROOT, "data", "upstream", "data-stfc-space");

function parseArgs(argv) {
  const compareIdx = argv.indexOf("--compare-dir");
  const markdownIdx = argv.indexOf("--markdown-out");
  return {
    check: !argv.includes("--compare-dir") || argv.includes("--check"),
    compareDir: compareIdx !== -1 && argv[compareIdx + 1] ? path.resolve(argv[compareIdx + 1]) : null,
    markdownOut: markdownIdx !== -1 && argv[markdownIdx + 1] ? path.resolve(argv[markdownIdx + 1]) : null,
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  let beforeDir;
  let afterDir;
  let beforeLabel;
  let afterLabel;
  let cleanup = null;

  if (args.compareDir) {
    beforeDir = args.compareDir;
    afterDir = COMMITTED_DIR;
    beforeLabel = "before";
    afterLabel = "after";
  } else {
    beforeDir = COMMITTED_DIR;
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "kobayashi-upstream-live-"));
    cleanup = () => fs.rmSync(tmp, { recursive: true, force: true });
    await fetchLiveSummaries(tmp);
    afterDir = tmp;
    beforeLabel = "committed";
    afterLabel = "live";
  }

  const reports = compareSummaryDirs(beforeDir, afterDir);
  const text = formatTextReport(reports, { beforeLabel, afterLabel });
  console.log(text);

  if (args.markdownOut) {
    const md = formatMarkdownReport(reports, {
      beforeLabel,
      afterLabel,
      title: args.compareDir ? "Summary catalog changes (fetch step)" : "Upstream summary drift",
    });
    fs.mkdirSync(path.dirname(args.markdownOut), { recursive: true });
    fs.writeFileSync(args.markdownOut, `${md}\n`, "utf8");
  }

  if (cleanup) cleanup();

  if (anyDrift(reports)) {
    console.error(`\n${DRIFT_REMEDIATION}\n`);
    process.exit(1);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
