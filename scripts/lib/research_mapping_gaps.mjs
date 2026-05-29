/**
 * Research mapping gap scan: unmapped upstream buff ids and catalog rows mapped globally
 * despite scoped description text (armada / PvP / station / ship-specific).
 */

import fs from "node:fs/promises";
import path from "node:path";

import { resolveBuffStatMappings } from "./research_buff_resolve.mjs";
import {
  categorizeResearchDescription,
  isSuspectGlobalScopeCategory,
} from "./research_scope_categorize.mjs";

/**
 * @param {string} repoRoot
 */
export function researchDataPaths(repoRoot) {
  const upstream = path.join(repoRoot, "data", "upstream", "data-stfc-space");
  return {
    catalog: path.join(repoRoot, "data", "research_catalog.json"),
    baseline: path.join(repoRoot, "data", "research", "mapping_gaps_baseline.json"),
    summary: path.join(upstream, "summary-research.json"),
    researchDir: path.join(upstream, "research"),
    translations: path.join(upstream, "translations-research.json"),
    buildingsBuffMap: path.join(repoRoot, "data", "buildings", "buff_id_to_stat.json"),
    researchBuffMap: path.join(repoRoot, "data", "research", "buff_id_to_stat.json"),
    researchLocaMap: path.join(repoRoot, "data", "research", "loca_id_to_stat.json"),
  };
}

async function readJsonObject(filePath) {
  try {
    const raw = await fs.readFile(filePath, "utf8");
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

async function loadResearchDescriptionsByLocaId(translationsPath) {
  const map = new Map();
  try {
    const rows = JSON.parse(await fs.readFile(translationsPath, "utf8"));
    for (const r of rows) {
      if (r.key === "research_project_description" && typeof r.id === "number") {
        map.set(r.id, r.text || "");
      }
    }
  } catch {
    // optional
  }
  return map;
}

async function loadResearchProjectNames(translationsPath) {
  const map = new Map();
  try {
    const rows = JSON.parse(await fs.readFile(translationsPath, "utf8"));
    for (const r of rows) {
      if (r.key === "research_project_name" && typeof r.id === "number") {
        map.set(r.id, r.text || "");
      }
    }
  } catch {
    // optional
  }
  return map;
}

function makeBuffResolveCtx(descriptionByLocaId, projectNamesByLocaId, maps) {
  return {
    researchBuffById: maps.researchBuffById,
    commonBuffNormalization: maps.commonBuffNormalization,
    locaIdToStat: maps.locaIdToStat,
    descriptionByLocaId,
    projectNamesByLocaId,
  };
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

function bonusIsConditional(bonus) {
  return !!(
    bonus.defender_ship_class ||
    bonus.defender_faction ||
    bonus.attacker_faction ||
    (bonus.attacker_factions || []).length ||
    bonus.requires_morale ||
    bonus.requires_defender_burning ||
    bonus.requires_defender_hull_breach
  );
}

function descriptionForScopeCheck({ projectLocaId, buffLocaId, descriptionByLocaId }) {
  if (typeof buffLocaId === "number") {
    const t = descriptionByLocaId.get(buffLocaId);
    if (t && t.trim()) return t;
  }
  if (typeof projectLocaId === "number") {
    return descriptionByLocaId.get(projectLocaId) || "";
  }
  return "";
}

/**
 * @param {string} repoRoot
 * @returns {Promise<{ unmapped_buff_ids: object[], suspect_global_scopes: object[], summary: object }>}
 */
export async function scanResearchMappingGaps(repoRoot) {
  const paths = researchDataPaths(repoRoot);
  const [summaryRaw, catalogRaw, maps] = await Promise.all([
    fs.readFile(paths.summary, "utf8").catch(() => "[]"),
    fs.readFile(paths.catalog, "utf8").catch(() => null),
    Promise.all([
      readJsonObject(paths.buildingsBuffMap),
      readJsonObject(paths.researchBuffMap),
      readJsonObject(paths.researchLocaMap),
    ]).then(([commonBuffNormalization, researchBuffById, locaIdToStat]) => ({
      commonBuffNormalization,
      researchBuffById,
      locaIdToStat,
    })),
  ]);

  const summary = JSON.parse(summaryRaw);
  const descriptionByLocaId = await loadResearchDescriptionsByLocaId(paths.translations);
  const projectNamesByLocaId = await loadResearchProjectNames(paths.translations);
  const buffResolveCtx = makeBuffResolveCtx(descriptionByLocaId, projectNamesByLocaId, maps);

  const locaByRid = new Map();
  for (const entry of summary) {
    if (entry && typeof entry.id === "number") {
      locaByRid.set(entry.id, typeof entry.loca_id === "number" ? entry.loca_id : null);
    }
  }

  const unmappedByBuffId = new Map();
  let upstreamDetailFiles = 0;
  let upstreamDetailMissing = 0;

  for (const entry of summary) {
    if (!entry || typeof entry.id !== "number") continue;
    const rid = entry.id;
    const projectLocaId = typeof entry.loca_id === "number" ? entry.loca_id : null;
    const detailPath = path.join(paths.researchDir, `${rid}.json`);
    let detail;
    try {
      detail = JSON.parse(await fs.readFile(detailPath, "utf8"));
      upstreamDetailFiles += 1;
    } catch {
      upstreamDetailMissing += 1;
      continue;
    }
    if (!detail || !Array.isArray(detail.buffs)) continue;
    for (const buff of detail.buffs) {
      const mappings = resolveBuffStatMappings(buffResolveCtx, buff, projectLocaId);
      if (mappings.length === 0) {
        addUnmapped(unmappedByBuffId, rid, buff);
      }
    }
  }

  const unmapped_buff_ids = Array.from(unmappedByBuffId.values()).sort((a, b) => b.count - a.count);
  for (const row of unmapped_buff_ids) {
    row.category = categorizeResearchDescription(
      row.loca_id != null ? descriptionByLocaId.get(row.loca_id) || "" : ""
    );
  }

  const suspect_global_scopes = [];
  if (catalogRaw) {
    const catalog = JSON.parse(catalogRaw);
    const buffLocaByRid = new Map();
    for (const entry of summary) {
      if (!entry || typeof entry.id !== "number") continue;
      const detailPath = path.join(paths.researchDir, `${entry.id}.json`);
      try {
        const detail = JSON.parse(await fs.readFile(detailPath, "utf8"));
        if (detail && Array.isArray(detail.buffs) && detail.buffs.length > 0) {
          buffLocaByRid.set(entry.id, detail.buffs[0]?.loca_id ?? null);
        }
      } catch {
        // skip
      }
    }

    for (const rec of catalog.items || []) {
      const projectLocaId = locaByRid.get(rec.rid) ?? null;
      const buffLocaId = buffLocaByRid.get(rec.rid) ?? null;
      const desc = descriptionForScopeCheck({
        projectLocaId,
        buffLocaId,
        descriptionByLocaId,
      });
      const category = categorizeResearchDescription(desc);
      if (!isSuspectGlobalScopeCategory(category)) continue;

      let flagged = false;
      for (const lvl of rec.levels || []) {
        for (const bonus of lvl.bonuses || []) {
          if (bonusIsConditional(bonus)) continue;
          suspect_global_scopes.push({
            rid: rec.rid,
            name: rec.name ?? null,
            stat: bonus.stat,
            level: lvl.level,
            scope_category: category,
            description_snippet: String(desc).replace(/\s+/g, " ").slice(0, 160),
          });
          flagged = true;
          break;
        }
        if (flagged) break;
      }
    }
    suspect_global_scopes.sort((a, b) => a.rid - b.rid);
  }

  return {
    unmapped_buff_ids,
    suspect_global_scopes,
    summary: {
      upstream_summary_rows: summary.length,
      upstream_detail_files: upstreamDetailFiles,
      upstream_detail_missing: upstreamDetailMissing,
      unmapped_buff_id_count: unmapped_buff_ids.length,
      suspect_global_scope_count: suspect_global_scopes.length,
      catalog_projects: catalogRaw ? JSON.parse(catalogRaw).items?.length ?? 0 : 0,
    },
  };
}

/**
 * @param {string} repoRoot
 */
export async function loadResearchMappingGapsBaseline(repoRoot) {
  const p = researchDataPaths(repoRoot).baseline;
  try {
    return JSON.parse(await fs.readFile(p, "utf8"));
  } catch {
    return null;
  }
}
