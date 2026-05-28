/**
 * Shared bonus value normalization for research import (see import_stfcspace_research.mjs).
 */

/** Stats merged as additive fractions then (1 + bonus) on hull/shield in scenario (same as buildings). */
export const HULL_SHIELD_HP_STATS = new Set(["hull_hp", "shield_hp"]);

/** Stats that sometimes appear with value_is_percentage false but use fractional bonuses like 0.05. */
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
  let value = rawValue;
  if (buff.value_is_percentage) {
    // Percentage-point display (buildings + research hull/shield/weapon rows): 1.3 → +1.3% → 0.013.
    // Large tier totals (130 → +130%, 1300 → +1300%) use the >1.5 branch when show_percentage is false.
    if (buff.show_percentage && value >= 1 && value <= 100) {
      return value / 100;
    }
    value = value >= 0 && value <= 1.5 ? value : value / 100;
    return value;
  }
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
