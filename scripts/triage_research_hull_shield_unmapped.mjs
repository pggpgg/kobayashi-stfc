#!/usr/bin/env node
/**
 * List unmapped research buff IDs whose project description infers hull_hp / shield_hp.
 * Usage: node scripts/import_stfcspace_research.mjs --from-upstream --limit 0 --dump-unmapped 2>&1 \
 *   | node scripts/triage_research_hull_shield_unmapped.mjs [--json]
 */

import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { inferCombatStatFromDescription } from "./lib/research_stat_inference.mjs";

const REPO_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const RESEARCH_DIR = path.join(REPO_ROOT, "data", "upstream", "data-stfc-space", "research");
const TRANSLATIONS = path.join(REPO_ROOT, "data", "upstream", "data-stfc-space", "translations-research.json");

function extractUnmappedJson(text) {
  const key = '"unmapped_buff_ids"';
  const i = text.indexOf(`{${key}`) >= 0 ? text.indexOf(`{${key}`) : text.indexOf(`{\n  ${key}`);
  if (i < 0) throw new Error("unmapped_buff_ids JSON not found on stdin");
  let depth = 0;
  for (let j = i; j < text.length; j++) {
    if (text[j] === "{") depth++;
    else if (text[j] === "}") {
      depth--;
      if (depth === 0) return JSON.parse(text.slice(i, j + 1));
    }
  }
  throw new Error("unmapped_buff_ids JSON truncated on stdin");
}

async function main() {
  const asJson = process.argv.includes("--json");
  const chunks = [];
  for await (const c of process.stdin) chunks.push(c);
  const text = Buffer.concat(chunks).toString("utf8");
  const dump = extractUnmappedJson(text);
  const unmapped = dump.unmapped_buff_ids || [];

  const translations = JSON.parse(await fs.readFile(TRANSLATIONS, "utf8"));
  const descByLoca = new Map();
  for (const r of translations) {
    if (r.key === "research_project_description" && typeof r.id === "number") {
      descByLoca.set(r.id, r.text || "");
    }
  }

  const hullShield = [];
  for (const row of unmapped) {
    let desc = row.loca_id != null ? descByLoca.get(row.loca_id) : "";
    if (!desc) {
      for (const rid of row.example_rids || []) {
        try {
          const d = JSON.parse(await fs.readFile(path.join(RESEARCH_DIR, `${rid}.json`), "utf8"));
          desc = descByLoca.get(d.loca_id) || "";
          if (desc) break;
        } catch {
          /* missing upstream file */
        }
      }
    }
    const inferred = inferCombatStatFromDescription(desc);
    const stat = typeof inferred === "string" ? inferred : inferred?.stat;
    if (stat === "hull_hp" || stat === "shield_hp") {
      hullShield.push({
        buff_id: row.buff_id,
        count: row.count,
        stat,
        value_is_percentage: row.value_is_percentage,
        loca_id: row.loca_id,
        description_snippet: String(desc).replace(/<[^>]+>/g, " ").slice(0, 140),
      });
    }
  }
  hullShield.sort((a, b) => b.count - a.count);

  if (asJson) {
    console.log(JSON.stringify({ hull_shield_unmapped: hullShield }, null, 2));
    return;
  }
  console.error(`Hull/shield inferrable but unmapped: ${hullShield.length} buff IDs`);
  for (const r of hullShield.slice(0, 30)) {
    console.error(`  ${r.buff_id} (${r.stat}, count=${r.count}, pct=${r.value_is_percentage}) ${r.description_snippet}`);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
