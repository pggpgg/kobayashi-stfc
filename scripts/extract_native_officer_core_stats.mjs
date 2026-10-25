#!/usr/bin/env node
// Generate only public static officer/growth definitions. No player/profile
// state is read. The source digest and schema/conversion proof are reviewed,
// rather than accepting an arbitrary cache as the same native contract.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';

export const REVIEWED_SOURCE = Object.freeze({
  static_cache_sha256: 'b472e4460e3869cc8abb44e456a0873e3c5702ed6f3a9701b18d27938022f3cf',
  reference_client_build: 272,
  reference_metadata_sha256: 'ab0ccdc3e9e09e59d16a13431e2a0722255a71d5ff373cc09d95a47cd130ca04',
  reference_binary_sha256: 'a5df6314bd78f15892a232a44e77cdcf2e3d20e9a6192f99120c0f3e52632274',
});
export const VERIFIED_ALGORITHM_273 = Object.freeze({
  client_build: 273,
  metadata_sha256: '2b586d6f23eb130c210ddca13d8ebdd21f09f807e2eb94f4972bf321b5f821da',
  binary_sha256: 'a9ab2edbc143cfba3aff35665e129006f73853070dbff206884fda04a235ab94',
  create_core_stats_address: '0x1818848c0',
  create_core_stats_evidence_sha256: '5470c8d46b47729d10dfebedb5695fad85d235cefcfc030f7453416584c8856a',
  round_helper_address: '0x1816f7870',
  round_helper_evidence_sha256: '16cc878ea19184d5ba28fe4bb57ccd5456dcc90576958ea04a55bbf48d9f08d9',
});
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const fail = message => {throw Error(message);};

/** Bind both reviewed checkout EOL forms, without accepting other byte changes. */
export function upstreamSourceByteBindings(raw, id) {
  if (!Buffer.isBuffer(raw)) fail('Upstream binding must use exact file bytes');
  const text = new TextDecoder('utf-8', {fatal: true}).decode(raw);
  const lf = text.replace(/\r\n/g, '\n');
  if (lf.includes('\r')) fail(`Unsupported upstream line endings for officer ${id}`);
  const lfBytes = Buffer.from(lf, 'utf8'), crlfBytes = Buffer.from(lf.replace(/\n/g, '\r\n'), 'utf8');
  if (!raw.equals(lfBytes) && !raw.equals(crlfBytes)) fail(`Mixed/unreviewed upstream line endings for officer ${id}`);
  const parsed = JSON.parse(lf);
  if (!Number.isSafeInteger(parsed.id) || parsed.id !== Number(id)) fail(`Upstream identity mismatch for native officer ${id}`);
  return {lf_sha256: digest(lfBytes), crlf_sha256: digest(crlfBytes)};
}

function fields(bytes) {
  let cursor = 0;
  const varint = () => {
    let value = 0n;
    for (let i = 0; i < 10; i++) {
      if (cursor >= bytes.length) fail('Truncated protobuf varint');
      const byte = bytes[cursor++];
      if (i === 9 && byte > 1) fail('Overflowing protobuf varint');
      value |= BigInt(byte & 127) << BigInt(7 * i);
      if (!(byte & 128)) return value;
    }
    fail('Overlong protobuf varint');
  };
  const result = [];
  while (cursor < bytes.length) {
    const tag = varint(), number = Number(tag >> 3n), wire = Number(tag & 7n);
    if (!number || number > 0x1fffffff) fail('Invalid protobuf field number');
    let value;
    if (wire === 0) value = varint();
    else if (wire === 2) {
      const length = varint();
      if (length > BigInt(bytes.length - cursor)) fail('Truncated protobuf bytes');
      value = bytes.subarray(cursor, cursor + Number(length));
      cursor += Number(length);
    } else if (wire === 1 || wire === 5) {
      const length = wire === 1 ? 8 : 4;
      if (cursor + length > bytes.length) fail('Truncated protobuf fixed value');
      value = bytes.subarray(cursor, cursor + length);
      cursor += length;
    } else fail(`Unsupported protobuf wire ${wire}`);
    result.push({number, wire, value});
  }
  return result;
}
function one(rows, number) {
  const selected = rows.filter(row => row.number === number);
  if (selected.length > 1) fail(`Duplicate singular protobuf field ${number}`);
  return selected[0];
}
function integer(rows, number, fallback = 0n) {
  const field = one(rows, number);
  if (!field) return fallback;
  if (field.wire !== 0) fail(`Expected varint field ${number}`);
  return field.value;
}
function message(rows, number) {
  const field = one(rows, number);
  if (!field || field.wire !== 2) fail(`Expected message field ${number}`);
  return field.value;
}
function positiveSafeInteger(value, label) {
  if (value <= 0n || value > BigInt(Number.MAX_SAFE_INTEGER)) fail(`Unsupported ${label}`);
  return Number(value);
}
function entries(payload) {
  return fields(payload).filter(row => row.number === 1).map(row => {
    if (row.wire !== 2) fail('Expected native map entry');
    const entry = fields(row.value);
    return {key: integer(entry, 1), bytes: message(entry, 2)};
  });
}
function unique(rows, key, label) {
  if (new Set(rows.map(row => row[key])).size !== rows.length) fail(`Duplicate ${label}`);
}

/** Decode the schema-bound groups; caller verifies the reviewed source digest. */
export function decodeNativeOfficerCoreStats(bytes) {
  const groups = fields(bytes).filter(row => row.number === 1).map(row => {
    if (row.wire !== 2) fail('Expected static-cache group');
    const group = fields(row.value);
    return {id: integer(group, 1), fields: group};
  });
  const group = id => {
    const found = groups.filter(row => row.id === BigInt(id));
    if (found.length !== 1) fail(`Expected exactly one native static group ${id}`);
    return message(found[0].fields, 2);
  };
  const officerPayload = group(7), growthPayload = group(18);
  const growth = entries(growthPayload).map(entry => {
    const spec = fields(entry.bytes), level = positiveSafeInteger(entry.key, 'growth level');
    if (integer(spec, 1) !== entry.key) fail('Growth level/map-key mismatch');
    const stats = spec.filter(row => row.number === 2).map(row => {
      if (row.wire !== 2) fail('Expected native growth map');
      const entry = fields(row.value), rarity = positiveSafeInteger(integer(entry, 1), 'growth rarity');
      const value = one(entry, 2);
      if (!value || value.wire !== 5) fail('Expected native float32 growth');
      const amount = value.value.readFloatLE(0);
      if (!Number.isFinite(amount) || amount <= 0) fail('Invalid native growth value');
      return {rarity, amount};
    });
    if (!stats.length) fail('Empty native growth row');
    unique(stats, 'rarity', 'growth rarity');
    return {level, row: {source_sha256: digest(entry.bytes),
      by_rarity: Object.fromEntries(stats.toSorted((a, b) => a.rarity - b.rarity).map(row => [row.rarity, row.amount]))}};
  });
  unique(growth, 'level', 'growth level');
  const officers = entries(officerPayload).map(entry => {
    const spec = fields(entry.bytes), id = positiveSafeInteger(entry.key, 'officer id');
    if (integer(spec, 1) !== entry.key) fail('Native officer ID/map-key mismatch');
    const rarity = positiveSafeInteger(integer(spec, 11), 'officer rarity');
    const axis = number => {
      const raw = integer(spec, number);
      if (raw > 0x7fffffffn) fail('Unsupported negative/overflowing native officer weight');
      return Number(raw);
    };
    return {id, row: {id, rarity, weights: {attack: axis(15), defense: axis(16), health: axis(17)}, source_sha256: digest(entry.bytes)}};
  });
  unique(officers, 'id', 'officer source id');
  if (!officers.length || !growth.length) fail('Missing native officer/growth rows');
  return {officer_specs_group: {id: 7, payload_sha256: digest(officerPayload), count: officers.length},
    core_stat_specs_group: {id: 18, payload_sha256: digest(growthPayload), count: growth.length},
    officers: Object.fromEntries(officers.toSorted((a, b) => a.id - b.id).map(row => [row.id, row.row])),
    growth_by_level: Object.fromEntries(growth.toSorted((a, b) => a.level - b.level).map(row => [row.level, row.row]))};
}

export function buildNativeOfficerCoreStatsCatalog(bytes, upstreamFiles = new Map()) {
  if (digest(bytes) !== REVIEWED_SOURCE.static_cache_sha256) fail('Static cache is not the reviewed source digest');
  const decoded = decodeNativeOfficerCoreStats(bytes);
  let bound = 0;
  for (const [id, officer] of Object.entries(decoded.officers)) {
    const raw = upstreamFiles.get(id);
    officer.upstream_byte_bindings = null;
    if (raw === undefined) continue;
    officer.upstream_byte_bindings = upstreamSourceByteBindings(raw, id);
    bound++;
  }
  return {schema_version: 1, provenance: {...REVIEWED_SOURCE,
    officer_specs_group: decoded.officer_specs_group, core_stat_specs_group: decoded.core_stat_specs_group,
    bound_upstream_count: bound,
    upstream_binding_policy: 'exact_utf8_lf_or_crlf_v1',
    algorithm_verification_273: VERIFIED_ALGORITHM_273,
    source_note: 'Static definitions retained from client272 source evidence; cache copied2026-10-23. Conversion independently verified in client273. Each upstream binding contains exact LF and CRLF UTF-8 byte hashes derived from the same reviewed JSON content; mixed line endings are rejected. Runtime compares raw bytes without normalization. No player state. Runtime/server consumers remain separate evidence.'},
  officers: decoded.officers, growth_by_level: decoded.growth_by_level};
}

function main() {
  const args = process.argv.slice(2);
  let source, output = 'data/native_officer_core_stats.json', upstream = 'data/upstream/data-stfc-space/officers', check = false;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--static-cache') source = args[++i];
    else if (args[i] === '--output') output = args[++i];
    else if (args[i] === '--upstream-officers') upstream = args[++i];
    else if (args[i] === '--check') check = true;
    else fail(`Unknown argument ${args[i]}`);
  }
  if (!source || !output || !upstream) fail('Usage: node scripts/extract_native_officer_core_stats.mjs --static-cache PATH [--output PATH] [--upstream-officers PATH] [--check]');
  const upstreamFiles = new Map(fs.readdirSync(upstream).filter(name => /^\d+\.json$/.test(name))
    .map(name => [name.slice(0, -5), fs.readFileSync(path.join(upstream, name))]));
  const catalog = buildNativeOfficerCoreStatsCatalog(fs.readFileSync(source), upstreamFiles);
  const serialized = JSON.stringify(catalog, null, 2) + '\n';
  if (check) {
    if (fs.readFileSync(output, 'utf8') !== serialized) fail('Generated native officer CoreStats catalog differs');
  } else fs.writeFileSync(output, serialized);
  console.log(JSON.stringify({output, check, sha256: digest(Buffer.from(serialized)),
    officers: Object.keys(catalog.officers).length, growth_levels: Object.keys(catalog.growth_by_level).length,
    bound_upstream_count: catalog.provenance.bound_upstream_count}));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
