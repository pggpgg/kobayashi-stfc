import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {
  buildNativeOfficerCoreStatsCatalog,
  decodeNativeOfficerCoreStats,
  upstreamSourceByteBindings,
} from '../extract_native_officer_core_stats.mjs';

const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const varint = value => {
  let n = BigInt(value);
  const bytes = [];
  do {
    const byte = Number(n & 127n);
    n >>= 7n;
    bytes.push(byte | (n ? 128 : 0));
  } while (n);
  return Buffer.from(bytes);
};
const integer = (number, value) => Buffer.concat([varint(number * 8), varint(value)]);
const message = (number, bytes) => Buffer.concat([varint(number * 8 + 2), varint(bytes.length), bytes]);
const float32 = (number, value) => {
  const bytes = Buffer.alloc(4);
  bytes.writeFloatLE(value);
  return Buffer.concat([varint(number * 8 + 5), bytes]);
};
const concat = (...bytes) => Buffer.concat(bytes);
const mapEntry = (key, value) => message(1, concat(integer(1, key), message(2, value)));
const group = (id, payload) => message(1, concat(integer(1, id), message(2, payload)));
const fixture = ({officerId = 3849424588, officerKey = officerId, weight = 10,
  growth = 42.06, duplicateOfficer = false, duplicateGrowth = false} = {}) => {
  const officer = concat(integer(1, officerId), integer(11, 3), integer(15, 6), integer(16, weight), integer(17, 12));
  const growthRow = concat(integer(1, 20), message(2, concat(integer(1, 3), float32(2, growth))));
  const officerEntry = mapEntry(officerKey, officer), growthEntry = mapEntry(20, growthRow);
  return concat(group(99, Buffer.alloc(0)),
    group(7, duplicateOfficer ? concat(officerEntry, officerEntry) : officerEntry),
    group(18, duplicateGrowth ? concat(growthEntry, growthEntry) : growthEntry));
};

test('native schema retains integer weights and the exact float32 growth value', () => {
  const decoded = decodeNativeOfficerCoreStats(fixture());
  assert.equal(decoded.officer_specs_group.id, 7);
  assert.equal(decoded.core_stat_specs_group.id, 18);
  assert.deepEqual(decoded.officers['3849424588'].weights, {attack: 6, defense: 10, health: 12});
  assert.equal(decoded.officers['3849424588'].rarity, 3);
  assert.equal(decoded.growth_by_level['20'].by_rarity['3'], Math.fround(42.06));
  assert.equal(Math.fround(10 * decoded.growth_by_level['20'].by_rarity['3']), 420.6000061035156);
});

test('schema rejects conflicting identities, duplicate rows and invalid growth/weight data', () => {
  for (const options of [
    {officerKey: 1}, {duplicateOfficer: true}, {duplicateGrowth: true},
    {growth: 0}, {growth: Number.NaN}, {growth: Number.POSITIVE_INFINITY}, {weight: 0x80000000},
  ]) {
    assert.throws(() => decodeNativeOfficerCoreStats(fixture(options)), undefined, JSON.stringify(options));
  }
  assert.throws(() => decodeNativeOfficerCoreStats(fixture().subarray(0, -1)), /Truncated/);
});

test('generation rejects a structurally valid cache without the reviewed source digest', () => {
  assert.throws(() => buildNativeOfficerCoreStatsCatalog(fixture()), /not the reviewed source digest/);
});

test('LF and CRLF source files produce the same explicit exact-byte bindings', () => {
  const lf = Buffer.from('{\n  "id": 3849424588,\n  "stats": []\n}\n');
  const crlf = Buffer.from(lf.toString('utf8').replace(/\n/g, '\r\n'));
  const expected = {lf_sha256: sha256(lf), crlf_sha256: sha256(crlf)};
  assert.deepEqual(upstreamSourceByteBindings(lf, '3849424588'), expected);
  assert.deepEqual(upstreamSourceByteBindings(crlf, '3849424588'), expected);
  // A valid JSON whitespace change is a new content binding, never accepted by
  // the old runtime hashes. Runtime does not call this generator helper.
  const changed = Buffer.concat([lf, Buffer.from(' ')]);
  assert.notDeepEqual(upstreamSourceByteBindings(changed, '3849424588'), expected);
  assert.notEqual(sha256(changed), expected.lf_sha256);
  assert.notEqual(sha256(changed), expected.crlf_sha256);
});

test('source bindings reject mismatched IDs, mixed EOL, lone CR and invalid UTF-8', () => {
  assert.throws(() => upstreamSourceByteBindings(Buffer.from('{"id": 1}'), 2), /identity mismatch/);
  assert.throws(() => upstreamSourceByteBindings(Buffer.from('{"id": "1"}'), 1), /identity mismatch/);
  assert.throws(() => upstreamSourceByteBindings(Buffer.from('{"id": 1.5}'), 1), /identity mismatch/);
  assert.throws(() => upstreamSourceByteBindings(Buffer.from('{\r\n"id":1\n}\r\n'), 1), /Mixed\/unreviewed/);
  assert.throws(() => upstreamSourceByteBindings(Buffer.from('{"id":1}\r'), 1), /Unsupported upstream line endings/);
  assert.throws(() => upstreamSourceByteBindings(Buffer.from([0xff]), 1), /encoded data/);
  assert.throws(() => upstreamSourceByteBindings('{"id":1}', 1), /exact file bytes/);
});
