/**
 * Merge `attacker_faction` / `attacker_factions` / `defender_faction` into `data/research/buff_id_to_stat.json`
 * for combat research lines surfaced by triage (`node scripts/triage_research_owner_faction.mjs --skip-economy`).
 *
 * Stat targets are resolved the same way as `import_stfcspace_research.mjs` (maps + loca + description/name inference).
 * Human review remains appropriate for dual-gated lines (Fed hull vs Klingon, etc.).
 *
 * Usage (repo root):
 *   node scripts/gen_research_faction_buff_patch.mjs [--dry-run] [--force-all]
 *
 *   --dry-run     print JSON patch keys only (stderr: counts)
 *   --force-all   also rewrite entries that already declare `attacker_faction` in the mapping JSON
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

import { resolveBuffStatMappings } from "./lib/research_buff_resolve.mjs";

const REPO_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const UPSTREAM_RESEARCH_DIR = path.join(REPO_ROOT, "data", "upstream", "data-stfc-space", "research");
const TRANSLATIONS_RESEARCH_PATH = path.join(REPO_ROOT, "data", "upstream", "data-stfc-space", "translations-research.json");
const BUFF_ID_TO_STAT_PATH = path.join(REPO_ROOT, "data", "buildings", "buff_id_to_stat.json");
const RESEARCH_BUFF_ID_TO_STAT_PATH = path.join(REPO_ROOT, "data", "research", "buff_id_to_stat.json");
const RESEARCH_LOCA_ID_TO_STAT_PATH = path.join(REPO_ROOT, "data", "research", "loca_id_to_stat.json");
const TRIAGE_SCRIPT = path.join(REPO_ROOT, "scripts", "triage_research_owner_faction.mjs");

/** Match `ALLOWED_COMBAT_STATS` in import_stfcspace_research.mjs */
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
  "isolytic_damage_morale",
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

const RESEARCH_BUFF_MAPPING = {};

function stripHtml(s) {
  return String(s || "").replace(/<[^>]+>/g, " ").trim();
}

function fk(t) {
  const m = /\b(federation|klingon|romulan)\b/i.exec(t || "");
  return m ? m[1].toLowerCase() : null;
}

function inferDefender(name, descFull, bucket) {
  const tx = `${stripHtml(name)} ${stripHtml(descFull)}`.toLowerCase();
  if (tx.includes("neutral zone")) return null;
  let m = tx.match(/\b(?:against|versus|vs\.?)\s+(?:the\s+|all\s+)?(federation|klingon|romulan)\b/);
  if (m) return m[1];
  m = tx.match(/defense\s+platforms?\b.*?\bagainst\s+(?:all\s+)?(federation|klingon|romulan)\b/);
  if (m) return m[1];
  if (/\bklingon\s+a?rmadas?\b|\bagainst\s+klingon\s+a?rmadas?/i.test(tx)) return "klingon";
  if (/\bfederation\s+a?rmadas?\b|\bagainst\s+federation\s+a?rmadas?/i.test(tx)) return "federation";
  if (/\bromulan\s+a?rmadas?\b|\bagainst\s+romulan\s+a?rmadas?/i.test(tx)) return "romulan";
  m = tx.match(/\bcontra\s+(federation|klingon|romulan)\b/i);
  if (m) return m[1].toLowerCase();
  if (bucket === "likely_defender") return fk(descFull) || fk(name);
  return null;
}

function inferOwner(name, descFull) {
  const tx = `${stripHtml(name)} ${stripHtml(descFull)}`.toLowerCase();
  if (/\b(cost efficiency|parts for components|reputation|daily.*bundle|faction store)\b/.test(tx)) {
    return { atk: null, atks: null };
  }
  if (
    /\bfederation\b.*\bklingon\b.*\bromulan\b.*\bships?\b/.test(tx) &&
    !tx.split("federation")[0].includes("against")
  ) {
    return { atk: null, atks: ["federation", "klingon", "romulan"] };
  }
  let m = tx.match(/\b(?:for all|increases[^\n]{0,120})\s+(federation|klingon|romulan)\s+ships\b/);
  if (m) {
    if (/\bfederation\s+ships\s+against\s+klingon|\bklingon\s+ships\s+against\s+federation\b/i.test(tx)) {
      return { atk: null, atks: null };
    }
    return { atk: m[1], atks: null };
  }
  m = tx.match(/\bbase dodge\b[^\n]{0,80}?(?:for all\s+)?(federation|klingon|romulan)\s+ships\b/);
  if (m) return { atk: m[1], atks: null };
  return { atk: null, atks: null };
}

/** @returns {Record<string, string|string[]>|null} */
function deriveFactionSpec(bucket, projectName, descFull) {
  const name = projectName || "";
  let defender = null;
  let atk = null;
  let atks = null;

  if (bucket === "likely_defender") {
    defender = inferDefender(name, descFull, bucket);
  } else if (bucket === "likely_owner" || bucket === "likely_owner_weak") {
    ({ atk, atks } = inferOwner(name, descFull));
  } else if (bucket === "unclear") {
    defender = inferDefender(name, descFull, bucket);
    if (!defender) ({ atk, atks } = inferOwner(name, descFull));
  } else if (bucket === "name_only") {
    ({ atk, atks } = inferOwner(name, descFull));
    if (!atk && !atks) {
      const d = fk(name.trim());
      if (d && inferDefender(name, descFull, "likely_defender") == null) atk = d;
    }
  } else {
    return null;
  }

  if (defender) return { defender_faction: defender };
  if (atks && atks.length) return { attacker_factions: atks };
  if (atk) return { attacker_faction: atk };
  return null;
}

function mappingHasAttackerFaction(entry) {
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
    return false;
  }
  if (typeof entry.attacker_faction === "string" && entry.attacker_faction.length > 0) {
    return true;
  }
  return Array.isArray(entry.attacker_factions) && entry.attacker_factions.some((x) => typeof x === "string" && x.trim());
}

function stripFactionKeys(e) {
  const {
    attacker_faction: _a,
    attacker_factions: _af,
    defender_faction: _d,
    ...rest
  } = e;
  return rest;
}

function encodeBuffMappingValue(entries) {
  const cleaned = entries.map((o) => {
    const x = { ...o };
    for (const k of Object.keys(x)) {
      if (x[k] == null) delete x[k];
    }
    return x;
  });
  if (cleaned.length === 1) {
    const o = cleaned[0];
    const keys = Object.keys(o);
    if (keys.length === 2 && o.stat && o.operator === "add") return o.stat;
    return o;
  }
  return cleaned;
}

function loadJsonObject(p) {
  try {
    const raw = fs.readFileSync(p, "utf8");
    const v = JSON.parse(raw);
    if (v && typeof v === "object" && !Array.isArray(v)) return { ...v };
  } catch (_) {
    // ignore
  }
  return {};
}

function loadTranslationMaps() {
  const nameById = new Map();
  const descById = new Map();
  try {
    const rows = JSON.parse(fs.readFileSync(TRANSLATIONS_RESEARCH_PATH, "utf8"));
    if (!Array.isArray(rows)) return { nameById, descById };
    for (const r of rows) {
      if (!r || typeof r.id !== "number" || typeof r.text !== "string") continue;
      if (r.key === "research_project_name") nameById.set(r.id, r.text);
      if (r.key === "research_project_description") descById.set(r.id, r.text);
    }
  } catch (_) {
    // optional
  }
  return { nameById, descById };
}

function runTriageRows() {
  const r = spawnSync(process.execPath, [TRIAGE_SCRIPT, "--json", "--skip-economy"], {
    cwd: REPO_ROOT,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  if (r.status !== 0) {
    throw new Error(r.stderr || `triage exited ${r.status}`);
  }
  const out = JSON.parse(r.stdout);
  return Array.isArray(out.rows) ? out.rows : [];
}

function makeBuffResolveCtx(researchBuffById, commonBuffNormalization, locaIdToStat, nameById, descById) {
  return {
    researchBuffMapping: RESEARCH_BUFF_MAPPING,
    researchBuffById,
    commonBuffNormalization,
    locaIdToStat,
    descriptionByLocaId: descById,
    projectNamesByLocaId: nameById,
  };
}

function main() {
  const dryRun = process.argv.includes("--dry-run");
  const forceAll = process.argv.includes("--force-all");

  const commonBuffNormalization = loadJsonObject(BUFF_ID_TO_STAT_PATH);
  let researchBuffById = loadJsonObject(RESEARCH_BUFF_ID_TO_STAT_PATH);
  const locaIdToStat = loadJsonObject(RESEARCH_LOCA_ID_TO_STAT_PATH);
  const { nameById, descById } = loadTranslationMaps();

  const rows = runTriageRows().sort((a, b) => a.rid - b.rid || a.buff_id - b.buff_id);
  const resolveCtx = makeBuffResolveCtx(
    researchBuffById,
    commonBuffNormalization,
    locaIdToStat,
    nameById,
    descById
  );

  const patch = {};
  for (const row of rows) {
    if (!forceAll && row.mapping_has_attacker_faction) continue;

    const detail = loadDetail(row.rid);
    if (!detail || !Array.isArray(detail.buffs)) continue;
    const lid = typeof detail.loca_id === "number" ? detail.loca_id : null;
    const descFull = lid != null ? descById.get(lid) || "" : "";

    const spec = deriveFactionSpec(row.bucket, row.project_name || "", descFull);
    if (!spec) continue;

    const buff = detail.buffs.find((b) => b && b.id === row.buff_id);
    if (!buff) continue;

    const projectLocaId = lid;
    const entries = resolveBuffStatMappings(resolveCtx, buff, projectLocaId).filter((e) =>
      ALLOWED_COMBAT_STATS.has(e.stat)
    );
    if (entries.length === 0) continue;

    const fused = entries.map((e) => ({
      ...stripFactionKeys(e),
      ...spec,
    }));
    patch[String(row.buff_id)] = encodeBuffMappingValue(fused);
  }

  const merged = { ...researchBuffById, ...patch };
  const sorted = Object.fromEntries(Object.keys(merged).sort((a, b) => Number(a) - Number(b)).map((k) => [k, merged[k]]));

  if (dryRun) {
    console.error(`# patch keys: ${Object.keys(patch).length}; merged keys: ${Object.keys(sorted).length}`);
    console.log(JSON.stringify(patch, null, 2));
    return;
  }

  fs.writeFileSync(RESEARCH_BUFF_ID_TO_STAT_PATH, `${JSON.stringify(sorted, null, 2)}\n`);
  console.error(`Wrote ${RESEARCH_BUFF_ID_TO_STAT_PATH} (${Object.keys(sorted).length} keys, ${Object.keys(patch).length} patched)`);
}

const _detailCache = new Map();

function loadDetail(rid) {
  if (_detailCache.has(rid)) return _detailCache.get(rid);
  const p = path.join(UPSTREAM_RESEARCH_DIR, `${rid}.json`);
  let detail = null;
  try {
    detail = JSON.parse(fs.readFileSync(p, "utf8"));
  } catch (_) {
    detail = null;
  }
  _detailCache.set(rid, detail);
  return detail;
}

main();
