import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { extractConsumableBuffSemantics } from "../extract_stfc_consumable_buff_semantics.mjs";
import { extractBuildingBuffSemantics } from "../extract_stfc_building_buff_semantics.mjs";
import { mapClientResearchBuff } from "../lib/research_client_semantics.mjs";

const concat = (...parts) => Buffer.concat(parts);
function varint(value) {
  let n = BigInt(value); const out = [];
  do { out.push(Number(n & 127n) | (n > 127n ? 128 : 0)); n >>= 7n; } while (n);
  return Buffer.from(out);
}
const integer = (field, value) => concat(varint(field * 8), varint(value));
const bytes = (field, value) => concat(varint(field * 8 + 2), varint(value.length), value);
const doubles = (field, values) => {
  const data = Buffer.alloc(values.length * 8);
  values.forEach((value, i) => data.writeDoubleLE(value, i * 8));
  return bytes(field, data);
};
const chance = () => { const data = Buffer.alloc(4); data.writeFloatLE(1); return bytes(13, data); };
const buff = (id, modifier = 61, target = 1) => concat(integer(1, id), integer(7, target), integer(8, 24),
  integer(12, 3), chance(), integer(18, modifier), doubles(21, [0.5, 5, 0]));
const map = (field, id, spec) => bytes(field, concat(integer(1, id), bytes(2, spec)));
const group = (type, payload) => bytes(1, concat(integer(1, type), bytes(2, payload)));

test("consumable extraction excludes research, building and FT sources and opaque player state", () => {
  const source = group(116, concat(...[42, 43, 44, 45].map(id => map(1, id, buff(id)))));
  const input = concat(source, group(66, map(3, 43, buff(43))), group(68, map(1, 44, buff(44))),
    group(141, map(1, 45, buff(45))), bytes(2, Buffer.from("player-secret")),
    group(99, Buffer.from("unknown-secret")));
  const actual = extractConsumableBuffSemantics(input);
  assert.deepEqual(actual, extractConsumableBuffSemantics(input));
  assert.equal(actual.source_group, 116);
  assert.deepEqual(Object.keys(actual.buffs), ["42"]);
  assert.deepEqual(actual.buffs[42].ranked_values, [0.5, 5, 0]);
  assert.deepEqual(mapClientResearchBuff(actual.buffs[42]).map(r => r.stat), ["hull_hp"]);
  assert.doesNotMatch(JSON.stringify(actual), /player-secret|unknown-secret/);
});

test("consumable extraction rejects duplicate IDs, wrong groups and malformed selected fields", () => {
  assert.throws(() => extractConsumableBuffSemantics(group(116, concat(map(1, 42, buff(42)), map(1, 42, buff(42))))), /Invalid buff ID/);
  assert.throws(() => extractConsumableBuffSemantics(group(66, map(3, 42, buff(42)))), /Expected one Consumable/);
  assert.throws(() => extractConsumableBuffSemantics(group(116, map(1, 43, buff(42)))), /Invalid buff ID/);
  assert.throws(() => extractConsumableBuffSemantics(group(116, map(1, 42, concat(integer(1, 42), integer(18, 61))))), /Missing ranked/);
});

test("global extraction selects piercing axes and all-piercing without changing other source ownership", () => {
  const codes = [6, 7, 8, 74];
  const source = group(116, concat(...codes.map(code => map(1, code, buff(code, code)))));
  const actual = extractConsumableBuffSemantics(source);
  assert.deepEqual(Object.keys(actual.buffs), ["6", "7", "8", "74"]);
  for (const [code, expected] of [[6, ["accuracy"]], [7, ["armor_piercing"]],
    [8, ["shield_piercing"]], [74, ["accuracy", "armor_piercing", "shield_piercing"]]]) {
    assert.deepEqual(mapClientResearchBuff(actual.buffs[code]).map(row => [row.stat, row.value_unit]),
      expected.map(stat => [stat, "fraction"]));
    assert.deepEqual(actual.buffs[code].ranked_values, [0.5, 5, 0]);
  }
  const collisions = concat(source, group(66, map(3, 6, buff(6, 6))),
    group(68, map(1, 7, buff(7, 7))), group(141, map(1, 8, buff(8, 8))));
  assert.deepEqual(Object.keys(extractConsumableBuffSemantics(collisions).buffs), ["74"]);
  assert.deepEqual(Object.keys(extractBuildingBuffSemantics(group(68, map(1, 74, buff(74, 74)))).buffs), [],
    "this scoped change does not broaden the building inventory");
});

test("tracked global piercing inventory separates six launch and nine opponent-gated CombatStart definitions", () => {
  const catalog = JSON.parse(fs.readFileSync(new URL("../../data/native_consumable_buff_catalog.json", import.meta.url)));
  const rows = Object.values(catalog.buffs).filter(row => [6, 7, 8, 74].includes(row.modifier_code));
  // Same frozen272 cache, broader selection: all fifteen previously omitted rows are modifier74.
  assert.equal(rows.length, 15);
  const launch = rows.filter(row => mapClientResearchBuff(row).length);
  assert.equal(launch.length, 6);
  for (const row of launch) {
    assert.equal(row.modifier_code, 74);
    assert.equal(row.operation_code, 3);
    assert.equal(row.trigger_code, 24);
    assert.equal(row.target_code, 1);
    assert.deepEqual(row.condition_codes, []);
    assert.deepEqual(row.attribute_fields, []);
    assert.deepEqual(row.unknown_fields, []);
    assert.equal(row.ranked_value_count, 99);
  }
  for (const row of rows.filter(row => !launch.includes(row))) {
    assert.equal(row.modifier_code, 74);
    assert.equal(row.operation_code, 3);
    assert.equal(row.target_code, 1);
    assert.equal(row.trigger_code, 25);
    assert.equal(row.num_rounds ?? 0, 0);
    assert.deepEqual(row.condition_codes, [27, 131]);
    assert.deepEqual(row.attribute_fields, []);
    assert.deepEqual(row.unknown_fields, []);
    assert.equal(row.ranked_value_type, 0);
    assert.equal(row.ranked_value_count, 99);
    assert.equal(row.ranked_chance_count, 99);
    assert.deepEqual(row.ranked_chances, [1]);
    assert.ok(row.ranked_values.every(value => Number.isFinite(value) && value > 0));
    assert.deepEqual(mapClientResearchBuff(row), [], "CombatStart opponent gates cannot enter the launch pool");
  }
});

test("global Apex inventory retains raw predicates without broadening generic launch eligibility", () => {
  const catalog = JSON.parse(fs.readFileSync(new URL("../../data/native_consumable_buff_catalog.json", import.meta.url)));
  const rows = Object.values(catalog.buffs).filter(row => row.modifier_code === 67001);
  assert.equal(rows.length, 33, "inventory all Apex sources independently of exact runtime eligibility");
  assert.ok(rows.every(row => mapClientResearchBuff(row).length === 0));
  const stella = catalog.buffs[3061559261];
  assert.equal(stella.sha256, "b6e7cca49475cd554ce866576e725b9ab9f54ee15fa5ef1832f37f5b5cc09262");
  assert.deepEqual(stella.condition_codes, [129, 150, 17]);
  assert.deepEqual(stella.attribute_fields, [8]);
  assert.equal(stella.faction_id, 1750120904);
  assert.equal(stella.operation_code, 1, "absolute ADD remains distinct from percentage MULT");
  assert.equal(stella.trigger_code, 25);
  assert.deepEqual(stella.ranked_values, [100]);
  assert.equal(stella.ranked_value_count, 99);
  assert.equal(stella.ranked_chance_count, 99);
  const source = group(116, map(1, 42, buff(42, 67001)));
  assert.ok(Object.hasOwn(extractConsumableBuffSemantics(source).buffs, 42));
  const collision = concat(source, group(66, map(3, 42, buff(42, 67001))));
  assert.ok(!Object.hasOwn(extractConsumableBuffSemantics(collision).buffs, 42));
});

test("reviewed consumable catalog agrees with its native predicates and supported coverage", () => {
  const c = JSON.parse(fs.readFileSync(new URL("../../data/native_consumable_buff_catalog.json", import.meta.url)));
  const research = JSON.parse(fs.readFileSync(new URL("../../data/research/client_buff_semantics_2026-10-20.json", import.meta.url)));
  const stats = {}, supported = Object.entries(c.buffs).filter(([, row]) => mapClientResearchBuff(row).length);
  assert.equal(c.source_group, 116);
  for (const [id, row] of Object.entries(c.buffs)) {
    assert.ok(!Object.hasOwn(research.buffs, id), "research source cannot be reimported as a consumable");
    assert.ok(row.ranked_value_count >= row.ranked_values.length);
    for (const mapping of mapClientResearchBuff(row)) stats[mapping.stat] = (stats[mapping.stat] ?? 0) + 1;
  }
  assert.equal(supported.length, c.coverage.supported_buff_count);
  assert.deepEqual(stats, c.coverage.supported_stats);
  assert.equal(c.coverage.selected_buff_count, c.coverage.supported_buff_count + c.coverage.quarantined_buff_count);
  assert.ok(!Object.hasOwn(c.buffs, 488045092), "Ablative hull must remain on the equipment path");
  assert.ok(!Object.hasOwn(c.buffs, 3207768523), "Ablative shield must remain on the equipment path");
});

test("building extraction selects group68 and never reclassifies another native source", () => {
  const input = concat(group(68, concat(map(1, 42, buff(42)), map(1, 43, buff(43)))),
    group(116, map(1, 43, buff(43))), group(66, map(3, 44, buff(44))), group(141, map(1, 45, buff(45))));
  const actual = extractBuildingBuffSemantics(input);
  assert.equal(actual.source_group, 68);
  assert.deepEqual(Object.keys(actual.buffs), ["42"]);
  assert.deepEqual(actual.excluded_source_groups, [66, 116, 141]);
  assert.equal(actual.coverage.supported_buff_count, 1);
});

test("building-only native loyalty minima require an explicit context-aware mapping opt-in", () => {
  const c = JSON.parse(fs.readFileSync(new URL("../../data/native_building_buff_catalog.json", import.meta.url)));
  for (const [id, minimum] of [[1944989778, 30], [1974652540, 33], [119659407, 35]]) {
    const row = c.buffs[id];
    assert.deepEqual(mapClientResearchBuff(row), [], "research/consumable default cannot discard this account predicate");
    const mapped = mapClientResearchBuff(row, { allowMinSyndicate: true });
    assert.equal(mapped.length, 1);
    assert.equal(mapped[0].attacker_min_syndicate_level, minimum);
  }
  const stats = {}, supported = Object.values(c.buffs).filter(row => mapClientResearchBuff(row, { allowMinSyndicate: true }).length);
  for (const row of Object.values(c.buffs)) {
    for (const m of mapClientResearchBuff(row, { allowMinSyndicate: true })) stats[m.stat] = (stats[m.stat] ?? 0) + 1;
  }
  assert.equal(supported.length, c.coverage.supported_buff_count);
  assert.deepEqual(stats, c.coverage.supported_stats);
});

test("static catalog includes isolytic707 while source groups and scope remain strict", () => {
  const building = JSON.parse(fs.readFileSync(new URL("../../data/native_building_buff_catalog.json", import.meta.url)));
  const consumable = JSON.parse(fs.readFileSync(new URL("../../data/native_consumable_buff_catalog.json", import.meta.url)));
  assert.equal(mapClientResearchBuff(building.buffs[2337243763])[0].stat, "isolytic_damage");
  assert.deepEqual(mapClientResearchBuff(building.buffs[301108821], { allowMinSyndicate: true }), []);
  assert.deepEqual(mapClientResearchBuff(consumable.buffs[1620437919]), []);
  assert.ok(!Object.hasOwn(consumable.buffs, 2337243763));
});

test("static catalogs select isolytic808 ADD and quarantine unknown account scope", () => {
  const building = JSON.parse(fs.readFileSync(new URL("../../data/native_building_buff_catalog.json", import.meta.url)));
  const consumable = JSON.parse(fs.readFileSync(new URL("../../data/native_consumable_buff_catalog.json", import.meta.url)));
  assert.equal(mapClientResearchBuff(building.buffs[469165341])[0].stat, "isolytic_defense");
  assert.deepEqual(mapClientResearchBuff(building.buffs[866732757], { allowMinSyndicate: true }), []);
  assert.deepEqual(mapClientResearchBuff(consumable.buffs[2627193769]), []);
  assert.ok(!Object.hasOwn(consumable.buffs, 469165341));
});

test("native defense catalogs retain launch-only all-axis and individual sources", () => {
  const research = JSON.parse(fs.readFileSync(new URL("../../data/research/client_buff_semantics_2026-10-20.json", import.meta.url)));
  const building = JSON.parse(fs.readFileSync(new URL("../../data/native_building_buff_catalog.json", import.meta.url)));
  const consumable = JSON.parse(fs.readFileSync(new URL("../../data/native_consumable_buff_catalog.json", import.meta.url)));
  assert.deepEqual(mapClientResearchBuff(research.buffs[4281890425]).map(r => r.stat),
    ["armor", "shield_deflection", "dodge"]);
  assert.deepEqual(mapClientResearchBuff(research.buffs[2817985671]), [],
    "global combat-start 1.75 is outside the proven launch pool");
  assert.deepEqual(mapClientResearchBuff(building.buffs[3707508878]).map(r => r.stat), ["armor"]);
  assert.deepEqual(mapClientResearchBuff(building.buffs[3089815719]).map(r => r.stat), ["shield_deflection"]);
  assert.deepEqual(mapClientResearchBuff(building.buffs[1400315411]).map(r => r.stat), ["dodge"]);
  assert.deepEqual(mapClientResearchBuff(consumable.buffs[2413654195]).map(r => r.stat),
    ["armor", "shield_deflection", "dodge"]);
});

test("global and building extraction retain every officer-stat axis without changing units or source ownership", () => {
  for (const [source, extract] of [[116, extractConsumableBuffSemantics], [68, extractBuildingBuffSemantics]]) {
    const input = group(source, concat(...[56, 57, 58, 59].map(code => map(1, code, buff(code, code, 5)))));
    const actual = extract(input);
    assert.deepEqual(Object.keys(actual.buffs), ["56", "57", "58", "59"]);
    for (const [code, stats] of [[56, ["officer_attack"]], [57, ["officer_defense"]],
      [58, ["officer_health"]], [59, ["officer_attack", "officer_defense", "officer_health"]]]) {
      const row = actual.buffs[code];
      assert.deepEqual(row.ranked_values, [0.5, 5, 0]);
      assert.deepEqual(mapClientResearchBuff(row).map(mapping => [mapping.stat, mapping.value_unit]),
        stats.map(stat => [stat, "fraction"]));
    }
    assert.equal(actual.coverage.supported_buff_count, 4);
    const collision = concat(input, group(source === 116 ? 68 : 116, map(1, 56, buff(56, 56, 5))));
    assert.equal(Object.hasOwn(extract(collision).buffs, 56), false);
  }
});

test("tracked global and building officer-stat definitions retain complete source ranks", () => {
  for (const [file, expected] of [["native_consumable_buff_catalog.json", 56], ["native_building_buff_catalog.json", 7]]) {
    const catalog = JSON.parse(fs.readFileSync(new URL(`../../data/${file}`, import.meta.url)));
    const rows = Object.values(catalog.buffs).filter(row => [56, 57, 58, 59].includes(row.modifier_code));
    assert.equal(rows.length, expected);
    for (const row of rows) {
      assert.equal(row.operation_code, 3);
      assert.equal(row.target_code, 5);
      assert.equal(row.trigger_code, 24);
      assert.deepEqual(row.condition_codes, []);
      assert.deepEqual(row.attribute_fields, []);
      assert.deepEqual(row.unknown_fields, []);
      assert.ok(row.ranked_value_count >= row.ranked_values.length);
      assert.ok(row.ranked_chance_count >= row.ranked_chances.length);
      assert.ok(mapClientResearchBuff(row).length > 0);
    }
  }
});
