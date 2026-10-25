import assert from "node:assert/strict";
import test from "node:test";
import { hostileCriticalSourceMatches, shipCriticalSourceMatches } from "../build_native_critical_damage_manifest.mjs";

const components = [
  { data: { tag: "Weapon", crit_modifier: 1.5 } },
  { data: { tag: "Weapon", crit_modifier: 2 } },
];

test("critical-source migration requires matching native scalar and every weapon", () => {
  const raw = { id: 123, stats: { critical_damage: 1.5 }, components };
  const normalized = { id: "123", crit_damage: 1.5, components };
  assert.ok(hostileCriticalSourceMatches(normalized, raw));
  assert.equal(hostileCriticalSourceMatches({ ...normalized, crit_damage: 2.5 }, raw), false);
  assert.equal(hostileCriticalSourceMatches({ ...normalized, id: "manual" }, raw), false);
  assert.equal(hostileCriticalSourceMatches({ ...normalized, components: components.slice(0, 1) }, raw), false);
  assert.equal(hostileCriticalSourceMatches({ ...normalized, components: [] }, { ...raw, components: [] }), false);
});

test("critical-source ship migration rejects any unmatched tier or weapon", () => {
  const raw = { tiers: [{ tier: 1, components }] };
  const normalized = { tiers: [{ tier: 1, crit_damage: 1.5,
    weapons: [{ crit_multiplier: 1.5 }, { crit_multiplier: 2 }] }] };
  assert.ok(shipCriticalSourceMatches(normalized, raw));
  assert.equal(shipCriticalSourceMatches({ tiers: [{ ...normalized.tiers[0], tier: 2 }] }, raw), false);
  assert.equal(shipCriticalSourceMatches({ tiers: [{ ...normalized.tiers[0],
    weapons: [{ crit_multiplier: 1.5 }, { crit_multiplier: 3 }] }] }, raw), false);
});

test("critical-source comparison follows native weapon firing order", () => {
  const raw = { tiers: [{ tier: 1, components: [
    { order: 2, data: { tag: "Weapon", crit_modifier: 1.1 } },
    { order: 1, data: { tag: "Weapon", crit_modifier: 1.5 } },
  ] }] };
  const normalized = { tiers: [{ tier: 1, crit_damage: 1.5,
    weapons: [{ crit_multiplier: 1.5 }, { crit_multiplier: 1.1 }] }] };
  assert.ok(shipCriticalSourceMatches(normalized, raw));
});
