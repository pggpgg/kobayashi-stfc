import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import test from "node:test";
import { extractResearchBuffSemantics } from "../extract_stfc_research_buff_semantics.mjs";
import { buildLevelsFromDetail } from "../import_stfcspace_research.mjs";
import { resolveBuffStatMappings } from "../lib/research_buff_resolve.mjs";
import { clientResearchBuffExclusion, mapClientResearchBuff, mapVerifiedOrlopDefenseBuff,
  isVerifiedEclipseDefenseBuff, isVerifiedEclipseDefenseResearchDetail, researchClientBuffCoverage } from "../lib/research_client_semantics.mjs";

const manifest = JSON.parse(fs.readFileSync(new URL("../../data/research/client_buff_semantics_2026-10-20.json", import.meta.url)));
const context = { researchBuffById: {}, commonBuffNormalization: {}, locaIdToStat: {}, clientBuffSemantics: manifest.buffs };
const verified = { modifier_code: 61, operation_code: 3, target_code: 1, trigger_code: 24,
  condition_codes: [], attribute_fields: [], ranked_value_type: 0,
  ranked_values: [0.5, 5, 0], ranked_chances: [1], unknown_fields: [] };

test("native Apex research retains supported scopes and quarantines unknown predicates", () => {
  const apex = { ...verified, modifier_code: 67001, operation_code: 1, trigger_code: 25,
    ranked_values: [500, 800] };
  assert.deepEqual(mapClientResearchBuff(apex)[0], { stat: "apex_barrier", operator: "add",
    value_unit: "absolute", client_verified: true, client_ranked_values: [500, 800] });
  assert.equal(mapClientResearchBuff({ ...apex, condition_codes: [2] })[0].attacker_ship_class, "battleship");
  assert.deepEqual(mapClientResearchBuff({ ...apex, condition_codes: [17] }), []);
  assert.deepEqual(mapClientResearchBuff({ ...apex, trigger_code: 18 }), []);
  assert.deepEqual(mapClientResearchBuff({ ...apex, operation_code: 3 }), []);

  // These independent native definitions include unresolved predicate combinations;
  // an explicit or prose mapping must not make them unconditional.
  for (const id of [1238401736, 3611224086, 2600848580, 3663216413]) {
    const native = manifest.buffs[id];
    assert.equal(native.modifier_code, 67001);
    assert.ok(native.condition_codes.length);
    assert.deepEqual(resolveBuffStatMappings({ ...context,
      researchBuffById: { [id]: "apex_barrier" } }, { id }, null), []);
  }
  // Absent native evidence cannot fall through to an inferred global Apex buff.
  assert.deepEqual(resolveBuffStatMappings({ ...context,
    researchBuffById: { 42: "apex_barrier" } }, { id: 42 }, null), []);
});

test("Eclipse CombatStart verifier binds only the exact native BuffSpec and project", () => {
  const row = manifest.buffs["1238121830"];
  const detail = JSON.parse(fs.readFileSync(new URL("../../data/upstream/data-stfc-space/research/3260398513.json", import.meta.url)));
  assert.equal(isVerifiedEclipseDefenseBuff(1238121830, row), true);
  assert.equal(isVerifiedEclipseDefenseResearchDetail(detail, row), true);
  assert.equal(isVerifiedEclipseDefenseBuff(2117532952, manifest.buffs["2117532952"]), false);
  assert.equal(isVerifiedEclipseDefenseBuff(1238121830, { ...row, faction_id: 157476182 }), false);
  assert.equal(isVerifiedEclipseDefenseBuff(1238121830, { ...row, condition_codes: [17, 27] }), false);
  assert.equal(isVerifiedEclipseDefenseBuff(1238121830, { ...row, attribute_fields: [8, 2] }), false);
  assert.equal(isVerifiedEclipseDefenseBuff(1238121830, { ...row, ranked_values: [...row.ranked_values, 7] }), false);
  assert.equal(isVerifiedEclipseDefenseResearchDetail({ ...detail, id: 999 }, row), false);
  assert.deepEqual(mapClientResearchBuff(row), [], "runtime faction source remains quarantined from static mappings");
  const wrongProject = { ...detail, id: 999 };
  assert.equal(isVerifiedEclipseDefenseResearchDetail(wrongProject, row), false);
  assert.deepEqual(resolveBuffStatMappings(context, { id: 1238121830, loca_id: 317 }, 317), [],
    "native faction-scoped row cannot enter the static research catalog");
});

test("raw native HP percentages, flat HP and critical ADD values bypass display flags", () => {
  for (const [id, rank, stat, expected] of [
    [4191797822, 10, "shield_hp_flat", 4000], [434613423, 10, "hull_hp_flat", 3400],
    [327147356, 2, "crit_chance", 0.02], [1195169860, 8, "crit_damage_flat", 0.1],
  ]) {
    const levels = buildLevelsFromDetail({ max_level: rank,
      buffs: [{ id, value_is_percentage: true, show_percentage: true, values: [{ value: 999999 }] }] },
    { buffResolveCtx: context });
    assert.deepEqual(levels.at(-1).bonuses, [{ stat, value: expected, operator: "add" }]);
  }
  const levels = buildLevelsFromDetail({ max_level: 3,
    buffs: [{ id: 42, values: [{ value: 99 }] }] },
  { buffResolveCtx: { ...context, clientBuffSemantics: { 42: verified } } });
  assert.deepEqual(levels.map((r) => r.bonuses), [
    [{ stat: "hull_hp", value: 0.5, operator: "add" }],
    [{ stat: "hull_hp", value: 5, operator: "add" }], [],
  ]);
});

test("each native ship class predicate survives import without becoming global", () => {
  for (const [code, shipClass] of [[0, "explorer"], [1, "interceptor"], [2, "battleship"], [3, "survey"], [116, "sentinel"]]) {
    const levels = buildLevelsFromDetail({ max_level: 1,
      buffs: [{ id: 42, values: [{ value: 99 }] }] },
    { buffResolveCtx: { ...context, clientBuffSemantics: { 42: { ...verified, condition_codes: [code] } } } });
    assert.deepEqual(levels[0].bonuses, [{ stat: "hull_hp", value: 0.5, operator: "add", attacker_ship_class: shipClass }]);
  }
});

test("native isolytic707 ADD maps as baseline while MULT and unknown scope stay out", () => {
  const add = { ...verified, modifier_code: 707, operation_code: 1,
    ranked_values: [0.015, 0.1, 0.11] };
  assert.equal(mapClientResearchBuff(add)[0].stat, "isolytic_damage");
  assert.deepEqual(mapClientResearchBuff({ ...add, operation_code: 3 }), []);
  assert.deepEqual(mapClientResearchBuff({ ...add, condition_codes: [129] }), []);
  assert.deepEqual(mapClientResearchBuff({ ...add, ranked_chances: [0.5] }), []);
  const levels = buildLevelsFromDetail({ max_level: 3,
    buffs: [{ id: 42, values: [{ value: 99 }] }] },
  { buffResolveCtx: { ...context, clientBuffSemantics: { 42: add } } });
  assert.deepEqual(levels.map((r) => r.bonuses), [0.015, 0.1, 0.11].map(value =>
    [{ stat: "isolytic_damage", value, operator: "add" }]));
});

test("native isolytic808 ADD maps as defense while MULT and conditional scope stay out", () => {
  const add = { ...verified, modifier_code: 808, operation_code: 1,
    ranked_values: [0.12, 0.2, 0.38] };
  assert.equal(mapClientResearchBuff(add)[0].stat, "isolytic_defense");
  assert.deepEqual(mapClientResearchBuff({ ...add, condition_codes: [0] })[0].attacker_ship_class, "explorer");
  assert.deepEqual(mapClientResearchBuff({ ...add, operation_code: 3 }), []);
  assert.deepEqual(mapClientResearchBuff({ ...add, condition_codes: [28] }), []);
  const levels = buildLevelsFromDetail({ max_level: 3,
    buffs: [{ id: 42, values: [{ value: 99 }] }] },
  { buffResolveCtx: { ...context, clientBuffSemantics: { 42: add } } });
  assert.deepEqual(levels.map(r => r.bonuses), [0.12, 0.2, 0.38].map(value =>
    [{ stat: "isolytic_defense", value, operator: "add" }]));
});

test("native launch defense MULT maps all-defense and individual axes without flattening combat-start", () => {
  const launch = { ...verified, modifier_code: 73, operation_code: 3, ranked_values: [0.5, 1.1] };
  assert.deepEqual(mapClientResearchBuff(launch).map(r => r.stat),
    ["armor", "shield_deflection", "dodge"]);
  assert.deepEqual(mapClientResearchBuff({ ...launch, condition_codes: [0] })
    .map(r => r.attacker_ship_class), ["explorer", "explorer", "explorer"]);
  for (const [code, stat] of [[11, "dodge"], [12, "armor"], [13, "shield_deflection"]]) {
    assert.deepEqual(mapClientResearchBuff({ ...launch, modifier_code: code }).map(r => r.stat), [stat]);
  }
  assert.deepEqual(mapClientResearchBuff({ ...launch, trigger_code: 25 }), []);
  assert.deepEqual(mapClientResearchBuff({ ...launch, trigger_code: 18 }), []);
  assert.deepEqual(mapClientResearchBuff({ ...launch, operation_code: 1 }), []);
  assert.deepEqual(mapClientResearchBuff({ ...launch, condition_codes: [123] }), []);
  const levels = buildLevelsFromDetail({ max_level: 2,
    buffs: [{ id: 42, values: [{ value: 999 }] }] },
  { buffResolveCtx: { ...context, clientBuffSemantics: { 42: launch } } });
  assert.deepEqual(levels.map(r => r.bonuses.map(b => b.value)), [[0.5, 0.5, 0.5], [1.1, 1.1, 1.1]]);
});

test("native research penetration ADD resolves exact axis and supported class gates", () => {
  const expected = [
    [3089064337, "accuracy", "explorer"], [3957763486, "accuracy", undefined],
    [2516817778, "armor_piercing", undefined], [2135415291, "armor_piercing", "sentinel"],
    [1014683189, "armor_piercing", "interceptor"],
    [2287504298, "shield_piercing", "sentinel"], [1779738336, "shield_piercing", "battleship"],
    [3666020608, "shield_piercing", undefined],
  ];
  for (const [id, stat, shipClass] of expected) {
    const row = manifest.buffs[id];
    assert.ok(row, `native static cache selection must include ${id}`);
    assert.deepEqual(mapClientResearchBuff(row).map(({ stat: actual, value_unit, attacker_ship_class }) =>
      ({ stat: actual, value_unit, ...(attacker_ship_class ? { attacker_ship_class } : {}) })),
    [{ stat, value_unit: "fraction", ...(shipClass ? { attacker_ship_class: shipClass } : {}) }]);
  }
});

test("native ModAllPiercing keeps only no-condition and exact Direct Hit grade scope", () => {
  const directHit = manifest.buffs[1994654654];
  const unconditional = manifest.buffs[3961476995];
  assert.equal(manifest.provenance.selected_static_cache_build, 273);
  assert.equal(manifest.provenance.consumer_build, 272);
  assert.equal(manifest.provenance.encoded_server_build_binding, false);
  assert.ok(directHit && unconditional, "selected static cache must include reviewed ModAllPiercing rows");
  assert.deepEqual(mapClientResearchBuff(directHit).map(({ stat, attacker_min_grade, value_unit }) =>
    ({ stat, attacker_min_grade, value_unit })), [
    { stat: "accuracy", attacker_min_grade: 4, value_unit: "fraction" },
    { stat: "armor_piercing", attacker_min_grade: 4, value_unit: "fraction" },
    { stat: "shield_piercing", attacker_min_grade: 4, value_unit: "fraction" },
  ]);
  assert.deepEqual(mapClientResearchBuff(unconditional).map(({ stat, value_unit }) =>
    ({ stat, value_unit })), [
    { stat: "accuracy", value_unit: "fraction" },
    { stat: "armor_piercing", value_unit: "fraction" },
    { stat: "shield_piercing", value_unit: "fraction" },
  ]);
  assert.deepEqual(mapClientResearchBuff({ ...directHit, condition_codes: [124] }), []);
  assert.deepEqual(mapClientResearchBuff({ ...directHit, condition_codes: [116] })
    .map(({ stat, attacker_ship_class, attacker_min_grade }) => ({ stat, attacker_ship_class, attacker_min_grade })), [
    { stat: "accuracy", attacker_ship_class: "sentinel", attacker_min_grade: undefined },
    { stat: "armor_piercing", attacker_ship_class: "sentinel", attacker_min_grade: undefined },
    { stat: "shield_piercing", attacker_ship_class: "sentinel", attacker_min_grade: undefined },
  ], "a native class predicate retains the class scope and does not become the direct-hit grade scope");
  assert.deepEqual(mapClientResearchBuff({ ...directHit, attribute_fields: [8] }), []);
  assert.deepEqual(mapClientResearchBuff({ ...directHit, operation_code: 1 }), []);
  for (const id of [62501701, 541802144, 2788053878, 3911070176,
    2610190811, 1960298662, 687745254]) {
    const row = manifest.buffs[id];
    assert.ok(row, `all ModAllPiercing rows in source cohort are retained for explicit quarantine: ${id}`);
    assert.deepEqual(resolveBuffStatMappings({ ...context,
      researchBuffById: { [id]: "pierce" },
      descriptionByLocaId: new Map([[1, "Increases penetration."]]),
    }, { id, loca_id: 1 }, 1), [], `unsupported native scope ${id} must not fall back`);
  }
});

test("unsupported native facts quarantine explicit and heuristic fallback mappings", () => {
  for (const delta of [
    { condition_codes: [999] }, { condition_codes: [0, 1] }, { condition_codes: [0, 999] },
    { target_code: 6 }, { operation_code: 2 }, { trigger_code: 18 },
    { modifier_code: 0 }, { modifier_code: 1 }, { ranked_value_type: 1 },
    { ranked_chances: [0.5] }, { ranked_values: [] }, { ranked_values: [NaN] },
    { attribute_fields: [12] }, { unknown_fields: [99] },
  ]) {
    const row = { ...verified, ...delta };
    assert.ok(clientResearchBuffExclusion(row));
    assert.deepEqual(mapClientResearchBuff(row), []);
    assert.deepEqual(resolveBuffStatMappings({ ...context,
      clientBuffSemantics: { 42: row }, researchBuffById: { 42: "weapon_damage" },
      descriptionByLocaId: new Map([[1, "Increases Critical Hit Chance."]]),
    }, { id: 42, loca_id: 1 }, 1), []);
  }
  // An unverified ID must not receive any selected-family bonus from old text/maps.
  assert.deepEqual(resolveBuffStatMappings({ ...context,
    researchBuffById: { 42: "hull_hp" } }, { id: 42 }), []);
  // Native modifier 7 targets penetration, so old armor prose must not leak
  // into the selected defense family when its ID is not in the 11/12/13/73 manifest.
  assert.deepEqual(resolveBuffStatMappings({ ...context,
    researchBuffById: { 42: "armor" } }, { id: 42 }), []);
  // Explicit prior verified defense work belongs to a different native family.
  assert.deepEqual(resolveBuffStatMappings({ ...context,
    researchBuffById: { 42: { stat: "base_mitigation_stats_bonus", value_unit: "fraction", requires_morale: true } } },
  { id: 42 }), [{ stat: "base_mitigation_stats_bonus", operator: "add", value_unit: "fraction", requires_morale: true }]);
});

test("three native-verified RoundStart defenses retain gated catalog fallback", () => {
  for (const [id, gate] of [
    [679782642, "requires_defender_burning"],
    [1142346453, "requires_morale"],
    [4033066544, "requires_defender_hull_breach"],
  ]) {
    const row = manifest.buffs[id];
    assert.ok(row);
    const mapped = resolveBuffStatMappings({ ...context,
      researchBuffById: { [id]: { stat: "armor", operator: "add" } },
      descriptionByLocaId: new Map([[1, "Increases armor."]]),
    }, { id, loca_id: 1 }, 1);
    assert.deepEqual(mapped, [{ stat: "base_mitigation_stats_bonus", operator: "add",
      value_unit: "fraction", [gate]: true, client_verified: true,
      client_ranked_values: row.ranked_values }]);
    const levels = buildLevelsFromDetail({ max_level: 2,
      buffs: [{ id, values: [{ value: 999 }] }] }, { buffResolveCtx: context });
    assert.deepEqual(levels.at(-1).bonuses, [{ stat: "base_mitigation_stats_bonus",
      value: 0.75, operator: "add", [gate]: true }]);
    for (const delta of [
      { sha256: "wrong" }, { modifier_code: 12 }, { operation_code: 1 },
      { target_code: 6 }, { trigger_code: 25 }, { condition_codes: [27] },
      { ranked_chances: [0.5] }, { ranked_values: [Infinity] },
      { ranked_values: [0.5, 0.76, ...row.ranked_values.slice(2)] },
      { attribute_fields: [] }, { num_rounds: 2 }, { unknown_fields: [99] },
    ]) {
      assert.deepEqual(resolveBuffStatMappings({ ...context,
        clientBuffSemantics: { [id]: { ...row, ...delta } },
        researchBuffById: { [id]: { stat: "base_mitigation_stats_bonus",
          value_unit: "fraction", [gate]: true } },
      }, { id, loca_id: 1 }, 1), []);
    }
  }
  const known = manifest.buffs[679782642];
  assert.deepEqual(resolveBuffStatMappings({ ...context,
    clientBuffSemantics: { 42: known },
    researchBuffById: { 42: { stat: "base_mitigation_stats_bonus", value_unit: "fraction" } },
  }, { id: 42 }), []);
});

test("Orlop Defense preserves its native below-decks target and exact rank-10 fraction", () => {
  const id = 1895868262;
  const native = manifest.buffs[id];
  const evidence = JSON.parse(fs.readFileSync(new URL("../../data/research/verified_orlop_defense_2026-10-23.json", import.meta.url)));
  const detail = JSON.parse(fs.readFileSync(new URL("../../data/upstream/data-stfc-space/research/282179003.json", import.meta.url)));
  const original = detail.buffs[0];
  assert.equal(detail.id, 282179003);
  assert.equal(original.id, id);
  assert.equal(native.sha256, "90eb257edcaceff8659184bcf2cdf1b41d4ce616dded4db5487b3ad806ec11ac");
  assert.equal(evidence.cache_sha256, manifest.provenance.cache_sha256);
  assert.equal(evidence.buff_spec_sha256, native.sha256);
  assert.equal(evidence.research_id, detail.id);
  assert.equal(evidence.project_loca_id, detail.loca_id);
  assert.equal(evidence.target_code, native.target_code);
  assert.deepEqual(evidence.ranked_values, native.ranked_values);
  assert.deepEqual(native.ranked_values, [0.2, 0.26, 0.33, 0.41, 0.5, 0.59, 0.69, 0.79, 0.89, 1, 0]);
  assert.equal(native.target_code, 11);
  assert.deepEqual(mapClientResearchBuff(native), [], "modifier 57 is not a general selected family");
  assert.deepEqual(mapVerifiedOrlopDefenseBuff(id, native).map(m => m.stat), ["officer_defense_below_decks"]);
  const levels = buildLevelsFromDetail(detail, { rid: detail.id, projectLocaId: detail.loca_id,
    buffResolveCtx: { ...context, researchBuffById: { [id]: "officer_defense" },
      descriptionByLocaId: new Map([[detail.loca_id, "Increases Defense for all officers."]]) } });
  assert.deepEqual(levels.map(l => l.bonuses), native.ranked_values.slice(0, 10)
    .map(value => [{ stat: "officer_defense_below_decks", value, operator: "add" }]));

  const nativeContext = { ...context, researchProjectId: detail.id, researchDetailId: detail.id,
    researchBuffById: { [id]: "officer_defense" } };
  const resolve = (buff = original, ctx = nativeContext, loca = detail.loca_id) =>
    resolveBuffStatMappings(ctx, buff, loca);
  assert.equal(resolve()[0].value_unit, "fraction");
  for (const delta of [
    { sha256: "wrong" }, { modifier_code: 59 }, { operation_code: 1 },
    { target_code: 5 }, { trigger_code: 25 }, { condition_codes: [231] },
    { ranked_values: [0.2, 0.26, 0.33, 0.41, 0.5, 0.59, 0.69, 0.79, 0.89, 0.01, 0] },
    { ranked_chances: [0.5] }, { attribute_fields: [2] }, { unknown_fields: [99] },
  ]) {
    assert.deepEqual(resolve(original, { ...nativeContext,
      clientBuffSemantics: { [id]: { ...native, ...delta } } }), []);
  }
  for (const buff of [
    { ...original, show_percentage: false }, { ...original, loca_id: 1 },
    { ...original, values: original.values.map((r, i) => i === 9 ? { ...r, value: 0.01 } : r) },
    { ...original, values: original.values.slice(0, 10) },
  ]) assert.deepEqual(resolve(buff), []);
  assert.deepEqual(resolve(original, { ...nativeContext, researchProjectId: 1 }), []);
  assert.deepEqual(resolve(original, { ...nativeContext, researchDetailId: 1 }), []);
  assert.deepEqual(resolve(original, nativeContext, 1), []);
  assert.deepEqual(resolve(original, { ...nativeContext, clientBuffSemantics: {} }), []);
});

function varint(value) {
  let n = BigInt(value); const bytes = [];
  do { bytes.push(Number(n & 127n) | (n > 127n ? 128 : 0)); n >>= 7n; } while (n);
  return Buffer.from(bytes);
}
const concat = (...parts) => Buffer.concat(parts);
const integer = (number, value) => concat(varint(number * 8), varint(value));
const bytes = (number, value) => concat(varint(number * 8 + 2), varint(value.length), value);
const packed = (number, values, size) => {
  const b = Buffer.alloc(values.length * size);
  values.forEach((v, i) => size === 8 ? b.writeDoubleLE(v, i * size) : b.writeFloatLE(v, i * size));
  return bytes(number, b);
};
const entry = (id, payload) => bytes(3, concat(integer(1, id), bytes(2, payload)));
const target = (code, id, name) => concat(integer(1, code), integer(2, id), bytes(3, Buffer.from(name)));
const targetEntry = (id, value) => bytes(1, concat(integer(1, id), bytes(2, value)));
const cache = (entries) => concat(
  bytes(1, concat(integer(1, 66), bytes(2, entries))),
  bytes(1, concat(integer(1, 54), bytes(2, concat(
    targetEntry(2988263710, target(1, 2988263710, "targ_self_ship")),
    targetEntry(1353979106, target(5, 1353979106, "targ_self_all")),
    targetEntry(307052814, target(11, 307052814, "targ_self_below_deck")),
  )))),
  bytes(2, Buffer.from("player-secret")),
  bytes(1, concat(integer(1, 99), bytes(2, Buffer.from("other-secret")))),
);
const buff = concat(integer(1, 42), integer(7, 1), integer(8, 24), integer(12, 3),
  packed(13, [1, 1, 1], 4), integer(18, 61), packed(21, [0.5, 0, 0], 8));

test("extractor is deterministic, excludes private groups and retains exact rank/byte provenance", () => {
  const input = cache(entry(42, buff));
  const output = extractResearchBuffSemantics(input);
  assert.deepEqual(extractResearchBuffSemantics(input), output);
  assert.deepEqual(output.buffs[42].ranked_values, [0.5, 0]);
  assert.equal(output.buffs[42].ranked_value_count, 3);
  assert.deepEqual(output.buffs[42].ranked_chances, [1]);
  assert.equal(output.buffs[42].ranked_chance_count, 3);
  assert.equal(output.buffs[42].sha256, crypto.createHash("sha256").update(buff).digest("hex"));
  assert.deepEqual(output.static_target_trigger_specs.filter(row => row.group === 54 && [1, 5, 11].includes(row.code))
    .map(row => [row.code, row.name]).sort((a, b) => a[0] - b[0]), [
    [1, "targ_self_ship"], [5, "targ_self_all"], [11, "targ_self_below_deck"],
  ]);
  assert.deepEqual(input.subarray(output.buffs[42].byte_offset,
    output.buffs[42].byte_offset + output.buffs[42].byte_length), buff);
  assert.doesNotMatch(JSON.stringify(output), /player-secret|other-secret/);
});

test("research extractor includes isolytic707 and quarantines its unsupported operation", () => {
  const add = concat(integer(1, 42), integer(7, 1), integer(8, 24), integer(12, 1),
    packed(13, [1], 4), integer(18, 707), packed(21, [0.015, 0.1], 8));
  const result = extractResearchBuffSemantics(cache(entry(42, add)));
  assert.equal(result.buffs[42].modifier_code, 707);
  assert.equal(mapClientResearchBuff(result.buffs[42])[0].stat, "isolytic_damage");
});

test("research extractor retains native Apex predicates and raw ranked values", () => {
  const gated = concat(integer(1, 42), integer(7, 1), integer(8, 25), integer(12, 1),
    packed(13, [1], 4), integer(17, 17), integer(18, 67001), packed(21, [500, 800], 8));
  const row = extractResearchBuffSemantics(cache(entry(42, gated))).buffs[42];
  assert.equal(row.modifier_code, 67001);
  assert.deepEqual(row.ranked_values, [500, 800]);
  assert.deepEqual(row.condition_codes, [17]);
  assert.equal(clientResearchBuffExclusion(row), "unsupported_condition");
});

test("research extractor includes isolytic808 with native ADD rank snapshots", () => {
  const add = concat(integer(1, 42), integer(7, 1), integer(8, 24), integer(12, 1),
    packed(13, [1], 4), integer(18, 808), packed(21, [0.12, 0.2], 8));
  const result = extractResearchBuffSemantics(cache(entry(42, add)));
  assert.equal(result.buffs[42].modifier_code, 808);
  assert.deepEqual(result.buffs[42].ranked_values, [0.12, 0.2]);
  assert.equal(mapClientResearchBuff(result.buffs[42])[0].stat, "isolytic_defense");
});

test("residual research extractor selects exact catalog-mapped BIDs instead of modifier-wide rows", () => {
  const officerAttack = concat(integer(1, 1523931286), integer(7, 1), integer(8, 24), integer(12, 3),
    packed(13, [1], 4), integer(18, 56), packed(21, [0.5, 1], 8));
  const allPiercing = concat(integer(1, 949755120), integer(7, 1), integer(8, 24), integer(12, 3),
    packed(13, [1], 4), integer(18, 74), packed(21, [0.8, 1.1], 8));
  const unlistedOfficerAttack = concat(integer(1, 4294000000), integer(7, 1), integer(8, 24), integer(12, 3),
    packed(13, [1], 4), integer(18, 56), packed(21, [0.5, 1], 8));
  const result = extractResearchBuffSemantics(cache(concat(
    entry(1523931286, officerAttack), entry(949755120, allPiercing),
    entry(4294000000, unlistedOfficerAttack))));
  assert.deepEqual(Object.keys(result.buffs).sort(), ["1523931286", "949755120"]);
  assert.equal(result.buffs[1523931286].sha256, crypto.createHash("sha256").update(officerAttack).digest("hex"));
  assert.equal(result.buffs[949755120].sha256, crypto.createHash("sha256").update(allPiercing).digest("hex"));
});

test("source-supported officer MULT rows preserve raw fraction ramps and reject unsupported predicates", () => {
  const attack = { ...verified, modifier_code: 56, target_code: 5, ranked_values: [0.5, 1, 1.25] };
  assert.deepEqual(mapClientResearchBuff(attack), [
    { stat: "officer_attack", operator: "add", value_unit: "fraction", client_verified: true,
      client_ranked_values: [0.5, 1, 1.25] },
  ]);
  assert.deepEqual(mapClientResearchBuff({ ...attack, condition_codes: [129] }), []);
  assert.deepEqual(mapClientResearchBuff({ ...attack, target_code: 11 }), [],
    "the selected target-11 officer row is below-decks only and must not become a crew-wide stat bonus");
  const levels = buildLevelsFromDetail({ max_level: 3,
    buffs: [{ id: 1523931286, values: [{ value: 0.5 }, { value: 1 }, { value: 1.25 }] }] },
  { buffResolveCtx: { ...context, clientBuffSemantics: { 1523931286: attack } } });
  assert.deepEqual(levels.map(r => r.bonuses), [0.5, 1, 1.25].map(value =>
    [{ stat: "officer_attack", value, operator: "add" }]));
});

test("all-piercing native rows map to three axes only for a known simple class gate", () => {
  const allPiercing = { ...verified, modifier_code: 74, ranked_values: [0.8, 1.1] };
  const mappings = mapClientResearchBuff(allPiercing);
  assert.deepEqual(mappings.map(({ stat, value_unit }) => ({ stat, value_unit })), [
    { stat: "accuracy", value_unit: "fraction" },
    { stat: "armor_piercing", value_unit: "fraction" },
    { stat: "shield_piercing", value_unit: "fraction" },
  ]);
  assert.deepEqual(mapClientResearchBuff({ ...allPiercing, condition_codes: [129] }), []);
  assert.deepEqual(mapClientResearchBuff({ ...allPiercing, condition_codes: [17], attribute_fields: [8] }), []);
  assert.deepEqual(mapClientResearchBuff({ ...allPiercing, condition_codes: [2] })
    .map(({ stat, attacker_ship_class }) => ({ stat, attacker_ship_class })), [
    { stat: "accuracy", attacker_ship_class: "battleship" },
    { stat: "armor_piercing", attacker_ship_class: "battleship" },
    { stat: "shield_piercing", attacker_ship_class: "battleship" },
  ]);
});

test("current residual issue-319 BIDs retain raw mapped ramps and quarantine unsupported scopes", () => {
  const supported = [
    [711650994, 3641613137], [949755120, 2335217665], [1269117837, 3312163487],
    [1380237269, 1496614022], [1523931286, 687490486], [1825424766, 1781205619],
    [2014886179, 2510109971], [2640187489, 699378899], [3753882631, 3891541983],
    [3846878234, 4110804153], [3876524125, 4231999], [3941283922, 3525583089],
    [3993587479, 3607839834], [4010211789, 713042947], [4062987421, 3898692756],
  ];
  const excluded = [
    [113240864, 3486647506], [511185366, 2193816350], [618925636, 1149881621],
    [624709978, 2848539271], [673716117, 3276089606], [1249674165, 51827304],
    [1284017479, 3732956730], [1290788123, 246419629], [1334510254, 3665418986],
    [1407234564, 648625101], [1458745130, 4254087973], [1464350686, 288128772],
    [1474989967, 2459777993], [1516726105, 3414000040], [1811839815, 56270788],
    [1829949531, 4159971023], [2198824419, 788182361], [2635530373, 1765649611],
    [3035764814, 1715948989], [3058907679, 3044444183], [3282480754, 4036961113],
    [3390416265, 36997703], [3610365465, 2396410066], [3680634033, 4232302261],
  ];
  const details = new Map(supported.map(([id, rid]) => [id, rid]));
  for (const [id, rid] of supported) {
    const row = manifest.buffs[id];
    assert.ok(row, `the exact source cache must select supported BID ${id}`);
    const mappings = mapClientResearchBuff(row);
    assert.ok(mappings.length > 0, `expected a source-supported stat mapping for ${id}`);
    assert.ok(mappings.every(mapping => mapping.value_unit === "fraction"));
    const detail = JSON.parse(fs.readFileSync(new URL(`../../data/upstream/data-stfc-space/research/${details.get(id)}.json`, import.meta.url)));
    const levels = buildLevelsFromDetail(detail, { rid: detail.id, buffResolveCtx: {
      ...context, researchProjectId: detail.id, researchDetailId: detail.id,
    } });
    const buff = detail.buffs.find(item => item.id === id);
    assert.ok(buff);
    const firstCrossing = buff.values.findIndex((rank, i) => i > 0 && buff.values[i - 1].value < 1 && rank.value >= 1 && rank.value <= 100);
    assert.ok(firstCrossing > 0, `expected a normalization-boundary crossing for ${id}`);
    for (const mapping of mappings) {
      const actual = levels[firstCrossing].bonuses.find(bonus => bonus.stat === mapping.stat);
      assert.ok(actual, `missing ${mapping.stat} at crossing for ${id}`);
      assert.equal(actual.value, buff.values[firstCrossing].value,
        `native op-3 fraction must remain raw for ${id}/${mapping.stat}`);
    }
  }
  for (const [id, rid] of excluded) {
    const row = manifest.buffs[id];
    assert.ok(row, `the exact source cache must retain excluded BID ${id} for diagnostics`);
    const detail = JSON.parse(fs.readFileSync(new URL(`../../data/upstream/data-stfc-space/research/${rid}.json`, import.meta.url)));
    const buff = detail.buffs.find(item => item.id === id);
    assert.ok(buff, `expected exact upstream project ${rid} to contain BID ${id}`);
    const expanded = [...row.ranked_values,
      ...Array(Math.max(0, row.ranked_value_count - row.ranked_values.length)).fill(row.ranked_values.at(-1))];
    assert.equal(buff.values.length, expanded.length, `native rank count must match upstream for excluded BID ${id}`);
    assert.ok(buff.values.every((rank, i) => rank.value === expanded[i]),
      `native rank values must match upstream for excluded BID ${id}`);
    assert.ok(clientResearchBuffExclusion(row), `unsupported source scope ${id} must be explicit`);
    assert.deepEqual(resolveBuffStatMappings({ ...context,
      researchBuffById: { [id]: "pierce" },
      descriptionByLocaId: new Map([[1, "Increases accuracy and penetration."]]),
    }, { id, loca_id: 1 }, 1), [], `unsupported native source ${id} must block legacy fallback`);
  }
});

test("research extractor retains all-defense73 and its exact native MULT ranks", () => {
  const launch = concat(integer(1, 42), integer(7, 1), integer(8, 24), integer(12, 3),
    packed(13, [1], 4), integer(18, 73), packed(21, [0.5, 1.1], 8));
  const result = extractResearchBuffSemantics(cache(entry(42, launch)));
  assert.equal(result.buffs[42].modifier_code, 73);
  assert.deepEqual(result.buffs[42].ranked_values, [0.5, 1.1]);
  assert.deepEqual(mapClientResearchBuff(result.buffs[42]).map(r => r.stat),
    ["armor", "shield_deflection", "dodge"]);
});

test("extractor fails on ambiguous IDs, invalid framing, absent doubles and nonfinite values", () => {
  assert.throws(() => extractResearchBuffSemantics(cache(concat(entry(42, buff), entry(42, buff)))), /Invalid buff ID/);
  assert.throws(() => extractResearchBuffSemantics(cache(entry(43, buff))), /Invalid buff ID/);
  assert.throws(() => extractResearchBuffSemantics(cache(entry(42, concat(integer(1, 42), integer(18, 61))))), /Missing ranked/);
  assert.throws(() => extractResearchBuffSemantics(cache(entry(42, concat(buff, packed(21, [Infinity], 8))))), /Nonfinite/);
  assert.throws(() => extractResearchBuffSemantics(Buffer.from([10, 128])), /Truncated/);
  const output = extractResearchBuffSemantics(cache(entry(42, concat(buff, integer(99, 1)))));
  assert.deepEqual(output.buffs[42].unknown_fields, [99]);
  assert.deepEqual(mapClientResearchBuff(output.buffs[42]), []);
});

test("canonical burning damage preserves raw native ranks and unknown HB scope stays quarantined", () => {
  const canonical = JSON.parse(fs.readFileSync(new URL("../../data/research_canonical.json", import.meta.url)));
  const burning = canonical.overrides.find((r) => r.rid === 365419690).effects[0];
  assert.deepEqual(burning.by_level, manifest.buffs[1898558353].ranked_values);
  assert.equal(burning.trigger, "round_start");
  assert.deepEqual(burning.conditions, [{ type: "defender_burning" }]);
  assert.equal(manifest.buffs[1898558353].num_rounds, 1);
  assert.deepEqual(canonical.overrides.find((r) => r.rid === 3407808029).effects, []);
  assert.deepEqual(manifest.buffs[981948123].condition_codes, [37, 137]);
});

test("coverage records every selected definition and retains unsupported conditions with public names", () => {
  const result = researchClientBuffCoverage({ provenance: { cache_sha256: "abc" },
    buffs: { 42: verified, 43: { ...verified, condition_codes: [999] } } },
  [{ buff_id: 43, rid: 9, name: "Special Ship Bonus" }]);
  assert.equal(result.summary.selected_buff_count, 2);
  assert.equal(result.summary.supported_buff_count, 1);
  assert.equal(result.summary.quarantined_buff_count, 1);
  assert.deepEqual(result.summary.quarantine_reasons, { unsupported_condition: 1 });
  assert.deepEqual(result.rows[1].condition_codes, [999]);
  assert.deepEqual(result.rows[1].projects, [{ rid: 9, name: "Special Ship Bonus" }]);
  assert.doesNotMatch(JSON.stringify(result), /client_ranked_values/);
});
