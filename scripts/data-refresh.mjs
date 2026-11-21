#!/usr/bin/env node
/**
 * Kobayashi v2 Data Refresh Orchestrator (declarative DAG + provenance manifest).
 *
 * Usage (repo root):
 *   npm run data:refresh
 *   npm run data:refresh -- --stfcspace
 *   npm run data:refresh -- --stfcspace --fetch --write-manifest
 *   npm run data:refresh -- --stfccommunity
 *   npm run data:refresh -- --all
 *   npm run data:refresh -- --stfcspace --dry-run
 *   npm run data:refresh -- --stages stfcspace:buildings,verify:manifest
 *   npm run data:refresh -- --stfcspace --skip stfcspace:building_gaps
 *
 * Flags:
 *   (none)                        Core: CSV importers under data/import/
 *   --stfccommunity               PowerShell fetch + cargo run --bin normalize_stfc_data
 *   --stfcspace                   Normalized stfc.space ship/hostile/building/research pipeline
 *   --all                         core + --stfccommunity + --stfcspace
 *   --fetch                       Run live data.stfc.space catalog + detail fetch before normalizing
 *   --full                        Pass --full when --fetch is enabled
 *   --concurrency N               Bounded parallel fetch workers for --fetch (default: 6)
 *   --stages id1,id2              Run only these explicit DAG stage IDs (topologically ordered)
 *   --skip id1,id2                Skip these DAG stage IDs
 *   --list-stages                 Print all available DAG stage IDs and exit
 *   --dry-run                     Print resolved execution plan without executing stages
 *   --continue-on-coverage-gap    Treat known upstream coverage-baseline checks as soft warnings
 *   --write-manifest              Recompute and write data/pipeline_manifest.json after normalization
 *   --check-manifest              Verify data/pipeline_manifest.json matches on-disk catalogs
 *   --validate                    Run upstream mapping validator + cargo run --bin validate_data
 *   --json-report PATH            Write structured DAG execution report JSON
 *   --markdown-report PATH        Write Markdown DAG execution summary
 *   --help, -h                    Show this help
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  countJsonFilesInDir,
  formatPipelineReportMarkdown,
  runPipelineDag,
  selectStages,
} from "./lib/pipeline_dag.mjs";
import {
  DEFAULT_MANIFEST_REL_PATH,
  verifyPipelineManifest,
  writePipelineManifest,
} from "./lib/pipeline_manifest.mjs";
import { atomicWriteFile, atomicWriteJson, clampConcurrency, DEFAULT_CONCURRENCY } from "./lib/stfcspace_http.mjs";

const ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const UPSTREAM = path.join(ROOT, "data", "upstream", "data-stfc-space");

function exists(rel) {
  return fs.existsSync(path.join(ROOT, rel));
}

function pythonExe() {
  if (process.env.PYTHON && process.env.PYTHON.trim()) return process.env.PYTHON.trim();
  return process.platform === "win32" ? "python" : "python3";
}

function nodeExe() {
  return process.env.NODE && process.env.NODE.trim() ? process.env.NODE.trim() : process.execPath;
}

function getFlagValue(argv, flag) {
  const idx = argv.indexOf(flag);
  return idx !== -1 && argv[idx + 1] ? argv[idx + 1] : null;
}

function parseCsvFlag(argv, flag) {
  const raw = getFlagValue(argv, flag);
  if (!raw) return null;
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

/**
 * Build the complete declarative pipeline DAG stage registry.
 * @param {{
 *   fullFetch: boolean,
 *   concurrency: number,
 *   writeManifest: boolean,
 *   checkManifest: boolean,
 * }} opts
 */
export function buildPipelineStages(opts) {
  const py = pythonExe();
  const node = nodeExe();
  const fetchModeArgs = opts.fullFetch ? ["--full"] : ["--missing-only"];
  const concArgs = ["--concurrency", String(opts.concurrency)];

  return [
    // ── Core CSV importers ───────────────────────────────────────────────────
    {
      id: "core:forbidden_chaos",
      group: "core",
      title: "Forbidden / chaos tech (CSV → data/forbidden_chaos_tech.json)",
      dependsOn: [],
      buildCommand: () => ({
        program: "cargo",
        args: ["run", "--bin", "import_forbidden_chaos"],
      }),
    },
    {
      id: "core:syndicate",
      group: "core",
      title: "Syndicate reputation (CSV → data/syndicate_reputation.json)",
      dependsOn: [],
      checkPrecondition: () =>
        exists("data/import/syndicate_reputation.csv")
          ? { ok: true }
          : { ok: false, reason: "data/import/syndicate_reputation.csv not found" },
      buildCommand: () => ({
        program: "cargo",
        args: ["run", "--bin", "import_syndicate_reputation"],
      }),
    },

    // ── STFCcommunity baseline ───────────────────────────────────────────────
    {
      id: "stfccommunity:fetch",
      group: "stfccommunity",
      title: "Download STFCcommunity/data (PowerShell)",
      dependsOn: [],
      buildCommand: () =>
        process.platform === "win32"
          ? {
              program: "powershell",
              args: ["-ExecutionPolicy", "Bypass", "-File", "scripts/fetch_stfc_data.ps1"],
            }
          : {
              program: "pwsh",
              args: ["-NoProfile", "-File", "scripts/fetch_stfc_data.ps1"],
            },
    },
    {
      id: "stfccommunity:normalize",
      group: "stfccommunity",
      title: "Normalize STFCcommunity baseline (hostiles, …)",
      dependsOn: ["stfccommunity:fetch"],
      buildCommand: () => ({
        program: "cargo",
        args: ["run", "--bin", "normalize_stfc_data"],
      }),
    },

    // ── Optional live data.stfc.space fetch stages (--fetch) ─────────────────
    {
      id: "fetch:catalog",
      group: "fetch",
      title: "Fetch data.stfc.space catalog summaries & translations",
      dependsOn: [],
      buildCommand: () => ({
        program: node,
        args: ["scripts/fetch_stfcspace_page_upstream.mjs", "--no-sample-ship", ...concArgs],
      }),
    },
    {
      id: "fetch:details",
      group: "fetch",
      title: "Fetch data.stfc.space ship / hostile / research detail JSON",
      dependsOn: ["fetch:catalog"],
      buildCommand: () => ({
        program: node,
        args: [
          "scripts/fetch_stfcspace_details.mjs",
          "--entities",
          "ships,hostiles,research",
          ...fetchModeArgs,
          ...concArgs,
        ],
      }),
    },

    // ── data.stfc.space normalization stages (--stfcspace) ───────────────────
    {
      id: "stfcspace:ship_registry",
      group: "stfcspace",
      title: "Build ship_id_registry.json (stfc.space)",
      dependsOn: ["fetch:details"],
      checkPrecondition: () => {
        const ok =
          fs.existsSync(path.join(UPSTREAM, "summary-ship.json")) &&
          fs.existsSync(path.join(UPSTREAM, "translations-ships.json"));
        return ok
          ? { ok: true }
          : {
              ok: false,
              hardFail: true,
              reason:
                "missing data/upstream/data-stfc-space/summary-ship.json or translations-ships.json",
            };
      },
      buildCommand: () => ({
        program: py,
        args: ["scripts/build_ship_registry.py"],
      }),
    },
    {
      id: "stfcspace:ships_extended",
      group: "stfcspace",
      title: "Normalize ships → data/ships_extended/",
      dependsOn: ["stfcspace:ship_registry"],
      buildCommand: () => ({
        program: "cargo",
        args: ["run", "--bin", "normalize_data_stfc_space"],
      }),
    },
    {
      id: "stfcspace:hostiles",
      group: "stfcspace",
      title: "Normalize hostiles → data/hostiles/",
      dependsOn: ["fetch:details"],
      checkPrecondition: () =>
        countJsonFilesInDir(path.join(UPSTREAM, "hostiles")) > 0
          ? { ok: true }
          : { ok: false, reason: "no JSON under data/upstream/data-stfc-space/hostiles/" },
      buildCommand: () => ({
        program: "cargo",
        args: ["run", "--bin", "normalize_hostiles_stfc_space"],
      }),
    },
    {
      id: "stfcspace:hostile_abilities",
      group: "stfcspace",
      title: "Hostile ability catalog (regenerate from upstream + translations)",
      dependsOn: ["stfcspace:hostiles"],
      checkPrecondition: () =>
        countJsonFilesInDir(path.join(UPSTREAM, "hostiles")) > 0
          ? { ok: true }
          : { ok: false, reason: "no JSON under data/upstream/data-stfc-space/hostiles/" },
      buildCommand: () => ({
        program: py,
        args: ["scripts/generate_full_hostile_ability_catalog.py"],
      }),
    },
    {
      id: "stfcspace:hull_registry",
      group: "stfcspace",
      title: "Hull id registry (roster ship mapping)",
      dependsOn: ["stfcspace:ships_extended"],
      buildCommand: () => ({
        program: node,
        args: ["scripts/build_hull_id_registry.mjs"],
      }),
    },
    {
      id: "stfcspace:buildings",
      group: "stfcspace",
      title: "Buildings import (stfc.space)",
      dependsOn: ["fetch:catalog"],
      buildCommand: () => ({
        program: node,
        args: exists("data/upstream/data-stfc-space/summary-building.json")
          ? ["scripts/import_stfcspace_buildings.mjs", "--from-upstream"]
          : ["scripts/import_stfcspace_buildings.mjs"],
      }),
    },
    {
      id: "stfcspace:building_allowlist",
      group: "stfcspace",
      title: "Building opaque buff allowlist check",
      dependsOn: ["stfcspace:buildings"],
      softFailure: true,
      buildCommand: () => ({
        program: node,
        args: ["scripts/seed_building_opaque_allowlist.mjs", "--check"],
      }),
    },
    {
      id: "stfcspace:building_gaps",
      group: "stfcspace",
      title: "Building opaque buff gap report (docs/building_gaps.md)",
      dependsOn: ["stfcspace:buildings"],
      optional: true,
      buildCommand: () => ({
        program: "cargo",
        args: ["run", "--bin", "report_building_mapping_gaps"],
        captureStdoutTo: "docs/building_gaps.md",
      }),
    },
    {
      id: "stfcspace:research",
      group: "stfcspace",
      title: "Research catalog (cached per-rid JSON + summary)",
      dependsOn: ["fetch:details"],
      checkPrecondition: () => {
        const hasDir = countJsonFilesInDir(path.join(UPSTREAM, "research")) > 0;
        const hasSummary = fs.existsSync(path.join(UPSTREAM, "summary-research.json"));
        return hasDir && hasSummary
          ? { ok: true }
          : {
              ok: false,
              reason: "need data/upstream/data-stfc-space/summary-research.json and research/*.json",
            };
      },
      buildCommand: () => ({
        program: node,
        args: ["scripts/import_stfcspace_research.mjs", "--from-upstream", "--limit", "0"],
      }),
    },
    {
      id: "stfcspace:research_gaps",
      group: "stfcspace",
      title: "Research mapping gaps (baseline check)",
      dependsOn: ["stfcspace:research"],
      optional: true,
      softFailure: true,
      buildCommand: () => ({
        program: node,
        args: ["scripts/research_mapping_gaps.mjs"],
      }),
    },

    // ── Manifest & validation stages ─────────────────────────────────────────
    {
      id: "verify:manifest",
      group: "manifest",
      title: opts.writeManifest
        ? `Update provenance manifest (${DEFAULT_MANIFEST_REL_PATH})`
        : `Verify provenance manifest (${DEFAULT_MANIFEST_REL_PATH})`,
      dependsOn: [
        "core:forbidden_chaos",
        "core:syndicate",
        "stfccommunity:normalize",
        "stfcspace:ships_extended",
        "stfcspace:hostiles",
        "stfcspace:hostile_abilities",
        "stfcspace:hull_registry",
        "stfcspace:buildings",
        "stfcspace:research",
      ],
      runCustom: async () => {
        if (opts.writeManifest) {
          const { manifest } = await writePipelineManifest(ROOT);
          return {
            ok: true,
            note: `combined=${manifest.combined_sha256.slice(0, 12)}`,
          };
        }
        const check = verifyPipelineManifest(ROOT);
        if (check.ok) {
          return { ok: true, note: `combined=${check.actual.combined_sha256.slice(0, 12)}` };
        }
        return {
          ok: false,
          error: `${check.diffs.length} manifest diff(s) — run with --write-manifest`,
        };
      },
    },
    {
      id: "verify:upstream_mapping",
      group: "validate",
      title: "Validate upstream mapping inventory",
      dependsOn: ["fetch:catalog"],
      softFailure: true,
      buildCommand: () => ({
        program: node,
        args: ["scripts/validate_stfcspace_mapping.mjs"],
      }),
    },
    {
      id: "verify:validate_data",
      group: "validate",
      title: "Strict data validation (cargo run --bin validate_data)",
      dependsOn: ["verify:manifest"],
      buildCommand: () => ({
        program: "cargo",
        args: ["run", "--bin", "validate_data"],
      }),
    },
  ];
}

function printHelp(stages) {
  console.log(`
Kobayashi data refresh v2 — declarative DAG orchestrator & provenance manifest.

Usage:
  npm run data:refresh
  npm run data:refresh -- --stfcspace
  npm run data:refresh -- --stfcspace --fetch --write-manifest
  npm run data:refresh -- --stfcspace --dry-run
  npm run data:refresh -- --stages stfcspace:buildings,verify:manifest
  npm run data:refresh -- --all

Flags:
  (none)                        Core: CSV importers under data/import/
  --stfccommunity               PowerShell fetch + cargo run --bin normalize_stfc_data
  --stfcspace                   Ship registry → ships_extended → hostiles → abilities → hull registry → buildings → research
  --all                         core + --stfccommunity + --stfcspace
  --fetch                       Also fetch live summaries/translations + ship/hostile/research details first
  --full                        Use --full for detail fetches when --fetch is set
  --concurrency N               Parallel workers for --fetch (1..16, default ${DEFAULT_CONCURRENCY})
  --stages id1,id2              Run only these stage IDs (in topological order)
  --skip id1,id2                Skip specific stage IDs
  --list-stages                 List all DAG stages and exit
  --dry-run                     Print resolved DAG plan without running commands
  --continue-on-coverage-gap    Record soft upstream coverage gaps without halting the DAG
  --write-manifest              Write deterministic ${DEFAULT_MANIFEST_REL_PATH}
  --check-manifest              Verify committed ${DEFAULT_MANIFEST_REL_PATH}
  --validate                    Run upstream mapping validator + cargo run --bin validate_data
  --json-report PATH            Write structured DAG telemetry JSON
  --markdown-report PATH        Write Markdown stage report

Available DAG stages:
${stages.map((s) => `  - ${s.id.padEnd(28)} [${s.group}] ${s.title}`).join("\n")}
`);
}

async function main() {
  const argv = process.argv.slice(2);
  const fullFetch = argv.includes("--full");
  const parsedConc = Number.parseInt(getFlagValue(argv, "--concurrency") ?? "", 10);
  const concurrency = clampConcurrency(Number.isFinite(parsedConc) ? parsedConc : DEFAULT_CONCURRENCY);
  const writeManifest = argv.includes("--write-manifest");
  const checkManifest = argv.includes("--check-manifest");
  const stages = buildPipelineStages({ fullFetch, concurrency, writeManifest, checkManifest });

  if (argv.includes("--help") || argv.includes("-h")) {
    printHelp(stages);
    process.exit(0);
  }
  if (argv.includes("--list-stages")) {
    for (const s of stages) {
      console.log(`${s.id}\t${s.group}\t${s.title}`);
    }
    process.exit(0);
  }

  const stfccommunity = argv.includes("--stfccommunity") || argv.includes("--all");
  const stfcspace = argv.includes("--stfcspace") || argv.includes("--all");
  const fetchLive = argv.includes("--fetch");
  const validate = argv.includes("--validate");
  const dryRun = argv.includes("--dry-run");
  const continueOnCoverageGap = argv.includes("--continue-on-coverage-gap");
  const onlyStages = parseCsvFlag(argv, "--stages") ?? parseCsvFlag(argv, "--only");
  const skipStages = parseCsvFlag(argv, "--skip");
  const jsonReportPath = getFlagValue(argv, "--json-report");
  const markdownReportPath = getFlagValue(argv, "--markdown-report");

  const groups = new Set(["core"]);
  if (fetchLive) groups.add("fetch");
  if (stfccommunity) groups.add("stfccommunity");
  if (stfcspace) groups.add("stfcspace");
  if (writeManifest || checkManifest) groups.add("manifest");
  if (validate) groups.add("validate");

  const selected = selectStages(stages, { groups, onlyStages, skipStages });

  console.log(
    `=== Kobayashi data refresh v2 (${selected.length} stage(s)${dryRun ? ", dry-run" : ""}) ===\n`,
  );

  const report = await runPipelineDag(selected, {
    cwd: ROOT,
    dryRun,
    continueOnSoftFailure: continueOnCoverageGap,
  });

  if (jsonReportPath) {
    await atomicWriteJson(path.resolve(jsonReportPath), report, 2);
  }
  if (markdownReportPath) {
    await atomicWriteFile(
      path.resolve(markdownReportPath),
      `${formatPipelineReportMarkdown(report)}\n`,
    );
  }

  console.log(
    `\n=== Data refresh finished (${(report.duration_ms / 1000).toFixed(1)}s, hard_failures=${report.hard_failure_count}, soft_coverage_gaps=${report.soft_failure_count}) ===`,
  );
  if (!report.ok) {
    process.exit(1);
  }
}

const isMain =
  process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
