#!/usr/bin/env node
/**
 * Categorize unmapped research buff ids after import (--dump-unmapped shape).
 * Read-only triage helper for Track B mapping work; does not modify repo files.
 *
 * Usage:
 *   node scripts/import_stfcspace_research.mjs --from-upstream --limit 0 --dump-unmapped 2>/dev/null \
 *     | node scripts/triage_research_unmapped.mjs [--json] [--top N]
 */

import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { categorizeResearchDescription } from "./lib/research_scope_categorize.mjs";

const REPO_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

function getArg(name, def) {
  const i = process.argv.indexOf(name);
  if (i === -1) return def;
  const v = process.argv[i + 1];
  return v === undefined ? def : v;
}

async function loadTranslations() {
  const p = path.join(REPO_ROOT, "data", "upstream", "data-stfc-space", "translations-research.json");
  const rows = JSON.parse(await fs.readFile(p, "utf8"));
  const descById = new Map();
  for (const r of rows) {
    if (r.key === "research_project_description" && typeof r.id === "number") {
      descById.set(r.id, r.text || "");
    }
  }
  return descById;
}

async function readUnmappedFromStdin() {
  const chunks = [];
  for await (const c of process.stdin) chunks.push(c);
  const txt = Buffer.concat(chunks).toString("utf8");
  const start = txt.indexOf("{");
  const end = txt.indexOf("}\n{");
  if (start === -1) throw new Error("expected JSON with unmapped_buff_ids on stdin");
  const slice = end === -1 ? txt.slice(start) : txt.slice(start, end + 1);
  const parsed = JSON.parse(slice);
  return parsed.unmapped_buff_ids || [];
}

async function main() {
  const asJson = process.argv.includes("--json");
  const topN = Math.max(1, parseInt(getArg("--top", "25"), 10));
  const unmapped = await readUnmappedFromStdin();
  const descById = await loadTranslations();

  const byCategory = {};
  const topByCount = [...unmapped].sort((a, b) => b.count - a.count);

  for (const row of unmapped) {
    const desc = row.loca_id != null ? descById.get(row.loca_id) || "" : "";
    const cat = categorizeResearchDescription(desc);
    byCategory[cat] = (byCategory[cat] || 0) + 1;
  }

  const pctFalse = unmapped.filter((u) => u.value_is_percentage === false).length;

  const report = {
    unmapped_buff_ids: unmapped.length,
    value_is_percentage_false: pctFalse,
    by_category: byCategory,
    top_by_count: topByCount.slice(0, topN).map((u) => ({
      buff_id: u.buff_id,
      count: u.count,
      loca_id: u.loca_id,
      value_is_percentage: u.value_is_percentage,
      category: categorizeResearchDescription(u.loca_id != null ? descById.get(u.loca_id) : ""),
      description_snippet: String(u.loca_id != null ? descById.get(u.loca_id) || "" : "").slice(0, 140),
    })),
  };

  if (asJson) {
    console.log(JSON.stringify(report, null, 2));
    return;
  }

  console.log(`Unmapped buff ids: ${report.unmapped_buff_ids}`);
  console.log(`value_is_percentage=false: ${report.value_is_percentage_false}`);
  console.log("\nBy category:");
  for (const [k, v] of Object.entries(byCategory).sort((a, b) => b[1] - a[1])) {
    console.log(`  ${k}: ${v}`);
  }
  console.log(`\nTop ${topN} by occurrence count:`);
  for (const row of report.top_by_count) {
    console.log(
      `  ${row.buff_id}  count=${row.count}  ${row.category}  ${row.description_snippet.replace(/\s+/g, " ").slice(0, 90)}`
    );
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
