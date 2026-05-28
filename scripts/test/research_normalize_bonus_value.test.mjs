import assert from "node:assert/strict";
import test from "node:test";
import { normalizeBonusValue } from "../lib/research_normalize_bonus_value.mjs";

test("show_percentage percentage points: 1 => 0.01 (NS Burning Damage pattern)", () => {
  const buff = { value_is_percentage: true, show_percentage: true, id: 1898558353 };
  const mapping = { stat: "weapon_damage", requires_defender_burning: true };
  assert.equal(normalizeBonusValue(buff, mapping, 1), 0.01);
  assert.equal(normalizeBonusValue(buff, mapping, 9), 0.09);
  assert.ok(Math.abs(normalizeBonusValue(buff, mapping, 1.45) - 0.0145) < 1e-9);
});

test("show_percentage preserves sub-1 fractional values", () => {
  const buff = { value_is_percentage: true, show_percentage: true };
  const mapping = { stat: "weapon_damage" };
  assert.equal(normalizeBonusValue(buff, mapping, 0.05), 0.05);
});

test("value_is_percentage without show_percentage keeps 1..1.5 as literal fraction", () => {
  const buff = { value_is_percentage: true, show_percentage: false };
  const mapping = { stat: "hull_hp" };
  assert.equal(normalizeBonusValue(buff, mapping, 1), 1);
  assert.equal(normalizeBonusValue(buff, mapping, 1.5), 1.5);
});

test("large percentage points divide by 100 without show_percentage flag", () => {
  const buff = { value_is_percentage: true };
  const mapping = { stat: "weapon_damage" };
  assert.equal(normalizeBonusValue(buff, mapping, 5), 0.05);
});

test("hull_hp show_percentage: Quark Scanner tier-1 (1.3 => 0.013)", () => {
  const buff = { value_is_percentage: true, show_percentage: true };
  const mapping = { stat: "hull_hp" };
  assert.ok(Math.abs(normalizeBonusValue(buff, mapping, 1.3) - 0.013) < 1e-9);
});

test("shield_hp preserves fractional upstream (+20% => 0.2)", () => {
  const buff = { value_is_percentage: true, show_percentage: true };
  const mapping = { stat: "shield_hp" };
  assert.equal(normalizeBonusValue(buff, mapping, 0.2), 0.2);
});

test("hull_hp large tier percentage points without show_percentage (130 => 1.3)", () => {
  const buff = { value_is_percentage: true, show_percentage: false };
  const mapping = { stat: "hull_hp" };
  assert.equal(normalizeBonusValue(buff, mapping, 130), 1.3);
});

test("hull_hp non-pct integer percentage points up to 10000", () => {
  const buff = { value_is_percentage: false };
  const mapping = { stat: "hull_hp" };
  assert.equal(normalizeBonusValue(buff, mapping, 500), 5);
  assert.equal(normalizeBonusValue(buff, mapping, 5000), 50);
});
