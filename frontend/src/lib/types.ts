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

export function belowDeckSlotCount(shipLevel: number): number {
  if (shipLevel < 25) return 1;
  if (shipLevel < 50) return 2;
  return 3;
}

export function createEmptyCrew(shipLevel: number): CrewState {
  const n = belowDeckSlotCount(shipLevel);
  return {
    captain: null,
    bridge: [null, null],
    belowDeck: Array(n).fill(null),
  };
}

export function createEmptyPins(shipLevel: number): PinsState {
  const n = belowDeckSlotCount(shipLevel);
  return {
    captain: false,
    bridge: [false, false],
    belowDeck: Array(n).fill(false),
  };
}
