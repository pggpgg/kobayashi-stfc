/**
 * Shared helpers for build_chaos_tech_csv_rows.mjs and build_forbidden_tech_csv_rows.mjs.
 * Keep stat mapping and value normalization identical for both pipelines.
 */

import fs from "fs";
import path from "path";

/** @param {string} text */
export function mapStatFromBuffText(text) {
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
  // Armada-scoped crit / damage lines are not global ship combat bonuses in Kobayashi merge.
  if (
    t.includes("armada") &&
    (t.includes("critical damage") ||
      t.includes("crit damage") ||
      (t.includes("crit") && t.includes("damage")))
  )
    return null;
  if (t.includes("hull breach") && t.includes("chance")) return null;
  if (t.includes("burning") && t.includes("chance")) return null;
  // PvP-only opponent debuffs: engine applies global ship bonuses only (see docs/ROADMAP.md).
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
  // Opponent-only cumulative debuff (e.g. Quantum Slipstream) — not a player profile shield_mitigation bonus.
  if (
    (t.includes("opponent") || t.includes("opponent's")) &&
    (t.includes("decrease") ||
      t.includes("decreases") ||
      t.includes("reduce") ||
      t.includes("reduces") ||
      t.includes("lower") ||
      t.includes("lowers")) &&
    (t.includes("shield mitigation") || t.includes("shield deflection"))
  ) {
    return null;
  }
  if (t.includes("shield mitigation") || t.includes("shield deflection"))
    return "shield_mitigation";
  if (t.includes("dodge")) return "dodge";
  if (t.includes("armor")) return "armor";
  // "weapon damage" before "hhp" — lines like "Weapon Damage when enemy HHP is below …" are weapon_damage, not hull.
  if (t.includes("weapon damage")) return "weapon_damage";
  if (t.includes("damage") && t.includes("increase")) return "weapon_damage";
  // "HHP" / "hhp" = hull health in STFC copy (not always the word "hull").
  if (t.includes("hhp")) return "hull_hp";
  if (t.includes("hull") && (t.includes("health") || t.includes("hp"))) return "hull_hp";
  if (t.includes("base shp") || (t.includes("shp") && t.includes("increase"))) return "shield_hp";
  if (t.includes("shield") && (t.includes("health") || t.includes("hp"))) return "shield_hp";
  if (t.includes("damage reduction")) return "damage_reduction";
  return null;
}

export function buffTextForLoca(translations, locaId) {
  const rows = translations.filter((e) => e.id === locaId);
  const name = rows.find((r) => r.key?.includes("forbidden_tech_buff_name"));
  const short = rows.find((r) => r.key?.includes("forbidden_tech_short_desc"));
  return `${name?.text ?? ""} ${short?.text ?? ""}`;
}

/**
 * @param {object | null} overrides - { by_buff_id?: Record<string, { stat?: string }>, by_loca_id?: Record<string, { stat?: string }> }
 * @param {object} b - buff object with optional id, loca_id
 */
export function resolveStatForBuff(b, translations, overrides) {
  const bid = b.id != null ? String(b.id) : null;
  const lid = b.loca_id != null ? String(b.loca_id) : null;
  if (overrides?.by_buff_id && bid && overrides.by_buff_id[bid]?.stat) {
    return overrides.by_buff_id[bid].stat;
  }
  if (overrides?.by_loca_id && lid && overrides.by_loca_id[lid]?.stat) {
    return overrides.by_loca_id[lid].stat;
  }
  const text = buffTextForLoca(translations, b.loca_id);
  return mapStatFromBuffText(text);
}

export function flattenBuffChains(detail) {
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

export function referenceValue(values) {
  if (!values?.length) return 0;
  const idx = Math.min(45, values.length - 1);
  return values[idx]?.value ?? 0;
}

export function lastNonzeroValue(values) {
  for (let i = values.length - 1; i >= 0; i--) {
    const v = values[i]?.value;
    if (typeof v === "number" && v !== 0) return v;
  }
  return 0;
}

export function fmtNum(n) {
  if (!Number.isFinite(n)) return "0";
  return String(Math.round(n * 1e6) / 1e6);
}

/**
 * Normalize raw value from buff.values[] to catalog decimal / flat apex.
 * @returns {{ ok: boolean, value: number }} ok false => skip this row
 */
export function catalogValueForBuff(b, stat) {
  let raw = referenceValue(b.values);
  if (raw === 0) raw = lastNonzeroValue(b.values);
  if (raw === 0) return { ok: false, value: 0 };

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
    return { ok: false, value: 0 };
  }
  if (stat === "apex_barrier" && b.value_is_percentage !== true) {
    if (raw < 100) return { ok: false, value: 0 };
    catalogValue = raw;
  } else if (b.value_is_percentage === true) {
    if (raw > 150) return { ok: false, value: 0 };
    catalogValue = raw / 100;
  } else if (pctLike && raw > 2 && raw <= 150) {
    catalogValue = raw / 100;
  } else if (raw > 2) {
    return { ok: false, value: 0 };
  } else {
    catalogValue = raw;
  }

  if (catalogValue === 0) return { ok: false, value: 0 };
  return { ok: true, value: catalogValue };
}

/**
 * Load optional chaos-tech buff overrides (buff id / loca_id -> stat).
 * @param {string} repoRoot - path to repo root (directory containing data/)
 */
export function loadChaosBuffOverrides(repoRoot) {
  const p = path.join(repoRoot, "data/import/chaos_tech_buff_overrides.json");
  if (!fs.existsSync(p)) return null;
  try {
    return JSON.parse(fs.readFileSync(p, "utf8"));
  } catch {
    return null;
  }
}
