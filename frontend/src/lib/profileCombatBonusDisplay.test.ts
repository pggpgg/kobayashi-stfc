import { describe, expect, it } from "vitest";
import {
  formatProfileCombatBonusDelta,
  formatProfileCombatBonusEntry,
  formatProfileCombatBonusListValue,
  profileCombatBonusDisplayMode,
} from "./profileCombatBonusDisplay";

describe("profileCombatBonusDisplay", () => {
  it("treats apex_barrier as flat", () => {
    expect(profileCombatBonusDisplayMode("apex_barrier")).toBe("flat");
    expect(formatProfileCombatBonusListValue("apex_barrier", 25695)).toBe(
      "25,695 flat",
    );
    expect(formatProfileCombatBonusEntry("apex_barrier", 2500)).toBe(
      "apex_barrier +2,500",
    );
  });

  it("treats fractional stats as percent", () => {
    expect(formatProfileCombatBonusEntry("weapon_damage", 0.05)).toBe(
      "weapon_damage +5.00%",
    );
    expect(formatProfileCombatBonusListValue("hull_hp", 31.29)).toBe(
      "3129.00% additive",
    );
  });

  it("treats crit_damage_floor as multiplier", () => {
    expect(formatProfileCombatBonusDelta("crit_damage_floor", 1.5)).toBe(
      "+1.5×",
    );
  });
});
