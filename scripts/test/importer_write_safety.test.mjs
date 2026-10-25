import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

async function fixture(t, script) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "kobayashi-importer-safety-"));
  t.after(async () => {
    assert.ok(path.isAbsolute(root) && root.startsWith(os.tmpdir()));
    await fs.rm(root, { recursive: true, force: true });
  });
  await fs.mkdir(path.join(root, "scripts"), { recursive: true });
  await fs.copyFile(path.join(repo, "scripts", script), path.join(root, "scripts", script));
  await fs.cp(path.join(repo, "scripts/lib"), path.join(root, "scripts/lib"), { recursive: true });
  const put = async (name, value) => {
    const destination = path.join(root, name);
    await fs.mkdir(path.dirname(destination), { recursive: true });
    await fs.writeFile(destination, typeof value === "string" ? value : JSON.stringify(value));
  };
  const run = (...args) => spawnSync(process.execPath, [path.join(root, "scripts", script), ...args], {
    cwd: root, encoding: "utf8", timeout: 30_000,
  });
  return { root, put, run, read: name => fs.readFile(path.join(root, name), "utf8") };
}

async function researchFixture(t) {
  const f = await fixture(t, "import_stfcspace_research.mjs");
  await f.put("data/research/client_buff_semantics_2026-10-20.json", {
    schema_version: 1, buffs: {}, provenance: { cache_sha256: "isolated-fixture" },
  });
  await f.put("data/research/buff_id_to_stat.json", { 777: { stat: "damage_reduction", value_unit: "fraction" } });
  await f.put("data/upstream/data-stfc-space/summary-research.json", [{ id: 1 }, { id: 2 }]);
  for (const id of [1, 2]) await f.put(`data/upstream/data-stfc-space/research/${id}.json`, {
    id, max_level: 1, buffs: [{ id: 777, value_is_percentage: true, values: [{ value: 0.1 }] }],
  });
  await f.put("data/research_catalog.json", "existing complete catalog\n");
  return f;
}

for (const args of [["--limit", "1"], ["--rid", "1"], ["--dump-unmapped"]]) {
  test(`research ${args.join(" ")} preserves the full catalog`, async t => {
    const f = await researchFixture(t);
    const result = f.run("--from-upstream", ...args);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /"items_emitted": [12]/, "fixture must exercise a nonempty import");
    assert.equal(await f.read("data/research_catalog.json"), "existing complete catalog\n");
  });
}

test("research full import writes all records while explicit subset output leaves the full catalog intact", async t => {
  const f=await researchFixture(t);
  const full=f.run("--from-upstream");
  assert.equal(full.status,0,full.stderr);
  const catalog=await f.read("data/research_catalog.json");
  assert.deepEqual(JSON.parse(catalog).items.map(item=>item.rid),[1,2]);
  const subsetPath=path.join(f.root,"subset.json");
  const subset=f.run("--from-upstream","--rid","1","--output",subsetPath);
  assert.equal(subset.status,0,subset.stderr);
  assert.deepEqual(JSON.parse(await fs.readFile(subsetPath,"utf8")).items.map(item=>item.rid),[1]);
  assert.equal(await f.read("data/research_catalog.json"),catalog);
});

test("full research import omits unitless percentage rows and writes an explicit quarantine report", async t => {
  const f = await researchFixture(t);
  await f.put("data/research/buff_id_to_stat.json", {
    777: { stat: "damage_reduction" },
    778: { stat: "accuracy", value_unit: "fraction" },
  });
  await f.put("data/upstream/data-stfc-space/research/1.json", {
    id: 1, max_level: 3, buffs: [{ id: 777, value_is_percentage: true, show_percentage: true,
      values: [{ value: 0.8 }, { value: 1 }, { value: 1.2 }] }],
  });
  await f.put("data/upstream/data-stfc-space/research/2.json", {
    id: 2, max_level: 1, buffs: [{ id: 778, value_is_percentage: true, show_percentage: false,
      values: [{ value: 0.2 }] }],
  });

  const result = f.run("--from-upstream");
  assert.equal(result.status, 0, result.stderr);
  const catalog = JSON.parse(await f.read("data/research_catalog.json"));
  assert.deepEqual(catalog.items.map(item => item.rid), [2]);
  assert.equal(catalog.items[0].levels[0].bonuses[0].value, 0.2);
  const quarantine = JSON.parse(await f.read("data/research/percentage_unit_quarantine.json"));
  assert.deepEqual(quarantine.summary, { unique_buff_count: 1, project_buff_instances: 1 });
  assert.deepEqual(quarantine.rows, [{ buff_id: 777, count: 1, example_rids: [1],
    value_is_percentage: true, show_percentage: true, loca_id: null,
    exclusion_reason: "percentage_unit_unverified" }]);
});

for (const summary of [[], [{ id: 1, max_level: 1, buffs: [] }]]) {
  test(`building import refuses ${summary.length ? "truncated" : "empty"} summaries before changing outputs`, async t => {
    const f = await fixture(t, "import_stfcspace_buildings.mjs");
    await f.put("data/upstream/data-stfc-space/summary-building.json", summary);
    const originalIndex = { buildings: [{ id: "first", bid: 1, file: "1_first" }, { id: "second", bid: 2, file: "2_second" }] };
    await f.put("data/buildings/index.json", originalIndex);
    await f.put("data/buildings/1_first.json", "first original");
    await f.put("data/buildings/2_second.json", "second original");
    const result = f.run("--from-upstream");
    assert.notEqual(result.status, 0, result.stdout);
    assert.equal(await f.read("data/buildings/1_first.json"), "first original");
    assert.equal(await f.read("data/buildings/2_second.json"), "second original");
    assert.deepEqual(JSON.parse(await f.read("data/buildings/index.json")), originalIndex);
  });
}

test("complete building import still emits records and updates its index", async t => {
  const f=await fixture(t,"import_stfcspace_buildings.mjs");
  await f.put("data/upstream/data-stfc-space/summary-building.json",[{id:1,max_level:1,buffs:[]}]);
  const result=f.run("--from-upstream");
  assert.equal(result.status,0,result.stderr);
  const index=JSON.parse(await f.read("data/buildings/index.json"));
  assert.equal(index.buildings.length,1);
  assert.equal(index.buildings[0].bid,1);
  assert.equal(JSON.parse(await f.read(`data/buildings/${index.buildings[0].file}.json`)).levels.length,1);
});

test("allowlist seeder preserves hand-written reasons and entries outside its proposals", async t => {
  const f = await fixture(t, "seed_building_opaque_allowlist.mjs");
  const reviewed = { version: 2, entries: { buff_430522112: { category: "economy_meta", reason: "Reviewed custom reason" },
    buff_manual_only: { category: "other", reason: "Retain a manual exception" } } };
  await f.put("data/buildings/opaque_buff_allowlist.json", reviewed);
  await f.put("data/buildings/first.json", { id: "first", levels: [{ bonuses: [
    { stat: "buff_430522112", notes: "loca_id=1" }, { stat: "buff_999", notes: "loca_id=2" },
  ] }] });
  await f.put("data/upstream/data-stfc-space/translations-starbase_modules.json", [
    { id: 1, key: "starbase_module_buff_description", text: "Increases mining rate" },
    { id: 2, key: "starbase_module_buff_description", text: "Increases mining rate" },
  ]);
  const result = f.run("--write");
  assert.equal(result.status, 0, result.stderr);
  const actual = JSON.parse(await f.read("data/buildings/opaque_buff_allowlist.json"));
  assert.deepEqual(actual.entries.buff_430522112, reviewed.entries.buff_430522112);
  assert.deepEqual(actual.entries.buff_manual_only, reviewed.entries.buff_manual_only);
  assert.ok(actual.entries.buff_999);
});

test("allowlist seeder refuses malformed existing JSON without overwriting it", async t => {
  const f = await fixture(t, "seed_building_opaque_allowlist.mjs");
  await f.put("data/buildings/opaque_buff_allowlist.json", "{reviewed but damaged");
  const result = f.run("--write");
  assert.notEqual(result.status, 0, result.stdout);
  assert.equal(await f.read("data/buildings/opaque_buff_allowlist.json"), "{reviewed but damaged");
});
