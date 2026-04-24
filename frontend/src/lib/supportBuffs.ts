/**
 * Support buff ids that represent in-game **Titan-A Fortify** (you become **Fortified**).
 * When either is selected (after exclusive-group resolution on the server), Kobayashi applies
 * Titan “while Fortified” research combat stats from the profile catalog.
 *
 * Must stay in sync with `TITAN_A_FORTIFY_SUPPORT_BUFF_IDS` in `src/data/profile.rs` and keys in
 * `data/support_buffs.json`.
 */
export const TITAN_A_FORTIFY_SUPPORT_BUFF_IDS = [
  "titan_a_fortification",
  "titan_a_max_fortification",
] as const;

/** Max Fortified research applies when this id is in resolved support buffs. Sync with `src/data/profile.rs`. */
export const TITAN_A_MAX_FORTIFICATION_BUFF_ID = "titan_a_max_fortification" as const;

/**
 * Cerritos alliance support buff id (`data/support_buffs.json`).
 * Sync with `CERRITOS_SUPPORT_BUFF_ID` in `src/data/profile.rs`.
 */
export const CERRITOS_SUPPORT_BUFF_ID = "cerritos_support" as const;

/**
 * Defiant reinforce buff id (`data/support_buffs.json`).
 * Sync with `DEFIANT_REINFORCE_BUFF_ID` in `src/data/profile.rs`.
 */
export const DEFIANT_REINFORCE_BUFF_ID = "defiant_reinforce" as const;

export type TitanAFortifySupportBuffId =
  (typeof TITAN_A_FORTIFY_SUPPORT_BUFF_IDS)[number];

export function isTitanAFortifySupportBuff(id: string): id is TitanAFortifySupportBuffId {
  return (TITAN_A_FORTIFY_SUPPORT_BUFF_IDS as readonly string[]).includes(id);
}

export function isTitanMaxFortificationBuff(id: string): id is typeof TITAN_A_MAX_FORTIFICATION_BUFF_ID {
  return id === TITAN_A_MAX_FORTIFICATION_BUFF_ID;
}

export function isCerritosSupportBuff(id: string): id is typeof CERRITOS_SUPPORT_BUFF_ID {
  return id === CERRITOS_SUPPORT_BUFF_ID;
}

export function isDefiantReinforceBuff(id: string): id is typeof DEFIANT_REINFORCE_BUFF_ID {
  return id === DEFIANT_REINFORCE_BUFF_ID;
}

const TITAN_A_FORTIFY_OPTIONS = [
  {
    id: "titan_a_fortification" satisfies TitanAFortifySupportBuffId,
    label: "Fortification",
    description:
      "Titan-A Fortify: Fortifies your ships and 2–13 alliance ships (+25% Critical Hit Damage). Checking this marks you Fortified for combat research that requires Fortify.",
  },
  {
    id: "titan_a_max_fortification" satisfies TitanAFortifySupportBuffId,
    label: "Max fortification",
    description:
      "Titan-A Fortify (max): all Fortified effects +250% base weapon damage. Checking this marks you Fortified for combat research that requires Fortify.",
  },
] as const;

const CERRITOS_SUPPORT_OPTION = {
  id: CERRITOS_SUPPORT_BUFF_ID,
  label: "Cerritos Support",
  description:
    "Marks Cerritos-supported combat research as active (catalog nodes gated to this alliance buff).",
} as const;

const DEFIANT_REINFORCE_OPTION = {
  id: DEFIANT_REINFORCE_BUFF_ID,
  label: "Defiant Reinforce",
  description:
    "Marks Defiant-reinforced combat research as active (catalog nodes gated to this buff).",
} as const;

/** Alliance / ship support buffs selectable in the workspace (sent to the API as `support_buffs`). */
export const SUPPORT_BUFF_OPTIONS = [
  ...TITAN_A_FORTIFY_OPTIONS,
  CERRITOS_SUPPORT_OPTION,
  DEFIANT_REINFORCE_OPTION,
] as const;

export type SupportBuffId = (typeof SUPPORT_BUFF_OPTIONS)[number]["id"];
