import assert from "node:assert/strict";
import test from "node:test";
import {
  inferCombatStatFromDescription,
  inferCombatStatFromProjectName,
} from "../lib/research_stat_inference.mjs";

test("description: shield deflection maps to shield_deflection, not dodge", () => {
  assert.equal(
    inferCombatStatFromDescription("Increases Shield Deflection of your ships."),
    "shield_deflection"
  );
});

test("description: dodge without deflection maps to dodge", () => {
  assert.equal(inferCombatStatFromDescription("Increases Dodge of your ships."), "dodge");
});

test("description: shield mitigation phrase maps to shield_mitigation", () => {
  assert.equal(
    inferCombatStatFromDescription("Increases Shield Mitigation for all ships."),
    "shield_mitigation"
  );
});

test("description: mitigation stats UI line does not collapse to a single stat", () => {
  assert.equal(
    inferCombatStatFromDescription(
      "Increases Mitigation Stats (Armor, Shield Deflection, and Dodge) for all ships."
    ),
    null
  );
});

test("description: morale isolytic offense maps to isolytic_damage + requires_morale shape", () => {
  assert.deepEqual(
    inferCombatStatFromDescription(
      "Increases Isolytic Damage while Morale is active during combat."
    ),
    { stat: "isolytic_damage", requires_morale: true }
  );
});

test("description: officer single-axis stats map to officer_* profile keys", () => {
  assert.equal(
    inferCombatStatFromDescription("Base Attack is increased for all Officers."),
    "officer_attack"
  );
  assert.equal(
    inferCombatStatFromDescription("Increases the base defense stat of all officers."),
    "officer_defense"
  );
  assert.equal(
    inferCombatStatFromDescription("Increases base health of all officers."),
    "officer_health"
  );
  assert.equal(
    inferCombatStatFromDescription(
      "Increase base Attack, Defense, and Health for all Officers."
    ),
    null
  );
});

test("project name: shield deflection before dodge / bare deflection", () => {
  assert.equal(inferCombatStatFromProjectName("Prime Shield Deflection"), "shield_deflection");
  assert.equal(inferCombatStatFromProjectName("Shield Mitigation Bonus"), "shield_mitigation");
  assert.equal(inferCombatStatFromProjectName("Dodge Training"), "dodge");
});
