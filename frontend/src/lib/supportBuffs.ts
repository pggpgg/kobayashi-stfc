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
  static_bonus_target?:
    | "attacker"
    | "defender_if_player_opponent"
    | "attacker_debuff_if_player_opponent";
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
export const TITAN_A_MAX_FORTIFICATION_BUFF_ID =
  "titan_a_max_fortification" as const;

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

/** Mantis sting placeholder id (`data/support_buffs.json`); static stats TBD. */
export const MANTIS_STING_BUFF_ID = "mantis_sting" as const;

/**
 * Ids whose direct `static_bonuses` merge onto the defender in PvP-shaped runs.
 * Keep aligned with `static_bonus_target: defender_if_player_opponent` in `data/support_buffs.json`.
 */
export const SUPPORT_BUFF_DEFENDER_ROUTED_WHEN_PLAYER_IDS = [
  ...TITAN_A_FORTIFY_SUPPORT_BUFF_IDS,
  DEFIANT_REINFORCE_BUFF_ID,
] as const;

/**
 * Alliance debuff ids applied to the attacker in PvP (`defender_alliance_debuffs` API field).
 * Keep aligned with `static_bonus_target: attacker_debuff_if_player_opponent` in `data/support_buffs.json`.
 */
export const SUPPORT_BUFF_ATTACKER_DEBUFF_WHEN_PLAYER_IDS = [
  MANTIS_STING_BUFF_ID,
] as const;

export type SupportBuffSide = "attacker" | "defender" | "debuff";

export type TitanAFortifySupportBuffId =
  (typeof TITAN_A_FORTIFY_SUPPORT_BUFF_IDS)[number];

export function isTitanAFortifySupportBuff(
  id: string,
): id is TitanAFortifySupportBuffId {
  return (TITAN_A_FORTIFY_SUPPORT_BUFF_IDS as readonly string[]).includes(id);
}

export function isTitanMaxFortificationBuff(
  id: string,
): id is typeof TITAN_A_MAX_FORTIFICATION_BUFF_ID {
  return id === TITAN_A_MAX_FORTIFICATION_BUFF_ID;
}

export function isCerritosSupportBuff(
  id: string,
): id is typeof CERRITOS_SUPPORT_BUFF_ID {
  return id === CERRITOS_SUPPORT_BUFF_ID;
}

export function isDefiantReinforceBuff(
  id: string,
): id is typeof DEFIANT_REINFORCE_BUFF_ID {
  return id === DEFIANT_REINFORCE_BUFF_ID;
}

export function isMantisStingBuff(
  id: string,
): id is typeof MANTIS_STING_BUFF_ID {
  return id === MANTIS_STING_BUFF_ID;
}

/** True when the catalog routes this buff's direct static bonuses to the defender in PvP-shaped runs. */
export function isDefenderRoutedWhenPlayerSupportBuff(id: string): boolean {
  return (
    SUPPORT_BUFF_DEFENDER_ROUTED_WHEN_PLAYER_IDS as readonly string[]
  ).includes(id);
}

export function isAttackerDebuffWhenPlayerSupportBuff(id: string): boolean {
  return (
    SUPPORT_BUFF_ATTACKER_DEBUFF_WHEN_PLAYER_IDS as readonly string[]
  ).includes(id);
}

function staticBonusTargetFor(id: SupportBuffId): NonNullable<
  SupportBuffCatalogEntry["static_bonus_target"]
> {
  const entry = SUPPORT_BUFF_CATALOG.buffs[id];
  return entry?.static_bonus_target ?? "attacker";
}

/** Options for a PvP support-buff side (attacker buffs, defender buffs, or alliance debuffs on attacker). */
export function supportBuffOptionsForSide(
  side: SupportBuffSide,
): readonly SupportBuffOption[] {
  return SUPPORT_BUFF_OPTIONS.filter((option) => {
    const target = staticBonusTargetFor(option.id);
    if (side === "attacker") {
      return target === "attacker";
    }
    if (side === "defender") {
      return target === "defender_if_player_opponent";
    }
    return target === "attacker_debuff_if_player_opponent";
  });
}

export const SUPPORT_BUFF_OPTION_IDS = [
  "titan_a_fortification",
  "titan_a_max_fortification",
  CERRITOS_SUPPORT_BUFF_ID,
  DEFIANT_REINFORCE_BUFF_ID,
  MANTIS_STING_BUFF_ID,
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
    throw new Error(
      `Support buff catalog entry is missing display metadata: ${id}`,
    );
  }
  if (!entry.provenance_notes?.some((note) => note.length > 0)) {
    throw new Error(
      `Support buff catalog entry is missing provenance notes: ${id}`,
    );
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

const SUPPORT_BUFF_OPTIONS_BY_ID = new Map<string, SupportBuffOption>(
  SUPPORT_BUFF_OPTIONS.map((option) => [option.id, option]),
);

export type SupportBuffSelectionIssueType =
  | "duplicate"
  | "incompatible"
  | "unsupported";

export interface SupportBuffSelectionIssue {
  type: SupportBuffSelectionIssueType;
  id: string;
  keptId?: SupportBuffId;
  message: string;
}

export interface SupportBuffSelectionValidation {
  ids: SupportBuffId[];
  issues: SupportBuffSelectionIssue[];
}

export function isSupportBuffId(id: string): id is SupportBuffId {
  return SUPPORT_BUFF_OPTIONS_BY_ID.has(id);
}

export function supportBuffLabel(id: string): string {
  return SUPPORT_BUFF_OPTIONS_BY_ID.get(id)?.label ?? id;
}

/**
 * Canonicalize support-buff ids before UI state or API requests use them.
 * This mirrors server resolution: trim, drop unsupported ids, dedupe, and let
 * the highest-priority member of an exclusive group win.
 */
export function normalizeSupportBuffSelection(
  ids: readonly string[] | undefined,
): SupportBuffSelectionValidation {
  const issues: SupportBuffSelectionIssue[] = [];
  const known: SupportBuffId[] = [];
  const seen = new Set<SupportBuffId>();

  for (const rawId of ids ?? []) {
    const id = rawId.trim();
    if (!id) {
      continue;
    }
    if (!isSupportBuffId(id)) {
      issues.push({
        type: "unsupported",
        id,
        message: `Unsupported support buff "${id}" was ignored.`,
      });
      continue;
    }
    if (seen.has(id)) {
      issues.push({
        type: "duplicate",
        id,
        keptId: id,
        message: `${supportBuffLabel(id)} was selected more than once; duplicates were ignored.`,
      });
      continue;
    }
    seen.add(id);
    known.push(id);
  }

  const membersByGroup = new Map<string, SupportBuffId[]>();
  for (const id of known) {
    const option = SUPPORT_BUFF_OPTIONS_BY_ID.get(id);
    if (option?.exclusiveGroup) {
      const members = membersByGroup.get(option.exclusiveGroup) ?? [];
      members.push(id);
      membersByGroup.set(option.exclusiveGroup, members);
    }
  }

  const remove = new Set<SupportBuffId>();
  for (const members of membersByGroup.values()) {
    if (members.length <= 1) {
      continue;
    }

    let winner = members[0];
    let winnerIndex = known.indexOf(winner);
    let winnerPriority = SUPPORT_BUFF_OPTIONS_BY_ID.get(winner)?.priority ?? 0;

    for (const id of members.slice(1)) {
      const priority = SUPPORT_BUFF_OPTIONS_BY_ID.get(id)?.priority ?? 0;
      const index = known.indexOf(id);
      if (
        priority > winnerPriority ||
        (priority === winnerPriority && index > winnerIndex)
      ) {
        winner = id;
        winnerIndex = index;
        winnerPriority = priority;
      }
    }

    for (const id of members) {
      if (id === winner) {
        continue;
      }
      remove.add(id);
      issues.push({
        type: "incompatible",
        id,
        keptId: winner,
        message: `${supportBuffLabel(id)} conflicts with ${supportBuffLabel(winner)}; using ${supportBuffLabel(winner)}.`,
      });
    }
  }

  const normalized = known.filter((id) => !remove.has(id));
  normalized.sort();

  return {
    ids: normalized,
    issues,
  };
}
