/**
 * Import research catalog from data.stfc.space for KOBAYASHI.
 * - Reads data/upstream/data-stfc-space/summary-research.json (or fetches research/summary.json).
 * - Fetches research/{id}.json for each node to get per-level buff values.
 * - Maps buff ids to engine stats via RESEARCH_BUFF_MAPPING or data/buildings/buff_id_to_stat.json.
 * - Writes data/research_catalog.json (KOBAYASHI schema: rid, levels[].bonuses).
 *
 * Usage:
 *   node scripts/import_stfcspace_research.mjs [--from-upstream] [--limit N] [--rid 123,456]
 *   --from-upstream  use data/upstream/data-stfc-space/summary-research.json instead of fetch
 *   --limit N        process at most N research nodes (default: 50 for subset)
 *   --rid 123,456    only process these rids (comma-separated)
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const OUT_PATH = path.join(REPO_ROOT, "data", "research_catalog.json");
const UPSTREAM_SUMMARY_PATH = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
  "summary-research.json"
);
const BUFF_ID_TO_STAT_PATH = path.join(REPO_ROOT, "data", "buildings", "buff_id_to_stat.json");
const BASE_URL = "https://data.stfc.space";

const FROM_UPSTREAM =
  process.argv.includes("--from-upstream") || process.env.USE_UPSTREAM_RESEARCH === "1";

function getArg(name, def) {
  const i = process.argv.indexOf(name);
  if (i === -1) return def;
  const v = process.argv[i + 1];
  return v === undefined ? def : v;
}

// Buff id → { stat, operator } for research. Reuse building combat stats where same buff appears.
// value_is_percentage: true → value from API is already fractional (0.05 = 5%); false → may be flat (e.g. attack).
const RESEARCH_BUFF_MAPPING = {
  // Add combat-relevant buff ids as we confirm from translations or game data.
  // Example: 434613423 might be attack/weapon - verify from loca_id/translations.
};

let commonBuffNormalization = {};

async function loadSummary() {
  if (FROM_UPSTREAM) {
    const raw = await fs.readFile(UPSTREAM_SUMMARY_PATH, "utf8");
    const summary = JSON.parse(raw);
    if (!Array.isArray(summary)) throw new Error("summary-research.json: expected array");
    return summary;
  }
  const res = await fetch(`${BASE_URL}/research/summary.json`);
  if (!res.ok) throw new Error(`HTTP ${res.status} for research/summary.json`);
  return res.json();
}

async function fetchResearchDetail(rid) {
  const res = await fetch(`${BASE_URL}/research/${rid}.json`);
  if (!res.ok) return null;
  return res.json();
}

function resolveBuffStat(buffId) {
  const known = RESEARCH_BUFF_MAPPING[buffId];
  if (known) return known;
  const stat = commonBuffNormalization[buffId];
  if (stat) return { stat: typeof stat === "string" ? stat : stat.stat ?? stat, operator: "add" };
  return null;
}

/**
 * Build KOBAYASHI levels from detail API response.
 * detail.buffs[].values[i] = { value, chance } for level i+1.
 * value_is_percentage true → use value as fraction (0.05 = +5%). value_is_percentage false → treat as flat or skip.
 */
function buildLevelsFromDetail(detail) {
  if (!detail || !Array.isArray(detail.buffs)) return [];
  const maxLevel = Math.min(
    Number(detail.max_level) || 0,
    ...detail.buffs.map((b) => (Array.isArray(b.values) ? b.values.length : 0))
  );
  if (maxLevel <= 0) return [];

  const levels = [];
  for (let level = 1; level <= maxLevel; level += 1) {
    const bonuses = [];
    for (const buff of detail.buffs) {
      const mapping = resolveBuffStat(buff.id);
      if (!mapping) continue;
      const values = Array.isArray(buff.values) ? buff.values : [];
      const idx = level - 1;
      if (idx < 0 || idx >= values.length) continue;
      const raw = values[idx];
      if (!raw || typeof raw.value !== "number") continue;
      let value = raw.value;
      if (buff.value_is_percentage) {
        // API may give 5 for 5% or 0.05 for 5%; normalize to fractional.
        value = value >= 0 && value <= 1.5 ? value : value / 100;
      } else {
        // Flat stat (e.g. armor, attack) - only include if we have a stat that accepts flat.
        if (mapping.stat !== "armor" && mapping.stat !== "weapon_damage") continue;
        // Heuristic: treat small numbers as fractional (e.g. 0.05), large as flat.
        if (value < 10) value = value; // could be fractional already
      }
      bonuses.push({ stat: mapping.stat, value, operator: mapping.operator ?? "add" });
    }
    if (bonuses.length > 0) {
      levels.push({ level, bonuses });
    }
  }
  return levels;
}

async function main() {
  try {
    const raw = await fs.readFile(BUFF_ID_TO_STAT_PATH, "utf8");
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      commonBuffNormalization = { ...parsed };
    }
  } catch (_) {
    // ignore
  }

  const limit = Math.max(0, parseInt(getArg("--limit", "50"), 10));
  const ridArg = getArg("--rid", "");
  const onlyRids = ridArg
    ? new Set(ridArg.split(",").map((s) => parseInt(s.trim(), 10)).filter((n) => !Number.isNaN(n)))
    : null;

  if (FROM_UPSTREAM) {
    console.log("Reading summary from data/upstream/data-stfc-space/summary-research.json …");
  } else {
    console.log("Fetching research summary from data.stfc.space …");
  }
  const summary = await loadSummary();

  let toProcess = summary;
  if (onlyRids && onlyRids.size > 0) {
    toProcess = summary.filter((e) => e && onlyRids.has(e.id));
    console.log(`Processing ${toProcess.length} research nodes (--rid filter).`);
  } else if (limit > 0) {
    toProcess = toProcess.slice(0, limit);
    console.log(`Processing first ${toProcess.length} research nodes (--limit ${limit}).`);
  }

  const items = [];
  for (let i = 0; i < toProcess.length; i++) {
    const entry = toProcess[i];
    if (!entry || typeof entry.id !== "number") continue;
    const rid = entry.id;
    const detail = await fetchResearchDetail(rid);
    if (!detail) {
      console.warn(`  [${i + 1}/${toProcess.length}] rid ${rid}: no detail`);
      continue;
    }
    const levels = buildLevelsFromDetail(detail);
    if (levels.length === 0) continue;
    items.push({
      rid,
      name: null,
      data_version: FROM_UPSTREAM ? "stfcspace-upstream" : "stfcspace-fetch",
      source_note: "data.stfc.space research API",
      levels,
    });
    if ((i + 1) % 10 === 0) console.log(`  Processed ${i + 1}/${toProcess.length} …`);
  }

  if (items.length === 0) {
    console.log("No research records with mapped combat buffs; leaving existing catalog unchanged.");
    console.log("Add buff id → stat mappings in RESEARCH_BUFF_MAPPING or data/buildings/buff_id_to_stat.json.");
    return;
  }
  const catalog = {
    source: "data.stfc.space",
    last_updated: new Date().toISOString().slice(0, 10),
    items,
  };
  await fs.writeFile(OUT_PATH, JSON.stringify(catalog, null, 2), "utf8");
  console.log(`Wrote ${items.length} research records to data/research_catalog.json`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
