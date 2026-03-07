import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

// Simple importer for stfc.space building buffs.
// - By default: fetches https://data.stfc.space/building/summary.json
// - With --from-upstream (or USE_UPSTREAM_BUILDINGS=1): reads
//   data/upstream/data-stfc-space/summary-building.json
// - Uses a small, explicit mapping from building ids + buff ids to engine stat
//   keys; writes normalized BuildingRecord JSON files under data/buildings/
//
// This is intentionally conservative: it only emits bonuses we explicitly map
// and currently focuses on Operations Center as a proof-of-concept. Extend the
// BUILDING_META and BUFF_MAPPING tables as you provide more labels.

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const OUT_DIR = path.join(REPO_ROOT, "data", "buildings");
const IMPORT_LOG_DIR = path.join(REPO_ROOT, "data", "import_logs");
const UPSTREAM_SUMMARY_PATH = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
  "summary-building.json",
);
const UPSTREAM_TRANSLATIONS_STARBASE_PATH = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
  "translations-starbase_modules.json",
);

const BASE_URL = "https://data.stfc.space";

const STARBASE_MODULE_NAME_KEY = "starbase_module_name";

/** Slug for filename: lowercase, non-alnum to underscore, trim. */
function slugify(text) {
  if (!text || typeof text !== "string") return "";
  return text
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_|_$/g, "") || "building";
}

/**
 * Load building id → display name from translations-starbase_modules.json
 * (key === "starbase_module_name"). Returns Map<number, string> or empty Map if file missing.
 */
async function loadBuildingNameTranslations() {
  const map = new Map();
  try {
    const raw = await fs.readFile(UPSTREAM_TRANSLATIONS_STARBASE_PATH, "utf8");
    const entries = JSON.parse(raw);
    if (!Array.isArray(entries)) return map;
    for (const e of entries) {
      if (e.key !== STARBASE_MODULE_NAME_KEY || e.id == null) continue;
      const text = (e.text && String(e.text).trim()) || "";
      if (text) map.set(Number(e.id), text);
    }
  } catch (_) {
    // missing or invalid: leave map empty
  }
  return map;
}

// When true, read summary from local upstream instead of fetching from API.
const FROM_UPSTREAM =
  process.argv.includes("--from-upstream") ||
  process.env.USE_UPSTREAM_BUILDINGS === "1";

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
  const buffs = Array.isArray(entry.buffs) ? entry.buffs : [];

  // Determine max level from explicit field or longest buff track (values are 1-based: values[0] = level 1).
  // Don't use unlock_level here — it's when the building becomes buildable (ops level), not the building's level range.
  let maxLevel =
    typeof entry.max_level === "number" && entry.max_level > 0
      ? entry.max_level
      : 0;
  for (const buff of buffs) {
    if (!Array.isArray(buff.values)) continue;
    if (buff.values.length > maxLevel) {
      maxLevel = buff.values.length;
    }
  }

  if (maxLevel === 0) {
    return [];
  }

  maxLevel = Math.min(maxLevel, ABS_MAX_LEVEL);

  const levels = [];
  // values[] in the summary is indexed by building level (1-based): values[0] = level 1, values[1] = level 2, ...
  // unlock_level is when the building becomes buildable (ops level), not the building's first level.
  for (let level = 1; level <= maxLevel; level += 1) {
    const bonuses = [];

    for (const buff of buffs) {
      const mapping = resolveBuffMapping(buff);
      const values = Array.isArray(buff.values) ? buff.values : [];

      const idx = level - 1; // 0-based index: level 1 -> values[0]
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

async function loadSummary() {
  if (FROM_UPSTREAM) {
    try {
      const raw = await fs.readFile(UPSTREAM_SUMMARY_PATH, "utf8");
      const summary = JSON.parse(raw);
      if (!Array.isArray(summary)) {
        throw new Error("Unexpected summary-building.json format: expected an array");
      }
      return summary;
    } catch (err) {
      if (err.code === "ENOENT") {
        throw new Error(
          "Upstream file not found. Run fetch or copy summary-building.json to data/upstream/data-stfc-space/.",
        );
      }
      throw err;
    }
  }
  return fetchJson("building/summary.json");
}

async function main() {
  if (FROM_UPSTREAM) {
    console.log("Reading building summary from data/upstream/data-stfc-space/summary-building.json …");
  } else {
    console.log("Fetching building summary from data.stfc.space …");
  }
  const summary = await loadSummary();
  if (!Array.isArray(summary)) {
    throw new Error("Unexpected summary format: expected an array");
  }

  const nameByBid = await loadBuildingNameTranslations();
  if (nameByBid.size > 0) {
    console.log(`Loaded ${nameByBid.size} building names from translations-starbase_modules.json`);
  }

  await ensureDir(OUT_DIR);
  await ensureDir(IMPORT_LOG_DIR);

  const records = [];
  const fileStems = [];
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

    const translatedName = nameByBid.get(buildingId);
    const displayName = translatedName ?? meta.building_name;
    const nameSlug = translatedName ? slugify(translatedName) : meta.id;
    const fileStem = `${buildingId}_${nameSlug}`;

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
      building_name: displayName,
      data_version: DATA_VERSION,
      source_note: SOURCE_NOTE,
      levels,
    };

    records.push(record);
    fileStems.push(fileStem);

    const outPath = path.join(OUT_DIR, `${fileStem}.json`);
    await fs.writeFile(outPath, JSON.stringify(record, null, 2), "utf8");
    console.log(
      `Wrote ${outPath} (${levels.length} level entries)`,
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
    buildings: records.map((r, i) => ({
      id: r.id,
      building_name: r.building_name,
      file: fileStems[i],
    })),
  };

  const validFileStems = new Set(fileStems);
  validFileStems.add("index");
  const dirEntries = await fs.readdir(OUT_DIR, { withFileTypes: true });
  for (const dirent of dirEntries) {
    if (!dirent.isFile() || !dirent.name.endsWith(".json")) continue;
    const stem = dirent.name.slice(0, -5);
    if (validFileStems.has(stem)) continue;
    const removePath = path.join(OUT_DIR, dirent.name);
    await fs.unlink(removePath);
    console.log(`Removed obsolete ${dirent.name}`);
  }

  const indexPath = path.join(OUT_DIR, "index.json");
  await fs.writeFile(indexPath, JSON.stringify(index, null, 2), "utf8");
  console.log(`Updated building index: ${indexPath}`);

  // Lightweight import log for debugging / future expansion.
  const log = {
    source: FROM_UPSTREAM ? "stfc.space (from upstream)" : "stfc.space",
    data_version: DATA_VERSION,
    ...(FROM_UPSTREAM
      ? { summary_path: "data/upstream/data-stfc-space/summary-building.json" }
      : { summary_url: `${BASE_URL}/building/summary.json` }),
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

