#!/usr/bin/env node
/**
 * Normalize cached stfc.space ship detail JSONs into KOBAYASHI canonical ship records.
 *
 * Stage 2 of 2: normalization only — requires fetch_stfcspace_ships.mjs to have run first.
 *
 * Usage:
 *   node scripts/normalize_stfcspace_ships.mjs [--dry-run] [--tier N]
 *
 * Options:
 *   --dry-run     Print what would be written without touching data/ships/
 *   --tier N      Use tier N for stat extraction (default: max_tier from summary)
 *
 * Reads:
 *   data/upstream/data-stfc-space/summary-ship.json
 *   data/upstream/data-stfc-space/translations-ships.json
 *   data/upstream/data-stfc-space/ships/{id}.json     (cached detail per ship)
 *
 * Writes:
 *   data/ships/{slug}.json                            (one ShipRecord per ship)
 *   data/ships/index.json                             (ShipIndex)
 *   data/import_logs/normalize-stfcspace-ships-{date}.json
 *
 * Run from repo root.
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const SUMMARY_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/summary-ship.json");
const TRANSLATIONS_PATH = path.join(REPO_ROOT, "data/upstream/data-stfc-space/translations-ships.json");
const CACHE_DIR = path.join(REPO_ROOT, "data/upstream/data-stfc-space/ships");
const OUT_DIR = path.join(REPO_ROOT, "data/ships");
const LOG_DIR = path.join(REPO_ROOT, "data/import_logs");

const DATA_VERSION = process.env.STFCSPACE_DATA_VERSION
  ?? `stfcspace-${new Date().toISOString().slice(0, 10)}`;
const SOURCE_NOTE = "stfc.space API (data.stfc.space/ship/{id}.json)";

// ── CLI args ──────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const tierIdx = args.indexOf("--tier");
const forceTier = tierIdx !== -1 ? parseInt(args[tierIdx + 1], 10) : null;

// ── Hull type → ship class ────────────────────────────────────────────────────
const HULL_TYPE_MAP = {
  0: "battleship",
  1: "survey",
  2: "interceptor",
  3: "explorer",
  4: "survey",   // variant — treat as survey until confirmed otherwise
  5: "survey",
};

// ── Slug generation ───────────────────────────────────────────────────────────
function slugify(name) {
  return name
    .toLowerCase()
    .replace(/['''.]/g, "")       // strip apostrophes and dots
    .replace(/[^a-z0-9]+/g, "_")  // non-alphanumeric → underscore
    .replace(/^_+|_+$/g, "");     // trim leading/trailing underscores
}

// ── Component tag classification ──────────────────────────────────────────────
// stfc.space uses a `tag` field on components (e.g. "Weapon1", "Shield", "Hull").
// Adjust these if the actual API uses different tags.
function isWeaponComponent(comp) {
  const tag = (comp.tag ?? comp.type ?? "").toLowerCase();
  return tag.startsWith("weapon") || tag === "weapon";
}
function isShieldComponent(comp) {
  const tag = (comp.tag ?? comp.type ?? "").toLowerCase();
  return tag === "shield" || tag === "shields";
}
function isHullComponent(comp) {
  const tag = (comp.tag ?? comp.type ?? "").toLowerCase();
  return tag === "hull" || tag === "armor";
}

// ── Stat extraction from a single component's data object ────────────────────
// Field names are best-guess from the strategy doc; log unknowns for refinement.
function extractWeaponStats(data, unknownFields, compTag) {
  const known = new Set([
    "accuracy", "armor_pierce", "armor_piercing",
    "shield_pierce", "shield_piercing",
    "crit_chance", "critical_chance",
    "crit_damage", "crit_modifier", "critical_damage",
    "minimum_damage", "min_damage",
    "maximum_damage", "max_damage",
    "damage", "dps", "warm_up", "cool_down",
    "shots", "weapon_type", "penetration", "modulation",
  ]);
  for (const key of Object.keys(data ?? {})) {
    if (!known.has(key)) unknownFields.add(`weapon.${compTag}.${key}`);
  }
  return {
    accuracy: data?.accuracy ?? 0,
    armor_piercing: data?.armor_pierce ?? data?.armor_piercing ?? 0,
    shield_piercing: data?.shield_pierce ?? data?.shield_piercing ?? 0,
    crit_chance: data?.crit_chance ?? data?.critical_chance ?? 0.1,
    crit_damage: data?.crit_modifier ?? data?.crit_damage ?? data?.critical_damage ?? 1.5,
    // Use mean of min/max; fall back to flat damage if only one is provided.
    attack: (() => {
      const min = data?.minimum_damage ?? data?.min_damage ?? null;
      const max = data?.maximum_damage ?? data?.max_damage ?? null;
      if (min !== null && max !== null) return (min + max) / 2;
      if (max !== null) return max;
      if (min !== null) return min;
      return data?.damage ?? data?.dps ?? 0;
    })(),
  };
}

function extractShieldStats(data, unknownFields, compTag) {
  const known = new Set(["health", "shield_health", "mitigation", "absorption", "hp"]);
  for (const key of Object.keys(data ?? {})) {
    if (!known.has(key)) unknownFields.add(`shield.${compTag}.${key}`);
  }
  return {
    shield_health: data?.health ?? data?.shield_health ?? data?.hp ?? 0,
    shield_mitigation: data?.mitigation ?? data?.absorption ?? null,
  };
}

function extractHullStats(data, unknownFields, compTag) {
  const known = new Set(["health", "hull_health", "hp", "armor"]);
  for (const key of Object.keys(data ?? {})) {
    if (!known.has(key)) unknownFields.add(`hull.${compTag}.${key}`);
  }
  return {
    hull_health: data?.health ?? data?.hull_health ?? data?.hp ?? 0,
  };
}

// ── Map a detail JSON + summary entry → ShipRecord ───────────────────────────
function mapToShipRecord(detail, summary, name) {
  const unknownFields = new Set();
  const warnings = [];

  // Choose tier: forced CLI arg, or max_tier from summary, or last tier in array.
  const tierNum = forceTier ?? summary.max_tier ?? null;
  let tier = null;
  if (Array.isArray(detail?.tiers) && detail.tiers.length > 0) {
    if (tierNum !== null) {
      tier = detail.tiers.find((t) => t.tier === tierNum) ?? detail.tiers[detail.tiers.length - 1];
    } else {
      tier = detail.tiers[detail.tiers.length - 1];
    }
  }

  // If the detail has a flat `stats` object (as seen in hostile records), use that.
  // Some ship entries might use this format instead of tiers+components.
  const flatStats = detail?.stats ?? null;

  const weaponStats = [];
  let shieldHealth = 0;
  let hullHealth = 0;
  let shieldMitigation = null;

  if (tier?.components?.length > 0) {
    for (const comp of tier.components) {
      // stfc.space detail components may nest stats under `data`, or directly.
      const compData = comp.data ?? comp;
      const compTag = comp.tag ?? comp.type ?? "unknown";

      if (isWeaponComponent(comp)) {
        const w = extractWeaponStats(compData, unknownFields, compTag);
        if (w.attack > 0 || w.accuracy > 0) weaponStats.push(w);
      } else if (isShieldComponent(comp)) {
        const s = extractShieldStats(compData, unknownFields, compTag);
        shieldHealth += s.shield_health;
        if (s.shield_mitigation !== null) shieldMitigation = s.shield_mitigation;
      } else if (isHullComponent(comp)) {
        const h = extractHullStats(compData, unknownFields, compTag);
        hullHealth += h.hull_health;
      } else {
        // Log unrecognised component tags for future mapping.
        unknownFields.add(`component.tag=${compTag}`);
      }
    }
  } else if (flatStats) {
    // Flat stats fallback (similar to hostile detail format).
    weaponStats.push({
      accuracy: flatStats.accuracy ?? 0,
      armor_piercing: flatStats.armor_piercing ?? flatStats.armor_pierce ?? 0,
      shield_piercing: flatStats.shield_piercing ?? flatStats.shield_pierce ?? 0,
      crit_chance: flatStats.critical_chance ?? flatStats.crit_chance ?? 0.1,
      crit_damage: flatStats.critical_damage ?? flatStats.crit_damage ?? 1.5,
      attack: flatStats.dpr ?? flatStats.attack ?? flatStats.damage ?? 0,
    });
    shieldHealth = flatStats.shield_hp ?? flatStats.shield_health ?? 0;
    hullHealth = flatStats.hull_hp ?? flatStats.hull_health ?? 0;
    shieldMitigation = flatStats.shield_mitigation ?? null;
    warnings.push("Used flat stats fallback (no tiers/components found)");
  } else {
    warnings.push("No tiers or flat stats found — all stats defaulted to 0");
  }

  // Aggregate weapon stats (sum piercing/accuracy, mean crits).
  const wCount = weaponStats.length;
  let armor_piercing = 0, shield_piercing = 0, accuracy = 0;
  let crit_chance = 0.1, crit_damage = 1.5, totalAttack = 0;

  if (wCount > 0) {
    armor_piercing = weaponStats.reduce((s, w) => s + w.armor_piercing, 0) / wCount;
    shield_piercing = weaponStats.reduce((s, w) => s + w.shield_piercing, 0) / wCount;
    accuracy = weaponStats.reduce((s, w) => s + w.accuracy, 0) / wCount;
    crit_chance = weaponStats.reduce((s, w) => s + w.crit_chance, 0) / wCount;
    crit_damage = weaponStats.reduce((s, w) => s + w.crit_damage, 0) / wCount;
    totalAttack = weaponStats.reduce((s, w) => s + w.attack, 0);
  }

  // Sanity checks and fallbacks.
  if (totalAttack <= 0) {
    warnings.push("attack=0 — ship may have no weapon components in this tier");
    totalAttack = 1;
  }
  if (hullHealth <= 0) {
    warnings.push("hull_health=0 — hull component missing or stats not mapped");
    hullHealth = shieldHealth * 2 || 1;
  }
  if (shieldHealth <= 0) {
    warnings.push("shield_health=0 — shield component missing or stats not mapped");
  }

  const ship_class = HULL_TYPE_MAP[summary.hull_type] ?? "battleship";
  const slug = slugify(name);

  const record = {
    id: slug,
    ship_name: name,
    ship_class,
    armor_piercing: round6(armor_piercing),
    shield_piercing: round6(shield_piercing),
    accuracy: round6(accuracy),
    attack: round6(totalAttack),
    crit_chance: round6(crit_chance),
    crit_damage: round6(crit_damage),
    hull_health: round6(hullHealth),
    shield_health: round6(shieldHealth),
    ...(shieldMitigation !== null ? { shield_mitigation: round6(shieldMitigation) } : {}),
    // apex_shred and isolytic_damage not present in stfc.space — omit (defaults to 0 in Rust).
  };

  // Per-weapon breakdown (only if multiple weapons with distinct attack values).
  if (weaponStats.length > 1) {
    record.weapons = weaponStats.map((w) => ({ attack: round6(w.attack) }));
  }

  return { record, unknownFields: [...unknownFields], warnings };
}

function round6(n) {
  return Math.round(n * 1e6) / 1e6;
}

// ── Main ──────────────────────────────────────────────────────────────────────
async function main() {
  if (!dryRun) await fs.mkdir(OUT_DIR, { recursive: true });
  await fs.mkdir(LOG_DIR, { recursive: true });

  const summary = JSON.parse(await fs.readFile(SUMMARY_PATH, "utf8"));
  const translations = JSON.parse(await fs.readFile(TRANSLATIONS_PATH, "utf8"));

  // Build name lookup: loca_id → ship name.
  const nameById = new Map();
  for (const t of translations) {
    if (t.key === "ship_name" && t.id !== null) {
      nameById.set(t.id, t.text);
    }
  }

  const log = {
    timestamp: new Date().toISOString(),
    data_version: DATA_VERSION,
    source_note: SOURCE_NOTE,
    total_in_summary: summary.length,
    normalized: 0,
    skipped_no_cache: 0,
    skipped_no_name: 0,
    warnings_by_ship: {},
    unknown_fields: new Set(),
  };

  const indexEntries = [];

  for (const s of summary) {
    const cachePath = path.join(CACHE_DIR, `${s.id}.json`);

    // Check cache exists.
    let detail;
    try {
      detail = JSON.parse(await fs.readFile(cachePath, "utf8"));
    } catch {
      console.warn(`  ⚠ No cached detail for ship ${s.id} — run fetch_stfcspace_ships.mjs first`);
      log.skipped_no_cache++;
      continue;
    }

    // Resolve name.
    const name = nameById.get(s.loca_id);
    if (!name) {
      console.warn(`  ⚠ No name translation for ship ${s.id} (loca_id=${s.loca_id}) — skipping`);
      log.skipped_no_name++;
      continue;
    }

    const { record, unknownFields, warnings } = mapToShipRecord(detail, s, name);

    for (const f of unknownFields) log.unknown_fields.add(f);
    if (warnings.length > 0) log.warnings_by_ship[record.id] = warnings;

    indexEntries.push({
      id: record.id,
      ship_name: record.ship_name,
      ship_class: record.ship_class,
    });

    const outPath = path.join(OUT_DIR, `${record.id}.json`);
    if (dryRun) {
      console.log(`[dry-run] Would write ${outPath}`);
      console.log(JSON.stringify(record, null, 2));
    } else {
      await fs.writeFile(outPath, JSON.stringify(record, null, 2));
    }
    log.normalized++;
    console.log(`  ✓ ${record.id} (${record.ship_class}) attack=${record.attack} hull=${record.hull_health}`);
  }

  // Write index.
  const index = {
    data_version: DATA_VERSION,
    source_note: SOURCE_NOTE,
    ships: indexEntries,
  };
  const indexPath = path.join(OUT_DIR, "index.json");
  if (dryRun) {
    console.log(`\n[dry-run] Would write ${indexPath} with ${indexEntries.length} entries`);
  } else {
    await fs.writeFile(indexPath, JSON.stringify(index, null, 2));
    console.log(`\nWrote index: ${indexPath} (${indexEntries.length} ships)`);
  }

  // Serialize log (Set → array).
  log.unknown_fields = [...log.unknown_fields].sort();
  const dateStr = new Date().toISOString().slice(0, 10);
  const logPath = path.join(LOG_DIR, `normalize-stfcspace-ships-${dateStr}.json`);
  await fs.writeFile(logPath, JSON.stringify(log, null, 2));

  console.log(`\nSummary:`);
  console.log(`  normalized:         ${log.normalized}`);
  console.log(`  skipped_no_cache:   ${log.skipped_no_cache}`);
  console.log(`  skipped_no_name:    ${log.skipped_no_name}`);
  console.log(`  unknown fields:     ${log.unknown_fields.length}`);
  if (log.unknown_fields.length > 0) {
    console.log(`  (see log for details: ${logPath})`);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
