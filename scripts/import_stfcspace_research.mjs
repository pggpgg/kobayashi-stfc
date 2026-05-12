/**
 * Import research catalog from data.stfc.space for KOBAYASHI.
 * - Reads data/upstream/data-stfc-space/summary-research.json (or fetches research/summary.json).
 * - Loads per-node detail **only** from data/upstream/data-stfc-space/research/{id}.json (no HTTP for details).
 * - Maps buff ids to engine stats: RESEARCH_BUFF_MAPPING, data/research/buff_id_to_stat.json,
 *   data/buildings/buff_id_to_stat.json, data/research/loca_id_to_stat.json, then description heuristics.
 *   JSON values may be a string, one object `{ stat, operator?, … }`, or an **array** of those (one buff → multiple profile stats).
 * - Writes data/research_catalog.json (KOBAYASHI schema: rid, name, levels[].bonuses).
 *
 * Usage:
 *   node scripts/import_stfcspace_research.mjs [--from-upstream] [--limit N] [--rid 123,456] [--dump-unmapped]
 *   --from-upstream  use data/upstream/data-stfc-space/summary-research.json instead of fetch for the summary
 *   --limit N        process at most N research nodes (default: 50); use 0 for all
 *   --rid 123,456    only process these rids (comma-separated)
 *   --dump-unmapped  print unmapped research buff ids (with counts) to stdout
 *
 * Full catalog (populate research/*.json first — e.g. scripts/fetch_stfcspace_research.mjs or your bulk job):
 *   node scripts/import_stfcspace_research.mjs --from-upstream --limit 0
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import { resolveBuffStatMappings } from "./lib/research_buff_resolve.mjs";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const OUT_PATH = path.join(REPO_ROOT, "data", "research_catalog.json");
const UPSTREAM_SUMMARY_PATH = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
  "summary-research.json"
);
const UPSTREAM_RESEARCH_DIR = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
  "research"
);
const TRANSLATIONS_RESEARCH_PATH = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
  "translations-research.json"
);
const BUFF_ID_TO_STAT_PATH = path.join(REPO_ROOT, "data", "buildings", "buff_id_to_stat.json");
const RESEARCH_BUFF_ID_TO_STAT_PATH = path.join(
  REPO_ROOT,
  "data",
  "research",
  "buff_id_to_stat.json"
);
const RESEARCH_LOCA_ID_TO_STAT_PATH = path.join(
  REPO_ROOT,
  "data",
  "research",
  "loca_id_to_stat.json"
);
const BASE_URL = "https://data.stfc.space";

/**
 * Stats merged into PlayerProfile via normalize_profile_combat_stat (see `normalize_profile_combat_stat` in
 * src/data/profile.rs). Must match that function's engine keys (aliases like armor_pierce fold in Rust only).
 *
 * Morale-gated isolytic uses `isolytic_damage` + `requires_morale` on catalog rows (compiled to round-start seats).
 */
const ALLOWED_COMBAT_STATS = new Set([
  "weapon_damage",
  "officer_attack",
  "officer_defense",
  "officer_health",
  "hull_hp",
  "shield_hp",
  "isolytic_damage",
  "isolytic_cascade",
  "isolytic_cascade_damage",
  "isolytic_defense",
  "crit_chance",
  "crit_damage",
  "pierce",
  "shield_mitigation",
  "armor",
  "shield_deflection",
  "dodge",
  "damage_reduction",
  "accuracy",
  "apex_shred",
  "apex_barrier",
]);

const FROM_UPSTREAM =
  process.argv.includes("--from-upstream") || process.env.USE_UPSTREAM_RESEARCH === "1";
const DUMP_UNMAPPED = process.argv.includes("--dump-unmapped");

if (process.argv.includes("--help") || process.argv.includes("-h")) {
  console.log(`Usage: node scripts/import_stfcspace_research.mjs [--from-upstream] [--limit N] [--rid 123,456] [--dump-unmapped]

  --from-upstream  use data/upstream/data-stfc-space/summary-research.json (no HTTP for summary)
  --limit N        process at most N nodes (default 50; use 0 for all)
  --rid a,b        only these comma-separated rids
  --dump-unmapped  print unmapped buff id counts

Writes data/research_catalog.json. Detail JSON must exist under data/upstream/data-stfc-space/research/.
`);
  process.exit(0);
}

function getArg(name, def) {
  const i = process.argv.indexOf(name);
  if (i === -1) return def;
  const v = process.argv[i + 1];
  return v === undefined ? def : v;
}

const RESEARCH_BUFF_MAPPING = {};

let commonBuffNormalization = {};
let researchBuffById = {};
let locaIdToStat = {};

function makeBuffResolveCtx(descriptionByLocaId, projectNamesByLocaId) {
  return {
    researchBuffMapping: RESEARCH_BUFF_MAPPING,
    researchBuffById,
    commonBuffNormalization,
    locaIdToStat,
    descriptionByLocaId,
    projectNamesByLocaId,
  };
}

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

/**
 * research_project_name by loca id (summary / project loca_id).
 */
async function loadResearchProjectNames() {
  const map = new Map();
  try {
    const raw = await fs.readFile(TRANSLATIONS_RESEARCH_PATH, "utf8");
    const rows = JSON.parse(raw);
    if (!Array.isArray(rows)) return map;
    for (const r of rows) {
      if (r && r.key === "research_project_name" && typeof r.id === "number" && typeof r.text === "string") {
        map.set(r.id, r.text);
      }
    }
  } catch (_) {
    // optional
  }
  return map;
}

/**
 * research_project_description by loca id — used for combat-stat inference when buff maps miss.
 */
async function loadResearchDescriptionsByLocaId() {
  const map = new Map();
  try {
    const raw = await fs.readFile(TRANSLATIONS_RESEARCH_PATH, "utf8");
    const rows = JSON.parse(raw);
    if (!Array.isArray(rows)) return map;
    for (const r of rows) {
      if (
        r &&
        r.key === "research_project_description" &&
        typeof r.id === "number" &&
        typeof r.text === "string"
      ) {
        map.set(r.id, r.text);
      }
    }
  } catch (_) {
    // optional
  }
  return map;
}

/** Per-rid detail: local file only (populate via scripts/fetch_stfcspace_research.mjs or an external bulk fetch). */
async function loadResearchDetailLocal(rid) {
  const cachePath = path.join(UPSTREAM_RESEARCH_DIR, `${rid}.json`);
  try {
    const raw = await fs.readFile(cachePath, "utf8");
    return JSON.parse(raw);
  } catch (_) {
    return null;
  }
}

function addUnmapped(unmappedByBuffId, rid, buff) {
  const buffId = buff && typeof buff.id === "number" ? buff.id : null;
  if (buffId == null) return;
  const key = String(buffId);
  const existing = unmappedByBuffId.get(key) ?? {
    buff_id: buffId,
    count: 0,
    example_rids: [],
    value_is_percentage: null,
    loca_id: null,
  };
  existing.count += 1;
  if (existing.example_rids.length < 5 && typeof rid === "number") {
    if (!existing.example_rids.includes(rid)) existing.example_rids.push(rid);
  }
  if (existing.value_is_percentage == null && typeof buff?.value_is_percentage === "boolean") {
    existing.value_is_percentage = buff.value_is_percentage;
  }
  if (existing.loca_id == null && typeof buff?.loca_id === "number") {
    existing.loca_id = buff.loca_id;
  }
  unmappedByBuffId.set(key, existing);
}

/**
 * Stats that sometimes appear with value_is_percentage false but use fractional bonuses like 0.05.
 */
const NON_PCT_DECIMAL_STATS = new Set([
  "armor",
  "shield_deflection",
  "weapon_damage",
  "isolytic_damage",
  "isolytic_defense",
  "hull_hp",
  "shield_hp",
  "crit_chance",
  "crit_damage",
  "pierce",
  "shield_mitigation",
  "damage_reduction",
  "dodge",
  "accuracy",
  "apex_shred",
  "apex_barrier",
]);

function normalizeBonusValue(buff, mapping, rawValue) {
  let value = rawValue;
  // NS Burning Damage buff: upstream uses percentage points (1 = +1% weapon damage).
  // The generic branch below would keep 1.0 as literal +100% for `value_is_percentage` rows ≤ 1.5.
  if (buff?.id === 1898558353 && mapping?.stat === "weapon_damage" && buff.value_is_percentage) {
    return value / 100;
  }
  if (buff.value_is_percentage) {
    value = value >= 0 && value <= 1.5 ? value : value / 100;
    return value;
  }
  // Apex barrier / shred with value_is_percentage false carry absolute integer values
  // (e.g. 250, 1000) that map directly to the engine's attacker stats without scaling.
  if (
    (mapping.stat === "apex_barrier" || mapping.stat === "apex_shred") &&
    !buff.value_is_percentage
  ) {
    if (value > 0 && Number.isFinite(value)) return value;
    return null;
  }
  if (!NON_PCT_DECIMAL_STATS.has(mapping.stat)) {
    return null;
  }
  if (value >= 0 && value <= 2) {
    return value;
  }
  if (value > 2 && value <= 100 && Number.isInteger(value)) {
    return value / 100;
  }
  return null;
}

function buildLevelsFromDetail(detail, opts) {
  if (!detail || !Array.isArray(detail.buffs)) return [];

  const { rid, unmappedByBuffId, descriptionByLocaId, projectLocaId, projectNamesByLocaId } = opts;
  const buffResolveCtx = makeBuffResolveCtx(descriptionByLocaId, projectNamesByLocaId);

  if (unmappedByBuffId) {
    for (const buff of detail.buffs) {
      const mappings = resolveBuffStatMappings(buffResolveCtx, buff, projectLocaId);
      if (mappings.length === 0) {
        addUnmapped(unmappedByBuffId, rid, buff);
      }
    }
  }

  const buffValuesLens = detail.buffs.map((b) =>
    Array.isArray(b.values) ? b.values.length : 0
  );

  const maxFromLevels = Array.isArray(detail.levels) ? detail.levels.length : 0;
  const maxFromLegacyField = Number(detail.max_level) || 0;
  const candidateMax = maxFromLegacyField || maxFromLevels || Infinity;

  const maxLevel = Math.min(candidateMax, ...buffValuesLens);
  if (maxLevel <= 0) return [];

  const levels = [];
  for (let level = 1; level <= maxLevel; level += 1) {
    const bonuses = [];
    for (const buff of detail.buffs) {
      const mappings = resolveBuffStatMappings(buffResolveCtx, buff, projectLocaId);
      for (const mapping of mappings) {
        if (!ALLOWED_COMBAT_STATS.has(mapping.stat)) {
          continue;
        }
        const values = Array.isArray(buff.values) ? buff.values : [];
        const idx = level - 1;
        if (idx < 0 || idx >= values.length) continue;
        const raw = values[idx];
        if (!raw || typeof raw.value !== "number") continue;
        const value = normalizeBonusValue(buff, mapping, raw.value);
        if (value == null || value === 0 || !Number.isFinite(value)) continue;
        const bonus = {
          stat: mapping.stat,
          value,
          operator: mapping.operator ?? "add",
        };
        if (mapping.requires_defender_burning) bonus.requires_defender_burning = true;
        if (mapping.requires_morale) bonus.requires_morale = true;
        if (mapping.requires_defender_hull_breach)
          bonus.requires_defender_hull_breach = true;
        if (typeof mapping.defender_ship_class === "string")
          bonus.defender_ship_class = mapping.defender_ship_class;
        if (typeof mapping.defender_faction === "string")
          bonus.defender_faction = mapping.defender_faction;
        if (typeof mapping.attacker_faction === "string")
          bonus.attacker_faction = mapping.attacker_faction;
        if (Array.isArray(mapping.attacker_factions) && mapping.attacker_factions.length)
          bonus.attacker_factions = mapping.attacker_factions.map(String).filter(Boolean);
        bonuses.push(bonus);
      }
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

  try {
    const raw = await fs.readFile(RESEARCH_BUFF_ID_TO_STAT_PATH, "utf8");
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      researchBuffById = { ...parsed };
    }
  } catch (_) {
    // ignore
  }

  try {
    const raw = await fs.readFile(RESEARCH_LOCA_ID_TO_STAT_PATH, "utf8");
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      locaIdToStat = { ...parsed };
    }
  } catch (_) {
    // ignore
  }

  const unmappedByBuffId = new Map();
  const descriptionByLocaId = await loadResearchDescriptionsByLocaId();
  const projectNamesByLocaId = await loadResearchProjectNames();

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
  const summaryTotal = summary.filter((e) => e && typeof e.id === "number").length;

  let toProcess = summary;
  if (onlyRids && onlyRids.size > 0) {
    toProcess = summary.filter((e) => e && onlyRids.has(e.id));
    console.log(`Processing ${toProcess.length} research nodes (--rid filter).`);
  } else if (limit > 0) {
    toProcess = toProcess.slice(0, limit);
    console.log(`Processing first ${toProcess.length} research nodes (--limit ${limit}).`);
  } else {
    console.log(`Processing all ${toProcess.length} research nodes (--limit 0).`);
  }

  let detailMissing = 0;
  let skippedNoLevels = 0;
  const items = [];

  for (let i = 0; i < toProcess.length; i++) {
    const entry = toProcess[i];
    if (!entry || typeof entry.id !== "number") continue;
    const rid = entry.id;
    const projectLocaId = typeof entry.loca_id === "number" ? entry.loca_id : null;
    const detail = await loadResearchDetailLocal(rid);
    if (!detail) {
      detailMissing += 1;
      if (detailMissing <= 5 || (i + 1) % 200 === 0) {
        console.warn(`  [${i + 1}/${toProcess.length}] rid ${rid}: no detail`);
      }
      continue;
    }
    const levels = buildLevelsFromDetail(detail, {
      rid,
      unmappedByBuffId,
      descriptionByLocaId,
      projectLocaId,
      projectNamesByLocaId,
    });
    if (levels.length === 0) {
      skippedNoLevels += 1;
      continue;
    }
    const name =
      projectLocaId != null ? projectNamesByLocaId.get(projectLocaId) ?? null : null;
    items.push({
      rid,
      name,
      data_version: FROM_UPSTREAM ? "stfcspace-upstream" : "stfcspace-fetch",
      source_note: "data.stfc.space research API",
      levels,
    });
    if ((i + 1) % 200 === 0) console.log(`  Processed ${i + 1}/${toProcess.length} …`);
  }

  items.sort((a, b) => a.rid - b.rid);

  if (DUMP_UNMAPPED) {
    const list = Array.from(unmappedByBuffId.values()).sort((a, b) => b.count - a.count);
    console.log(JSON.stringify({ unmapped_buff_ids: list }, null, 2));
  }

  console.log(
    JSON.stringify(
      {
        summary_total: summaryTotal,
        processed: toProcess.length,
        detail_missing: detailMissing,
        skipped_no_mapped_combat_levels: skippedNoLevels,
        items_emitted: items.length,
      },
      null,
      2
    )
  );

  if (items.length === 0) {
    console.log("No research records with mapped combat buffs; leaving existing catalog unchanged.");
    console.log(
      "Add mappings: data/research/loca_id_to_stat.json, data/research/buff_id_to_stat.json, data/buildings/buff_id_to_stat.json, cache research/*.json, or rely on translations heuristics in this script."
    );
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
