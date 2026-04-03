#!/usr/bin/env node
/**
 * Refresh data.stfc.space JSON caches used when loading the SPA (e.g. /ships/2251018025).
 *
 * Derived from stfc.space production bundle (VITE_API_URL = https://data.stfc.space):
 * - yukiData.fetchData: GET /{entity}/summary.json for catalogs
 * - yukiTranslations.fetchStaticTranslations: GET /translations/{lang}/{category}.json
 * - ItemLoadingWrapper (ship): GET /ship/{id}.json (no snapshot prop on ship route)
 *
 * Usage (repo root):
 *   node scripts/fetch_stfcspace_page_upstream.mjs
 *   node scripts/fetch_stfcspace_page_upstream.mjs --ship-id 2251018025
 *   node scripts/fetch_stfcspace_page_upstream.mjs --summaries-only
 *
 * Output: data/upstream/data-stfc-space/ (summary-*.json, translations-*.json, ships/{id}.json)
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

const ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const OUT = path.join(ROOT, "data", "upstream", "data-stfc-space");
const BASE = "https://data.stfc.space";
const DELAY_MS = 150;

const args = process.argv.slice(2);
const summariesOnly = args.includes("--summaries-only");
const shipIdx = args.indexOf("--ship-id");
const shipId =
  shipIdx !== -1 && args[shipIdx + 1] ? String(args[shipIdx + 1]).trim() : "2251018025";

/** App `fetchData` summary paths (minified client passes "/ship", etc.; URL is /ship/summary.json). */
const SUMMARY_SEGMENTS = [
  ["ship", "summary-ship.json"],
  ["officer", "summary-officer.json"],
  ["building", "summary-building.json"],
  ["research", "summary-research.json"],
  ["system", "summary-system.json"],
  ["hostile", "summary-hostile.json"],
  ["consumable", "summary-consumable.json"],
  ["forbidden_tech", "summary-forbidden_tech.json"],
  ["hazards", "summary-hazards.json"],
  ["wave_defense", "summary-wave_defense.json"],
  ["pvp_bands", "summary-pvp_bands.json"],
  ["mission", "summary-mission.json"],
  ["resource", "summary-ressource.json"],
];

/** `Ht` static translation paths (en); duplicates removed. */
const TRANSLATION_PATHS = [
  "materials",
  "ships",
  "officers",
  "officer_names",
  "officer_buffs",
  "officer_flavor_text",
  "traits",
  "research",
  "starbase_modules",
  "factions",
  "systems",
  "ship_components",
  "blueprints",
  "consumables",
  "mission_titles",
  "navigation",
  "ship_buffs",
  "loyalty",
  "forbidden_tech",
  "event_titles",
  "player_avatars",
  "hud",
];

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function fetchJson(url) {
  const res = await fetch(url, {
    headers: { Accept: "application/json", "User-Agent": "Kobayashi-upstream-fetch/1.0" },
  });
  if (!res.ok) {
    throw new Error(`${url} -> HTTP ${res.status}`);
  }
  return res.json();
}

async function writeJson(relPath, data) {
  const dest = path.join(OUT, relPath);
  await fs.mkdir(path.dirname(dest), { recursive: true });
  await fs.writeFile(dest, `${JSON.stringify(data)}\n`, "utf8");
}

async function main() {
  const jobs = [];

  for (const [seg, filename] of SUMMARY_SEGMENTS) {
    const u = `${BASE}/${seg}/summary.json`;
    jobs.push({ url: u, write: () => fetchJson(u).then((j) => writeJson(filename, j)) });
  }

  if (!summariesOnly) {
    const lang = "en";
    for (const p of TRANSLATION_PATHS) {
      const u = `${BASE}/translations/${lang}/${p}.json`;
      const filename = `translations-${p}.json`;
      jobs.push({ url: u, write: () => fetchJson(u).then((j) => writeJson(filename, j)) });
    }

    const shipUrl = `${BASE}/ship/${shipId}.json`;
    jobs.push({
      url: shipUrl,
      write: () => fetchJson(shipUrl).then((j) => writeJson(path.join("ships", `${shipId}.json`), j)),
    });
  }

  console.log(`Fetching ${jobs.length} JSON resources from ${BASE} …`);
  for (let i = 0; i < jobs.length; i++) {
    const { url, write } = jobs[i];
    process.stdout.write(`[${i + 1}/${jobs.length}] ${url}\n`);
    await write();
    if (i < jobs.length - 1) await sleep(DELAY_MS);
  }

  console.log(`Done. Wrote under ${path.relative(ROOT, OUT)}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
