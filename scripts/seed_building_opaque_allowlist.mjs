#!/usr/bin/env node
/**
 * Seed / verify data/buildings/opaque_buff_allowlist.json from building JSON + translations.
 *
 * Usage:
 *   node scripts/seed_building_opaque_allowlist.mjs           # summary + manual-review bucket
 *   node scripts/seed_building_opaque_allowlist.mjs --write     # merge proposed entries into allowlist
 *   node scripts/seed_building_opaque_allowlist.mjs --check     # exit 1 if economy buffs lack allowlist entry
 */

import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  allowlistCategoryForEntry,
  categorizeBuildingBuffDescription,
  defaultAllowlistReason,
  isAllowlistCandidate,
} from "./lib/building_buff_categorize.mjs";

const REPO_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const BUILDINGS_DIR = path.join(REPO_ROOT, "data", "buildings");
const ALLOWLIST_PATH = path.join(BUILDINGS_DIR, "opaque_buff_allowlist.json");
const TRANSLATIONS_PATH = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
  "translations-starbase_modules.json",
);
const INDEX_PATH = path.join(BUILDINGS_DIR, "index.json");

const SKIP_FILES = new Set([
  "index.json",
  "buff_id_to_stat.json",
  "buff_id_to_semantics.json",
  "opaque_buff_allowlist.json",
  "mapping_gaps_baseline.json",
  "hull_id_registry.json",
]);

function parseLocaIdFromNotes(notes) {
  if (!notes || typeof notes !== "string") return null;
  const m = notes.match(/loca_id=(\d+)/);
  return m ? Number(m[1]) : null;
}

async function loadStarbaseBuffDescriptions() {
  const map = new Map();
  try {
    const rows = JSON.parse(await fs.readFile(TRANSLATIONS_PATH, "utf8"));
    for (const r of rows) {
      if (r.key === "starbase_module_buff_description" && typeof r.id === "number") {
        map.set(r.id, r.text || "");
      }
    }
  } catch {
    // optional
  }
  return map;
}

async function loadBuildingNames() {
  const map = new Map();
  try {
    const index = JSON.parse(await fs.readFile(INDEX_PATH, "utf8"));
    for (const b of index.buildings || []) {
      if (b.id) map.set(b.id, b.building_name || b.id);
    }
  } catch {
    // optional
  }
  return map;
}

async function loadAllowlist() {
  try {
    const raw = JSON.parse(await fs.readFile(ALLOWLIST_PATH, "utf8"));
    return {
      version: raw.version ?? 1,
      entries: raw.entries && typeof raw.entries === "object" ? { ...raw.entries } : {},
    };
  } catch {
    return { version: 1, entries: {} };
  }
}

/**
 * @returns {Promise<Map<string, { stat: string, count: number, samples: string[], sampleNames: string[], firstLocaId: number | null }>>}
 */
async function scanOpaqueBuffStats() {
  const buildingNames = await loadBuildingNames();
  const opaque = new Map();
  const files = await fs.readdir(BUILDINGS_DIR);
  for (const file of files.sort()) {
    if (!file.endsWith(".json") || SKIP_FILES.has(file)) continue;
    const filePath = path.join(BUILDINGS_DIR, file);
    let record;
    try {
      record = JSON.parse(await fs.readFile(filePath, "utf8"));
    } catch {
      continue;
    }
    const buildingId = record.id || file.replace(/\.json$/, "");
    const buildingName = record.building_name || buildingNames.get(buildingId) || buildingId;
    for (const level of record.levels || []) {
      for (const bonus of level.bonuses || []) {
        const stat = bonus.stat;
        if (!stat || !stat.startsWith("buff_")) continue;
        const existing = opaque.get(stat) ?? {
          stat,
          count: 0,
          samples: [],
          sampleNames: [],
          firstLocaId: null,
        };
        existing.count += 1;
        if (existing.firstLocaId == null) {
          existing.firstLocaId = parseLocaIdFromNotes(bonus.notes);
        }
        if (existing.samples.length < 4 && !existing.samples.includes(buildingId)) {
          existing.samples.push(buildingId);
          existing.sampleNames.push(buildingName);
        }
        opaque.set(stat, existing);
      }
    }
  }
  return opaque;
}

function describeStat(agg, descByLoca) {
  if (agg.firstLocaId != null && descByLoca.has(agg.firstLocaId)) {
    return descByLoca.get(agg.firstLocaId);
  }
  return "";
}

function buildProposals(opaque, descByLoca, allowlist) {
  const proposals = [];
  const manualReview = [];
  const actionable = [];

  for (const agg of opaque.values()) {
    const description = describeStat(agg, descByLoca);
    const category = categorizeBuildingBuffDescription(description);
    const buildingName = agg.sampleNames[0] || agg.samples[0] || "?";

    if (allowlist.entries[agg.stat]) continue;

    if (isAllowlistCandidate(category, description)) {
      const entryCategory = allowlistCategoryForEntry(category);
      proposals.push({
        stat: agg.stat,
        category: entryCategory,
        reason: defaultAllowlistReason(agg.stat, description, buildingName, entryCategory),
        triageCategory: category,
        buildingName,
        description,
      });
    } else if (category === "other_meta" || category === "no_description") {
      manualReview.push({ ...agg, category, description, buildingName });
    } else {
      actionable.push({ ...agg, category, description, buildingName });
    }
  }

  return { proposals, manualReview, actionable };
}

function printSummary(opaque, allowlist, proposals, manualReview, actionable) {
  const allowlistedCount = [...opaque.keys()].filter((s) => allowlist.entries[s]).length;
  const afterWrite = allowlistedCount + proposals.length;
  const actionableAfter = opaque.size - afterWrite;

  console.log("# Building opaque buff allowlist seed\n");
  console.log(`Distinct opaque buff_*: ${opaque.size}`);
  console.log(`Already allowlisted: ${allowlistedCount}`);
  console.log(`Proposed new allowlist entries: ${proposals.length}`);
  console.log(`Manual review (other_meta / no_description): ${manualReview.length}`);
  console.log(`Actionable (scoped/unmapped combat): ${actionable.length}`);
  console.log(`After --write: allowlisted ~${afterWrite}, actionable ~${actionableAfter}\n`);

  if (proposals.length > 0) {
    console.log("## Proposed allowlist entries\n");
    for (const p of proposals.slice(0, 15)) {
      console.log(`  ${p.stat}  [${p.category}]  ${p.buildingName}`);
      console.log(`    ${p.reason.slice(0, 100)}`);
    }
    if (proposals.length > 15) console.log(`  ... +${proposals.length - 15} more\n`);
    else console.log("");
  }

  if (manualReview.length > 0) {
    console.log("## Manual review\n");
    for (const row of manualReview) {
      const desc = (row.description || "").slice(0, 70);
      console.log(`  ${row.stat}  [${row.category}]  ${row.buildingName}  ${desc}`);
    }
    console.log("");
  }
}

async function writeAllowlist(allowlist, proposals) {
  for (const p of proposals) {
    if (!allowlist.entries[p.stat]) {
      allowlist.entries[p.stat] = {
        category: p.category,
        reason: p.reason,
      };
    }
  }
  const sorted = {};
  for (const key of Object.keys(allowlist.entries).sort()) {
    sorted[key] = allowlist.entries[key];
  }
  const payload = { version: allowlist.version || 1, entries: sorted };
  await fs.writeFile(ALLOWLIST_PATH, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
  console.log(`Wrote ${proposals.length} new entries → ${ALLOWLIST_PATH}`);
  console.log(`Total allowlist entries: ${Object.keys(sorted).length}`);
}

async function main() {
  const write = process.argv.includes("--write");
  const check = process.argv.includes("--check");

  const [opaque, descByLoca, allowlist] = await Promise.all([
    scanOpaqueBuffStats(),
    loadStarbaseBuffDescriptions(),
    loadAllowlist(),
  ]);

  const { proposals, manualReview, actionable } = buildProposals(opaque, descByLoca, allowlist);

  if (write) {
    await writeAllowlist(allowlist, proposals);
    return;
  }

  printSummary(opaque, allowlist, proposals, manualReview, actionable);

  if (check) {
    const missing = proposals.filter((p) => !allowlist.entries[p.stat]);
    if (missing.length > 0) {
      console.error(
        `ERROR: ${missing.length} economy/meta opaque buff(s) missing from opaque_buff_allowlist.json`,
      );
      for (const p of missing.slice(0, 10)) {
        console.error(`  ${p.stat}  [${p.category}]  ${p.buildingName}`);
      }
      if (missing.length > 10) console.error(`  ... +${missing.length - 10} more`);
      console.error("Run: node scripts/seed_building_opaque_allowlist.mjs --write");
      process.exit(1);
    }
    console.log("OK: all economy/meta opaque buffs are allowlisted.");
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
