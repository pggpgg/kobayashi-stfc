/**
 * Shared bonus value normalization for research import (see import_stfcspace_research.mjs).
 */

/** Stats merged as additive fractions then (1 + bonus) on hull/shield in scenario (same as buildings). */
export const HULL_SHIELD_HP_STATS = new Set(["hull_hp", "shield_hp"]);

/** Stats with legacy fractional/point values only when upstream omits percentage flags. */
export const NON_PCT_DECIMAL_STATS = new Set([
  "armor",
  "shield_deflection",
  "weapon_damage",
  "isolytic_damage",
  "isolytic_defense",
  "hull_hp",
  "shield_hp",
  "crit_chance",
  "crit_damage",
  "crit_damage_floor",
  "pierce",
  "shield_mitigation",
  "damage_reduction",
  "dodge",
  "accuracy",
  "armor_piercing",
  "shield_piercing",
  "apex_shred",
  "apex_barrier",
]);

/**
 * Normalize upstream buff raw values to engine fractional units.
 * @param {object} buff upstream buff row
 * @param {object} mapping resolved stat mapping (may include condition fields)
 * @param {number} rawValue
 * @returns {number|null}
 */
export function normalizeBonusValue(buff, mapping, rawValue) {
  if (!Number.isFinite(rawValue)) return null;
  // A mapping's explicit unit is the only reliable way to distinguish a fraction
  // from percentage points. In particular, never infer units from the raw value's
  // magnitude or from the display-only upstream flags.
  if (mapping.value_unit === "fraction" || mapping.value_unit === "absolute") {
    return rawValue;
  }
  if (mapping.value_unit === "percentage_points") return rawValue / 100;
  // value_is_percentage/show_percentage do not describe the numeric encoding.
  // Keep the row out of the combat catalog until a reviewed mapping supplies a unit.
  if (buff.value_is_percentage === true) return null;
  const value = rawValue;
  if (
    (mapping.stat === "apex_barrier" || mapping.stat === "apex_shred") &&
    !buff.value_is_percentage
  ) {
    if (value > 0 && Number.isFinite(value)) return value;
    return null;
  }
  if (!NON_PCT_DECIMAL_STATS.has(mapping.stat)) {
    return null;
  }
  // Already-fractional upstream (e.g. Federation Graviton shield_hp 0.2 = +20%).
  if (value >= 0 && value <= 2) {
    return value;
  }
  // Integer percentage points without value_is_percentage (uncommon; same ÷100 as buildings tier display).
  if (
    HULL_SHIELD_HP_STATS.has(mapping.stat) &&
    value > 2 &&
    value <= 10000 &&
    Number.isFinite(value)
  ) {
    return value / 100;
  }
  if (value > 2 && value <= 100 && Number.isInteger(value)) {
    return value / 100;
  }
  return null;
}
