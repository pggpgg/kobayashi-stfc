#!/usr/bin/env node
/**
 * Propose buff_id → engine stat rows for human review (does not modify repo files).
 * Scans local data/upstream/data-stfc-space/research/*.json and joins translations-research.json.
 *
 * Usage:
 *   node scripts/suggest_research_buff_mappings.mjs [--min-count 2] [--json]
 *
 * Output: CSV (default) or JSON with columns buff_id, loca_id, occurrence_count, example_rid,
 * suggested_stat, description_snippet, project_name_snippet
 *
 * Suggestions mirror the conservative heuristics in import_stfcspace_research.mjs (approximate;
 * always verify before adding to data/research/buff_id_to_stat.json).
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const RESEARCH_DIR = path.join(REPO_ROOT, "data", "upstream", "data-stfc-space", "research");
const TRANSLATIONS_PATH = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
  "translations-research.json"
);

function getArg(name, def) {
  const i = process.argv.indexOf(name);
  if (i === -1) return def;
  const v = process.argv[i + 1];
  return v === undefined ? def : v;
}

function inferFromDescription(text) {
  if (!text || typeof text !== "string") return null;
  const t = text.toLowerCase();
  if (
    /construction speed|build speed|repair speed|research speed|mining\b|cargo |cost efficiency|unlock|blueprint|for components|foundry|away team|defense platform|survey ship|warp speed|tiering|protected cargo|rewards for defeating|not_convert/.test(
      t
    )
  ) {
    return null;
  }
  if (/\bisolytic\b/.test(t)) {
    if (/\b(defense|defence|resist)\b/.test(t)) return "isolytic_defense";
    if (/\b(damage|attack|potency|offense)\b/.test(t)) return "isolytic_damage";
    return null;
  }
  if (/damage reduction|incoming damage|less damage from/.test(t)) return "damage_reduction";
  if (
    /\baccuracy\b/.test(t) &&
    /\b(increased|improved|bonus|enhanced)\b/.test(t) &&
    !/\bofficer\b/.test(t)
  ) {
    return "accuracy";
  }
  if (/critical damage|crit damage|critical hit damage/.test(t)) return "crit_damage";
  if (/critical hit chance|critical chance|crit chance/.test(t)) return "crit_chance";
  if (/\btargeting array\b/.test(t)) return "accuracy";
  if (/shield piercing|armor piercing|shield penetration|shield pen\b|piercing against/.test(t)) {
    return "pierce";
  }
  if (/shield mitigation/.test(t)) return "shield_mitigation";
  if (/\bdodge\b|shield deflection/.test(t)) return "dodge";
  if (/\barmor\b/.test(t) && !/piercing|pierce/.test(t) && /ship|hull|all ships/.test(t)) {
    return "armor";
  }
  if (/hull health|hull hit points|max hull/.test(t) && /all ships|your ships|ship/.test(t)) {
    return "hull_hp";
  }
  if (/shield health|shield capacity|max shield/.test(t) && /all ships|your ships|ship/.test(t)) {
    return "shield_hp";
  }
  if (
    /weapon damage|base damage dealt|damage dealt to hostile|increases base damage/.test(t) &&
    !/defense platform|station/.test(t)
  ) {
    return "weapon_damage";
  }
  return null;
}

function inferFromName(name) {
  if (!name || typeof name !== "string") return null;
  const t = name.toLowerCase();
  if (
    /construction|mining|cargo|repair speed|research speed|warp speed|cost efficiency|unlock|survey|tiering|blueprint|building|module|resource|components\b/.test(
      t
    )
  ) {
    return null;
  }
  if (/\bisolytic\b/.test(t)) {
    if (/\b(defense|defence|resist)\b/.test(t)) return "isolytic_defense";
    return "isolytic_damage";
  }
  if (/damage reduction|critical damage reduction/.test(t)) return "damage_reduction";
  if (/critical damage|crit damage/.test(t)) return "crit_damage";
  if (/critical chance|crit chance/.test(t)) return "crit_chance";
  if (/\btargeting array\b/.test(t)) return "accuracy";
  if (/shield piercing|armor piercing|penetration/.test(t)) return "pierce";
  if (/shield mitigation/.test(t)) return "shield_mitigation";
  if (/\bdodge\b|deflection/.test(t)) return "dodge";
  if (/\barmor\b/.test(t) && !/piercing|pierce/.test(t)) return "armor";
  if (/hull density|hull health|max hull/.test(t)) return "hull_hp";
  if (/shield health|shield capacity|shield hardening/.test(t)) return "shield_hp";
  if (
    /weapon|damage|tactics|assault|offense|firepower|battleship|interceptor|explorer|starship/.test(
      t
    )
  ) {
    if (!/defense platform|station defense/.test(t)) return "weapon_damage";
  }
  return null;
}

async function main() {
  const minCount = Math.max(1, parseInt(getArg("--min-count", "1"), 10));
  const asJson = process.argv.includes("--json");

  const rawT = await fs.readFile(TRANSLATIONS_PATH, "utf8");
  const rows = JSON.parse(rawT);
  const descById = new Map();
  const nameById = new Map();
  for (const r of rows) {
    if (typeof r.id !== "number") continue;
    if (r.key === "research_project_description" && typeof r.text === "string") {
      descById.set(r.id, r.text);
    }
    if (r.key === "research_project_name" && typeof r.text === "string") {
      nameById.set(r.id, r.text);
    }
  }

  const agg = new Map();
  let files;
  try {
    files = await fs.readdir(RESEARCH_DIR);
  } catch (e) {
    console.error(`No research cache at ${RESEARCH_DIR}: ${e.message}`);
    process.exit(1);
  }

  for (const f of files) {
    if (!f.endsWith(".json")) continue;
    const rid = parseInt(f.replace(/\.json$/, ""), 10);
    if (Number.isNaN(rid)) continue;
    let detail;
    try {
      detail = JSON.parse(await fs.readFile(path.join(RESEARCH_DIR, f), "utf8"));
    } catch {
      continue;
    }
    const projectLoca = typeof detail.loca_id === "number" ? detail.loca_id : null;
    for (const buff of detail.buffs || []) {
      if (typeof buff.id !== "number") continue;
      const key = String(buff.id);
      const loca = typeof buff.loca_id === "number" ? buff.loca_id : null;
      const cur = agg.get(key) ?? {
        buff_id: buff.id,
        loca_id: loca,
        count: 0,
        example_rid: rid,
        project_loca: projectLoca,
      };
      cur.count += 1;
      agg.set(key, cur);
    }
  }

  const suggestions = [];
  for (const row of agg.values()) {
    if (row.count < minCount) continue;
    const desc =
      (row.loca_id != null ? descById.get(row.loca_id) : null) ??
      (row.project_loca != null ? descById.get(row.project_loca) : null) ??
      "";
    const pname =
      (row.loca_id != null ? nameById.get(row.loca_id) : null) ??
      (row.project_loca != null ? nameById.get(row.project_loca) : null) ??
      "";
    const suggested = inferFromDescription(desc) || inferFromName(pname);
    if (!suggested) continue;
    suggestions.push({
      buff_id: row.buff_id,
      loca_id: row.loca_id,
      occurrence_count: row.count,
      example_rid: row.example_rid,
      suggested_stat: suggested,
      description_snippet: desc.slice(0, 140),
      project_name_snippet: pname.slice(0, 80),
    });
  }

  suggestions.sort((a, b) => b.occurrence_count - a.occurrence_count);

  if (asJson) {
    console.log(JSON.stringify({ suggestions }, null, 2));
    return;
  }

  console.log(
    "buff_id,loca_id,occurrence_count,example_rid,suggested_stat,description_snippet,project_name_snippet"
  );
  for (const s of suggestions) {
    const esc = (x) => `"${String(x).replace(/"/g, '""')}"`;
    console.log(
      [
        s.buff_id,
        s.loca_id ?? "",
        s.occurrence_count,
        s.example_rid,
        s.suggested_stat,
        esc(s.description_snippet),
        esc(s.project_name_snippet),
      ].join(",")
    );
  }
  console.error(`\n${suggestions.length} rows (min count ${minCount}); verify before editing buff_id_to_stat.json`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
