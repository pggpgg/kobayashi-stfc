import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import {buildNativeSyndicateRewardsCatalog, decodeNativeSyndicateRewards,
  REVIEWED_CACHE_SHA256} from '../extract_native_syndicate_rewards.mjs';

const concat = (...parts) => Buffer.concat(parts);
const varint = value => {
  let n = BigInt(value); const out = [];
  do {const b = Number(n & 127n); n >>= 7n; out.push(b | (n ? 128 : 0));} while (n);
  return Buffer.from(out);
};
const integer = (n, value) => concat(varint(n * 8), varint(value));
const message = (n, value) => concat(varint(n * 8 + 2), varint(value.length), value);
const packed = (n, values, size) => {
  const bytes = Buffer.alloc(values.length * size);
  values.forEach((v, i) => size === 8 ? bytes.writeDoubleLE(v, i * size) : bytes.writeFloatLE(v, i * size));
  return message(n, bytes);
};
const group = (id, value) => message(1, concat(integer(1, id), message(2, value)));
const entry = (id, value) => message(1, concat(integer(1, id), message(2, value)));
const fixture = ({duplicateTier = false, duplicateCollection = false, duplicateSource = false,
  missing = false, tierString = 'Loyalty_1', nonfinite = false} = {}) => {
  const reward = message(1, concat(integer(1, 42), integer(2, 1)));
  const collection = message(6, concat(reward, integer(2, 1)));
  const tier = message(1, concat(integer(1, 100), message(2, Buffer.from(tierString)),
    integer(4, 2499), collection, duplicateCollection ? collection : Buffer.alloc(0), integer(7, 1)));
  const loyalty = concat(message(1, concat(integer(1, 123), integer(2, 10), integer(3, 27))),
    message(2, duplicateTier ? concat(tier, tier) : tier));
  const buff = concat(integer(1, 42), integer(7, 1), integer(8, 24), integer(12, 3),
    packed(13, [1, 1], 4), integer(18, 74), packed(21, [0.1, nonfinite ? Infinity : 0.2], 8));
  return concat(group(122, loyalty), group(68, duplicateSource ? entry(42, buff) : Buffer.alloc(0)),
    group(116, missing ? Buffer.alloc(0) : entry(42, buff)),
    group(99, Buffer.from('opaque-player-secret')), message(2, Buffer.from('other-player-secret')));
};

test('native reward decoder keeps source and rank provenance without reading opaque state', () => {
  const bytes = fixture(), result = decodeNativeSyndicateRewards(bytes);
  assert.deepEqual(result, decodeNativeSyndicateRewards(bytes));
  assert.equal(result.tiers.length, 1);
  assert.equal(result.tiers[0].collections[0].rewards[0].buff_id, 42);
  assert.equal(result.buffs[42].source_group, 116);
  assert.equal(result.buffs[42].rank_source, 'operations_level');
  assert.deepEqual(result.buffs[42].ranked_values, [0.1, 0.2]);
  assert.deepEqual(result.unresolved_buff_ids, []);
  assert.doesNotMatch(JSON.stringify(result), /player-secret/);
});

test('missing source references stay unresolved rather than receiving invented definitions', () => {
  const result = decodeNativeSyndicateRewards(fixture({missing: true}));
  assert.deepEqual(result.unresolved_buff_ids, [42]);
  assert.deepEqual(result.buffs, {});
});

test('ambiguous source IDs, tier identity, duplicate descriptors and nonfinite ranks fail', () => {
  for (const options of [{duplicateTier: true}, {duplicateCollection: true},
    {duplicateSource: true}, {tierString: 'Loyalty_2'}, {nonfinite: true}]) {
    assert.throws(() => decodeNativeSyndicateRewards(fixture(options)), undefined, JSON.stringify(options));
  }
  assert.throws(() => decodeNativeSyndicateRewards(Buffer.from([10, 128])), /Truncated/);
  assert.throws(() => buildNativeSyndicateRewardsCatalog(fixture()), /not the reviewed source digest/);
});

const catalog = JSON.parse(fs.readFileSync(new URL('../../data/native_syndicate_rewards.json', import.meta.url)));
test('tracked source preserves all65 tiers and229 references including one missing definition', () => {
  assert.equal(catalog.provenance.static_cache_sha256, REVIEWED_CACHE_SHA256);
  assert.deepEqual(catalog.coverage, {tier_count: 65, reward_reference_count: 229,
    unique_buff_count: 229, resolved_buff_count: 228});
  assert.deepEqual(catalog.tiers.map(t => t.level), Array.from({length: 65}, (_, i) => i + 1));
  const ids = catalog.tiers.flatMap(t => t.collections.flatMap(c => c.rewards.map(r => r.buff_id)));
  assert.equal(ids.length, 229);
  assert.equal(new Set(ids).size, 229);
  assert.deepEqual(catalog.unresolved_buff_ids, [3786995299]);
  assert.equal(Object.keys(catalog.buffs).length, 228);
  assert.doesNotMatch(JSON.stringify(catalog), /profiles[\\/]|access_token|C:\\/);
});

test('global Ops snapshots and linked building-level curves remain separate', () => {
  const definitions = Object.values(catalog.buffs);
  assert.equal(definitions.filter(r => r.source_group === 116 && r.rank_source === 'operations_level').length, 225);
  assert.equal(definitions.filter(r => r.source_group === 68 && r.rank_source === 'building_level').length, 3);
  for (const [id, condition] of [[995187207, 203], [1974652540, 204], [560958387, 205]]) {
    assert.deepEqual(catalog.buffs[id].condition_codes, [condition]);
    assert.equal(catalog.buffs[id].rank_source, 'building_level');
  }
  const value = (id, ops) => catalog.buffs[id].ranked_values[Math.min(ops - 1, catalog.buffs[id].ranked_values.length - 1)];
  assert.equal(value(3716446006, 40), 0.2);
  assert.equal(value(3716446006, 50), 0.2);
  assert.equal(value(3716446006, 51), 0.35);
  assert.equal(value(3716446006, 61), 0.6);
  assert.equal(value(2849989052, 40), 1);
});

test('all six piercing rewards preserve native all-axis scope and raw current-rank units', () => {
  const ids = [1333781883, 3809461333, 1085339663, 675636702, 609129565, 2259348690];
  let sum = 0;
  for (const id of ids) {
    const row = catalog.buffs[id];
    assert.equal(row.modifier_code, 74);
    assert.equal(row.operation_code, 3);
    assert.equal(row.target_code, 1);
    assert.equal(row.trigger_code, 24);
    assert.deepEqual(row.condition_codes, []);
    assert.deepEqual(row.attribute_fields, []);
    assert.deepEqual(row.unknown_fields, []);
    assert.deepEqual(row.ranked_chances, [1]);
    sum += row.ranked_values[Math.min(60, row.ranked_values.length - 1)];
  }
  assert.equal(sum, 6.4);
});
