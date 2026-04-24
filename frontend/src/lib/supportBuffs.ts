import supportBuffCatalogJson from "../../../data/support_buffs.json";

interface SupportBuffCatalogEntry {
  id?: string;
  label?: string;
  display_name?: string;
  description?: string;
  source?: string;
  provenance_notes?: string[];
  exclusive_group?: string;
  priority?: number;
  stat_targets?: SupportBuffStatTarget[];
}

interface SupportBuffCatalog {
  buffs: Record<string, SupportBuffCatalogEntry>;
}

export interface SupportBuffStatTarget {
  stat: string;
  value: number;
  stacking: "additive" | "multiplicative";
  layer?: string;
}

const SUPPORT_BUFF_CATALOG = supportBuffCatalogJson as SupportBuffCatalog;

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

const SUPPORT_BUFF_OPTION_IDS = [
  "titan_a_fortification",
  "titan_a_max_fortification",
  CERRITOS_SUPPORT_BUFF_ID,
  DEFIANT_REINFORCE_BUFF_ID,
] as const;

export type SupportBuffId = (typeof SUPPORT_BUFF_OPTION_IDS)[number];

export interface SupportBuffOption {
  id: SupportBuffId;
  label: string;
  description: string;
  source: string;
  provenanceNotes: readonly string[];
  statTargets: readonly SupportBuffStatTarget[];
  exclusiveGroup?: string;
  priority: number;
}

function optionFromCatalog(id: SupportBuffId): SupportBuffOption {
  const entry = SUPPORT_BUFF_CATALOG.buffs[id];
  if (!entry) {
    throw new Error(`Missing support buff catalog entry: ${id}`);
  }
  if (entry.id !== id) {
    throw new Error(`Support buff catalog entry id mismatch: ${id}`);
  }
  const displayName = entry.display_name ?? entry.label;
  if (!displayName || !entry.description || !entry.source) {
    throw new Error(`Support buff catalog entry is missing display metadata: ${id}`);
  }
  if (!entry.provenance_notes?.some((note) => note.length > 0)) {
    throw new Error(`Support buff catalog entry is missing provenance notes: ${id}`);
  }
  return {
    id,
    label: displayName,
    description: entry.description,
    source: entry.source,
    provenanceNotes: entry.provenance_notes,
    statTargets: entry.stat_targets ?? [],
    exclusiveGroup: entry.exclusive_group,
    priority: entry.priority ?? 0,
  };
}

/** Alliance / ship support buffs selectable in the workspace (sent to the API as `support_buffs`). */
export const SUPPORT_BUFF_OPTIONS: readonly SupportBuffOption[] =
  SUPPORT_BUFF_OPTION_IDS.map(optionFromCatalog);
