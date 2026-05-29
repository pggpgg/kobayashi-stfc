import test from "node:test";
import assert from "node:assert/strict";

import {
  categorizeResearchDescription,
  isSuspectGlobalScopeCategory,
  shouldExcludeUnconditionalGlobalMerge,
} from "../lib/research_scope_categorize.mjs";

test("categorizeResearchDescription detects armada-only scope", () => {
  assert.equal(
    categorizeResearchDescription("Increases Isolytic Damage against Armadas"),
    "armada_scope"
  );
  assert.ok(isSuspectGlobalScopeCategory("armada_scope"));
});

test("categorizeResearchDescription detects economy meta", () => {
  assert.equal(
    categorizeResearchDescription("Base construction speed is increased for all buildings."),
    "economy_meta"
  );
  assert.ok(!isSuspectGlobalScopeCategory("economy_meta"));
});

test("categorizeResearchDescription detects station defense scope", () => {
  assert.equal(
    categorizeResearchDescription("Increases base Damage against Defense Platforms."),
    "station_defense_scope"
  );
});

test("non-Armada hostile bonuses are not suspect global scopes", () => {
  const cat = categorizeResearchDescription(
    "Increases the Apex Barrier vs Hostiles (not Armadas) of your deployed ships."
  );
  assert.equal(cat, "non_armada_hostile_scope");
  assert.ok(!isSuspectGlobalScopeCategory(cat));
});

test("hostiles and armadas dual scope is not suspect", () => {
  const cat = categorizeResearchDescription(
    "Increases Critical Hit Damage for G5+ FKR ships against Hostiles and Armadas."
  );
  assert.equal(cat, "hostile_and_armada_scope");
  assert.ok(!isSuspectGlobalScopeCategory(cat));
});

test("shouldExcludeUnconditionalGlobalMerge skips armada-only unconditional rows", () => {
  const desc = new Map([
    [1, "Increases base Damage against Exchange Armadas"],
  ]);
  assert.ok(
    shouldExcludeUnconditionalGlobalMerge({
      mapping: { stat: "weapon_damage", operator: "add" },
      buff: { loca_id: 1 },
      projectLocaId: null,
      descriptionByLocaId: desc,
    })
  );
});

test("shouldExcludeUnconditionalGlobalMerge keeps faction-gated rows", () => {
  const desc = new Map([
    [1, "Increases base Damage against Exchange Armadas"],
  ]);
  assert.ok(
    !shouldExcludeUnconditionalGlobalMerge({
      mapping: {
        stat: "weapon_damage",
        operator: "add",
        attacker_faction: "federation",
      },
      buff: { loca_id: 1 },
      projectLocaId: null,
      descriptionByLocaId: desc,
    })
  );
});

test("shouldExcludeUnconditionalGlobalMerge keeps non-Armada hostile rows", () => {
  const desc = new Map([
    [1, "Increases base Weapon Damage against non-Armada Hostiles"],
  ]);
  assert.ok(
    !shouldExcludeUnconditionalGlobalMerge({
      mapping: { stat: "weapon_damage", operator: "add" },
      buff: { loca_id: 1 },
      projectLocaId: null,
      descriptionByLocaId: desc,
    })
  );
});
