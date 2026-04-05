export interface CrewState {
  captain: string | null;
  bridge: [string | null, string | null];
  belowDeck: (string | null)[];
}

export interface PinsState {
  captain: boolean;
  bridge: [boolean, boolean];
  belowDeck: boolean[];
}

/**
 * Typical STFC below-decks unlock levels when ship JSON has no `crew_slots` (player ships).
 * Per-ship schedules come from GET /api/ships/:id/tiers-levels → `crew_slots`.
 */
export const DEFAULT_BELOW_DECK_UNLOCK_LEVELS: readonly number[] = [
  5, 10, 20, 30, 40, 45, 55,
];

/** Count below-decks slots unlocked at `shipLevel` using optional per-ship unlock levels. */
export function belowDeckSlotCount(
  shipLevel: number,
  unlockLevels?: readonly number[] | null,
): number {
  const levels =
    unlockLevels != null && unlockLevels.length > 0
      ? [...unlockLevels].sort((a, b) => a - b)
      : [...DEFAULT_BELOW_DECK_UNLOCK_LEVELS];
  return levels.filter((u) => u <= shipLevel).length;
}

export function createEmptyCrew(
  shipLevel: number,
  unlockLevels?: readonly number[] | null,
): CrewState {
  const n = belowDeckSlotCount(shipLevel, unlockLevels);
  return {
    captain: null,
    bridge: [null, null],
    belowDeck: Array(n).fill(null),
  };
}

export function createEmptyPins(
  shipLevel: number,
  unlockLevels?: readonly number[] | null,
): PinsState {
  const n = belowDeckSlotCount(shipLevel, unlockLevels);
  return {
    captain: false,
    bridge: [false, false],
    belowDeck: Array(n).fill(false),
  };
}
