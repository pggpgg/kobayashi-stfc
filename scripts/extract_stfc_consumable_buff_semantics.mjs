#!/usr/bin/env node
/** Select only native ConsumableBuffs(116).consumableBuffsSpecs(1).
 * Research(66), Starbase(68), and ForbiddenTech(141) IDs are excluded explicitly.
 * This static group includes permanent account buffs as well as timed boosts;
 * its name alone does not mean a buff is temporary.
 */
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { fields } from "./inspect_stfc_combat_config.mjs";
import { extractNativeBuffSpec } from "./extract_stfc_research_buff_semantics.mjs";
import { researchClientBuffCoverage } from "./lib/research_client_semantics.mjs";

const one = (rows, n, wire) => {
  const matches = rows.filter(r => r.number === n);
  if (matches.length !== 1 || matches[0].wire !== wire) throw new Error(`Invalid field ${n}`);
  return matches[0];
};
const nested = f => fields(f.value, f.dataOffset);
const integer = (rows, n) => {
  const value = one(rows, n, 0).value;
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error("Unsafe integer");
  return Number(value);
};
export function extractConsumableBuffSemantics(bytes) {
  return extractNativeGlobalBuffSemantics(bytes, 116);
}
export function extractNativeGlobalBuffSemantics(bytes, sourceGroup) {
  if (![68, 116].includes(sourceGroup)) throw new Error("Unsupported static buff source group");
  const sourceName = sourceGroup === 116 ? "ConsumableBuffs" : "StarbaseBuffs";
  const excludedGroups = [66, sourceGroup === 116 ? 68 : 116, 141];
  if (!bytes.length || bytes.length > 128 * 1024 * 1024) throw new Error("Invalid cache size");
  const groups = fields(bytes).filter(f => f.number === 1).map(f => {
    if (f.wire !== 2) throw new Error("Invalid static group");
    return nested(f);
  });
  const source = groups.filter(r => integer(r, 1) === sourceGroup);
  if (source.length !== 1) throw new Error(`Expected one ${sourceName} group`);
  const excluded = new Set();
  for (const group of groups) {
    const code = integer(group, 1), field = code === 66 ? 3 : 1;
    if (!excludedGroups.includes(code)) continue;
    for (const entry of nested(one(group, 2, 2)).filter(f => f.number === field)) {
      if (entry.wire !== 2) throw new Error("Invalid excluded map entry");
      excluded.add(integer(nested(entry), 1));
    }
  }
  const buffs = {};
  for (const entry of nested(one(source[0], 2, 2)).filter(f => f.number === 1)) {
    if (entry.wire !== 2) throw new Error("Invalid consumable map entry");
    const map = nested(entry), id = integer(map, 1), payload = one(map, 2, 2), r = nested(payload);
    if (id <= 0 || id !== integer(r, 1) || Object.hasOwn(buffs, id)) throw new Error("Invalid buff ID");
    if (excluded.has(id)) continue;
    const modifier = r.find(f => f.number === 18);
    const code = modifier ? integer(r, 18) : 0;
    if (![0, 1, 2, 9, 10, 11, 12, 13, 60, 61, 73, 707, 808].includes(code)) continue;
    buffs[id] = extractNativeBuffSpec(payload);
  }
  const manifest = { schema_version: 1, source_group: sourceGroup, excluded_source_groups: excludedGroups,
    provenance: { cache_sha256: crypto.createHash("sha256").update(bytes).digest("hex"),
      cache_byte_length: bytes.length, schema_revision: "netniV/stfc-mod@e0b8552bc2e8922423c6cdf3ba00ba0a81bf23c0",
      selection: `Static ${sourceName}(${sourceGroup}), map field1; modifiers0/1/2/9/10/11/12/13/60/61/73/707/808. No player state.`,
      rank_contract: "GlobalBuffEntry.level is one-based; journal ActiveBuff.Ranks is zero-based and must be converted separately.",
      limitation: "Client configuration does not prove server behavior; unsupported native predicates and attributes remain quarantined." },
    buffs };
  manifest.coverage = researchClientBuffCoverage(manifest, [], { allowMinSyndicate: sourceGroup === 68 }).summary;
  return manifest;
}
export function main(args) {
  if (args.length !== 2) throw new Error("Usage: node scripts/extract_stfc_consumable_buff_semantics.mjs <cache.bin> <new-output.json>");
  const input = path.resolve(args[0]), output = path.resolve(args[1]);
  if (input === output) throw new Error("Output must differ from input");
  const stat = fs.statSync(input);
  if (!stat.isFile() || stat.size > 128 * 1024 * 1024) throw new Error("Invalid input");
  const result = extractConsumableBuffSemantics(fs.readFileSync(input));
  fs.writeFileSync(output, JSON.stringify(result, null, 2) + "\n", { flag: "wx" });
  console.log(JSON.stringify(result.coverage));
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
