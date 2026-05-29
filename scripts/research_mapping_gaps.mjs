#!/usr/bin/env node
/**
 * Research mapping gap report (Track E validation hygiene).
 * Same resolution order as import_stfcspace_research.mjs for unmapped buff ids.
 *
 * Usage:
 *   node scripts/research_mapping_gaps.mjs [--json] [--top N]
 */

import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  loadResearchMappingGapsBaseline,
  scanResearchMappingGaps,
} from "./lib/research_mapping_gaps.mjs";

const REPO_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

function getArg(name, def) {
  const i = process.argv.indexOf(name);
  if (i === -1) return def;
  const v = process.argv[i + 1];
  return v === undefined ? def : v;
}

async function main() {
  const asJson = process.argv.includes("--json");
  const topN = Math.max(1, parseInt(getArg("--top", "25"), 10));

  const report = await scanResearchMappingGaps(REPO_ROOT);
  const baseline = await loadResearchMappingGapsBaseline(REPO_ROOT);
  const payload = {
    ...report,
    baseline,
    regression:
      baseline == null
        ? null
        : {
            unmapped_buff_ids_delta:
              report.summary.unmapped_buff_id_count - (baseline.unmapped_buff_ids ?? 0),
            suspect_global_scopes_delta:
              report.summary.suspect_global_scope_count - (baseline.suspect_global_scopes ?? 0),
          },
  };

  if (asJson) {
    console.log(JSON.stringify(payload, null, 2));
    return;
  }

  console.log("# Research mapping gaps\n");
  console.log(`Unmapped buff ids: ${report.summary.unmapped_buff_id_count}`);
  console.log(`Suspect global scopes: ${report.summary.suspect_global_scope_count}`);
  if (baseline) {
    console.log(
      `Baseline: unmapped=${baseline.unmapped_buff_ids}, suspect=${baseline.suspect_global_scopes}`
    );
    if (payload.regression) {
      console.log(
        `Delta: unmapped ${payload.regression.unmapped_buff_ids_delta >= 0 ? "+" : ""}${payload.regression.unmapped_buff_ids_delta}, suspect ${payload.regression.suspect_global_scopes_delta >= 0 ? "+" : ""}${payload.regression.suspect_global_scopes_delta}`
      );
    }
  }
  console.log(`\nTop ${topN} unmapped buff ids by occurrence:`);
  for (const row of report.unmapped_buff_ids.slice(0, topN)) {
    console.log(
      `  ${row.buff_id}  count=${row.count}  ${row.category ?? "?"}  loca=${row.loca_id ?? "—"}`
    );
  }
  console.log(`\nTop ${topN} suspect global scopes (sample):`);
  for (const row of report.suspect_global_scopes.slice(0, topN)) {
    console.log(
      `  rid=${row.rid}  ${row.stat}@${row.level}  ${row.scope_category}  ${row.description_snippet.slice(0, 90)}`
    );
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
