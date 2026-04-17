/**
 * Reads cached forbidden tech (summary tech_type === 0) detail JSON from
 * data/upstream/data-stfc-space/forbidden_tech/{id}.json and prints CSV rows
 * for data/import/forbidden_chaos_tech.csv (stdout).
 *
 * Uses values[45] when present (level-46-style calibration; same as build_chaos_tech_csv_rows.mjs).
 * Skips economy / proc lines without a clear combat stat mirror.
 *
 * Usage:
 *   node scripts/build_forbidden_tech_csv_rows.mjs
 *   node scripts/build_forbidden_tech_csv_rows.mjs --missing-only
 *
 * --missing-only: only emit rows for fids not already present in data/forbidden_chaos_tech.json
 * (use when extending the catalog).
 */

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.join(__dirname, "..");

function loadJson(p) {
  return JSON.parse(fs.readFileSync(path.join(ROOT, p), "utf8"));
}

/** @param {string} text */
function mapStatFromBuffText(text) {
  const t = text.toLowerCase();
  if (
    t.includes("isomatter") ||
    t.includes("resources you get") ||
    t.includes("resources from") ||
    (t.includes("amount of") && t.includes("gained"))
  )
    return null;
  if (t.includes("mining") || t.includes("loot")) return null;
  if (t.includes("armada") && t.includes("shots")) return null;
  if (t.includes("hull breach") && t.includes("chance")) return null;
  if (t.includes("burning") && t.includes("chance")) return null;
  if (
    t.includes("against players") &&
    (t.includes("opponent") || t.includes("their") || t.includes("reduces the opponent")) &&
    (t.includes("reduce") || t.includes("decrease") || t.includes("lowers") || t.includes("lower"))
  ) {
    return null;
  }
  if (t.includes("reduces critical damage of players")) return null;
  if (t.includes("against players") && t.includes("reduces the opponent")) return null;

  if (t.includes("apex barrier")) return "apex_barrier";
  if (t.includes("apex shred")) return "apex_shred";
  if (t.includes("isolytic") && t.includes("defense")) return "isolytic_defense";
  if (t.includes("isolytic") && t.includes("damage")) return "isolytic_damage";
  if (
    t.includes("critical damage") ||
    t.includes("critical hit damage") ||
    t.includes("crit damage")
  )
    return "crit_damage";
  if (t.includes("critical hit chance") || t.includes("crit chance")) return "crit_chance";
  if (t.includes("accuracy")) return "accuracy";
  if (t.includes("pierce") || t.includes("penetration")) return "pierce";
  if (t.includes("shield mitigation") || t.includes("shield deflection"))
    return "shield_mitigation";
  if (t.includes("dodge")) return "dodge";
  if (t.includes("armor")) return "armor";
  if (t.includes("hull") && (t.includes("health") || t.includes("hp"))) return "hull_hp";
  if (t.includes("base shp") || (t.includes("shp") && t.includes("increase"))) return "shield_hp";
  if (t.includes("shield") && (t.includes("health") || t.includes("hp"))) return "shield_hp";
  if (t.includes("weapon damage")) return "weapon_damage";
  if (t.includes("damage") && t.includes("increase")) return "weapon_damage";
  if (t.includes("damage reduction")) return "damage_reduction";
  return null;
}

function buffTextForLoca(translations, locaId) {
  const rows = translations.filter((e) => e.id === locaId);
  const name = rows.find((r) => r.key?.includes("forbidden_tech_buff_name"));
  const short = rows.find((r) => r.key?.includes("forbidden_tech_short_desc"));
  return `${name?.text ?? ""} ${short?.text ?? ""}`;
}

function flattenBuffChains(detail) {
  const out = [];
  for (const g of detail.buffs ?? []) {
    if (g.tier != null && Array.isArray(g.buffs)) {
      for (const b of g.buffs) {
        if (b.values?.length) out.push(b);
      }
    } else if (g.values?.length) {
      out.push(g);
    }
  }
  return out;
}

function referenceValue(values) {
  if (!values?.length) return 0;
  const idx = Math.min(45, values.length - 1);
  return values[idx]?.value ?? 0;
}

function lastNonzeroValue(values) {
  for (let i = values.length - 1; i >= 0; i--) {
    const v = values[i]?.value;
    if (typeof v === "number" && v !== 0) return v;
  }
  return 0;
}

function fmtNum(n) {
  if (!Number.isFinite(n)) return "0";
  return String(Math.round(n * 1e6) / 1e6);
}

function main() {
  const missingOnly = process.argv.includes("--missing-only");

  const summary = loadJson("data/upstream/data-stfc-space/summary-forbidden_tech.json");
  const translations = loadJson(
    "data/upstream/data-stfc-space/translations-forbidden_tech.json"
  );

  let catalogFids = new Set();
  if (missingOnly) {
    try {
      const catalog = loadJson("data/forbidden_chaos_tech.json");
      catalogFids = new Set(catalog.items.map((x) => x.fid).filter((f) => f != null));
    } catch {
      catalogFids = new Set();
    }
  }

  const forbidden = summary.filter((e) => e.tech_type === 0);
  const rows = [];

  for (const s of forbidden) {
    if (missingOnly && catalogFids.has(s.id)) continue;

    const nameRow = translations.find(
      (e) => e.id === s.loca_id && e.key?.endsWith("forbidden_tech_name")
    );
    const name = nameRow?.text?.replace(/"/g, '""') ?? `fid_${s.id}`;
    const tier = s.tier_max ?? 12;

    const detailPath = path.join(
      ROOT,
      "data/upstream/data-stfc-space/forbidden_tech",
      `${s.id}.json`
    );
    if (!fs.existsSync(detailPath)) {
      console.error(`// skip ${s.id} ${name}: missing ${detailPath}`);
      continue;
    }
    const detail = JSON.parse(fs.readFileSync(detailPath, "utf8"));

    const chains = flattenBuffChains(detail);
    let emitted = 0;

    for (const b of chains) {
      if (!b.values?.length) continue;
      const loca = b.loca_id;
      const text = buffTextForLoca(translations, loca);
      const stat = mapStatFromBuffText(text);
      if (!stat) continue;

      let raw = referenceValue(b.values);
      if (raw === 0) raw = lastNonzeroValue(b.values);
      if (raw === 0) continue;

      const pctLike =
        stat !== "apex_barrier" &&
        stat !== "apex_shred" &&
        [
          "weapon_damage",
          "hull_hp",
          "shield_hp",
          "armor",
          "dodge",
          "pierce",
          "shield_mitigation",
          "crit_chance",
          "crit_damage",
          "accuracy",
          "damage_reduction",
          "isolytic_damage",
          "isolytic_defense",
        ].includes(stat);

      let catalogValue;
      if (stat === "apex_barrier" && b.value_is_percentage === true && raw < 500) {
        continue;
      }
      if (stat === "apex_barrier" && b.value_is_percentage !== true) {
        if (raw < 100) continue;
        catalogValue = raw;
      } else if (b.value_is_percentage === true) {
        if (raw > 150) continue;
        catalogValue = raw / 100;
      } else if (pctLike && raw > 2 && raw <= 150) {
        catalogValue = raw / 100;
      } else if (raw > 2) {
        continue;
      } else {
        catalogValue = raw;
      }

      if (catalogValue === 0) continue;
      rows.push(`${name},forbidden,${tier},${s.id},${stat},${fmtNum(catalogValue)},add`);
      emitted++;
    }

    if (emitted === 0) {
      let best = 0;
      for (const b of chains) {
        if (!b.values?.length || !b.value_is_percentage) continue;
        const r = referenceValue(b.values) || lastNonzeroValue(b.values);
        if (r > 0 && r <= 150 && r > best) best = r;
      }
      if (best > 0) {
        console.error(
          `// fallback weapon_damage for ${name} (${s.id}) — review translations / engine scope`
        );
        rows.push(`${name},forbidden,${tier},${s.id},weapon_damage,${fmtNum(best / 100)},add`);
      } else {
        console.error(`// warning: no usable values for ${name} (${s.id}) — placeholder hull_hp 0`);
        rows.push(`${name},forbidden,${tier},${s.id},hull_hp,0,add`);
      }
    }
  }

  for (const line of rows) console.log(line);
}

main();
