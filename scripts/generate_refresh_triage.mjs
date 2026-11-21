#!/usr/bin/env node
/**
 * Assemble structured Pull Request body, triage metadata, and GitHub Actions outputs
 * from an automated data-refresh run.
 *
 * Usage:
 *   node scripts/generate_refresh_triage.mjs \
 *     --drift-json /tmp/drift.json \
 *     --pipeline-json /tmp/pipeline_report.json \
 *     --validate-json /tmp/validate_data.json \
 *     --before-manifest /tmp/before_manifest.json \
 *     --gates-json /tmp/coverage_gates.json \
 *     --pr-body-out /tmp/pr_body.md \
 *     --meta-out /tmp/triage_meta.json
 */

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import { computePipelineManifest, diffPipelineManifests } from "./lib/pipeline_manifest.mjs";
import { buildRefreshPrBody } from "./lib/refresh_triage.mjs";
import { atomicWriteFile, atomicWriteJson } from "./lib/stfcspace_http.mjs";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));

function readOptionalJson(filePath) {
  if (!filePath || !fs.existsSync(filePath)) return null;
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function getArg(argv, flag) {
  const idx = argv.indexOf(flag);
  return idx !== -1 && argv[idx + 1] ? argv[idx + 1] : null;
}

async function main() {
  const argv = process.argv.slice(2);
  const driftJsonPath = getArg(argv, "--drift-json");
  const pipelineJsonPath = getArg(argv, "--pipeline-json");
  const validateJsonPath = getArg(argv, "--validate-json");
  const beforeManifestPath = getArg(argv, "--before-manifest");
  const gatesJsonPath = getArg(argv, "--gates-json");
  const prBodyOut = getArg(argv, "--pr-body-out");
  const metaOut = getArg(argv, "--meta-out");
  const workflowName = getArg(argv, "--workflow-name") ?? process.env.GITHUB_WORKFLOW ?? "Data refresh (stfc.space)";
  const runUrl = getArg(argv, "--run-url");
  const fetchModeSummary = getArg(argv, "--fetch-summary");

  const driftRaw = readOptionalJson(driftJsonPath);
  const driftDomains = Array.isArray(driftRaw) ? driftRaw : driftRaw?.domains ?? null;
  const pipelineReport = readOptionalJson(pipelineJsonPath);
  const validationReport = readOptionalJson(validateJsonPath);
  const beforeManifest = readOptionalJson(beforeManifestPath);
  const coverageGateResults = readOptionalJson(gatesJsonPath) ?? [];

  let manifestDiffs = null;
  if (beforeManifest) {
    const afterManifest = computePipelineManifest(REPO_ROOT);
    manifestDiffs = diffPipelineManifests(beforeManifest, afterManifest);
  }

  const built = buildRefreshPrBody({
    workflowName,
    runUrl,
    driftDomains,
    pipelineReport,
    validationReport,
    manifestDiffs,
    coverageGateResults,
    ...(fetchModeSummary ? { fetchModeSummary } : {}),
  });

  if (prBodyOut) {
    await atomicWriteFile(path.resolve(prBodyOut), built.markdown);
  } else {
    process.stdout.write(built.markdown);
  }

  const meta = {
    schema_version: 1,
    needs_triage: built.needsTriage,
    pr_title: built.title,
    triage_item_count: built.triage.items.length,
    validation_errors: built.triage.validationErrors,
    items: built.triage.items,
  };
  if (metaOut) {
    await atomicWriteJson(path.resolve(metaOut), meta, 2);
  }

  if (process.env.GITHUB_OUTPUT) {
    const outLines = [
      `needs_triage=${built.needsTriage ? "true" : "false"}`,
      `pr_title=${built.title}`,
      `triage_item_count=${built.triage.items.length}`,
    ];
    fs.appendFileSync(process.env.GITHUB_OUTPUT, `${outLines.join("\n")}\n`, "utf8");
  }

  if (process.env.GITHUB_STEP_SUMMARY) {
    fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${built.markdown}\n`, "utf8");
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
