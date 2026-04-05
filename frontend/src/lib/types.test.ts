import { describe, expect, it } from "vitest";
import {
  belowDeckSlotCount,
  createEmptyCrew,
  createEmptyPins,
  DEFAULT_BELOW_DECK_UNLOCK_LEVELS,
} from "./types";

describe("belowDeckSlotCount", () => {
  it("matches STFC default unlock ladder (no per-ship schedule)", () => {
    expect(belowDeckSlotCount(1)).toBe(0);
    expect(belowDeckSlotCount(4)).toBe(0);
    expect(belowDeckSlotCount(5)).toBe(1);
    expect(belowDeckSlotCount(9)).toBe(1);
    expect(belowDeckSlotCount(10)).toBe(2);
    expect(belowDeckSlotCount(19)).toBe(2);
    expect(belowDeckSlotCount(20)).toBe(3);
    expect(belowDeckSlotCount(29)).toBe(3);
    expect(belowDeckSlotCount(30)).toBe(4);
    expect(belowDeckSlotCount(39)).toBe(4);
    expect(belowDeckSlotCount(40)).toBe(5);
    expect(belowDeckSlotCount(44)).toBe(5);
    expect(belowDeckSlotCount(45)).toBe(6);
    expect(belowDeckSlotCount(54)).toBe(6);
    expect(belowDeckSlotCount(55)).toBe(7);
    expect(belowDeckSlotCount(60)).toBe(7);
  });

  it("uses custom unlock levels when provided", () => {
    const custom = [10, 20];
    expect(belowDeckSlotCount(5, custom)).toBe(0);
    expect(belowDeckSlotCount(10, custom)).toBe(1);
    expect(belowDeckSlotCount(25, custom)).toBe(2);
  });

  it("treats empty custom schedule like missing (falls back to default)", () => {
    expect(belowDeckSlotCount(30, [])).toBe(
      belowDeckSlotCount(30, DEFAULT_BELOW_DECK_UNLOCK_LEVELS),
    );
  });
});

describe("createEmptyCrew", () => {
  it("returns crew with null captain, bridge of 2 nulls, and belowDeck length by ship level", () => {
    const crew0 = createEmptyCrew(4);
    expect(crew0.belowDeck).toHaveLength(0);

    const crew1 = createEmptyCrew(10);
    expect(crew1.captain).toBeNull();
    expect(crew1.bridge).toEqual([null, null]);
    expect(crew1.belowDeck).toHaveLength(2);
    expect(crew1.belowDeck).toEqual([null, null]);

    const crew2 = createEmptyCrew(25);
    expect(crew2.belowDeck).toHaveLength(3);

    const crew3 = createEmptyCrew(50);
    expect(crew3.belowDeck).toHaveLength(6);
  });
});

describe("createEmptyPins", () => {
  it("returns pins with all false, belowDeck length by ship level", () => {
    const pins1 = createEmptyPins(10);
    expect(pins1.captain).toBe(false);
    expect(pins1.bridge).toEqual([false, false]);
    expect(pins1.belowDeck).toHaveLength(2);
    expect(pins1.belowDeck).toEqual([false, false]);

    const pins2 = createEmptyPins(50);
    expect(pins2.belowDeck).toHaveLength(6);
  });
});
