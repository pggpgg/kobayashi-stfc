import assert from "node:assert/strict";
import test from "node:test";
import {
  formatPipelineReportMarkdown,
  resolveStageOrder,
  runPipelineDag,
  selectStages,
} from "../lib/pipeline_dag.mjs";
import { buildRefreshPrBody, collectTriageItems } from "../lib/refresh_triage.mjs";

test("resolveStageOrder topologically sorts dependencies and detects cycles", () => {
  const stages = [
    { id: "c", group: "g", dependsOn: ["b"] },
    { id: "a", group: "g", dependsOn: [] },
    { id: "b", group: "g", dependsOn: ["a"] },
  ];
  assert.deepEqual(
    resolveStageOrder(stages).map((s) => s.id),
    ["a", "b", "c"],
  );

  assert.throws(
    () =>
      resolveStageOrder([
        { id: "x", group: "g", dependsOn: ["y"] },
        { id: "y", group: "g", dependsOn: ["x"] },
      ]),
    /Cycle detected/,
  );
});

test("selectStages filters by group, --stages, and --skip", () => {
  const all = [
    { id: "core:a", group: "core", dependsOn: [] },
    { id: "stfc:b", group: "stfcspace", dependsOn: ["core:a"] },
    { id: "stfc:c", group: "stfcspace", dependsOn: ["stfc:b"] },
  ];

  const byGroup = selectStages(all, {
    groups: new Set(["core", "stfcspace"]),
    skipStages: ["stfc:c"],
  });
  assert.deepEqual(
    byGroup.map((s) => s.id),
    ["core:a", "stfc:b"],
  );

  const only = selectStages(all, {
    groups: new Set(["core"]),
    onlyStages: ["stfc:c", "stfc:b"],
  });
  assert.deepEqual(
    only.map((s) => s.id),
    ["stfc:b", "stfc:c"],
  );

  assert.throws(
    () => selectStages(all, { groups: new Set(["core"]), onlyStages: ["does:not:exist"] }),
    /Unknown stage/,
  );
});

test("runPipelineDag handles dry-run, soft coverage gaps, and hard failure halting", async () => {
  const stages = [
    {
      id: "step:ok",
      group: "stfcspace",
      title: "OK stage",
      dependsOn: [],
      buildCommand: () => ({ program: "echo", args: ["ok"] }),
    },
    {
      id: "step:soft",
      group: "stfcspace",
      title: "Soft coverage check",
      dependsOn: ["step:ok"],
      softFailure: true,
      buildCommand: () => ({ program: "check-coverage", args: [] }),
    },
    {
      id: "step:after",
      group: "stfcspace",
      title: "Downstream stage",
      dependsOn: ["step:ok"],
      buildCommand: () => ({ program: "echo", args: ["after"] }),
    },
  ];

  const dry = await runPipelineDag(stages, { cwd: process.cwd(), dryRun: true, logger: () => {} });
  assert.equal(dry.ok, true);
  assert.equal(dry.stages.every((s) => s.status === "dry_run"), true);

  // Soft failure continues when continueOnSoftFailure is true and marks needs_triage.
  const softRun = await runPipelineDag(stages, {
    cwd: process.cwd(),
    continueOnSoftFailure: true,
    logger: () => {},
    executeCommand: async (spec) =>
      spec.program === "check-coverage"
        ? { exitCode: 1, error: "5 new opaque buffs" }
        : { exitCode: 0, error: null },
  });
  assert.equal(softRun.ok, true);
  assert.equal(softRun.needs_triage, true);
  assert.equal(softRun.soft_failure_count, 1);
  assert.equal(softRun.stages[2].status, "ok");

  const md = formatPipelineReportMarkdown(softRun);
  assert.match(md, /step:soft/);
  assert.match(md, /coverage gap/);

  // Without continueOnSoftFailure, the same failure halts the DAG as a hard failure.
  const hardRun = await runPipelineDag(stages, {
    cwd: process.cwd(),
    continueOnSoftFailure: false,
    logger: () => {},
    executeCommand: async (spec) =>
      spec.program === "check-coverage"
        ? { exitCode: 1, error: "5 new opaque buffs" }
        : { exitCode: 0, error: null },
  });
  assert.equal(hardRun.ok, false);
  assert.equal(hardRun.hard_failure_count, 1);
  assert.equal(hardRun.stages.length, 2);
});

test("buildRefreshPrBody distinguishes clean refreshes from Draft Needs Triage PRs", () => {
  const clean = buildRefreshPrBody({
    workflowName: "Data refresh (stfc.space)",
    runUrl: "https://example.test/runs/1",
    coverageGateResults: [
      { id: "building_opaque_buff_baseline", title: "Building opaque buff baseline", ok: true },
    ],
  });
  assert.equal(clean.needsTriage, false);
  assert.equal(clean.title, "chore(data): automated stfc.space refresh");
  assert.match(clean.markdown, /All core combat tests/);

  const triage = collectTriageItems({
    pipelineReport: {
      stages: [{ id: "stfcspace:building_allowlist", status: "soft_failed", error: "2 unallowlisted" }],
    },
    coverageGateResults: [
      {
        id: "building_opaque_buff_baseline",
        title: "Building opaque buff baseline",
        ok: false,
        summary: "actionable opaque buff count 5 exceeds baseline 0",
        remediation: "Run seed_building_opaque_allowlist.mjs --write",
      },
    ],
  });
  assert.equal(triage.needsTriage, true);
  assert.equal(triage.items.length, 2);

  const draft = buildRefreshPrBody({
    coverageGateResults: [
      {
        id: "building_opaque_buff_baseline",
        title: "Building opaque buff baseline",
        ok: false,
        summary: "actionable opaque buff count 5 exceeds baseline 0",
        remediation: "Run seed_building_opaque_allowlist.mjs --write",
      },
    ],
  });
  assert.equal(draft.needsTriage, true);
  assert.match(draft.title, /\[Needs Triage\]/);
  assert.match(draft.markdown, /Actionable modeling triage/);
  assert.match(draft.markdown, /actionable opaque buff count 5 exceeds baseline 0/);
});
