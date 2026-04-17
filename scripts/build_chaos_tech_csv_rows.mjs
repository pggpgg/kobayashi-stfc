/**
 * Chaos tech (summary tech_type === 1): fetch or load cached detail JSON and print CSV rows
 * for data/import/forbidden_chaos_tech.csv (stdout).
 *
 * Uses values[45] when present (level-46-style calibration; same as S31 docs).
 * Skips economy / proc lines without a clear combat stat mirror.
 *
 * Usage:
 *   node scripts/build_chaos_tech_csv_rows.mjs
 *   node scripts/build_chaos_tech_csv_rows.mjs --local
 *
 * --local: read only data/upstream/data-stfc-space/forbidden_tech/{id}.json (no network).
 * Optional overrides: data/import/chaos_tech_buff_overrides.json (by_buff_id / by_loca_id -> stat).
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
  loadChaosBuffOverrides,
  resolveStatForBuff,
} from "./lib/forbidden_tech_csv_shared.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.join(__dirname, "..");

function loadJson(p) {
  return JSON.parse(fs.readFileSync(path.join(ROOT, p), "utf8"));
}

async function fetchJson(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url} -> ${r.status}`);
  return r.json();
}

async function loadDetail(id, useLocal) {
  const p = path.join(ROOT, "data/upstream/data-stfc-space/forbidden_tech", `${id}.json`);
  if (useLocal) {
    if (!fs.existsSync(p)) {
      throw new Error(`missing cached file: ${p}`);
    }
    return JSON.parse(fs.readFileSync(p, "utf8"));
  }
  return fetchJson(`https://data.stfc.space/forbidden_tech/${id}.json`);
}

async function main() {
  const useLocal = process.argv.includes("--local");

  const summary = loadJson("data/upstream/data-stfc-space/summary-forbidden_tech.json");
  const translations = loadJson(
    "data/upstream/data-stfc-space/translations-forbidden_tech.json"
  );
  const overrides = loadChaosBuffOverrides(ROOT);

  const chaos = summary.filter((e) => e.tech_type === 1);
  const rows = [];

  for (const s of chaos) {
    const nameRow = translations.find(
      (e) => e.id === s.loca_id && e.key?.endsWith("forbidden_tech_name")
    );
    const name = nameRow?.text?.replace(/"/g, '""') ?? `fid_${s.id}`;
    const tier = s.tier_max ?? 12;

    let detail;
    try {
      detail = await loadDetail(s.id, useLocal);
    } catch (e) {
      console.error(`// skip ${s.id} ${name}: ${e.message}`);
      continue;
    }

    const chains = flattenBuffChains(detail);
    let emitted = 0;

    for (const b of chains) {
      if (!b.values?.length) continue;
      const stat = resolveStatForBuff(b, translations, overrides);
      if (!stat) continue;

      const { ok, value: catalogValue } = catalogValueForBuff(b, stat);
      if (!ok) continue;
      if (catalogValue === 0) continue;

      rows.push(`${name},chaos,${tier},${s.id},${stat},${fmtNum(catalogValue)},add`);
      emitted++;
    }

    if (emitted === 0) {
      const hadCombatMappedBuff = chains.some(
        (b) =>
          b.values?.length &&
          resolveStatForBuff(b, translations, overrides) != null
      );
      if (!hadCombatMappedBuff) {
        console.error(
          `// no combat-mappable buff text for ${name} (${s.id}) — placeholder hull_hp 0 (warp/economy-only lines skipped)`
        );
        rows.push(`${name},chaos,${tier},${s.id},hull_hp,0,add`);
        continue;
      }
      let best = 0;
      for (const b of chains) {
        if (!b.values?.length || !b.value_is_percentage) continue;
        const r = referenceValue(b.values) || lastNonzeroValue(b.values);
        if (r > 0 && r <= 150 && r > best) best = r;
      }
      if (best > 0) {
        console.error(
          `// [review] fallback weapon_damage for ${name} (${s.id}) — mapped stats dropped by value rules; check overrides`
        );
        rows.push(`${name},chaos,${tier},${s.id},weapon_damage,${fmtNum(best / 100)},add`);
      } else {
        console.error(
          `// warning: no usable values for ${name} (${s.id}) — placeholder hull_hp 0`
        );
        rows.push(`${name},chaos,${tier},${s.id},hull_hp,0,add`);
      }
    }
  }

  for (const line of rows) console.log(line);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
