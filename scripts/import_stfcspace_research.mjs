/**
 * Import research catalog from data.stfc.space for KOBAYASHI.
 * - Reads data/upstream/data-stfc-space/summary-research.json (or fetches research/summary.json).
 * - Loads per-node detail **only** from data/upstream/data-stfc-space/research/{id}.json (no HTTP for details).
 * - Maps buff ids to engine stats: RESEARCH_BUFF_MAPPING, data/research/buff_id_to_stat.json,
 *   data/buildings/buff_id_to_stat.json, data/research/loca_id_to_stat.json, then description heuristics.
 *   JSON values may be a string, one object `{ stat, operator?, … }`, or an **array** of those (one buff → multiple profile stats).
 * - Writes data/research_catalog.json with level_value_mode: "snapshot": values are complete owned-rank bonuses.
 *
 * Usage:
 *   node scripts/import_stfcspace_research.mjs [--from-upstream] [--limit N] [--rid 123,456] [--dump-unmapped]
 *   --from-upstream  use data/upstream/data-stfc-space/summary-research.json instead of fetch for the summary
 *   --limit N        inspect at most N research nodes (default: 0, all); subsets do not replace the catalog
 *   --rid 123,456    only process these rids (comma-separated)
 *   --dump-unmapped  inspect unmapped research buff ids; write only with an explicit --output
 *   --output PATH    explicitly write results to this path (including a requested subset)
 *
 * Full catalog (populate research/*.json first — e.g. scripts/fetch_stfcspace_research.mjs or your bulk job):
 *   node scripts/import_stfcspace_research.mjs --from-upstream --limit 0
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import { resolveBuffStatMappings } from "./lib/research_buff_resolve.mjs";
import { normalizeBonusValue } from "./lib/research_normalize_bonus_value.mjs";
import { researchClientBuffCoverage } from "./lib/research_client_semantics.mjs";
import {
  descriptionForScopeCheck,
  mappingHasCombatConditions,
  shouldExcludeUnconditionalGlobalMerge,
} from "./lib/research_scope_categorize.mjs";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const IS_MAIN = process.argv[1] && path.resolve(process.argv[1]) === url.fileURLToPath(import.meta.url);
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
const CLIENT_SEMANTICS_PATH = path.join(REPO_ROOT, "data", "research", "client_buff_semantics_2026-10-20.json");
const PERCENTAGE_UNIT_QUARANTINE_PATH = path.join(REPO_ROOT, "data", "research", "percentage_unit_quarantine.json");
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
  "officer_defense_below_decks",
  "officer_health",
  "hull_hp",
  "shield_hp",
  "hull_hp_flat",
  "shield_hp_flat",
  "crit_damage_flat",
  "isolytic_damage",
  "isolytic_cascade",
  "isolytic_cascade_damage",
  "isolytic_defense",
  "crit_chance",
  "crit_damage",
  "crit_damage_floor",
  "pierce",
  "shield_mitigation",
  "armor",
  "shield_deflection",
  "dodge",
  "damage_reduction",
  "accuracy",
  "armor_piercing",
  "shield_piercing",
  "base_mitigation_stats_bonus",
  "officer_shield_regen_max_fraction",
  "apex_shred",
  "apex_barrier",
]);

const FROM_UPSTREAM =
  process.argv.includes("--from-upstream") || process.env.USE_UPSTREAM_RESEARCH === "1";
const DUMP_UNMAPPED = process.argv.includes("--dump-unmapped");

if (IS_MAIN && (process.argv.includes("--help") || process.argv.includes("-h"))) {
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
let clientBuffSemantics;

function makeBuffResolveCtx(descriptionByLocaId, projectNamesByLocaId) {
  return {
    researchBuffMapping: RESEARCH_BUFF_MAPPING,
    researchBuffById,
    commonBuffNormalization,
    locaIdToStat,
    descriptionByLocaId,
    projectNamesByLocaId,
    clientBuffSemantics,
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
    show_percentage: null,
    loca_id: null,
    exclusion_reasons: [],
  };
  existing.count += 1;
  if (existing.example_rids.length < 5 && typeof rid === "number") {
    if (!existing.example_rids.includes(rid)) existing.example_rids.push(rid);
  }
  if (existing.value_is_percentage == null && typeof buff?.value_is_percentage === "boolean") {
    existing.value_is_percentage = buff.value_is_percentage;
  }
  if (existing.show_percentage == null && typeof buff?.show_percentage === "boolean") {
    existing.show_percentage = buff.show_percentage;
  }
  if (existing.loca_id == null && typeof buff?.loca_id === "number") {
    existing.loca_id = buff.loca_id;
  }
  unmappedByBuffId.set(key, existing);
}

function addUnmappedReason(unmappedByBuffId, rid, buff, reason) {
  addUnmapped(unmappedByBuffId, rid, buff);
  const row = unmappedByBuffId.get(String(buff.id));
  if (row && !row.exclusion_reasons.includes(reason)) row.exclusion_reasons.push(reason);
}

export function buildLevelsFromDetail(detail, opts) {
  if (!detail || !Array.isArray(detail.buffs)) return [];

  const { rid, unmappedByBuffId, descriptionByLocaId, projectLocaId, projectNamesByLocaId } = opts;
  const buffResolveCtx = {
    ...(opts.buffResolveCtx ?? makeBuffResolveCtx(descriptionByLocaId, projectNamesByLocaId)),
    researchProjectId: rid ?? detail.id,
    researchDetailId: detail.id,
  };

  if (unmappedByBuffId) {
    for (const buff of detail.buffs) {
      const mappings = resolveBuffStatMappings(buffResolveCtx, buff, projectLocaId);
      if (mappings.length === 0) {
        addUnmapped(unmappedByBuffId, rid, buff);
      } else if (buff.value_is_percentage === true && mappings.some(mapping =>
        !["fraction", "absolute", "percentage_points"].includes(mapping.value_unit))) {
        addUnmappedReason(unmappedByBuffId, rid, buff, "percentage_unit_unverified");
      }
    }
  }

  const buffValuesLens = detail.buffs.map((b) =>
    Array.isArray(b.values) ? b.values.length : 0
  );

  const maxFromLevels = Array.isArray(detail.levels) ? detail.levels.length : 0;
  const maxFromLegacyField = Number(detail.max_level);
  const maxLevel =
    Number.isInteger(maxFromLegacyField) && maxFromLegacyField > 0
      ? maxFromLegacyField
      : maxFromLevels || Math.max(0, ...buffValuesLens);
  if (!Number.isFinite(maxLevel) || maxLevel <= 0) return [];

  const levels = [];
  for (let level = 1; level <= maxLevel; level += 1) {
    const bonuses = [];
    for (const buff of detail.buffs) {
      const mappings = resolveBuffStatMappings(buffResolveCtx, buff, projectLocaId);
      for (const mapping of mappings) {
        if (!ALLOWED_COMBAT_STATS.has(mapping.stat)) {
          continue;
        }
        if (
          shouldExcludeUnconditionalGlobalMerge({
            mapping,
            buff,
            projectLocaId,
            descriptionByLocaId,
          })
        ) {
          continue;
        }
        const values = mapping.client_ranked_values ?? (Array.isArray(buff.values) ? buff.values : []);
        if (values.length === 0) continue;
        // Native rank lookup clamps past a nonempty buff list; the project bounds rank count.
        // Explicit trailing zeroes are still values and must not inherit an earlier bonus.
        const idx = Math.min(level - 1, values.length - 1);
        const raw = mapping.client_ranked_values ? values[idx] : values[idx]?.value;
        if (typeof raw !== "number") continue;
        const value = normalizeBonusValue(buff, mapping, raw);
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
        if (typeof mapping.attacker_ship_class === "string")
          bonus.attacker_ship_class = mapping.attacker_ship_class;
        if (typeof mapping.defender_faction === "string")
          bonus.defender_faction = mapping.defender_faction;
        if (typeof mapping.attacker_faction === "string")
          bonus.attacker_faction = mapping.attacker_faction;
        if (Array.isArray(mapping.attacker_factions) && mapping.attacker_factions.length)
          bonus.attacker_factions = mapping.attacker_factions.map(String).filter(Boolean);
        if (mapping.attacker_min_grade != null)
          bonus.attacker_min_grade = mapping.attacker_min_grade;
        bonuses.push(bonus);
      }
    }
    // Preserve empty snapshots, including a zero final rank, so runtime cannot reuse a prior bonus.
    levels.push({ level, bonuses });
  }
  return levels.some((level) => level.bonuses.length > 0) ? levels : [];
}

async function main() {
  // Required evidence: a missing/broken manifest must not silently revive old
  // percentage guesses or discard unsupported native conditions.
  const semantics = JSON.parse(await fs.readFile(CLIENT_SEMANTICS_PATH, "utf8"));
  if (semantics.schema_version !== 1 || !semantics.buffs || Array.isArray(semantics.buffs)) {
    throw new Error("Invalid client research semantics manifest");
  }
  clientBuffSemantics = semantics.buffs;
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

  const limitText = getArg("--limit", "0");
  if (!/^\d+$/.test(limitText) || !Number.isSafeInteger(Number(limitText))) {
    throw new Error("--limit must be a nonnegative integer (0 means all)");
  }
  const limit = Number(limitText);
  const ridArg = getArg("--rid", "");
  if (process.argv.includes("--rid") && !ridArg) throw new Error("--rid requires integer IDs");
  const onlyRids = ridArg
    ? new Set(ridArg.split(",").map((s) => parseInt(s.trim(), 10)).filter((n) => !Number.isNaN(n)))
    : null;
  if (ridArg && !ridArg.split(",").every((value) => /^\d+$/.test(value.trim()) && Number.isSafeInteger(Number(value)))) {
    throw new Error("--rid must contain comma-separated integer IDs");
  }
  const outputArg = getArg("--output", "");
  if (process.argv.includes("--output") && (!outputArg || outputArg.startsWith("--"))) {
    throw new Error("--output requires a path");
  }

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
  const clientCoverageReferences = [];

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
    for (const buff of detail.buffs ?? []) {
      if (Object.hasOwn(clientBuffSemantics, String(buff.id))) {
        clientCoverageReferences.push({ buff_id: buff.id, rid,
          name: projectLocaId != null ? projectNamesByLocaId.get(projectLocaId) ?? null : null });
      }
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
      source_note: "data.stfc.space research API; selected HP/weapon/critical families use source-verified client BuffSpec facts (data/research/client_buff_semantics_2026-10-20.json); unsupported native scope is excluded",
      level_value_mode: "snapshot",
      levels,
    });
    if ((i + 1) % 200 === 0) console.log(`  Processed ${i + 1}/${toProcess.length} …`);
  }

  items.sort((a, b) => a.rid - b.rid);
  const clientCoverage = researchClientBuffCoverage(semantics, clientCoverageReferences);
  const percentageUnitRows = Array.from(unmappedByBuffId.values())
    .filter(row => row.exclusion_reasons.includes("percentage_unit_unverified"))
    .sort((a, b) => a.buff_id - b.buff_id);
  const percentageUnitQuarantine = {
    schema_version: 1,
    policy: "Percentage display flags do not establish the raw numeric unit. A mapped percentage-flagged BuffSpec without an explicit fraction, absolute, or percentage_points unit is omitted until that unit is reviewed.",
    summary: {
      unique_buff_count: percentageUnitRows.length,
      project_buff_instances: percentageUnitRows.reduce((total, row) => total + row.count, 0),
    },
    rows: percentageUnitRows.map(({ buff_id, count, example_rids, value_is_percentage,
      show_percentage, loca_id }) => ({ buff_id, count, example_rids, value_is_percentage,
      show_percentage, loca_id, exclusion_reason: "percentage_unit_unverified" })),
  };

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
        client_stat_buff_coverage: clientCoverage.summary,
        percentage_unit_quarantine: percentageUnitQuarantine.summary,
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
  if (!outputArg && (onlyRids || limit > 0 || DUMP_UNMAPPED)) {
    console.log("Inspection only; existing catalog unchanged. Use --output PATH to save these results explicitly.");
    return;
  }
  const outputPath = outputArg ? path.resolve(outputArg) : OUT_PATH;
  await fs.writeFile(outputPath, JSON.stringify(catalog, null, 2), "utf8");
  if (!outputArg && !onlyRids && limit === 0) {
    await fs.writeFile(path.join(REPO_ROOT, "data", "research", "client_buff_coverage_2026-10-20.json"),
      JSON.stringify(clientCoverage, null, 2) + "\n", "utf8");
    await fs.writeFile(PERCENTAGE_UNIT_QUARANTINE_PATH,
      JSON.stringify(percentageUnitQuarantine, null, 2) + "\n", "utf8");
  }
  console.log(`Wrote ${items.length} research records to ${outputPath}`);
}

if (IS_MAIN) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
