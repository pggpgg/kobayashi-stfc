import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { buildLevelsFromDetail } from "../import_stfcspace_research.mjs";
import { statEntryFromJsonValue } from "../lib/research_buff_resolve.mjs";

const readJson = (relative) => JSON.parse(fs.readFileSync(new URL(relative, import.meta.url), "utf8"));
const context = {
  researchBuffById: readJson("../../data/research/buff_id_to_stat.json"),
  commonBuffNormalization: {},
  locaIdToStat: {},
  clientBuffSemantics: readJson("../../data/research/client_buff_semantics_2026-10-20.json").buffs,
};

test("4-star Direct Hit imports three separate base piercing stats at the current rank", () => {
  const detail = readJson("../../data/upstream/data-stfc-space/research/2999437.json");
  const levels = buildLevelsFromDetail(detail, { buffResolveCtx: context });
  assert.equal(levels.length, 10);
  for (const [index, expected] of [[0, 0.21], [1, 0.24], [9, 0.8]]) {
    assert.deepEqual(levels[index], {
      level: index + 1,
      bonuses: ["accuracy", "armor_piercing", "shield_piercing"].map((stat) => ({
        stat, value: expected, operator: "add", attacker_min_grade: 4,
      })),
    });
  }
});

test("zero and empty snapshots retain ranks and do not revive earlier bonuses", () => {
  const detail = {
    max_level: 3,
    buffs: [{ id: 1, value_is_percentage: true, values: [{ value: 0.2 }, { value: 0 }, { value: 0 }] }],
  };
  const opts = {
    buffResolveCtx: { ...context, researchBuffById: { 1: { stat: "accuracy", value_unit: "fraction" } } },
  };
  const levels = buildLevelsFromDetail(detail, opts);
  assert.equal(levels.length, 3);
  assert.deepEqual(levels.slice(1), [{ level: 2, bonuses: [] }, { level: 3, bonuses: [] }]);
  detail.buffs[0].values[0].value = 0;
  assert.deepEqual(buildLevelsFromDetail(detail, opts), []);
});

test("unitless percentage mapping is omitted with an explicit quarantine diagnostic", () => {
  const unmappedByBuffId = new Map();
  const detail = {
    id: 77,
    max_level: 3,
    buffs: [{ id: 777, value_is_percentage: true, show_percentage: true,
      values: [{ value: 0.8 }, { value: 1 }, { value: 1.2 }] }],
  };
  const levels = buildLevelsFromDetail(detail, {
    rid: 77,
    unmappedByBuffId,
    buffResolveCtx: { ...context,
      researchBuffById: { 777: { stat: "officer_attack" } } },
  });
  assert.deepEqual(levels, [], "a guessed unit must not become a combat catalog bonus");
  assert.deepEqual(unmappedByBuffId.get("777"), {
    buff_id: 777,
    count: 1,
    example_rids: [77],
    value_is_percentage: true,
    show_percentage: true,
    loca_id: null,
    exclusion_reasons: ["percentage_unit_unverified"],
  });
});

test("a malformed grade mapping fails rather than silently becoming unconditional", () => {
  for (const attacker_min_grade of [0, -1, 4.5, "4", 0x100000000]) {
    assert.throws(() => statEntryFromJsonValue({ stat: "accuracy", attacker_min_grade }), /Invalid attacker_min_grade/);
  }
  assert.equal(statEntryFromJsonValue({ stat: "accuracy", attacker_min_grade: 4 }).attacker_min_grade, 4);
});

test("project ranks survive short unrelated buffs and nonempty mapped lists clamp independently", () => {
  const detail = {
    max_level: 3,
    buffs: [
      { id: 1, value_is_percentage: true, values: [{ value: 0.21 }, { value: 0.24 }, { value: 0.8 }, { value: 0.9 }] },
      { id: 2, value_is_percentage: true, values: [{ value: 0.1 }] },
      { id: 3, value_is_percentage: true, values: [] },
      { id: 4, values: [] }, // Unmapped, so cannot constrain a mapped buff's rank count.
    ],
  };
  const opts = {
    buffResolveCtx: {
      ...context,
      researchBuffById: {
        1: { stat: "accuracy", value_unit: "fraction" },
        2: { stat: "armor_piercing", value_unit: "fraction" },
        3: { stat: "shield_piercing", value_unit: "fraction" },
      },
    },
  };
  const levels = buildLevelsFromDetail(detail, opts);
  assert.equal(levels.length, 3, "declared project maximum bounds longer buff arrays");
  assert.deepEqual(levels.map((row) => row.bonuses), [0.21, 0.24, 0.8].map((value) => [
    { stat: "accuracy", value, operator: "add" },
    { stat: "armor_piercing", value: 0.1, operator: "add" },
  ]));

  delete detail.max_level;
  detail.levels = [{}, {}];
  assert.equal(buildLevelsFromDetail(detail, opts).length, 2, "project level rows also bound rank count");

  delete detail.levels;
  assert.equal(buildLevelsFromDetail(detail, opts).length, 4, "without project bounds, use the longest list");
});

test("bundled canonical overrides select snapshots of their referenced upstream ranked arrays", () => {
  // These factors describe the existing canonical unit conversions, not a new generic
  // normalization rule or validation of each canonical modifier/condition.
  const reviewedSources = new Map([
    [2580836593, [1855248137, 70100, 1]],
    [2047743532, [679782642, 70103, 1]],
    [365419690, [1898558353, 70106, 1]],
    [851540444, [3573127943, 80007, 1]],
    [4133019450, [2585023800, 70101, 1]],
    [535909811, [1142346453, 70104, 1]],
    [1233598019, [4033066544, 70105, 1]],
    [1995496344, [638902322, 70102, 1]],
    [3288570685, [2286920377, 70107, 1]],
    [3407808029, [981948123, 70108, 1]],
    [545866536, [810035059, 67626, 1]],
    [2392190200, [1478241687, 35006, 1]],
  ]);
  const canonical = readJson("../../data/research_canonical.json");
  assert.equal(canonical.overrides.length, reviewedSources.size);
  for (const record of canonical.overrides) {
    const [buffId, locaId, scale] = reviewedSources.get(record.rid);
    const detail = readJson(`../../data/upstream/data-stfc-space/research/${record.rid}.json`);
    if ([851540444, 3407808029].includes(record.rid)) {
      assert.deepEqual(record.effects, [], "deployment cargo or unknown native scope must not become a combat effect");
      continue;
    }
    assert.equal(record.effects.length, 1);
    const effect = record.effects[0];
    assert.deepEqual(effect.source_ref, { rid: record.rid, buff_id: buffId, loca_id: locaId });
    const buff = detail.buffs.find((candidate) => candidate.id === buffId);
    assert.ok(buff, `source buff exists for ${record.rid}`);
    assert.equal(buff.loca_id, locaId);
    assert.equal(effect.by_level.length, detail.max_level || detail.levels.length);
    assert.equal(effect.snapshot_by_level, true, `${record.rid} stores full ranked values, not deltas`);
    effect.by_level.forEach((value, index) => {
      assert.ok(
        Math.abs(value - buff.values[index].value * scale) < 1e-12,
        `${record.rid} rank ${index + 1} must match its reviewed source value and unit conversion`
      );
    });
  }
});

test("client-verified defense and repair mappings retain raw fractions, source ranks, and single gates", () => {
  const evidence = readJson("../../data/research/client_verified_research_2026-10-20.json");
  const canonical = readJson("../../data/research_canonical.json");
  const contracts = new Map([
    [2047743532, ["base_mitigation_stats_bonus", "requires_defender_burning", "defender_burning"]],
    [535909811, ["base_mitigation_stats_bonus", "requires_morale", "morale_active"]],
    [1233598019, ["base_mitigation_stats_bonus", "requires_defender_hull_breach", "defender_hull_breach"]],
    [3288570685, ["officer_shield_regen_max_fraction", "requires_morale", "morale_active"]],
  ]);
  assert.equal(evidence.provenance.current_build_correspondence_verified, false);
  for (const [rid, [stat, gate, canonicalGate]] of contracts) {
    const observed = evidence.projects.find((project) => project.rid === rid);
    const detail = readJson(`../../data/upstream/data-stfc-space/research/${rid}.json`);
    const levels = buildLevelsFromDetail(detail, { buffResolveCtx: context });
    assert.equal(observed.buff.target_code, 1);
    assert.equal(observed.buff.trigger_code, 18);
    assert.equal(observed.buff.operation_code, 3);
    assert.equal(observed.buff.modifier_code, stat === "base_mitigation_stats_bonus" ? 73 : 16);
    assert.deepEqual(levels, observed.buff.ranked_values.map((value, index) => ({
      level: index + 1, bonuses: [{ stat, value, operator: "add", [gate]: true }],
    })));
    const effect = canonical.overrides.find((record) => record.rid === rid).effects[0];
    assert.equal(effect.modifier, stat);
    assert.equal(effect.trigger, "round_start");
    assert.deepEqual(effect.conditions, [{ type: canonicalGate }]);
    assert.deepEqual(effect.by_level, observed.buff.ranked_values);
  }
});

test("deployment-only cargo research is explicitly excluded even when prose suggests a combat fallback", () => {
  const detail = readJson("../../data/upstream/data-stfc-space/research/851540444.json");
  const maliciousFallback = new Map([[80007, "Increases Shield Mitigation while your ship has Morale."]]);
  assert.deepEqual(buildLevelsFromDetail(detail, {
    projectLocaId: 80007,
    descriptionByLocaId: maliciousFallback,
    buffResolveCtx: { ...context, descriptionByLocaId: maliciousFallback },
  }), []);
  const mapping = statEntryFromJsonValue(context.researchBuffById["3573127943"]);
  assert.equal(mapping.exclude_global_merge, true);
});

test("fresh capture evidence preserves original selected payload hashes and explicit freshness limits", () => {
  const evidence = readJson("../../data/research/client_verified_research_2026-10-20.json");
  const fresh = evidence.fresh_capture_verification;
  assert.equal(fresh.observed_client_build, 272);
  assert.equal(fresh.encoded_server_build_binding, false);
  assert.equal(fresh.server_version, null);
  assert.equal(fresh.server_time, null);
  assert.notEqual(fresh.cache_sha256, evidence.provenance.cache_sha256);
  assert.equal(fresh.selected_projects.length, evidence.projects.length);
  for (const selected of fresh.selected_projects) {
    const original = evidence.projects.find((project) => project.rid === selected.rid);
    assert.ok(original);
    assert.equal(selected.matches_original_bytes, true);
    assert.equal(selected.project.sha256, original.project.sha256);
    assert.equal(selected.buff.sha256, original.buff.sha256);
  }
  assert.equal(fresh.static_target_trigger_specs.length, evidence.static_target_trigger_specs.length);
  for (const selected of fresh.static_target_trigger_specs) {
    const original = evidence.static_target_trigger_specs.find((spec) => spec.group === selected.group && spec.mapKey === selected.mapKey);
    assert.ok(original);
    assert.equal(selected.matches_original_bytes, true);
    for (const key of ["code", "id", "idStr", "schema"]) {
      assert.equal(selected[key], original[key]);
    }
  }
});

test("explicit verified fraction units reject invalid declarations and non-finite source values", async () => {
  const { normalizeBonusValue } = await import("../lib/research_normalize_bonus_value.mjs");
  assert.throws(() => statEntryFromJsonValue({ stat: "base_mitigation_stats_bonus", value_unit: "percent-ish" }), /Invalid value_unit/);
  assert.equal(statEntryFromJsonValue({ stat: "weapon_damage", value_unit: "percentage_points" }).value_unit,
    "percentage_points");
  const mapping = statEntryFromJsonValue({ stat: "base_mitigation_stats_bonus", value_unit: "fraction" });
  for (const value of [0.5, 0.75, 1.15, 5]) {
    assert.equal(normalizeBonusValue({ value_is_percentage: true, show_percentage: true }, mapping, value), value);
  }
  for (const value of [NaN, Infinity, -Infinity]) {
    assert.equal(normalizeBonusValue({}, mapping, value), null);
  }
});
