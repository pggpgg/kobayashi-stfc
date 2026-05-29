import test from "node:test";
import assert from "node:assert/strict";

import {
  categorizeResearchDescription,
  isSuspectGlobalScopeCategory,
} from "../lib/research_scope_categorize.mjs";

test("categorizeResearchDescription detects armada scope", () => {
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
