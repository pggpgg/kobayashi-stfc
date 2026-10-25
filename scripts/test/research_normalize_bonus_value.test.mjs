import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { normalizeBonusValue } from "../lib/research_normalize_bonus_value.mjs";

test("unknown percentage unit cannot jump when a raw rank crosses one", () => {
  const buff = { id: 999999999, value_is_percentage: true, show_percentage: true };
  const mapping = { stat: "weapon_damage" };
  const values = [0.8, 1, 1.2].map(raw => normalizeBonusValue(buff, mapping, raw));
  assert.deepEqual(values, [null, null, null],
    `unitless percentage values must be quarantined, not guessed as ${values.join(", ")}`);
});

test("native source-backed fractions remain raw regardless of display flags", () => {
  const mapping = { stat: "weapon_damage", value_unit: "fraction", requires_defender_burning: true };
  for (const buff of [
    { value_is_percentage: true, show_percentage: true, id: 1898558353 },
    { value_is_percentage: true, show_percentage: false },
  ]) {
    assert.equal(normalizeBonusValue(buff, mapping, 1), 1);
    assert.equal(normalizeBonusValue(buff, mapping, 9), 9);
    assert.equal(normalizeBonusValue(buff, mapping, 1.45), 1.45);
  }
});

test("explicit absolute units preserve finite values", () => {
  assert.equal(normalizeBonusValue({ value_is_percentage: true },
    { stat: "hull_hp_flat", value_unit: "absolute" }, 3400), 3400);
});

test("explicit percentage-point units convert without magnitude thresholds", () => {
  const mapping = { stat: "weapon_damage", value_unit: "percentage_points" };
  for (const buff of [
    { value_is_percentage: true, show_percentage: true },
    { value_is_percentage: true, show_percentage: false },
  ]) {
    assert.equal(normalizeBonusValue(buff, mapping, 0.05), 0.0005);
    assert.equal(normalizeBonusValue(buff, mapping, 1), 0.01);
    assert.equal(normalizeBonusValue(buff, mapping, 130), 1.3);
  }
});

test("native RID 1030229359 rank curve stays fractional across the 1.0 boundary", () => {
  const buff = { id: 936000619, value_is_percentage: true, show_percentage: true };
  const mapping = { stat: "hull_hp", value_unit: "fraction" };
  assert.equal(normalizeBonusValue(buff, mapping, 0.8), 0.8);
  assert.equal(normalizeBonusValue(buff, mapping, 1), 1);
});

test("native RID 1030229359 mapping declares the verified fractional unit", async () => {
  const mappings = JSON.parse(
    await readFile(new URL("../../data/research/buff_id_to_stat.json", import.meta.url), "utf8"),
  );
  assert.equal(mappings["936000619"].value_unit, "fraction");
});

test("percentage flags without a declared unit are rejected for either display flag", () => {
  for (const buff of [
    { value_is_percentage: true, show_percentage: true },
    { value_is_percentage: true, show_percentage: false },
    { value_is_percentage: true },
  ]) {
    for (const value of [0.2, 0.8, 1, 1.3, 5, 130]) {
      assert.equal(normalizeBonusValue(buff, { stat: "hull_hp" }, value), null);
    }
  }
});

test("a reviewed percentage-point fixture can represent Quark Scanner tier-1 (1.3 => 0.013)", () => {
  const buff = { value_is_percentage: true, show_percentage: true };
  const mapping = { stat: "hull_hp", value_unit: "percentage_points" };
  assert.ok(Math.abs(normalizeBonusValue(buff, mapping, 1.3) - 0.013) < 1e-9);
});

test("explicit shield fraction preserves its raw rank (+20% => 0.2)", () => {
  const buff = { value_is_percentage: true, show_percentage: true };
  const mapping = { stat: "shield_hp", value_unit: "fraction" };
  assert.equal(normalizeBonusValue(buff, mapping, 0.2), 0.2);
});

test("explicit percentage-point unit normalizes large tier hull values without display flags", () => {
  const buff = { value_is_percentage: true, show_percentage: false };
  const mapping = { stat: "hull_hp", value_unit: "percentage_points" };
  assert.equal(normalizeBonusValue(buff, mapping, 130), 1.3);
});

test("non-percentage-flagged percentage points still require an explicit mapping unit", () => {
  const buff = { value_is_percentage: false };
  const mapping = { stat: "hull_hp", value_unit: "percentage_points" };
  assert.equal(normalizeBonusValue(buff, mapping, 500), 5);
  assert.equal(normalizeBonusValue(buff, mapping, 5000), 50);
});
