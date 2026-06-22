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
