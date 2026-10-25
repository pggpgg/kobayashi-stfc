#!/usr/bin/env node
/** Extract only research BuffSpec facts for HP, weapon, critical, isolytic,
 * launch defense, penetration and selected officer-stat setup families.
 * Input is an explicitly selected ServiceResponse cache; player groups are ignored.
 * Wire schema: netniV/stfc-mod e0b8552, BuffSpec and StaticSyncResearchTreeSpecsResponse.
 * Values are complete rank snapshots, never display percentages. Repeated identical
 * trailing values are compacted with their original count recorded; lookup clamps.
 */
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { fields } from "./inspect_stfc_combat_config.mjs";

const FAMILY_CODES = new Set([0, 1, 2, 9, 10, 11, 12, 13, 60, 61, 73, 707, 808]);
// Issue #319 audit: exact residual BIDs from current legacy-mapped catalog rows.
// Do not select every definition sharing these modifier codes: the selected
// rows include unsupported predicates that the resolver must quarantine.
const VERIFIED_PENETRATION_BUFF_IDS = new Set([
  3089064337, 3957763486, 2516817778, 2135415291, 1014683189,
  2287504298, 1779738336, 3666020608, 62501701, 541802144,
  2788053878, 3911070176, 2610190811, 1960298662, 687745254,
  1994654654, 3961476995,
  1464350686, 3610365465, 3680634033, 511185366, 618925636, 673716117,
  113240864, 1249674165, 1516726105, 1811839815, 2635530373,
  1334510254, 1458745130, 1474989967, 3035764814, 3390416265,
  1269117837, 1284017479, 1290788123, 1407234564, 1825424766,
  2198824419, 2640187489, 3058907679, 3282480754, 3846878234,
  3876524125, 3941283922, 3993587479, 4062987421, 711650994, 949755120,
]);
// Exact residual rows in the ModOfficerStat* family; never select by modifier
// alone because other native officer-stat predicates are not reviewed here.
const VERIFIED_OFFICER_BUFF_IDS = new Set([
  1895868262, 1523931286, 624709978, 2014886179, 1829949531, 3753882631,
  1380237269, 4010211789,
]);
const BUFF_FIELDS = new Set([1, 7, 8, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21]);
const one = (rows, n, wire, required = false) => {
  const found = rows.filter((r) => r.number === n);
  if (found.length > 1 || (required && !found.length) || (found[0] && found[0].wire !== wire)) {
    throw new Error(`Invalid field ${n}`);
  }
  return found[0];
};
const nested = (f) => fields(f.value, f.dataOffset);
const int = (rows, n) => {
  const f = one(rows, n, 0);
  if (!f) return 0;
  const value = BigInt.asIntN(64, f.value);
  if (value < BigInt(Number.MIN_SAFE_INTEGER) || value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error(`Unsafe integer field ${n}`);
  }
  return Number(value);
};
function packedInts(rows, n) {
  return rows.filter((f) => f.number === n).flatMap((f) => {
    // Reuse the bounded wire reader by wrapping each packed varint with a tag.
    if (f.wire === 0) return [int([f], n)];
    if (f.wire !== 2) throw new Error(`Invalid repeated integer ${n}`);
    const tagged = [];
    for (let p = 0; p < f.value.length;) {
      const start = p;
      while (p < f.value.length && (f.value[p++] & 128)) {}
      if (p - start > 10 || f.value[p - 1] & 128) throw new Error("Invalid packed integer");
      tagged.push(Buffer.from([8]), f.value.subarray(start, p));
    }
    return fields(Buffer.concat(tagged)).map((r) => int([r], 1));
  });
}
function numbers(rows, n, size) {
  return rows.filter((f) => f.number === n).flatMap((f) => {
    if (![2, size === 8 ? 1 : 5].includes(f.wire) || f.value.length % size) {
      throw new Error(`Invalid numeric array ${n}`);
    }
    const out = [];
    for (let p = 0; p < f.value.length; p += size) {
      const value = size === 8 ? f.value.readDoubleLE(p) : f.value.readFloatLE(p);
      if (!Number.isFinite(value)) throw new Error("Nonfinite research value");
      out.push(value);
    }
    return out;
  });
}
function compact(values) {
  const copy = [...values];
  while (copy.length > 1 && copy.at(-1) === copy.at(-2)) copy.pop();
  return copy;
}

/** Shared strict BuffSpec wire reader; only static allowlisted callers may select payloads. */
export function extractNativeBuffSpec(payload) {
  const r = nested(payload);
  const values = numbers(r, 21, 8);
  if (!values.length) throw new Error("Missing ranked double values");
  const chances = numbers(r, 13, 4);
  const attributes = one(r, 16, 2);
  const attributeRows = attributes ? nested(attributes) : [];
  return {
    modifier_code: int(r, 18), operation_code: int(r, 12), target_code: int(r, 7), trigger_code: int(r, 8),
    condition_codes: packedInts(r, 17),
    attribute_fields: attributeRows.map((f) => f.number),
    unknown_fields: r.filter((f) => !BUFF_FIELDS.has(f.number)).map((f) => f.number),
    ...(attributeRows.length ? { num_rounds: int(attributeRows, 2), faction_id: int(attributeRows, 8) } : {}),
    ranked_value_type: int(r, 19),
    ranked_values: compact(values), ranked_value_count: values.length,
    ranked_chances: compact(chances), ranked_chance_count: chances.length,
    byte_offset: payload.dataOffset, byte_length: payload.value.length,
    sha256: crypto.createHash("sha256").update(payload.value).digest("hex"),
  };
}

export function extractResearchBuffSemantics(bytes) {
  if (!bytes.length || bytes.length > 128 * 1024 * 1024) throw new Error("Invalid cache size");
  const allGroups = fields(bytes).filter((f) => f.number === 1).map((f) => {
    if (f.wire !== 2) throw new Error("Invalid entity group");
    return nested(f);
  });
  const groups = allGroups.filter((r) => int(r, 1) === 66);
  if (groups.length !== 1) throw new Error("Expected one ResearchSpecs group");
  const entries = nested(one(groups[0], 2, 2, true)).filter((f) => f.number === 3);
  const buffs = {};
  for (const entry of entries) {
    const map = nested(entry);
    const payload = one(map, 2, 2, true);
    const r = nested(payload);
    const modifier_code = int(r, 18);
    const id = int(r, 1);
    if (!FAMILY_CODES.has(modifier_code) && !VERIFIED_OFFICER_BUFF_IDS.has(id) &&
        !VERIFIED_PENETRATION_BUFF_IDS.has(id)) continue;
    if (id <= 0 || int(map, 1) !== id || Object.hasOwn(buffs, id)) throw new Error("Invalid buff ID");
    buffs[id] = extractNativeBuffSpec(payload);
  }
  const static_target_trigger_specs = [];
  for (const group of allGroups.filter((r) => [54, 55].includes(int(r, 1)))) {
    const groupType = int(group, 1);
    for (const entry of nested(one(group, 2, 2, true)).filter((f) => f.number === 1)) {
      const r = nested(one(nested(entry), 2, 2, true));
      const code = int(r, 1);
      if (!(groupType === 54 ? [1, 5, 11].includes(code) : [18, 24, 25].includes(code))) continue;
      const idString = one(r, 3, 2, true);
      static_target_trigger_specs.push({ group: groupType, code, id: int(r, 2),
        name: new TextDecoder("utf-8", { fatal: true }).decode(idString.value),
        schema: groupType === 55 ? int(r, 4) : null });
    }
  }
  return {
    schema_version: 1,
    source_group: 66,
    provenance: {
      cache_sha256: crypto.createHash("sha256").update(bytes).digest("hex"), cache_byte_length: bytes.length,
      schema_revision: "netniV/stfc-mod@e0b8552bc2e8922423c6cdf3ba00ba0a81bf23c0",
      selection: "ResearchSpecs(66).researchEffects(3), modifiers 0/1/2/9/10/11/12/13/60/61/73/707/808 plus exact Orlop Defense BID 1895868262 and exact issue-319 residual mapped BIDs (39 BuffSpecs; supported mappings only after target/condition gates); no player state",
      selected_static_cache_build: 273, consumer_build: 272, encoded_server_build_binding: false,
      metadata_sha256: "ab0ccdc3e9e09e59d16a13431e2a0722255a71d5ff373cc09d95a47cd130ca04",
      game_assembly_sha256: "a5df6314bd78f15892a232a44e77cdcf2e3d20e9a6192f99120c0f3e52632274",
      limitations: "Client configuration is not proof of server combat behavior. Unsupported predicates, operators, targets, triggers and weapon subtypes are excluded by the resolver. Rank arrays compact only identical trailing values; original counts are retained.",
    },
    static_target_trigger_specs,
    buffs,
  };
}

export function main(args) {
  if (args.length !== 2) throw new Error("Usage: node scripts/extract_stfc_research_buff_semantics.mjs <cache.bin> <new-output.json>");
  const input = path.resolve(args[0]), output = path.resolve(args[1]);
  if (input === output) throw new Error("Output must differ from input");
  const stat = fs.statSync(input);
  if (!stat.isFile() || stat.size > 128 * 1024 * 1024) throw new Error("Invalid input file");
  const result = extractResearchBuffSemantics(fs.readFileSync(input));
  fs.writeFileSync(output, JSON.stringify(result, null, 2) + "\n", { flag: "wx" });
  console.log(`Extracted ${Object.keys(result.buffs).length} static research buff definitions`);
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
