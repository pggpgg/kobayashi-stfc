#!/usr/bin/env node
/**
 * Single orchestrated data-refresh entrypoint (audit task 19).
 * Chains importers/normalizers in the order documented in scripts/README.md.
 *
 * Usage (repo root):
 *   npm run data:refresh
 *   npm run data:refresh -- --stfcspace
 *   npm run data:refresh -- --stfccommunity
 *   npm run data:refresh -- --all
 *
 * Flags:
 *   --stfccommunity  Fetch STFCcommunity/data (PowerShell) then normalize hostiles baseline
 *   --stfcspace      data.stfc.space ship/hostile/building/research pipeline (needs upstream cache where noted)
 *   --all            core + --stfccommunity + --stfcspace
 *   --help           Show this help
 *
 * Default (no flags): "core" only — CSV importers that use files committed under data/import/.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const UPSTREAM = path.join(ROOT, "data", "upstream", "data-stfc-space");

function printHelp() {
  console.log(`
Kobayashi data refresh — orchestrates importers in documented order (scripts/README.md).

Usage:
  npm run data:refresh
  npm run data:refresh -- --stfcspace
  npm run data:refresh -- --stfccommunity
  npm run data:refresh -- --all

Flags:
  (none)           Core: CSV importers under data/import/
  --stfccommunity  PowerShell fetch + cargo run --bin normalize_stfc_data
  --stfcspace      Ship registry → ships_extended → hostiles → ability catalog → hull registry → buildings → research (when cached)
  --all            All of the above
`);
}

function run(title, command, { optional = false } = {}) {
  console.log(`\n── ${title} ──\n${command}\n`);
  const r = spawnSync(command, {
    cwd: ROOT,
    shell: true,
    stdio: "inherit",
    env: process.env,
  });
  if (r.status !== 0 && r.status !== null) {
    if (optional) {
      console.warn(`(optional step failed with ${r.status}, continuing)\n`);
      return false;
    }
    process.exit(r.status);
  }
  return true;
}

function exists(rel) {
  return fs.existsSync(path.join(ROOT, rel));
}

function countJsonFiles(dir) {
  if (!fs.existsSync(dir)) return 0;
  return fs.readdirSync(dir).filter((f) => f.endsWith(".json")).length;
}

function runCore() {
  run("Forbidden / chaos tech (CSV → data/forbidden_chaos_tech.json)", "cargo run --bin import_forbidden_chaos");
  if (exists("data/import/syndicate_reputation.csv")) {
    run(
      "Syndicate reputation (CSV → data/syndicate_reputation.json)",
      "cargo run --bin import_syndicate_reputation",
    );
  } else {
    console.log("\n── Syndicate reputation ──\n(skip: data/import/syndicate_reputation.csv not found)\n");
  }
}

function runStfcCommunityFetch() {
  if (process.platform === "win32") {
    run(
      "Download STFCcommunity/data (PowerShell)",
      "powershell -ExecutionPolicy Bypass -File scripts/fetch_stfc_data.ps1",
    );
    return;
  }
  const pwsh = spawnSync("pwsh", ["-NoProfile", "-File", "scripts/fetch_stfc_data.ps1"], {
    cwd: ROOT,
    stdio: "inherit",
  });
  if (pwsh.status === 0) return;
  const pwsh2 = spawnSync("powershell", ["-ExecutionPolicy", "Bypass", "-File", "scripts/fetch_stfc_data.ps1"], {
    cwd: ROOT,
    stdio: "inherit",
  });
  if (pwsh2.status !== 0) {
    console.error(
      "\nSTFCcommunity fetch failed. On Linux/macOS install PowerShell (`pwsh`) or run `scripts/fetch_stfc_data.ps1` manually, then re-run with --stfccommunity.\n",
    );
    process.exit(pwsh2.status ?? 1);
  }
}

function runStfcCommunityNormalize() {
  run("Normalize STFCcommunity baseline (hostiles, …)", "cargo run --bin normalize_stfc_data");
}

function runStfcSpace() {
  const summaryShip = path.join(UPSTREAM, "summary-ship.json");
  const translationsShips = path.join(UPSTREAM, "translations-ships.json");
  if (!fs.existsSync(summaryShip) || !fs.existsSync(translationsShips)) {
    console.error(
      "\n--stfcspace skipped: need data/upstream/data-stfc-space/summary-ship.json and translations-ships.json\n" +
        "Populate data/upstream/data-stfc-space/ (see docs/STFC_SPACE_DATA_STRATEGY.md), then re-run.\n",
    );
    process.exit(1);
  }

  const py = process.platform === "win32" ? "python" : "python3";
  run("Build ship_id_registry.json (stfc.space)", `${py} scripts/build_ship_registry.py`);

  run("Normalize ships → data/ships_extended/", "cargo run --bin normalize_data_stfc_space");

  const hostileDir = path.join(UPSTREAM, "hostiles");
  if (countJsonFiles(hostileDir) > 0) {
    run("Normalize hostiles → data/hostiles/", "cargo run --bin normalize_hostiles_stfc_space");
    // Must run after every hostile fetch: tests/hostile_ability_catalog_parity.rs requires a
    // catalog row for every upstream ability[].id, and new hostiles routinely introduce new ids
    // (Update 92's Elite Assassins added 3442434952). Unrecognised descriptions fall back to a
    // catalogued combat_noop with a review bucket in hostile_ability_audit_meta.json, so this
    // keeps the refresh green while still surfacing new mechanics in the PR diff for modeling.
    run(
      "Hostile ability catalog (regenerate from upstream + translations)",
      `${py} scripts/generate_full_hostile_ability_catalog.py`,
    );
  } else {
    console.log(
      "\n── Hostiles ──\n(skip: no JSON under data/upstream/data-stfc-space/hostiles/; run your hostile fetch/cache job first)\n",
    );
  }

  run("Hull id registry (roster ship mapping)", "node scripts/build_hull_id_registry.mjs");

  if (exists(path.join("data", "upstream", "data-stfc-space", "summary-building.json"))) {
    run(
      "Buildings import (cached summary-building.json)",
      "node scripts/import_stfcspace_buildings.mjs --from-upstream",
    );
  } else {
    run("Buildings import (fetch from data.stfc.space)", "node scripts/import_stfcspace_buildings.mjs");
  }

  run(
    "Building opaque buff allowlist check",
    "node scripts/seed_building_opaque_allowlist.mjs --check",
  );

  run(
    "Building opaque buff gap report (docs/building_gaps.md)",
    "cargo run --bin report_building_mapping_gaps > docs/building_gaps.md",
    { optional: true },
  );

  const researchDir = path.join(UPSTREAM, "research");
  const summaryResearch = path.join(UPSTREAM, "summary-research.json");
  if (countJsonFiles(researchDir) > 0 && fs.existsSync(summaryResearch)) {
    run(
      "Research catalog (cached per-rid JSON + summary)",
      "node scripts/import_stfcspace_research.mjs --from-upstream --limit 0",
    );
    run(
      "Research mapping gaps (baseline check)",
      "node scripts/research_mapping_gaps.mjs",
      { optional: true },
    );
  } else {
    console.log(
      "\n── Research catalog ──\n(skip: need data/upstream/data-stfc-space/summary-research.json and research/*.json;\n" +
        "  see data/README.md § Research — fetch_stfcspace_research.mjs, then import_stfcspace_research.mjs)\n",
    );
  }
}

const argv = process.argv.slice(2);
if (argv.includes("--help") || argv.includes("-h")) {
  printHelp();
  process.exit(0);
}

const stfccommunity = argv.includes("--stfccommunity") || argv.includes("--all");
const stfcspace = argv.includes("--stfcspace") || argv.includes("--all");

console.log("=== Kobayashi data refresh (audit task 19 entrypoint) ===\n");

runCore();

if (stfccommunity) {
  runStfcCommunityFetch();
  runStfcCommunityNormalize();
}

if (stfcspace) {
  runStfcSpace();
}

console.log(
  "\n=== Data refresh finished ===\nTip: run `npm run verify` to mirror CI (tests, clippy, frontend build).\n",
);
