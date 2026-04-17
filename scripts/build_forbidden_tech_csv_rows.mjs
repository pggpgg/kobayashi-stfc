/**
 * Reads cached forbidden tech (summary tech_type === 0) detail JSON from
 * data/upstream/data-stfc-space/forbidden_tech/{id}.json and prints CSV rows
 * for data/import/forbidden_chaos_tech.csv (stdout).
 *
 * Uses values[45] when present (level-46-style calibration; same as build_chaos_tech_csv_rows.mjs).
 * Skips economy / proc lines without a clear combat stat mirror.
 *
 * Usage:
 *   node scripts/build_forbidden_tech_csv_rows.mjs
 *   node scripts/build_forbidden_tech_csv_rows.mjs --missing-only
 *
 * --missing-only: only emit rows for fids not already present in data/forbidden_chaos_tech.json
 */

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import {
  flattenBuffChains,
  referenceValue,
  lastNonzeroValue,
  fmtNum,
  catalogValueForBuff,
  resolveStatForBuff,
} from "./lib/forbidden_tech_csv_shared.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.join(__dirname, "..");

function loadJson(p) {
  return JSON.parse(fs.readFileSync(path.join(ROOT, p), "utf8"));
}

function main() {
  const missingOnly = process.argv.includes("--missing-only");

  const summary = loadJson("data/upstream/data-stfc-space/summary-forbidden_tech.json");
  const translations = loadJson(
    "data/upstream/data-stfc-space/translations-forbidden_tech.json"
  );

  let catalogFids = new Set();
  if (missingOnly) {
    try {
      const catalog = loadJson("data/forbidden_chaos_tech.json");
      catalogFids = new Set(catalog.items.map((x) => x.fid).filter((f) => f != null));
    } catch {
      catalogFids = new Set();
    }
  }

  const forbidden = summary.filter((e) => e.tech_type === 0);
  const rows = [];

  for (const s of forbidden) {
    if (missingOnly && catalogFids.has(s.id)) continue;

    const nameRow = translations.find(
      (e) => e.id === s.loca_id && e.key?.endsWith("forbidden_tech_name")
    );
    const name = nameRow?.text?.replace(/"/g, '""') ?? `fid_${s.id}`;
    const tier = s.tier_max ?? 12;

    const detailPath = path.join(
      ROOT,
      "data/upstream/data-stfc-space/forbidden_tech",
      `${s.id}.json`
    );
    if (!fs.existsSync(detailPath)) {
      console.error(`// skip ${s.id} ${name}: missing ${detailPath}`);
      continue;
    }
    const detail = JSON.parse(fs.readFileSync(detailPath, "utf8"));

    const chains = flattenBuffChains(detail);
    let emitted = 0;

    for (const b of chains) {
      if (!b.values?.length) continue;
      const stat = resolveStatForBuff(b, translations, null);
      if (!stat) continue;

      const { ok, value: catalogValue } = catalogValueForBuff(b, stat);
      if (!ok) continue;
      if (catalogValue === 0) continue;

      rows.push(`${name},forbidden,${tier},${s.id},${stat},${fmtNum(catalogValue)},add`);
      emitted++;
    }

    if (emitted === 0) {
      let best = 0;
      for (const b of chains) {
        if (!b.values?.length || !b.value_is_percentage) continue;
        const r = referenceValue(b.values) || lastNonzeroValue(b.values);
        if (r > 0 && r <= 150 && r > best) best = r;
      }
      if (best > 0) {
        console.error(
          `// fallback weapon_damage for ${name} (${s.id}) — review translations / engine scope`
        );
        rows.push(`${name},forbidden,${tier},${s.id},weapon_damage,${fmtNum(best / 100)},add`);
      } else {
        console.error(`// warning: no usable values for ${name} (${s.id}) — placeholder hull_hp 0`);
        rows.push(`${name},forbidden,${tier},${s.id},hull_hp,0,add`);
      }
    }
  }

  for (const line of rows) console.log(line);
}

main();
