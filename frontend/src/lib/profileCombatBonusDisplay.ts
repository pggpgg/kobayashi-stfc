/** How merged profile combat bonus values should be shown in the UI. */
export type ProfileCombatBonusDisplayMode = "fractional" | "flat" | "multiplier";

/**
 * Engine storage units for profile bonuses (see `apply_profile_to_attacker` in
 * `src/data/profile.rs`). Most keys are fractional adds (0.05 → +5%); apex_barrier
 * is a flat pool value like ship/hostile records (400, 2500, …).
 */
export function profileCombatBonusDisplayMode(
  stat: string,
): ProfileCombatBonusDisplayMode {
  switch (stat) {
    case "apex_barrier":
      return "flat";
    case "crit_damage_floor":
      return "multiplier";
    default:
      return "fractional";
  }
}

function formatFlatCombatValue(value: number): string {
  if (Number.isInteger(value) || Math.abs(value - Math.round(value)) < 1e-9) {
    return Math.round(value).toLocaleString();
  }
  return value.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

/** Compact delta for inline maps, e.g. `weapon_damage +5.00%` or `apex_barrier +2500`. */
export function formatProfileCombatBonusEntry(
  stat: string,
  value: number,
): string {
  const mode = profileCombatBonusDisplayMode(stat);
  switch (mode) {
    case "flat":
      return `${stat} +${formatFlatCombatValue(value)}`;
    case "multiplier":
      return `${stat} +${formatFlatCombatValue(value)}×`;
    default:
      return `${stat} +${(value * 100).toFixed(2)}%`;
  }
}

/** Value column in summary lists, e.g. `25695 flat` or `5.00% additive`. */
export function formatProfileCombatBonusListValue(
  stat: string,
  value: number,
): string {
  const mode = profileCombatBonusDisplayMode(stat);
  switch (mode) {
    case "flat":
      return `${formatFlatCombatValue(value)} flat`;
    case "multiplier":
      return `${formatFlatCombatValue(value)}× crit floor`;
    default:
      return `${(value * 100).toFixed(2)}% additive`;
  }
}

/** Signed value only (no stat name), for conditional rows. */
export function formatProfileCombatBonusDelta(
  stat: string,
  value: number,
): string {
  const mode = profileCombatBonusDisplayMode(stat);
  switch (mode) {
    case "flat":
      return `+${formatFlatCombatValue(value)}`;
    case "multiplier":
      return `+${formatFlatCombatValue(value)}×`;
    default:
      return `+${(value * 100).toFixed(2)}%`;
  }
}
