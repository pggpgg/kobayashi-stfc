#!/usr/bin/env node
/**
 * Read static combat configuration from one explicitly selected local STFC cache.
 *
 * Usage:
 *   node scripts/inspect_stfc_combat_config.mjs <cache.bin> [--output report.json]
 *
 * Reads protobuf ServiceResponse.entityGroups only. The allowlist is BattleConfig
 * (38), MitigationCapsSpecs (52), and GlobalDamageReductionConfig (53). Other
 * groups, playerEntityGroups, identifiers, and unknown field contents are omitted.
 * Equations are data: this script never evaluates them or connects to the game.
 * Output files are created exclusively; existing files are never overwritten.
 *
 * Schema: netniV/stfc-mod at e0b8552bc2e8922423c6cdf3ba00ba0a81bf23c0,
 * mods/src/prime/proto/Digit.PrimeServer.Models.proto and EntityGroup.h.
 * Cache presence does not establish the
 * current server version or prove that the server uses these client values.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const MAX_BYTES = 128 * 1024 * 1024;
const MAX_FIELDS = 100_000;
const MAX_TEXT_BYTES = 65_536;
const GROUPS = new Map([
  [38, "BattleConfig"],
  [52, "MitigationCapsSpecs"],
  [53, "GlobalDamageReductionConfig"],
]);
const UTF8 = new TextDecoder("utf-8", { fatal: true });

function fail(message, offset) {
  throw new Error(`${message} at byte ${offset}`);
}

/** Framing is checked even for fields whose payload is deliberately not read. */
export function fields(bytes, baseOffset = 0) {
  let pos = 0;
  const result = [];
  function varint() {
    const start = pos;
    let value = 0n;
    for (let i = 0; i < 10; i++) {
      if (pos >= bytes.length) fail("Truncated varint", baseOffset + start);
      const next = bytes[pos++];
      if (i === 9 && next > 1) fail("Varint exceeds uint64", baseOffset + start);
      value |= BigInt(next & 127) << BigInt(i * 7);
      if (!(next & 128)) return value;
    }
    fail("Unterminated varint", baseOffset + start);
  }
  while (pos < bytes.length) {
    if (result.length >= MAX_FIELDS) fail("Too many fields", baseOffset + pos);
    const start = pos;
    const tag = varint();
    if (tag > 0xffffffffn) fail("Invalid protobuf tag", baseOffset + start);
    const number = Number(tag >> 3n);
    const wire = Number(tag & 7n);
    if (!number) fail("Invalid field zero", baseOffset + start);
    let value;
    let dataOffset = pos;
    if (wire === 0) value = varint();
    else if (wire === 2) {
      const length = varint();
      if (length > BigInt(bytes.length - pos)) {
        fail("Truncated length-delimited field", baseOffset + start);
      }
      dataOffset = pos;
      value = bytes.subarray(pos, pos + Number(length));
      pos += Number(length);
    } else if (wire === 5 || wire === 1) {
      const size = wire === 5 ? 4 : 8;
      if (size > bytes.length - pos) fail("Truncated fixed field", baseOffset + start);
      value = bytes.subarray(pos, pos + size);
      pos += size;
    } else fail(`Unsupported wire type ${wire}`, baseOffset + start);
    result.push({
      number, wire, value, offset: baseOffset + start,
      dataOffset: baseOffset + dataOffset, endOffset: baseOffset + pos,
    });
  }
  return result;
}

function one(rows, number, wire, required = false) {
  const matches = rows.filter((f) => f.number === number);
  if (matches.length > 1) fail(`Duplicate singular field ${number}`, matches[1].offset);
  if (!matches.length) {
    if (required) throw new Error(`Missing required field ${number}`);
    return undefined;
  }
  const field = matches[0];
  if (field.wire !== wire) fail(`Wrong wire type for field ${number}`, field.offset);
  return field;
}

function nested(field) {
  if (field.wire !== 2) fail("Expected nested message", field.offset);
  return fields(field.value, field.dataOffset);
}

function int32(field) {
  const value = BigInt.asIntN(32, field.value);
  if (BigInt.asUintN(64, value) !== field.value) {
    fail("Invalid int32 representation", field.offset);
  }
  return Number(value);
}

function float(field) {
  const value = field.value.readFloatLE(0);
  if (!Number.isFinite(value)) fail("Non-finite float", field.offset);
  return value;
}

function text(field) {
  if (field.value.length > MAX_TEXT_BYTES) fail("Text exceeds size limit", field.offset);
  try {
    return UTF8.decode(field.value);
  } catch {
    fail("Invalid UTF-8", field.offset);
  }
}

function spans(rows) {
  return rows.map((f) => ({
    field: f.number, wire_type: f.wire,
    offset: f.offset, data_offset: f.dataOffset, end_offset: f.endOffset,
  }));
}

function readMap(rows, number, keyType, valueType) {
  // Entries stay an array: keys such as "__proto__" must remain inert data.
  const entries = [];
  const keys = new Set();
  for (const field of rows.filter((f) => f.number === number)) {
    const entry = nested(field);
    const keyField = one(entry, 1, keyType === "text" ? 2 : 0);
    const valueField = one(entry, 2, valueType === "text" ? 2 : 5);
    const key = keyField ? (keyType === "text" ? text(keyField) : int32(keyField))
      : keyType === "text" ? "" : 0;
    const value = valueField ? (valueType === "text" ? text(valueField) : float(valueField))
      : valueType === "text" ? "" : 0;
    if (keys.has(key)) fail(`Duplicate map key in field ${number}`, field.offset);
    keys.add(key);
    entries.push({
      key, value, key_encoded: Boolean(keyField), value_encoded: Boolean(valueField),
      offset: field.offset, end_offset: field.endOffset,
    });
  }
  return entries;
}

function battleConfig(rows) {
  const names = {
    1: "stationCombatDelaySeconds", 2: "pvpCombatDelaySeconds", 3: "resultLimit",
    4: "engageRangeLimit", 5: "combatLengthSeconds", 8: "pveCombatDelaySeconds",
    9: "cleanupWaitTime", 10: "starbaseProducerProtection", 11: "starbaseStealableModifier",
  };
  const scalars = {};
  for (const [raw, name] of Object.entries(names)) {
    const number = Number(raw);
    const isFloat = number === 10 || number === 11;
    const field = one(rows, number, isFloat ? 5 : 0);
    if (field) scalars[name] = isFloat ? float(field) : int32(field);
  }
  return {
    scalars,
    equations: readMap(rows, 12, "text", "text"),
    static: readMap(rows, 13, "text", "float"),
  };
}

function mitigationCaps(rows) {
  const caps = [];
  for (const field of rows.filter((f) => f.number === 1)) {
    const cap = nested(field);
    const name = one(cap, 2, 2);
    caps.push({
      name: name ? text(name) : "",
      name_encoded: Boolean(name),
      caps: readMap(cap, 1, "int32", "float"),
      officerBonusMultipliers: readMap(cap, 3, "int32", "float"),
      offset: field.offset, end_offset: field.endOffset,
    });
  }
  return { mitigationCaps: caps };
}

function globalDamageReduction(rows) {
  const result = {};
  for (const [number, name] of [
    [1, "damageReductionMin"],
    [2, "damageReductionIncrease"],
    [3, "damageReductionOffset"],
  ]) {
    const field = one(rows, number, 5);
    if (field) result[name] = float(field);
  }
  return result;
}

function envelopeProvenance(rows) {
  const version = one(rows, 5, 2);
  const timestamp = one(rows, 3, 2);
  let serverTime = null;
  if (timestamp) {
    const values = nested(timestamp);
    const secondsField = one(values, 1, 0);
    const nanosField = one(values, 2, 0);
    const seconds = secondsField ? BigInt.asIntN(64, secondsField.value) : 0n;
    const nanos = nanosField ? int32(nanosField) : 0;
    // google.protobuf.Timestamp permits years 0001..9999 and nonnegative nanos.
    if (seconds < -62135596800n || seconds > 253402300799n
        || nanos < 0 || nanos > 999999999) {
      fail("Invalid server timestamp", timestamp.offset);
    }
    serverTime = {
      unix_seconds: seconds.toString(),
      nanos,
      seconds_encoded: Boolean(secondsField),
      nanos_encoded: Boolean(nanosField),
      utc: new Date(Number(seconds) * 1000).toISOString()
        .replace(".000Z", `.${String(nanos).padStart(9, "0")}Z`),
    };
  }
  return {
    server_version: version ? text(version) : null,
    server_time: serverTime,
  };
}

export function inspectCombatConfig(bytes, sourceName = "cache.bin") {
  if (!Buffer.isBuffer(bytes) || !bytes.length || bytes.length > MAX_BYTES) {
    throw new Error(`Input must be a nonempty Buffer no larger than ${MAX_BYTES} bytes`);
  }
  const groups = [];
  const envelope = fields(bytes);
  const envelopeMetadata = envelopeProvenance(envelope);
  for (const top of envelope) {
    if (top.number !== 1) continue;
    const group = nested(top);
    const typeField = one(group, 1, 0);
    const type = typeField ? int32(typeField) : 0;
    if (!GROUPS.has(type)) continue;
    if (groups.length >= 32) throw new Error("Too many selected combat groups");
    const payload = one(group, 2, 2, true);
    const wrapper = nested(payload);
    const config = one(wrapper, 1, 2, true);
    const rows = nested(config);
    const data = type === 38 ? battleConfig(rows)
      : type === 52 ? mitigationCaps(rows) : globalDamageReduction(rows);
    groups.push({
      type, name: GROUPS.get(type),
      offset: top.offset, end_offset: top.endOffset,
      payload_offset: payload.dataOffset, payload_end_offset: payload.endOffset,
      config_offset: config.dataOffset, config_end_offset: config.endOffset,
      config_sha256: crypto.createHash("sha256").update(config.value).digest("hex"),
      field_spans: spans(rows),
      data,
    });
  }
  if (!groups.length) throw new Error("No allowlisted static combat groups found");
  return {
    schema_version: 1,
    provenance: {
      source_filename: path.basename(sourceName),
      source_sha256: crypto.createHash("sha256").update(bytes).digest("hex"),
      source_bytes: bytes.length,
      ...envelopeMetadata,
      schema: "Digit.PrimeServer.Models.proto: ServiceResponse.entityGroups -> EntityGroup -> static response",
      schema_source: {
        repository: "https://github.com/netniV/stfc-mod",
        revision: "e0b8552bc2e8922423c6cdf3ba00ba0a81bf23c0",
        protobuf_path: "mods/src/prime/proto/Digit.PrimeServer.Models.proto",
        entity_types_path: "mods/src/prime/EntityGroup.h",
      },
      allowed_group_types: [...GROUPS.keys()],
      offsets: "Zero-based byte offsets in the input file; end offsets are exclusive.",
    },
    limitations: [
      "A local cache is evidence of client configuration, not proof of current server combat behavior.",
      "Null server_version or server_time means the envelope did not encode it; cache freshness is not inferred.",
      "Scalar configuration fields are reported only when encoded. Map entries, names, and timestamp components include encoded flags to distinguish protobuf defaults from bytes present.",
      "Absent equation maps are not reconstructed.",
      "Equations are returned as strings and are never executed.",
      "Player groups and other group payloads are not decoded or exported.",
    ],
    groups,
  };
}

export function main(args) {
  if (args.length === 1 && (args[0] === "--help" || args[0] === "-h")) {
    console.log("Usage: node scripts/inspect_stfc_combat_config.mjs <cache.bin> [--output report.json]");
    return;
  }
  if (!args[0] || args[0].startsWith("-")
      || !(args.length === 1 || (args.length === 3 && args[1] === "--output" && args[2]))) {
    throw new Error("Expected <cache.bin> [--output report.json]; use --help");
  }
  const source = path.resolve(args[0]);
  const stat = fs.statSync(source);
  if (!stat.isFile() || !stat.size || stat.size > MAX_BYTES) {
    throw new Error(`Expected a nonempty regular file no larger than ${MAX_BYTES} bytes`);
  }
  const report = inspectCombatConfig(fs.readFileSync(source), source);
  const json = JSON.stringify(report, null, 2) + "\n";
  if (args[2]) {
    const output = path.resolve(args[2]);
    if (output === source) throw new Error("Output must not replace the input cache");
    fs.writeFileSync(output, json, { flag: "wx" });
    console.log(`Wrote ${report.groups.length} static combat groups to ${output}`);
  } else process.stdout.write(json);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`Combat config inspection failed: ${error.message}`);
    process.exitCode = 1;
  }
}
