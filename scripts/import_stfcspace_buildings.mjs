import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

// Simple, API-based importer for stfc.space building buffs.
// - Fetches https://data.stfc.space/building/summary.json
// - Uses a small, explicit mapping from building ids + buff ids to engine stat
//   keys
// - Writes normalized BuildingRecord JSON files under data/buildings/
//
// This is intentionally conservative: it only emits bonuses we explicitly map
// and currently focuses on Operations Center as a proof-of-concept. Extend the
// BUILDING_META and BUFF_MAPPING tables as you provide more labels.

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const OUT_DIR = path.join(REPO_ROOT, "data", "buildings");
const IMPORT_LOG_DIR = path.join(REPO_ROOT, "data", "import_logs");

const BASE_URL = "https://data.stfc.space";

// Data-version string recorded into BuildingRecord / BuildingIndex.
const DATA_VERSION =
  process.env.STFCSPACE_DATA_VERSION ??
  `stfcspace-${new Date().toISOString().slice(0, 10)}`;

const SOURCE_NOTE =
  "stfc.space backend API (data.stfc.space building summary/buffs)";

// Known building id → normalized id + display name.
// Extend this as you confirm more building identities.
const BUILDING_META = {
  0: { id: "ops_center", building_name: "OPERATIONS CENTER" },
  1: { id: "parsteel_generator_a", building_name: "Parsteel Generator A" },
  2: { id: "parsteel_generator_b", building_name: "Parsteel Generator B" },
  3: { id: "parsteel_generator_c", building_name: "Parsteel Generator C" },
  4: { id: "parsteel_warehouse", building_name: "Parsteel Warehouse" },
  5: { id: "tritanium_generator_a", building_name: "Tritanium Generator A" },
  6: { id: "tritanium_generator_b", building_name: "Tritanium Generator B" },
  7: { id: "tritanium_generator_c", building_name: "Tritanium Generator C" },
  8: { id: "tritanium_warehouse", building_name: "Tritanium Warehouse" },
  9: { id: "dilithium_generator_a", building_name: "Dilithium Generator A" },
  10: { id: "dilithium_generator_b", building_name: "Dilithium Generator B" },
  11: { id: "dilithium_generator_c", building_name: "Dilithium Generator C" },
  12: { id: "dilithium_warehouse", building_name: "Dilithium Warehouse" },
  13: { id: "treasury", building_name: "Treasury" },
  14: { id: "engine_technology_lab", building_name: "Engine Technology Lab" },
  15: { id: "shipyard", building_name: "Shipyard" },
  16: { id: "ship_hangar", building_name: "Ship Hangar" },
  17: { id: "drydock_a", building_name: "Drydock A" },
  18: { id: "drydock_b", building_name: "Drydock B" },
  19: { id: "drydock_c", building_name: "Drydock C" },
  20: { id: "drydock_d", building_name: "Drydock D" },
};

// Buff id → stat mapping for effects we understand.
// value_is_percentage from the API is already represented as fractional values
// (e.g. 0.01 = +1%), which matches the KOBAYASHI schema expectation.
const BUFF_MAPPING = {
  // Operations Center: Weapon Damage Bonus
  919263260: {
    stat: "weapon_damage",
    operator: "add",
    conditions: [],
    notes: null,
  },
  // Parsteel Generator A (and others): Hull Health Bonus
  995187207: {
    stat: "hull_hp",
    operator: "add",
    conditions: [],
    notes: null,
  },
  // Tritanium Generator A (and others): Shield Health Bonus
  1974652540: {
    stat: "shield_hp",
    operator: "add",
    conditions: [],
    notes: null,
  },
  // Dilithium Generator A (and others): Damage / Weapon Damage Bonus
  560958387: {
    stat: "weapon_damage",
    operator: "add",
    conditions: [],
    notes: null,
  },
};

async function fetchJson(relativePath) {
  const url = `${BASE_URL}/${relativePath.replace(/^\/+/, "")}`;
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status} for ${url}`);
  }
  return res.json();
}

function bMetaOrNull(buildingId) {
  const meta = BUILDING_META[buildingId];
  if (!meta) return null;
  return meta;
}

const ABS_MAX_LEVEL = 80;

function resolveBuffMapping(buff) {
  const known = BUFF_MAPPING[buff.id];
  if (known) return known;

  // Fallback: treat as an opaque buff keyed by id. This preserves numeric
  // values now and allows us to refine the mapping later once we have labels.
  return {
    stat: `buff_${buff.id}`,
    operator: "add",
    conditions: [],
    notes:
      buff && typeof buff.loca_id === "number"
        ? `auto-imported from data.stfc.space; loca_id=${buff.loca_id}`
        : "auto-imported from data.stfc.space; unknown buff label",
  };
}

function buildLevelsFromSummaryEntry(entry) {
  const unlockLevel = entry.unlock_level ?? 1;
  const buffs = Array.isArray(entry.buffs) ? entry.buffs : [];

  if (unlockLevel > ABS_MAX_LEVEL) {
    return [];
  }

  // Determine max level either from explicit field or the longest buff track,
  // but never exceed ABS_MAX_LEVEL.
  let maxLevel =
    typeof entry.max_level === "number" && entry.max_level > 0
      ? entry.max_level
      : 0;
  for (const buff of buffs) {
    if (!Array.isArray(buff.values)) continue;
    const candidate = unlockLevel - 1 + buff.values.length;
    if (candidate > maxLevel) {
      maxLevel = candidate;
    }
  }

  if (maxLevel === 0) {
    return [];
  }

  maxLevel = Math.min(maxLevel, ABS_MAX_LEVEL);

  const levels = [];
  for (let level = unlockLevel; level <= maxLevel; level += 1) {
    const bonuses = [];

    for (const buff of buffs) {
      const mapping = resolveBuffMapping(buff);
      const values = Array.isArray(buff.values) ? buff.values : [];

      const idx = level - unlockLevel;
      if (idx < 0 || idx >= values.length) continue;

      const raw = values[idx];
      if (!raw || typeof raw.value !== "number") continue;

      const value = raw.value; // already fractional if value_is_percentage is true

      bonuses.push({
        stat: mapping.stat,
        value,
        operator: mapping.operator ?? "add",
        conditions: mapping.conditions ?? [],
        notes: mapping.notes ?? null,
      });
    }

    if (bonuses.length === 0) {
      continue;
    }

    levels.push({
      level,
      ops_min: null,
      ops_max: null,
      bonuses,
    });
  }

  return levels;
}

async function ensureDir(dir) {
  await fs.mkdir(dir, { recursive: true });
}

async function main() {
  console.log("Fetching building summary from data.stfc.space …");
  const summary = await fetchJson("building/summary.json");
  if (!Array.isArray(summary)) {
    throw new Error("Unexpected summary.json format: expected an array");
  }

  await ensureDir(OUT_DIR);
  await ensureDir(IMPORT_LOG_DIR);

  const records = [];
  const unmappedBuildings = [];
  const unmappedBuffs = new Set();

  for (const entry of summary) {
    if (!entry || typeof entry.id !== "number") continue;
    const buildingId = entry.id;
    let meta = bMetaOrNull(buildingId);
    if (!meta) {
      meta = {
        id: `building_${buildingId}`,
        building_name: `BUILDING ${buildingId}`,
      };
      unmappedBuildings.push(buildingId);
    }

    const levels = buildLevelsFromSummaryEntry(entry);
    // Track any buff ids we don't have a first-class mapping for yet.
    const buffs = Array.isArray(entry.buffs) ? entry.buffs : [];
    for (const buff of buffs) {
      if (
        buff &&
        typeof buff.id === "number" &&
        !Object.prototype.hasOwnProperty.call(BUFF_MAPPING, buff.id)
      ) {
        unmappedBuffs.add(buff.id);
      }
    }
    const record = {
      id: meta.id,
      building_name: meta.building_name,
      data_version: DATA_VERSION,
      source_note: SOURCE_NOTE,
      levels,
    };

    records.push(record);

    const outPath = path.join(OUT_DIR, `${meta.id}.json`);
    await fs.writeFile(outPath, JSON.stringify(record, null, 2), "utf8");
    console.log(
      `Wrote ${outPath} (levels with combat bonuses: ${levels.length})`,
    );
  }

  if (records.length === 0) {
    console.warn(
      "Warning: no buildings were emitted. Check BUILDING_META/BUFF_MAPPING.",
    );
  }

  const index = {
    data_version: DATA_VERSION,
    source_note: SOURCE_NOTE,
    buildings: records.map((r) => ({
      id: r.id,
      building_name: r.building_name,
    })),
  };

  const indexPath = path.join(OUT_DIR, "index.json");
  await fs.writeFile(indexPath, JSON.stringify(index, null, 2), "utf8");
  console.log(`Updated building index: ${indexPath}`);

  // Lightweight import log for debugging / future expansion.
  const log = {
    source: "stfc.space",
    data_version: DATA_VERSION,
    summary_url: `${BASE_URL}/building/summary.json`,
    emitted_buildings: records.length,
    emitted_ids: records.map((r) => r.id),
    unmapped_building_ids: Array.from(new Set(unmappedBuildings)).sort(
      (a, b) => a - b,
    ),
    unmapped_buff_ids: Array.from(unmappedBuffs).sort((a, b) => a - b),
    generated_at: new Date().toISOString(),
  };

  const logPath = path.join(
    IMPORT_LOG_DIR,
    `buildings-stfcspace-${new Date().toISOString().slice(0, 10)}.json`,
  );
  await fs.writeFile(logPath, JSON.stringify(log, null, 2), "utf8");
  console.log(`Wrote import log: ${logPath}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

