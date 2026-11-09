#!/usr/bin/env node
/** Generate public native Syndicate reward provenance, never player bonuses. */
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {fields} from './inspect_stfc_combat_config.mjs';
import {extractNativeBuffSpec} from './extract_stfc_research_buff_semantics.mjs';

export const REVIEWED_CACHE_SHA256 = 'b472e4460e3869cc8abb44e456a0873e3c5702ed6f3a9701b18d27938022f3cf';
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const nested = field => {
  if (!field || field.wire !== 2) throw Error('Expected native message');
  return fields(field.value, field.dataOffset);
};
const one = (rows, number, required = false) => {
  const found = rows.filter(r => r.number === number);
  if (found.length > 1 || (required && !found.length)) throw Error(`Invalid singular field ${number}`);
  return found[0];
};
const integer = (rows, number, fallback = 0) => {
  const field = one(rows, number);
  if (!field) return fallback;
  if (field.wire !== 0 || field.value > BigInt(Number.MAX_SAFE_INTEGER)) throw Error(`Invalid integer ${number}`);
  return Number(field.value);
};
const shape = (rows, allowed) => {
  if (rows.some(r => !allowed.includes(r.number))) throw Error('Unknown Syndicate schema field');
};
const positive = (value, label) => {
  if (!Number.isSafeInteger(value) || value <= 0) throw Error(`Invalid ${label}`);
  return value;
};
const unique = (rows, key, label) => {
  if (new Set(rows.map(r => r[key])).size !== rows.length) throw Error(`Duplicate ${label}`);
};

/** Schema decoder exported for synthetic tests; publication also requires the reviewed digest. */
export function decodeNativeSyndicateRewards(bytes) {
  if (!Buffer.isBuffer(bytes) || !bytes.length || bytes.length > 128 * 1024 * 1024) throw Error('Invalid static cache');
  const groups = fields(bytes).filter(f => f.number === 1).map(nested);
  const get = code => {
    const selected = groups.filter(r => integer(r, 1) === code);
    if (selected.length !== 1) throw Error(`Expected one native group ${code}`);
    return one(selected[0], 2, true);
  };
  const loyalty = get(122), loyaltyRows = nested(loyalty);
  shape(loyaltyRows, [1, 2]);
  const configRows = nested(one(loyaltyRows, 1, true));
  shape(configRows, [1, 2, 3]);
  const spec = one(loyaltyRows, 2, true), specRows = nested(spec);
  shape(specRows, [1]);
  const tiers = specRows.map(field => {
    const r = nested(field);
    shape(r, [1, 2, 3, 4, 5, 6, 7]);
    const text = one(r, 2, true);
    if (text.wire !== 2) throw Error('Invalid tier string');
    const idString = new TextDecoder('utf-8', {fatal: true}).decode(text.value);
    const level = positive(integer(r, 7), 'tier level');
    if (idString !== `Loyalty_${level}`) throw Error('Tier string/level mismatch');
    const collections = r.filter(f => f.number === 6).map(collection => {
      const cr = nested(collection);
      shape(cr, [1, 2]);
      const rewards = cr.filter(f => f.number === 1).map(reward => {
        const br = nested(reward);
        shape(br, [1, 2]);
        return {buff_id: positive(integer(br, 1), 'reward BuffID'), index: positive(integer(br, 2), 'reward index')};
      }).sort((a, b) => a.index - b.index);
      unique(rewards, 'index', 'reward index');
      return {index: positive(integer(cr, 2), 'collection index'), rewards};
    }).sort((a, b) => a.index - b.index);
    unique(collections, 'index', 'collection index');
    const min = integer(r, 3), max = integer(r, 4);
    if (min > max) throw Error('Invalid tier point range');
    return {level, id: positive(integer(r, 1), 'tier ID'), id_string: idString,
      min_points: min, max_points: max, reward_descriptor_count: integer(r, 5), collections,
      sha256: sha256(field.value), byte_offset: field.dataOffset, byte_length: field.value.length};
  }).sort((a, b) => a.level - b.level);
  unique(tiers, 'level', 'tier level');
  unique(tiers, 'id', 'tier ID');
  if (!tiers.length || tiers.some((t, i) => t.level !== i + 1)) throw Error('Noncontiguous native tiers');
  const wanted = new Set(tiers.flatMap(t => t.collections.flatMap(c => c.rewards.map(r => r.buff_id))));
  const buffs = {};
  const sources = [{id: 122, payload_sha256: sha256(loyalty.value)}];
  for (const group of [68, 116]) {
    const payload = get(group);
    sources.push({id: group, payload_sha256: sha256(payload.value)});
    for (const entry of nested(payload).filter(f => f.number === 1)) {
      const map = nested(entry), id = integer(map, 1);
      if (!wanted.has(id)) continue;
      const value = one(map, 2, true), r = nested(value);
      if (id <= 0 || integer(r, 1) !== id || Object.hasOwn(buffs, id)) throw Error('Ambiguous reward BuffID/source');
      const attributes = one(r, 16);
      buffs[id] = {source_group: group, rank_source: group === 116 ? 'operations_level' : 'building_level',
        ...extractNativeBuffSpec(value),
        attributes: attributes ? nested(attributes).map(f => ({field: f.number, wire: f.wire,
          value: typeof f.value === 'bigint' ? String(BigInt.asIntN(64, f.value)) : f.value.toString('hex')})) : []};
    }
  }
  return {config: {resource_id: integer(configRows, 1), min_player_level: integer(configRows, 2),
    claim_all_min_level: integer(configRows, 3)}, sources, tiers, buffs,
    unresolved_buff_ids: [...wanted].filter(id => !Object.hasOwn(buffs, id)).sort((a, b) => a - b)};
}

export function buildNativeSyndicateRewardsCatalog(bytes) {
  if (sha256(bytes) !== REVIEWED_CACHE_SHA256) throw Error('Static cache is not the reviewed source digest');
  const decoded = decodeNativeSyndicateRewards(bytes);
  return {schema_version: 1,
    provenance: {static_cache_sha256: REVIEWED_CACHE_SHA256, static_cache_build: 273,
      schema_revision: 'netniV/stfc-mod@e0b8552bc2e8922423c6cdf3ba00ba0a81bf23c0',
      wire_reference_metadata_build: 274,
      wire_reference_metadata_sha256: 'abd0233dfb3c66eb6a8b70789e0c5d1e5a4b77b7977378768b9eeb497f62601f',
      cross_checked_cache_sha256: 'b7fcb7a021fa982144702a682e81eb6b7a31406522b0154cc77553a2c0776418',
      groups: decoded.sources,
      public_corroboration: 'https://data.stfc.space/syndicate/summary.json',
      source_note: 'Static personal LoyaltySpecs(122) reward references plus referenced global116/building68 BuffSpecs only. No player state. Client definitions do not establish current server behavior.'},
    rank_contract: {global: 'Each global116 reward uses its current Ops-level snapshot, index Ops-1. Unlock tier is separate; do not sum rank history.',
      building: 'Group68 reward links use building level, never substitute Ops. Preserve Syndicate predicates203/204/205.',
      indices: 'Collection and reward indices order descriptors; neither is a stat rank.',
      deduplication: 'A report counts each unlocked BuffID once. Runtime separately compiles active BuffIDs; never add this report over those bonuses.'},
    config: decoded.config,
    coverage: {tier_count: decoded.tiers.length,
      reward_reference_count: decoded.tiers.reduce((n, t) => n + t.collections.reduce((k, c) => k + c.rewards.length, 0), 0),
      unique_buff_count: Object.keys(decoded.buffs).length + decoded.unresolved_buff_ids.length,
      resolved_buff_count: Object.keys(decoded.buffs).length},
    tiers: decoded.tiers, buffs: decoded.buffs, unresolved_buff_ids: decoded.unresolved_buff_ids};
}

export function main(args) {
  const options = {};
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--check') options.check = true;
    else if (['--static-cache', '--output'].includes(arg) && args[i + 1] && !args[i + 1].startsWith('--')) {
      if (options[arg]) throw Error(`Duplicate ${arg}`);
      options[arg] = args[++i];
    } else throw Error('Usage: node scripts/extract_native_syndicate_rewards.mjs --static-cache <cache.bin> [--output <catalog.json>] [--check]');
  }
  if (!options['--static-cache']) throw Error('--static-cache is required');
  const input = path.resolve(options['--static-cache']);
  const output = path.resolve(options['--output'] ?? fileURLToPath(new URL('../data/native_syndicate_rewards.json', import.meta.url)));
  if (input === output) throw Error('Output cannot replace static cache');
  if (!fs.statSync(input).isFile() || fs.statSync(input).size > 128 * 1024 * 1024) throw Error('Invalid static cache file');
  const result = buildNativeSyndicateRewardsCatalog(fs.readFileSync(input));
  const text = JSON.stringify(result, null, 2) + '\n';
  if (options.check) {
    if (fs.readFileSync(output, 'utf8').replaceAll('\r\n', '\n') !== text) throw Error('Native Syndicate catalog differs; regenerate it');
  } else fs.writeFileSync(output, text);
  console.log(JSON.stringify({checked: Boolean(options.check), ...result.coverage, unresolved_buff_ids: result.unresolved_buff_ids}));
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
