/**
 * Structured triage report & Pull Request body builder for automated upstream
 * data refreshes.
 *
 * Classifies refresh outcomes cleanly into:
 * - Hard failures (schema/corrupt JSON/engine test regression) → blocks PR creation
 * - Soft upstream modeling coverage gaps (new Scopely building opaque buffs,
 *   unmapped research buffs, new hostile ability no-ops, undocumented upstream files)
 *   → opens a Draft `[Needs Triage]` PR with an actionable checklist instead of
 *   failing silently in Actions logs.
 */

import { formatMarkdownReport } from "./check_stfcspace_summary_drift.mjs";
import { formatPipelineReportMarkdown } from "./pipeline_dag.mjs";
import { formatManifestDiffMarkdown } from "./pipeline_manifest.mjs";

/**
 * Known upstream coverage-baseline test targets whose failure indicates new
 * unmodeled upstream game content rather than an engine or schema corruption bug.
 */
export const COVERAGE_GAP_TEST_TARGETS = [
  "building_opaque_buff_baseline",
  "hostile_ability_catalog_parity",
];

/**
 * Extract structured soft-triage findings from pipeline, validation, and coverage-gate inputs.
 *
 * @param {{
 *   pipelineReport?: { stages?: Array<{ id: string, status: string, error?: string }>, soft_failure_count?: number } | null,
 *   validationReport?: { summary?: { errors: number, warnings: number }, categories?: Array<{ name: string, diagnostics?: Array<{ severity: string, message?: string }> }> } | null,
 *   coverageGateResults?: Array<{ id: string, title: string, ok: boolean, summary?: string, remediation?: string }>,
 * }} inputs
 */
export function collectTriageItems(inputs) {
  const items = [];

  for (const stage of inputs.pipelineReport?.stages ?? []) {
    if (stage.status === "soft_failed") {
      items.push({
        source: `pipeline:${stage.id}`,
        title: stage.id,
        detail: stage.error ?? "Coverage check flagged new upstream items",
        remediation:
          stage.id === "stfcspace:building_allowlist"
            ? "Run `node scripts/seed_building_opaque_allowlist.mjs --write` and review `docs/building_gaps.md`."
            : "Inspect the stage log and update mapping/allowlist tables before merging.",
      });
    }
  }

  for (const gate of inputs.coverageGateResults ?? []) {
    if (!gate.ok) {
      items.push({
        source: `gate:${gate.id}`,
        title: gate.title,
        detail: gate.summary ?? "Coverage baseline exceeded by new upstream data",
        remediation:
          gate.remediation ??
          "Review the newly added upstream buffs/abilities, map combat stats or extend allowlists, and re-run tests.",
      });
    }
  }

  const valCats = inputs.validationReport?.categories ?? [];
  const warningSummary = [];
  for (const cat of valCats) {
    const warns = (cat.diagnostics ?? []).filter((d) => d.severity === "warning" || d.severity === "Warning");
    if (warns.length > 0) {
      warningSummary.push({ category: cat.name, count: warns.length });
    }
  }

  return {
    needsTriage: items.length > 0,
    items,
    validationWarnings: warningSummary,
    validationErrors: inputs.validationReport?.summary?.errors ?? 0,
  };
}

/**
 * Build a complete Markdown Pull Request body (and GitHub Step Summary) combining
 * triage status, upstream drift, pipeline timings, manifest digest deltas, and
 * validation warnings.
 *
 * @param {{
 *   workflowName?: string,
 *   runUrl?: string,
 *   driftDomains?: Array<any> | null,
 *   pipelineReport?: any | null,
 *   validationReport?: any | null,
 *   manifestDiffs?: Array<any> | null,
 *   coverageGateResults?: Array<{ id: string, title: string, ok: boolean, summary?: string, remediation?: string }>,
 *   fetchModeSummary?: string,
 * }} opts
 */
export function buildRefreshPrBody(opts) {
  const {
    workflowName = "Data refresh (stfc.space)",
    runUrl = null,
    driftDomains = null,
    pipelineReport = null,
    validationReport = null,
    manifestDiffs = null,
    coverageGateResults = [],
    fetchModeSummary = "Concurrent v2 Node fetcher (`--concurrency 6`, ETag conditional cache, drift-targeted refresh)",
  } = opts;

  const triage = collectTriageItems({
    pipelineReport,
    validationReport,
    coverageGateResults,
  });

  const lines = [];
  if (runUrl) {
    lines.push(`Automated update from workflow **${workflowName}** ([run](${runUrl})).`);
  } else {
    lines.push(`Automated update from workflow **${workflowName}**.`);
  }
  lines.push("");

  if (triage.needsTriage) {
    lines.push(
      "> [!WARNING]",
      "> **Draft — Needs Modeling Triage:** Core combat engine tests and strict schema validation passed, but new upstream STFC game content introduced unmodeled buffs or coverage-baseline gaps listed below. Review and map/allowlist before marking ready for merge.",
      "",
      "## Actionable modeling triage",
      "",
    );
    for (const item of triage.items) {
      lines.push(`- [ ] **${item.title}** (\`${item.source}\`): ${item.detail}`);
      if (item.remediation) {
        lines.push(`  - *Next step:* ${item.remediation}`);
      }
    }
    lines.push("");
  } else {
    lines.push(
      "> [!NOTE]",
      "> All core combat tests, schema validation (`validate_data`), and upstream coverage baselines passed cleanly.",
      "",
    );
  }

  if (Array.isArray(driftDomains) && driftDomains.length > 0) {
    lines.push(
      formatMarkdownReport(driftDomains, {
        beforeLabel: "before",
        afterLabel: "after",
        title: "Summary catalog changes (fetch step)",
      }),
    );
    lines.push("");
  }

  if (pipelineReport) {
    lines.push(formatPipelineReportMarkdown(pipelineReport));
    lines.push("");
  }

  if (Array.isArray(manifestDiffs) && manifestDiffs.length > 0) {
    lines.push("## Catalog manifest digest changes", "");
    lines.push(formatManifestDiffMarkdown(manifestDiffs));
    lines.push("");
  }

  if (triage.validationWarnings.length > 0) {
    lines.push("## Data validation warnings", "");
    lines.push("| Category | Warnings |");
    lines.push("| --- | ---: |");
    for (const w of triage.validationWarnings) {
      lines.push(`| \`${w.category}\` | ${w.count} |`);
    }
    lines.push("");
  }

  lines.push("## Pipeline summary", "");
  lines.push(`- Fetch: ${fetchModeSummary}`);
  lines.push("- Normalize: `npm run data:refresh -- --stfcspace --write-manifest`");
  lines.push("- Verify: `cargo nextest run --profile ci` + `cargo run --bin validate_data`");
  lines.push("");
  lines.push("Please review large `data/` diffs before merge.");

  return {
    needsTriage: triage.needsTriage,
    title: triage.needsTriage
      ? "chore(data): [Needs Triage] automated stfc.space refresh"
      : "chore(data): automated stfc.space refresh",
    markdown: `${lines.join("\n")}\n`,
    triage,
  };
}
