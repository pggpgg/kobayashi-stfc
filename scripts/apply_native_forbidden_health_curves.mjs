/** Refresh reviewed equipment-scoped Quantum Slipstream curves from native group141.
 * node scripts/apply_native_forbidden_health_curves.mjs <cache.bin> <catalog.json> <manifest.json>
 * Only reviewed Quantum Slipstream combat rows are selected. No player state is read.
 */
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { fields } from "./inspect_stfc_combat_config.mjs";
import { extractNativeBuffSpec } from "./extract_stfc_research_buff_semantics.mjs";
const FID = 2439729135, BID = 331547130;
const REVIEWED = [
  {id:BID, stat:"shield_hp", modifier:60, operation:3, target:1, trigger:24, tier:8},
  {id:1746739507, stat:"crit_chance", modifier:9, operation:1, target:1, trigger:24, tier:3},
  {id:2885215064, stat:"shield_mitigation", modifier:76, operation:2, target:6, trigger:18, tier:6},
];
const sha = bytes => crypto.createHash("sha256").update(bytes).digest("hex");
const nested = field => fields(field.value, field.dataOffset);
function one(rows, number, wire) {
  const selected = rows.filter(row => row.number === number);
  if (selected.length !== 1 || selected[0].wire !== wire) throw Error(`Invalid field ${number}`);
  return selected[0];
}
const integer = (rows, number) => Number(one(rows, number, 0).value);

export function extractNativeForbiddenHealthCurve(cache, detailBytes) {
  if (!cache.length || cache.length > 128 * 1024 * 1024) throw Error("Invalid cache size");
  const groups = fields(cache).filter(row => row.number === 1).map(row => nested(row));
  const ft = groups.filter(group => integer(group, 1) === 141);
  if (ft.length !== 1) throw Error("Expected one ForbiddenTech group141");
  const detail = JSON.parse(detailBytes), map=nested(one(ft[0], 2, 2)).filter(row=>row.number===1).map(row=>nested(row));
  const items=REVIEWED.map(reviewed=>{
  const matches=map.filter(row=>integer(row,1)===reviewed.id);
  if(matches.length!==1)throw Error("Expected one reviewed Quantum buff");
  const payload=one(matches[0],2,2);
  if(integer(nested(payload),1)!==reviewed.id)throw Error("Buff ID mismatch");
  const spec=extractNativeBuffSpec(payload);
  if (spec.modifier_code !== reviewed.modifier || spec.operation_code !== reviewed.operation || spec.target_code !== reviewed.target ||
      spec.trigger_code !== reviewed.trigger || spec.condition_codes.length || spec.attribute_fields.length ||
      spec.unknown_fields.length || spec.ranked_value_type !== 0 ||
      !spec.ranked_chances.length || spec.ranked_chances.some(chance => chance !== 1) ||
      spec.ranked_value_count !== spec.ranked_chance_count) throw Error("Unreviewed Quantum semantics");
  const values = Array.from({length: spec.ranked_value_count}, (_, rank) =>
    spec.ranked_values[Math.min(rank, spec.ranked_values.length - 1)]);
  const linked = (detail.buffs ?? []).flatMap(group => (group.buffs ?? [])
    .filter(buff => buff.id === reviewed.id).map(buff => ({buff, minimum_tier: group.tier})));
  if (detail.id !== FID || linked.length !== 1 || linked[0].minimum_tier !== reviewed.tier ||
      JSON.stringify(linked[0].buff.values.map(row => row.value)) !== JSON.stringify(values) ||
      linked[0].buff.values.some(row => row.chance !== 1)) throw Error("Upstream/native binding drift");
  return {fid:FID,stat:reviewed.stat,buff_id:reviewed.id,minimum_tier:reviewed.tier,values,
    native_spec_sha256:spec.sha256,native_spec_byte_offset:spec.byte_offset,native_spec_byte_length:spec.byte_length,
    modifier_code:spec.modifier_code,operation_code:spec.operation_code,target_code:spec.target_code,trigger_code:spec.trigger_code};
  });
  return {schema_version: 1, source_group: 141, selection: "Quantum Slipstream shield HP, critical chance and opponent cumulative shield-mitigation subtraction. Warp range is noncombat.",
    provenance: {cache_sha256: sha(cache), cache_byte_length: cache.length,
      upstream_detail_sha256: sha(detailBytes),
      upstream_detail: `data/upstream/data-stfc-space/forbidden_tech/${FID}.json`,
      unit: "Native ADD/SUB absolute fractions and MULT additive fractions; value_is_percentage is display metadata, not a request to divide the native double by100.",
      rank_contract: "Owned level-1 selects the rank; owned tier must unlock each native buff; equipment selection remains mandatory."},items};
}

export function applyNativeForbiddenHealthCurves(catalog, manifest) {
  const result = structuredClone(catalog);
  for (const curve of manifest.items) {
    const records = result.items.filter(item => item.fid === curve.fid);
    if (records.length !== 1) throw Error("Expected one equipped-tech catalog record");
    const bonuses = records[0].bonuses.filter(bonus => bonus.stat === curve.stat);
    if (bonuses.length !== 1) throw Error("Expected one reviewed equipment bonus");
    // Scalar catalog summaries show the maximum published native rank. Combat
    // requires the owned rank curve, not an arbitrary summary-level fallback.
    bonuses[0].value = Math.max(...curve.values);
    bonuses[0].operator = "add";
    bonuses[0].rank_curve = {buff_id: curve.buff_id, minimum_tier: curve.minimum_tier, values: curve.values};
  }
  return result;
}

export function main(args) {
  if (args.length !== 3) throw Error("Usage: node scripts/apply_native_forbidden_health_curves.mjs <cache.bin> <catalog.json> <manifest.json>");
  const [cachePath, catalogPath, manifestPath] = args.map(arg => path.resolve(arg));
  if (new Set([cachePath, catalogPath, manifestPath]).size !== 3) throw Error("Distinct paths required");
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const manifest = extractNativeForbiddenHealthCurve(fs.readFileSync(cachePath),
    fs.readFileSync(path.join(root, `data/upstream/data-stfc-space/forbidden_tech/${FID}.json`)));
  const result = applyNativeForbiddenHealthCurves(JSON.parse(fs.readFileSync(catalogPath)), manifest);
  fs.writeFileSync(catalogPath, JSON.stringify(result, null, 2) + "\n");
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
  console.log(JSON.stringify({curves:manifest.items.length,cache_sha256:manifest.provenance.cache_sha256,
    fid:FID,buff_id:BID,minimum_tier:8,rank_count:manifest.items[0].values.length}));
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
