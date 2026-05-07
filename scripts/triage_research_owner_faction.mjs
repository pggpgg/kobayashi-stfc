#!/usr/bin/env node
/**
 * Triage research combat buffs for possible `attacker_faction` (player hull) gating vs
 * defender/opponent wording — using buff_id_to_semantics notes + research_project_* loca strings.
 *
 * Does not modify repo files. Human review required before changing buff_id_to_stat.json.
 *
 * Usage:
 *   node scripts/triage_research_owner_faction.mjs [--json] [--min-score 1] [--skip-economy]
 *
 *   --skip-economy   Drop rows whose description looks like repair/component/tritanium economy (not combat stats).
 *
 * Buckets (heuristic):
 *   likely_owner     — text suggests bonus applies to your Federation/Klingon/Romulan hull
 *   likely_defender  — text suggests vs / against that faction (target), armada defects, contra-*, etc.
 *   name_only        — project name mentions a faction but description did not match owner/defender patterns
 *   unclear          — faction words present but both or neither pattern matched
 *   no_faction_kw    — no major faction keywords in combined text
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
const SEMANTICS_PATH = path.join(REPO_ROOT, "data", "buildings", "buff_id_to_semantics.json");
const RESEARCH_BUFF_MAP = path.join(REPO_ROOT, "data", "research", "buff_id_to_stat.json");

function getArg(name, def) {
  const i = process.argv.indexOf(name);
  if (i === -1) return def;
  const v = process.argv[i + 1];
  return v === undefined ? def : v;
}

/** Non-combat research lines (still faction-flavored) — hide from owner/defender triage when --skip-economy */
function looksEconomyOnly(description, projectName) {
  const t = `${description || ""}\n${projectName || ""}`.toLowerCase();
  return (
    /cost efficiency|for components|parts for components|ship repairs repair|tritanium|dilithium|Σ-tritanium|Σ-dilithium|pure crystal|pure gas|mining\b|cargo |research speed|unlock|blueprint/.test(
      t
    ) && !/\b(weapon damage|piercing|mitigation|shield deflection|critical|crit |damage against|armor\b|dodge\b)/i.test(
      description || ""
    )
  );
}

/** Load research translations into maps by loca id */
async function loadTranslations() {
  const nameById = new Map();
  const descById = new Map();
  try {
    const raw = await fs.readFile(TRANSLATIONS_PATH, "utf8");
    const rows = JSON.parse(raw);
    if (!Array.isArray(rows)) return { nameById, descById };
    for (const r of rows) {
      if (!r || typeof r.id !== "number" || typeof r.text !== "string") continue;
      if (r.key === "research_project_name") nameById.set(r.id, r.text);
      if (r.key === "research_project_description") descById.set(r.id, r.text);
    }
  } catch {
    // optional
  }
  return { nameById, descById };
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

function semanticsNotes(semantics, buffId) {
  const row = semantics[String(buffId)];
  if (!row) return "";
  if (typeof row.notes === "string") return row.notes;
  return "";
}

/**
 * Score owner-hull vs defender/opponent intent from English text (conservative).
 * Returns { ownerScore, defenderScore } integers in [0, ...].
 */
function scoreTexts(projectName, description, semanticsNotesText) {
  const combined = `${projectName || ""}\n${description || ""}\n${semanticsNotesText || ""}`;
  const t = combined.toLowerCase();

  let ownerScore = 0;
  let defenderScore = 0;

  // Strong defender / opponent signals
  const defenderRes = [
    /\b(against|versus|vs\.?)\s+(?:the\s+)?(?:federation|klingon|romulan)\b/,
    /\bwhen\s+(?:attacking|fighting|battling)\s+(?:federation|klingon|romulan)\b/,
    /\bdamage\s+(?:dealt\s+)?to\s+(?:federation|klingon|romulan)\b/,
    /\b(?:federation|klingon|romulan)\s+(?:hostiles?|targets?|ships?\s+in\s+enemy)\b/,
    /\bcontra\s+(?:federation|klingon|romulan)\s/,
    /\barmada\s+defects\b/,
    /\b(?:federation|klingon|romulan)\s+weakpoints\b/,
    /\b(?:federation|klingon|romulan)\s+vulnerabilit(?:y|ies)\b/,
    /\bfederation\s+vulnerabilit/i,
    /\bwhen\s+battling\s+/,
  ];
  for (const re of defenderRes) {
    if (re.test(t)) defenderScore += 2;
  }

  // Strong owner-hull signals (player ship faction)
  const ownerRes = [
    /\b(?:your|for)\s+(?:a\s+)?(?:federation|klingon|romulan)\s+(?:ship|hull|ships|hulls|vessels?)\b/,
    /\b(?:federation|klingon|romulan)\s+(?:ship|hull)s?\s+only\b/,
    /\bwhen\s+(?:using|commanding|flying)\s+(?:a\s+)?(?:federation|klingon|romulan)\b/,
    /\bmodulated\s+federation\b/,
    /\bgated\s+on\s+(?:player\s+)?ship.*faction/i,
    /\bincreases?\s+(?:base\s+)?shield\s+deflection\s+for\s+(?:federation|klingon|romulan)/i,
    /\b(?:pure|only)\s+(?:federation|klingon|romulan)\s+(?:explorer|battleship|interceptor)/i,
  ];
  for (const re of ownerRes) {
    if (re.test(t)) ownerScore += 2;
  }

  // Weaker owner hints (easy false positives — count once total)
  if (
    /\b(?:federation|klingon|romulan)\s+(?:firepower|shields?|shield\s+resonance|demolition|domination|analysis|expansion)\b/i.test(
      t
    ) &&
    defenderScore === 0
  ) {
    ownerScore += 1;
  }

  return { ownerScore, defenderScore, combined };
}

function bucketFromScores(ownerScore, defenderScore, projectName, combined) {
  const nameHas =
    projectName &&
    /\b(federation|klingon|romulan)\b/i.test(projectName);
  const textHas = /\b(federation|klingon|romulan)\b/i.test(combined);

  if (ownerScore >= 2 && defenderScore >= 2) return "unclear";
  if (ownerScore > defenderScore && ownerScore >= 2) return "likely_owner";
  if (defenderScore > ownerScore && defenderScore >= 2) return "likely_defender";
  if (defenderScore > ownerScore && defenderScore === 1 && ownerScore === 0)
    return "likely_defender";
  if (ownerScore > defenderScore && ownerScore === 1 && defenderScore === 0)
    return "likely_owner_weak";

  if (nameHas && !/\b(against|versus|vs\.|battling|hostiles?|defects|weakpoints|vulnerabilit|contra)\b/i.test(combined)) {
    return "name_only";
  }
  if (textHas && ownerScore === 0 && defenderScore === 0) return "unclear";
  return "no_faction_kw";
}

async function main() {
  const asJson = process.argv.includes("--json");
  const skipEconomy = process.argv.includes("--skip-economy");
  const minScore = Math.max(0, parseInt(getArg("--min-score", "1"), 10) || 1);

  const [{ nameById, descById }, semanticsRaw, buffMapRaw] = await Promise.all([
    loadTranslations(),
    fs.readFile(SEMANTICS_PATH, "utf8").catch(() => "{}"),
    fs.readFile(RESEARCH_BUFF_MAP, "utf8").catch(() => "{}"),
  ]);
  const semantics = JSON.parse(semanticsRaw);
  const buffMap = JSON.parse(buffMapRaw);

  const files = (await fs.readdir(RESEARCH_DIR)).filter((f) => f.endsWith(".json"));
  const rows = [];
  const buffAgg = new Map(); // buff_id -> { count, buckets, example_rid }

  for (const file of files) {
    const rid = parseInt(file.replace(/\.json$/, ""), 10);
    if (Number.isNaN(rid)) continue;

    let detail;
    try {
      detail = JSON.parse(await fs.readFile(path.join(RESEARCH_DIR, file), "utf8"));
    } catch {
      continue;
    }
    const locaId = typeof detail.loca_id === "number" ? detail.loca_id : null;
    const projectName = locaId != null ? nameById.get(locaId) ?? "" : "";
    const description = locaId != null ? descById.get(locaId) ?? "" : "";

    if (!Array.isArray(detail.buffs)) continue;

    for (const buff of detail.buffs) {
      if (typeof buff.id !== "number") continue;
      const buffId = buff.id;
      const notes = semanticsNotes(semantics, buffId);
      const mapEntry = buffMap[String(buffId)];
      const alreadyGated = mappingHasAttackerFaction(
        typeof mapEntry === "object" && mapEntry !== null && !Array.isArray(mapEntry)
          ? mapEntry
          : null
      );

      const { ownerScore, defenderScore, combined } = scoreTexts(
        projectName,
        description,
        notes
      );
      const bucket = bucketFromScores(ownerScore, defenderScore, projectName, combined);

      const row = {
        rid,
        buff_id: buffId,
        project_name: projectName || null,
        description_preview:
          description.length > 220 ? `${description.slice(0, 217)}…` : description || null,
        semantics_preview:
          notes.length > 160 ? `${notes.slice(0, 157)}…` : notes || null,
        owner_score: ownerScore,
        defender_score: defenderScore,
        bucket,
        mapping_has_attacker_faction: alreadyGated,
      };

      const interesting =
        alreadyGated ||
        bucket === "likely_owner" ||
        bucket === "likely_defender" ||
        bucket === "likely_owner_weak" ||
        bucket === "unclear" ||
        (bucket === "name_only" && /\b(federation|klingon|romulan)\b/i.test(projectName || ""));

      if (!interesting) continue;

      if (
        skipEconomy &&
        looksEconomyOnly(description, projectName) &&
        !alreadyGated &&
        bucket !== "likely_defender"
      ) {
        continue;
      }

      rows.push(row);

      const key = String(buffId);
      const cur = buffAgg.get(key) ?? {
        buff_id: buffId,
        count: 0,
        buckets: new Set(),
        example_rid: rid,
      };
      cur.count += 1;
      cur.buckets.add(bucket);
      buffAgg.set(key, cur);
    }
  }

  rows.sort((a, b) => {
    const bk = a.bucket.localeCompare(b.bucket);
    if (bk !== 0) return bk;
    return a.rid - b.rid || a.buff_id - b.buff_id;
  });

  const summary = {};
  for (const r of rows) {
    summary[r.bucket] = (summary[r.bucket] || 0) + 1;
  }

  const filteredRows =
    minScore <= 1
      ? rows
      : rows.filter((r) => r.owner_score >= minScore || r.defender_score >= minScore);

  const out = {
    meta: {
      research_files_scanned: files.length,
      translation_rows_loaded: nameById.size + descById.size > 0,
      rows_emitted: filteredRows.length,
      buckets: summary,
    },
    by_buff_id: [...buffAgg.values()].sort((a, b) => b.count - a.count),
    rows: filteredRows,
  };

  if (asJson) {
    console.log(JSON.stringify(out, null, 2));
    return;
  }

  console.log(`Scanned ${files.length} research detail JSON files.`);
  console.log(`Triage rows (interesting buckets only): ${filteredRows.length}`);
  console.log("Bucket counts:", JSON.stringify(summary));
  console.log("");
  console.log("Top buff_ids by occurrence in triage set:");
  for (const b of out.by_buff_id.slice(0, 25)) {
    console.log(
      `  buff ${b.buff_id}  ×${b.count}  buckets=[${[...b.buckets].join(", ")}]  e.g. rid=${b.example_rid}`
    );
  }
  console.log("");
  for (const r of filteredRows) {
    if (r.mapping_has_attacker_faction) continue;
    const flag =
      r.bucket === "likely_owner" || r.bucket === "likely_owner_weak" ? " <<< review owner gate"
      : r.bucket === "likely_defender" ? " (vs opponent / not owner gate)"
      : "";
    console.log(
      `[${r.bucket}] rid=${r.rid} buff=${r.buff_id} own=${r.owner_score} def=${r.defender_score}${flag}`
    );
    if (r.project_name) console.log(`  name: ${r.project_name}`);
    if (r.semantics_preview) console.log(`  semantics: ${r.semantics_preview}`);
    if (r.description_preview)
      console.log(`  desc: ${r.description_preview.replace(/\s+/g, " ").trim()}`);
    console.log("");
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
