#!/usr/bin/env node
/**
 * Copy hull_hp / shield_hp entries from data/buildings/buff_id_to_stat.json into
 * data/research/buff_id_to_stat.json when the buff id appears in cached research upstream.
 *
 * Research import already falls back to the buildings map; this makes mappings explicit
 * for Track D maintainability.
 *
 * Usage: node scripts/gen_research_hull_shield_building_buff_patch.mjs [--dry-run]
 */

import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const BUILDING_BUFF = path.join(REPO_ROOT, "data", "buildings", "buff_id_to_stat.json");
const RESEARCH_BUFF = path.join(REPO_ROOT, "data", "research", "buff_id_to_stat.json");
const UPSTREAM_DIR = path.join(REPO_ROOT, "data", "upstream", "data-stfc-space", "research");

const dryRun = process.argv.includes("--dry-run");

async function collectResearchBuffIds() {
  const ids = new Set();
  let files;
  try {
    files = await fs.readdir(UPSTREAM_DIR);
  } catch {
    return ids;
  }
  for (const f of files) {
    if (!f.endsWith(".json")) continue;
    try {
      const d = JSON.parse(await fs.readFile(path.join(UPSTREAM_DIR, f), "utf8"));
      for (const b of d.buffs || []) {
        if (typeof b?.id === "number") ids.add(String(b.id));
      }
    } catch {
      /* skip */
    }
  }
  return ids;
}

async function main() {
  const building = JSON.parse(await fs.readFile(BUILDING_BUFF, "utf8"));
  const research = JSON.parse(await fs.readFile(RESEARCH_BUFF, "utf8"));
  const inResearch = await collectResearchBuffIds();

  const patch = {};
  for (const [id, stat] of Object.entries(building)) {
    if (stat !== "hull_hp" && stat !== "shield_hp") continue;
    if (!inResearch.has(id)) continue;
    if (research[id] != null) continue;
    patch[id] = { stat, operator: "add" };
  }

  const keys = Object.keys(patch);
  if (keys.length === 0) {
    console.error("No new hull/shield building buff ids to merge into research map.");
    return;
  }

  if (dryRun) {
    console.log(JSON.stringify(patch, null, 2));
    console.error(`Would merge ${keys.length} keys (--dry-run).`);
    return;
  }

  const merged = { ...research, ...patch };
  const sorted = Object.fromEntries(
    Object.entries(merged).sort(([a], [b]) => Number(a) - Number(b)),
  );
  await fs.writeFile(RESEARCH_BUFF, `${JSON.stringify(sorted, null, 2)}\n`, "utf8");
  console.error(`Merged ${keys.length} hull/shield buff ids into data/research/buff_id_to_stat.json`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
