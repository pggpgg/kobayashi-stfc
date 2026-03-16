#!/usr/bin/env node
/**
 * Build hull_id_registry.json: maps STFC game hull_id (from stfc-mod sync) to
 * Kobayashi ship id. Used by the API when owned_only is true so the ship
 * dropdown in Roster mode shows only owned ships.
 *
 * Usage: node scripts/build_hull_id_registry.mjs [--dry-run]
 *
 * Reads:
 *   data/upstream/data-stfc-space/summary-ship.json   (id = hull_id, loca_id)
 *   data/upstream/data-stfc-space/translations-ships.json (key "ship_name", id = loca_id -> text)
 *   data/upstream/data-stfc-space/translations-blueprints.json (fallback: blueprint_description, id = loca_id -> first segment)
 *   data/ships_extended/index.json   (preferred) or data/ships/index.json (legacy fallback if present)
 *     Kobayashi ship list: ships[].id, ships[].ship_name for name -> id
 *
 * Writes:
 *   data/hull_id_registry.json
 *
 * Run from repo root.
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const SUMMARY_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/summary-ship.json");
const TRANSLATIONS_SHIPS_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/translations-ships.json");
const TRANSLATIONS_BLUEPRINTS_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/translations-blueprints.json");
const TRANSLATIONS_SHIP_BUFFS_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/translations-ship_buffs.json");
const EXTENDED_INDEX_PATH = path.join(REPO_ROOT, "data/ships_extended/index.json");
const LEGACY_INDEX_PATH = path.join(REPO_ROOT, "data/ships/index.json");
const REGISTRY_PATH = path.join(REPO_ROOT, "data/hull_id_registry.json");

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");

/** Normalize game display name to match Kobayashi index ship_name (uppercase, collapse spaces, U.S.S. -> USS). */
function normalizeName(name) {
  if (!name || typeof name !== "string") return "";
  return name
    .trim()
    .toUpperCase()
    .replace(/\s+/g, " ")
    .replace(/\bU\.S\.S\.\s*/gi, "USS ");
}

/** Alias map: normalized game name -> Kobayashi ship_name (from index) for known mismatches. */
const NAME_ALIASES = new Map([
  ["U.S.S. ENTERPRISE", "USS ENTERPRISE"],
  ["U.S.S. SALADIN", "USS SALADIN"],
  ["U.S.S. MAYFLOWER", "USS MAYFLOWER"],
  ["U.S.S. FRANKLIN", "USS FRANKLIN"],
  ["U.S.S. FRANKLIN A", "USS FRANKLIN A"],
  ["U.S.S. DEFIANT", "USS DEFIANT"],
  ["U.S.S. DISCOVERY", "USS DISCOVERY"],
  ["U.S.S. ENTERPRISE A", "USS ENTERPRISE A"],
  ["U.S.S. ENTERPRISE D", "USS ENTERPRISE D"],
  ["U.S.S. CERRITOS", "USS CERRITOS"],
  ["U.S.S. CROZIER", "USS CROZIER"],
  ["U.S.S. HYDRA", "USS HYDRA"],
  ["U.S.S. INTREPID", "USS INTREPID"],
  ["U.S.S. KELVIN", "USS KELVIN"],
  ["U.S.S. NEWTON", "USS NEWTON"],
  ["U.S.S. NORTHCUTT", "USS NORTHCUTT"],
  ["U.S.S. TITAN A", "USS TITAN A"],
  ["U.S.S. ANTARES", "USS ANTARES"],
  ["U.S.S. BEATTY", "USS BEATTY"],
  ["USS ENTERPRISE-A", "USS ENTERPRISE A"],
]);

async function main() {
  const summary = JSON.parse(await fs.readFile(SUMMARY_PATH, "utf8"));
  const translationsShips = JSON.parse(await fs.readFile(TRANSLATIONS_SHIPS_PATH, "utf8"));
  let translationsBlueprints = [];
  let translationsShipBuffs = [];
  try {
    translationsBlueprints = JSON.parse(await fs.readFile(TRANSLATIONS_BLUEPRINTS_PATH, "utf8"));
  } catch {}
  try {
    translationsShipBuffs = JSON.parse(await fs.readFile(TRANSLATIONS_SHIP_BUFFS_PATH, "utf8"));
  } catch {}
  let ships = [];
  try {
    const extended = JSON.parse(await fs.readFile(EXTENDED_INDEX_PATH, "utf8"));
    if (extended.ships && extended.ships.length > 0) {
      ships = extended.ships;
    }
  } catch {}
  if (ships.length === 0) {
    try {
      const legacy = JSON.parse(await fs.readFile(LEGACY_INDEX_PATH, "utf8"));
      ships = legacy.ships || [];
    } catch (e) {
      console.error("No ship index found: tried", EXTENDED_INDEX_PATH, "and", LEGACY_INDEX_PATH);
      throw e;
    }
  }

  // loca_id -> ship name (from translations-ships key "ship_name")
  const nameById = new Map();
  for (const t of translationsShips) {
    if (t.key === "ship_name" && t.id != null) {
      nameById.set(Number(t.id), t.text);
    }
  }
  // Fallback: blueprint_description id -> first segment (e.g. "REALTA, Grade 1" -> "REALTA")
  for (const t of translationsBlueprints) {
    if (t.key === "blueprint_description" && t.id != null && t.text) {
      const first = t.text.split(",")[0]?.trim();
      if (first && !nameById.has(Number(t.id))) {
        nameById.set(Number(t.id), first);
      }
    }
  }
  // Fallback: blueprint_name_short from ship_buffs (id = loca_id -> ship name)
  for (const t of translationsShipBuffs) {
    if (t.key === "blueprint_name_short" && t.id != null && t.text) {
      if (!nameById.has(Number(t.id))) {
        nameById.set(Number(t.id), t.text);
      }
    }
  }

  // Kobayashi: normalized ship_name -> id (and by exact ship_name)
  const kobayashiByNormalName = new Map();
  const kobayashiByExactName = new Map();
  for (const s of ships) {
    const sn = s.ship_name;
    if (sn) {
      kobayashiByExactName.set(sn, s.id);
      kobayashiByNormalName.set(normalizeName(sn), s.id);
    }
  }

  const hullIdToShipId = {};
  let mapped = 0;
  let skippedNoName = 0;
  let skippedNoMatch = 0;

  for (const s of summary) {
    const hullId = s.id;
    const locaId = s.loca_id;
    const name = nameById.get(locaId);
    if (!name) {
      skippedNoName++;
      continue;
    }
    const normalized = normalizeName(name);
    const aliasResolved = NAME_ALIASES.get(normalized) ?? normalized;
    let kobayashiId =
      kobayashiByExactName.get(aliasResolved) ??
      kobayashiByNormalName.get(aliasResolved) ??
      kobayashiByExactName.get(normalized) ??
      kobayashiByNormalName.get(normalized);
    if (!kobayashiId) {
      skippedNoMatch++;
      if (dryRun && skippedNoMatch <= 10) {
        console.warn(`  No Kobayashi match for hull_id=${hullId} name="${name}" normalized="${normalized}"`);
      }
      continue;
    }
    hullIdToShipId[String(hullId)] = kobayashiId;
    mapped++;
  }

  const registry = {
    _comment:
      "Maps STFC game hull_id (from stfc-mod sync) to kobayashi ship id. Add entries as mappings are discovered. When empty or no match, Roster mode falls back to all ships. Regenerate with: node scripts/build_hull_id_registry.mjs",
    hull_id_to_ship_id: hullIdToShipId,
  };

  if (dryRun) {
    console.log("[dry-run] Would write", REGISTRY_PATH);
    console.log("Mapped:", mapped, "Skipped (no name):", skippedNoName, "Skipped (no match):", skippedNoMatch);
    console.log(JSON.stringify(registry, null, 2));
    return;
  }

  await fs.writeFile(REGISTRY_PATH, JSON.stringify(registry, null, 2) + "\n", "utf8");
  console.log("Wrote", REGISTRY_PATH);
  console.log("Mapped:", mapped, "hull_id -> ship_id. Skipped (no name):", skippedNoName, "Skipped (no match):", skippedNoMatch);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
