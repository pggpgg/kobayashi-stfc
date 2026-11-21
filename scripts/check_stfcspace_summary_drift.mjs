#!/usr/bin/env node
/**
 * Compare data.stfc.space summary catalogs for upstream drift (count, keys, and content hashes).
 *
 * Usage (repo root):
 *   node scripts/check_stfcspace_summary_drift.mjs --check
 *   node scripts/check_stfcspace_summary_drift.mjs --all-domains --compare-dir /tmp/before \
 *     --json-out /tmp/drift.json --markdown-out /tmp/report.md
 *
 * Modes & options:
 *   --check (default)      Fetch live summaries and compare to committed data/upstream/data-stfc-space/
 *   --compare-dir DIR      Compare DIR (before) to committed upstream dir (after)
 *   --domains a,b,c        Select specific domains (ship,hostile,research,officer,building,forbidden_tech)
 *   --all-domains          Check all 6 combat-relevant summary domains
 *   --include-modified     Treat in-place row payload modifications as exit-code-1 drift
 *   --json-out PATH        Write machine-readable JSON drift report (used by --since-drift)
 *   --markdown-out PATH    Write Markdown drift report
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
  resolveDomains,
} from "./lib/check_stfcspace_summary_drift.mjs";

const ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const COMMITTED_DIR = path.join(ROOT, "data", "upstream", "data-stfc-space");

function parseArgs(argv) {
  const compareIdx = argv.indexOf("--compare-dir");
  const markdownIdx = argv.indexOf("--markdown-out");
  const jsonIdx = argv.indexOf("--json-out");
  const domainsIdx = argv.indexOf("--domains");
  const domainNames =
    domainsIdx !== -1 && argv[domainsIdx + 1]
      ? argv[domainsIdx + 1]
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean)
      : null;
  return {
    check: !argv.includes("--compare-dir") || argv.includes("--check"),
    allDomains: argv.includes("--all-domains"),
    includeModified: argv.includes("--include-modified"),
    domainNames,
    compareDir:
      compareIdx !== -1 && argv[compareIdx + 1] ? path.resolve(argv[compareIdx + 1]) : null,
    markdownOut:
      markdownIdx !== -1 && argv[markdownIdx + 1] ? path.resolve(argv[markdownIdx + 1]) : null,
    jsonOut: jsonIdx !== -1 && argv[jsonIdx + 1] ? path.resolve(argv[jsonIdx + 1]) : null,
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const domains = resolveDomains({
    allDomains: args.allDomains,
    domainNames: args.domainNames,
  });

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
    await fetchLiveSummaries(tmp, { domains });
    afterDir = tmp;
    beforeLabel = "committed";
    afterLabel = "live";
  }

  const reports = compareSummaryDirs(beforeDir, afterDir, {
    domains,
    includeModifiedInDrifted: args.includeModified,
  });
  const text = formatTextReport(reports, { beforeLabel, afterLabel });
  console.log(text);

  if (args.jsonOut) {
    const payload = {
      schema_version: 1,
      beforeLabel,
      afterLabel,
      anyDrift: anyDrift(reports),
      domains: reports,
    };
    fs.mkdirSync(path.dirname(args.jsonOut), { recursive: true });
    fs.writeFileSync(args.jsonOut, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
  }

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
