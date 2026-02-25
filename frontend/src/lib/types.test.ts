import { describe, it, expect } from 'vitest';
import {
  belowDeckSlotCount,
  createEmptyCrew,
  createEmptyPins,
} from './types';

describe('belowDeckSlotCount', () => {
  it('returns 1 for ship level below 25', () => {
    expect(belowDeckSlotCount(1)).toBe(1);
    expect(belowDeckSlotCount(24)).toBe(1);
  });

  it('returns 2 for ship level 25 to 49', () => {
    expect(belowDeckSlotCount(25)).toBe(2);
    expect(belowDeckSlotCount(49)).toBe(2);
  });

  it('returns 3 for ship level 50 and above', () => {
    expect(belowDeckSlotCount(50)).toBe(3);
    expect(belowDeckSlotCount(60)).toBe(3);
  });
});

describe('createEmptyCrew', () => {
  it('returns crew with null captain, bridge of 2 nulls, and belowDeck length by ship level', () => {
    const crew1 = createEmptyCrew(10);
    expect(crew1.captain).toBeNull();
    expect(crew1.bridge).toEqual([null, null]);
    expect(crew1.belowDeck).toHaveLength(1);
    expect(crew1.belowDeck).toEqual([null]);

    const crew2 = createEmptyCrew(25);
    expect(crew2.belowDeck).toHaveLength(2);
    expect(crew2.belowDeck).toEqual([null, null]);

    const crew3 = createEmptyCrew(50);
    expect(crew3.belowDeck).toHaveLength(3);
    expect(crew3.belowDeck).toEqual([null, null, null]);
  });
});

describe('createEmptyPins', () => {
  it('returns pins with all false, belowDeck length by ship level', () => {
    const pins1 = createEmptyPins(10);
    expect(pins1.captain).toBe(false);
    expect(pins1.bridge).toEqual([false, false]);
    expect(pins1.belowDeck).toHaveLength(1);
    expect(pins1.belowDeck).toEqual([false]);

    const pins2 = createEmptyPins(50);
    expect(pins2.belowDeck).toHaveLength(3);
    expect(pins2.belowDeck).toEqual([false, false, false]);
  });
});
