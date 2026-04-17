#!/usr/bin/env node
/**
 * Compares upstream summary forbidden-tech ids to catalog fids in data/forbidden_chaos_tech.json.
 * Prints missing entries with display names from translations-forbidden_tech.json.
 *
 * Usage:
 *   node scripts/report_forbidden_chaos_fid_coverage.mjs
 *   node scripts/report_forbidden_chaos_fid_coverage.mjs --strict
 *   node scripts/report_forbidden_chaos_fid_coverage.mjs --forbidden-only
 *
 * --strict: exit 1 if any upstream summary row has an id not present in the catalog (any tech_type).
 * --forbidden-only: only list / enforce gaps for tech_type === 0 (forbidden); chaos is ignored in strict mode.
 */

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.join(__dirname, "..");

function loadJson(p) {
  return JSON.parse(fs.readFileSync(path.join(ROOT, p), "utf8"));
}

function main() {
  const strict = process.argv.includes("--strict");
  const forbiddenOnly = process.argv.includes("--forbidden-only");

  const summary = loadJson("data/upstream/data-stfc-space/summary-forbidden_tech.json");
  const catalog = loadJson("data/forbidden_chaos_tech.json");
  const translations = loadJson(
    "data/upstream/data-stfc-space/translations-forbidden_tech.json"
  );

  const catalogFids = new Set(
    catalog.items.map((x) => x.fid).filter((fid) => fid != null)
  );

  function nameForLoca(locaId) {
    const row = translations.find(
      (e) => e.id === locaId && e.key?.endsWith("forbidden_tech_name")
    );
    return row?.text ?? `(loca ${locaId})`;
  }

  const entries = forbiddenOnly
    ? summary.filter((e) => e.tech_type === 0)
    : summary;

  const missing = [];
  for (const e of entries) {
    if (!catalogFids.has(e.id)) {
      missing.push({
        id: e.id,
        loca_id: e.loca_id,
        tech_type: e.tech_type,
        name: nameForLoca(e.loca_id),
      });
    }
  }

  console.log(
    `Catalog fids: ${catalogFids.size}; upstream ${forbiddenOnly ? "forbidden" : "all"} entries checked: ${entries.length}`
  );
  console.log(`Missing from catalog: ${missing.length}`);
  for (const m of missing.sort((a, b) => a.id - b.id)) {
    const tt =
      m.tech_type === 0 ? "forbidden" : m.tech_type === 1 ? "chaos" : String(m.tech_type);
    console.log(`  fid=${m.id}  tech_type=${tt}  ${m.name}`);
  }

  if (strict && missing.length > 0) {
    console.error(
      `\n[report_forbidden_chaos_fid_coverage] --strict: ${missing.length} summary id(s) not in catalog.`
    );
    process.exit(1);
  }
}

main();
