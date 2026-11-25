// Combat scenarios ("enemy types") for officer eligibility. The ids are the snake_case
// serialization of the backend `EnemyType` enum (src/combat/types.rs). Order is the display order
// for the scenario selector. Loot/Utility are intentionally excluded — they are not combat targets.

export type EnemyTypeId =
  | "pvp_space"
  | "pvp_station"
  | "red_moving_space"
  | "waves"
  | "mission_bosses"
  | "q_trial"
  | "solo_armadas"
  | "group_armadas"
  | "assaults"
  | "invading_entities"
  | "outpost_armadas"
  | "outpost_retaliation_attackers";

export interface EnemyTypeOption {
  id: EnemyTypeId;
  label: string;
  icon: string;
}

export const ENEMY_TYPE_OPTIONS: readonly EnemyTypeOption[] = [
  { id: "red_moving_space", label: "Non-Armada Hostiles", icon: "🔴" },
  { id: "mission_bosses", label: "Mission Bosses", icon: "⭐" },
  { id: "solo_armadas", label: "Solo Armadas", icon: "🛡️" },
  { id: "group_armadas", label: "Group Armadas", icon: "⚔️" },
  { id: "waves", label: "Wave Defense", icon: "🌊" },
  { id: "q_trial", label: "Q's Trial", icon: "❓" },
  { id: "assaults", label: "Assaults", icon: "💥" },
  { id: "invading_entities", label: "Invading Entities", icon: "👾" },
  { id: "outpost_armadas", label: "Outpost Armadas", icon: "🏰" },
  {
    id: "outpost_retaliation_attackers",
    label: "Outpost Retaliators",
    icon: "🎯",
  },
  { id: "pvp_space", label: "PvP (Space)", icon: "🚀" },
  { id: "pvp_station", label: "PvP (Station)", icon: "🛰️" },
] as const;

export const ENEMY_TYPE_BY_ID: ReadonlyMap<string, EnemyTypeOption> = new Map(
  ENEMY_TYPE_OPTIONS.map((o) => [o.id, o]),
);

export function isEnemyTypeId(id: string): id is EnemyTypeId {
  return ENEMY_TYPE_BY_ID.has(id);
}

export function enemyTypeLabel(id: string): string {
  return ENEMY_TYPE_BY_ID.get(id)?.label ?? id;
}

export function enemyTypeIcon(id: string): string {
  return ENEMY_TYPE_BY_ID.get(id)?.icon ?? "⚔️";
}

/** Override options excluding `"waves"` (which has a dedicated additive toggle in the header). */
export const ENEMY_TYPE_OVERRIDE_OPTIONS: readonly EnemyTypeOption[] =
  ENEMY_TYPE_OPTIONS.filter((o) => o.id !== "waves");

const FACTION_LABELS: Readonly<Record<string, string>> = {
  federation: "Federation",
  klingon: "Klingon",
  romulan: "Romulan",
  borg: "Borg",
  cardassian: "Cardassian",
  augment: "Augment",
  dominion: "Dominion",
  mirror_universe: "Mirror Universe",
  assimilated: "Assimilated",
  ex_borg: "Ex-Borg",
  swarm: "Swarm",
  actian: "Actian",
  gorn_hunting_pack: "Gorn Hunting Pack",
  xindi: "Xindi",
  breen: "Breen",
  venari_ral: "Venari Ral",
};

export function factionTagLabel(tag: string | null | undefined): string | null {
  if (!tag) return null;
  const key = tag.trim().toLowerCase();
  if (!key || key === "unknown") return null;
  return FACTION_LABELS[key] ?? key.replace(/_/g, " ");
}

const HOSTILE_TAG_LABELS: Readonly<Record<string, string>> = {
  conqueror_borg: "Conqueror Borg",
  conqueror_borg_suppressor: "Suppressor",
  conqueror_borg_obliterator: "Obliterator",
  aggregation_hostile: "Aggregation",
  gorn_hunter: "Gorn Hunter",
  breen_warship: "Breen Warship",
  texas_class: "Texas-class",
  xindi_aquatic: "Xindi-Aquatic",
  vger_gorn_hunter: "V'Ger Gorn Hunter",
};

export function hostileTagLabel(tag: string): string {
  const key = tag.trim().toLowerCase();
  return HOSTILE_TAG_LABELS[key] ?? key.replace(/_/g, " ");
}

export function shipClassLabel(cls: string | null | undefined): string | null {
  if (!cls) return null;
  const key = cls.trim().toLowerCase();
  if (!key) return null;
  return key.charAt(0).toUpperCase() + key.slice(1);
}

export function isArmadaEnemyType(id: string | null | undefined): boolean {
  return (
    id === "group_armadas" || id === "solo_armadas" || id === "outpost_armadas"
  );
}

/**
 * Mirror backend `resolve_combat_context` additive Wave Defense tag composition for UI display
 * and CrewBuilder live eligibility badges.
 */
export function resolveEffectiveScenarioTags(
  inferredEnemyType?: string | null,
  enemyTypeOverride?: string | null,
  waveDefense?: boolean,
): string[] {
  const inferred = inferredEnemyType?.trim() || "red_moving_space";
  const override = enemyTypeOverride?.trim() || "";
  const wantsWaves = waveDefense === true || override === "waves";
  const base = override && override !== "waves" ? override : inferred;
  const tags: string[] = [base];
  if (wantsWaves && !tags.includes("waves")) {
    tags.push("waves");
  }
  return tags;
}
