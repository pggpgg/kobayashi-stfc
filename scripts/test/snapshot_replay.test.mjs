import test from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { SNAPSHOT_FILES, PROFILE_FILES, parseJsonLossless, sha256, verifySnapshot, selectJournal, validateJournalBinding, defaultAdapter, comparePrediction, publicPrediction, isolatedEnvironment, withReplayServer, runSnapshotReplay } from '../lib/snapshot_replay.mjs';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const comparison = { metric: 'incoming_base_stat_mitigation', weapon_index: 0, round_index: 1, observed_path: '/interval' };
function prediction(value = 0.4, truncated = false) {
  return { scenario: { ship: 'synthetic', hostile: '123', scenario_seed: 0 }, summary: { attacker_won: false, rounds_simulated: 1 }, trace: { truncated, event_count: 1, events: [{ event_type: 'mitigation_calc', phase: 'counter', weapon_index: 0, round_index: 1, values: { base_stat_mitigation: value, base_stat_mitigation_unrounded: value } }] } };
}
function fleet(player) {
  return { hull_ids: [player ? 987222969 : 456], initial_ship_levels: [player ? 20 : 53], initial_ship_hps: [100], initial_ship_shps: [80], max_ship_hps: [100], max_ship_shps: [80], faction_id: 7, bridge_officers: [{ id: 111, rank: 1, level: 5 }, null, null, null, { id: 222, rank: 2, level: 10 }, null, null, null, null, null], deployed_fleet: { ship_ids: [player ? '9007199254740993' : '0'], ship_tiers: { '9007199254740993': 4, '0': 1 }, ship_components: { '9007199254740993': [10, -1], '0': [20, 20, -1] } } };
}
async function fixture(t) {
  await fs.mkdir(path.join(repo, 'local-analysis'), { recursive: true });
  const root = await fs.mkdtemp(path.join(repo, 'local-analysis', 'snapshot-replay-test-'));
  t.after(async () => { assert(path.basename(root).startsWith('snapshot-replay-test-')); await fs.rm(root, { recursive: true, force: true }); });
  const snapshot = path.join(root, 'frozen');
  const data = path.join(root, 'data');
  await fs.mkdir(snapshot);
  const journal = { battle_time: '2026-10-24T02:54:29', initiator_fleet_data: fleet(true), target_fleet_data: fleet(false), account_id: 'private-account', battle_id: 'private-battle' };
  const sources = Object.fromEntries(SNAPSHOT_FILES.map(name => [name, { private_name: 'PRIVATE DO NOT EMIT' }]));
  sources['battlelogs.imported.json'] = { battlelogs: [{ journal }] };
  sources['roster.imported.json'] = { officers: [{ canonical_officer_id: 'test-captain', rank: 1, level: 5 }, { canonical_officer_id: 'test-below', rank: 2, level: 10 }] };
  const snapshot_files = {};
  for (const name of SNAPSHOT_FILES) { const bytes = JSON.stringify(sources[name]); await fs.writeFile(path.join(snapshot, name), bytes); snapshot_files[name] = sha256(bytes); }
  const canonical = { officers: [{ id: 'test-captain', source_officer_id: 111 }, { id: 'test-below', source_officer_id: 222 }] };
  const catalogs = { 'officers/officers.canonical.json': canonical, 'ships_extended/index.json': {}, 'hostiles/index.json': {}, 'upstream/data-stfc-space/ship_id_registry.json': { ships: [{ numeric_id: 987222969, id: 'synthetic' }] }, 'hostiles/123.json': { level: 53, faction: { id: 7 }, hull_health: 100, shield_health: 80, components: [{ id: 20 }, { id: 20 }] } };
  for (const [relative, object] of Object.entries(catalogs)) { const file = path.join(data, relative); await fs.mkdir(path.dirname(file), { recursive: true }); await fs.writeFile(file, JSON.stringify(object)); }
  const observedBytes = JSON.stringify({ interval: [0.1, 0.2], source_json_sha256: snapshot_files['battlelogs.imported.json'], battle_time: journal.battle_time });
  await fs.writeFile(path.join(root, 'observed.json'), observedBytes);
  const binding = defaultAdapter.binding(journal);
  const request = { ship: 'synthetic', hostile: '123', ship_tier: 4, ship_level: 20, seed: 0, sim_index: 0, max_trace_events: 2000, crew: { captain: 'test-captain', bridge: [null, null], below_deck: [null, 'test-below', null] } };
  const manifest = { schema_version: 1, snapshot_files, observation: { path: 'observed.json', sha256: sha256(observedBytes) }, journals: [{ battle_time: journal.battle_time, ...binding, request, comparisons: [comparison] }], catalog_files: { 'hostiles/123.json': sha256(JSON.stringify(catalogs['hostiles/123.json'])) } };
  const manifestPath = path.join(root, 'manifest.json');
  const saveManifest = () => fs.writeFile(manifestPath, JSON.stringify(manifest));
  await saveManifest();
  const mock = path.join(root, 'mock-server.mjs');
  await fs.writeFile(mock, `import http from 'node:http'; import fs from 'node:fs';
fs.writeFileSync('owned.pid', String(process.pid));
if (process.argv.includes('--never-listen')) { setInterval(() => {},1000); } else {
const server = http.createServer((req,res) => {
 if(req.url === '/api/health') { res.setHeader('content-type','application/json'); return res.end(JSON.stringify({status:'ok',build:{cargo_pkg_version:'mock'}})); }
 if(process.argv.includes('--reject-all-keys') || req.headers['x-api-key'] !== process.env.KOBAYASHI_API_KEY) {res.statusCode=401;return res.end('{}');}
 let body='';req.on('data',part => body+=part);req.on('end',()=>{
  if(req.headers['content-type'] !== 'application/json') {res.statusCode=415;return res.end('{}');}
  if(body === '{}') {res.statusCode=400;return res.end('{}');}
  if(process.argv.includes('--hang-request')) return;
  const files=fs.readdirSync('profiles/snapshot-replay').sort();
  if(JSON.stringify(files)!==${JSON.stringify(JSON.stringify([...PROFILE_FILES].sort()))}) {res.statusCode=500;return res.end('{}');}
  if(req.headers['x-profile-id']!=='snapshot-replay') {res.statusCode=400;return res.end('{}');}
  console.error('PRIVATE SERVER LOG');res.end(${JSON.stringify(JSON.stringify(prediction()))});
 });
});server.listen(Number(process.env.KOBAYASHI_BIND.split(':')[1]),'127.0.0.1'); }
`);
  const options = { manifest: manifestPath, snapshot, data, binary: process.execPath, output: path.join(root, 'report.json'), serverOptions: { args: [mock], startupTimeoutMs: 3000, requestTimeoutMs: 3000 } };
  return { root, snapshot, data, sources, canonical, journal, manifest, saveManifest, mock, options };
}

test('lossless JSON keeps unrelated huge IDs exact and rejects unsafe exponential encoding', () => {
  assert.deepEqual(parseJsonLossless('{"id":9007199254740993,"safe":42}'), { id: '9007199254740993', safe: 42 });
  assert.throws(() => parseJsonLossless('{"id":9.007199254740993e15}'), /Unsafe JSON number/);
});

test('all eight source bytes verified before any source parsing', async t => {
  const f = await fixture(t);
  await fs.writeFile(path.join(f.snapshot, 'profile.json'), '{broken');
  await assert.rejects(verifySnapshot(f.snapshot, f.manifest.snapshot_files), /hash mismatch/);
  await fs.rm(path.join(f.snapshot, 'profile.json'));
  await assert.rejects(verifySnapshot(f.snapshot, f.manifest.snapshot_files), { code: 'ENOENT' });
  await assert.rejects(verifySnapshot(f.snapshot, { ...f.manifest.snapshot_files, '../index.json': '0'.repeat(64) }), /keys/);
});

test('literal journal selector rejects missing and ambiguous captures', () => {
  assert.throws(() => selectJournal([{ battle_time: 'x' }, { battle_time: 'x' }], 'x'), /exactly one/);
  assert.throws(() => selectJournal([{ battle_time: 'x' }], 'y'), /exactly one/);
  assert.equal(selectJournal([{ battle_time: 'x' }], 'x').battle_time, 'x');
});

test('journal bindings reject crew or target drift and retain every null slot', async t => {
  const f = await fixture(t);
  const actual = defaultAdapter.binding(f.journal);
  assert.deepEqual(actual.player.below_decks, [null, { officer_id: 222, rank: 2, level: 10 }, null, null, null, null, null]);
  validateJournalBinding(actual, f.manifest.journals[0]);
  const changed = structuredClone(actual); changed.player.bridge[0] = { officer_id: 999, rank: 1, level: 1 };
  assert.throws(() => validateJournalBinding(changed, f.manifest.journals[0]), /Player/);
  const target = structuredClone(actual); target.target.components[0] = 999;
  assert.throws(() => validateJournalBinding(target, f.manifest.journals[0]), /Target/);
  const damaged = structuredClone(f.journal); damaged.initiator_fleet_data.initial_ship_hps[0] = 99;
  assert.throws(() => defaultAdapter.binding(damaged), /Full initial hull/);
});

test('request slots and officer rank/level bind catalog and independent frozen roster', async t => {
  const f = await fixture(t);
  const expected = f.manifest.journals[0];
  const context = { sources: f.sources, canonical: f.canonical, dataRoot: f.data };
  const request = await defaultAdapter.request(expected, {}, context);
  assert.deepEqual(request.crew.below_deck, [null, 'test-below', null]);
  expected.request.crew.below_deck = ['test-below'];
  await assert.rejects(defaultAdapter.request(expected, {}, context), /Empty request crew slot/);
  expected.request.crew.below_deck = [null, 'test-below'];
  f.sources['roster.imported.json'].officers[1].level = 11;
  await assert.rejects(defaultAdapter.request(expected, {}, context), /roster officer level/);
});

test('known mitigation mismatch remains mismatch; missing or truncated evidence is insufficient', () => {
  assert.equal(comparePrediction(prediction(), [comparison], { interval: [0.1, 0.2] })[0].status, 'mismatch');
  assert.equal(comparePrediction(prediction(0.15), [comparison], { interval: [0.1, 0.2] })[0].status, 'within_observed_interval');
  assert.equal(comparePrediction(prediction(0.15, true), [comparison], { interval: [0.1, 0.2] })[0].status, 'insufficient');
  assert.equal(comparePrediction({ trace: { events: [] } }, [comparison], { interval: [0.1, 0.2] })[0].status, 'insufficient');
  const mixed = prediction(0.15); mixed.trace.events.push({ ...mixed.trace.events[0], values: { base_stat_mitigation: 0.9 } });
  assert.equal(comparePrediction(mixed, [comparison], { interval: [0.1, 0.2] })[0].status, 'mismatch');
  assert.throws(() => comparePrediction(prediction(), [{ ...comparison, minimum: 0.3 }], { interval: [0.1, 0.2] }), /only from/);
});

test('prediction export rejects placeholders and whitelists server fields', () => {
  assert.throws(() => publicPrediction({ ...prediction(), using_placeholder_combatants: true }), /placeholder/);
  assert.throws(() => publicPrediction({ ...prediction(), warnings: ['Combat used deterministic placeholder stats.'] }), /placeholder/);
  const raw = prediction(); raw.scenario.account_id = 'PRIVATE'; raw.summary.battle_id = 'PRIVATE';
  assert(!JSON.stringify(publicPrediction(raw)).includes('PRIVATE'));
});

test('pooled fight comparisons cover every selected weapon once without claiming per-weapon observation', () => {
  const pooled = { metric: 'incoming_base_stat_mitigation', weapon_indexes: [0, 1], observed_path: '/interval', evidence_scope: 'pooled_fight' };
  const raw = prediction(0.15);
  raw.trace.events.push({ ...raw.trace.events[0], weapon_index: 1, round_index: 3, values: { base_stat_mitigation: 0.18, base_stat_mitigation_unrounded: 0.18 } });
  let result = comparePrediction(raw, [pooled], { interval: [0.1, 0.2] });
  assert.equal(result.length, 1);
  assert.equal(result[0].status, 'within_observed_interval');
  assert.equal(result[0].evidence_scope, 'pooled_fight');
  assert.deepEqual([result[0].predicted.minimum, result[0].predicted.maximum, result[0].predicted.count], [0.15, 0.18, 2]);
  assert(!Object.hasOwn(result[0], 'weapon_index'));
  raw.trace.events[1].values.base_stat_mitigation = 0.3;
  raw.trace.events[1].values.base_stat_mitigation_unrounded = 0.3;
  assert.equal(comparePrediction(raw, [pooled], { interval: [0.1, 0.2] })[0].status, 'mismatch');
  result = comparePrediction(prediction(0.15), [pooled], { interval: [0.1, 0.2] });
  assert.equal(result[0].status, 'insufficient');
  assert.deepEqual(result[0].missing_weapon_indexes, [1]);
  assert.throws(() => comparePrediction(raw, [{ ...pooled, weapon_indexes: [0, 0] }], { interval: [0.1, 0.2] }), /unique/);
  assert.throws(() => comparePrediction(raw, [{ ...pooled, weapon_index: 0 }], { interval: [0.1, 0.2] }), /exactly one/);
  assert.throws(() => comparePrediction(raw, [{ ...pooled, round_index: 1 }], { interval: [0.1, 0.2] }), /narrower round scope/);
});

test('legacy display rounding cannot falsely validate narrow observed intervals', () => {
  const legacy = prediction(0.123456);
  delete legacy.trace.events[0].values.base_stat_mitigation_unrounded;
  const observed = { interval: [0.1234558, 0.1234562] };
  let result = comparePrediction(legacy, [comparison], observed)[0];
  assert.equal(result.status, 'insufficient_precision');
  assert.equal(result.predicted.precision, 'rounded_6_decimal');
  assert(result.predicted.maximum_display_rounding_uncertainty > 0.5e-6);
  assert.deepEqual(result.observed_interval, observed.interval);
  assert.equal(comparePrediction(legacy, [comparison], { interval: [0.2, 0.3] })[0].status, 'mismatch');
  result = comparePrediction(prediction(0.1234561), [comparison], observed)[0];
  assert.equal(result.status, 'within_observed_interval');
  assert.equal(result.predicted.precision, 'unrounded_f64');
  assert.equal(result.predicted.maximum_display_rounding_uncertainty, 0);
  const preferred = prediction(0.123456);
  preferred.trace.events[0].values.base_stat_mitigation_unrounded = 0.1234563;
  assert.equal(comparePrediction(preferred, [comparison], observed)[0].status, 'mismatch');
  const spanning = prediction(0.1234561);
  spanning.trace.events.push({ ...spanning.trace.events[0], values: { base_stat_mitigation_unrounded: 0.2 } });
  assert.equal(comparePrediction(spanning, [comparison], observed)[0].status, 'mismatch');
});

test('isolated environment clears every inherited KOBAYASHI setting including wrong case', () => {
  const env = isolatedEnvironment('runtime', 12345, false, { PATH: 'bin', KOBAYASHI_API_KEY: 'secret', kobayashi_unknown: 'bad', KOBAYASHI_HOME: 'live' });
  assert.equal(env.PATH, 'bin'); assert.equal(env.KOBAYASHI_HOME, 'runtime'); assert.equal(env.KOBAYASHI_RAYON_THREADS, '1');
  assert.equal(env.KOBAYASHI_FT_LEVEL_TIER_SCALING, '0'); assert.equal(env.KOBAYASHI_API_KEY, undefined); assert.equal(env.kobayashi_unknown, undefined);
});

test('unsafe observation paths, output overwrite, and pinned catalog drift fail before spawning', async t => {
  const f = await fixture(t);
  await fs.writeFile(f.options.output, 'existing');
  await assert.rejects(runSnapshotReplay(f.options), /overwrite/);
  assert.equal(await fs.readFile(f.options.output, 'utf8'), 'existing');
  await fs.rm(f.options.output);
  f.manifest.observation.path = '../observed.json'; await f.saveManifest();
  await assert.rejects(runSnapshotReplay(f.options), /Unsafe relative path/);
  f.manifest.observation.path = 'observed.json'; await f.saveManifest();
  await fs.writeFile(path.join(f.data, 'hostiles/123.json'), '{}');
  await assert.rejects(runSnapshotReplay(f.options), /catalog hash mismatch/);
  await fs.rm(path.join(f.data, 'hostiles/123.json'));
  await assert.rejects(runSnapshotReplay(f.options), /catalog hash mismatch/);
  assert.equal((await fs.readdir(f.root)).filter(name => name.startsWith('.snapshot-replay-')).length, 0);
});

test('complete replay uses isolated seven-file profile, records honest mismatch, and cleans owned runtime', async t => {
  const f = await fixture(t);
  const before = await fs.readFile(path.join(f.snapshot, 'profile.json'));
  const report = await runSnapshotReplay(f.options);
  assert.equal(report.results[0].comparisons[0].status, 'mismatch');
  assert.equal(report.results[0].coverage, 'scoped_mitigation_not_full_fight');
  assert.equal(report.results[0].outcome_coverage, 'diagnostic_seed_not_game_seed');
  assert.equal(report.snapshot_files['profile.json'], sha256(before));
  assert.deepEqual(await fs.readFile(path.join(f.snapshot, 'profile.json')), before);
  const serialized = await fs.readFile(f.options.output, 'utf8');
  for (const forbidden of ['PRIVATE', 'private-account', 'private-battle', 'sync_token', 'account_id', 'battle_id']) assert(!serialized.includes(forbidden));
  assert.equal((await fs.readdir(f.root)).filter(name => name.startsWith('.snapshot-replay-')).length, 0);
});

test('owned child is terminated after operation failure and health timeout', async t => {
  const f = await fixture(t);
  let pid;
  await assert.rejects(withReplayServer({ binary: process.execPath, runtime: f.root, args: [f.mock], startupTimeoutMs: 3000 }, async info => { pid = info.pid; throw new Error('operation failed'); }), /operation failed/);
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
  await assert.rejects(withReplayServer({ binary: process.execPath, runtime: f.root, args: [f.mock, '--never-listen'], startupTimeoutMs: 150 }, async () => assert.fail('not ready')), /startup timeout/);
  const timedOutPid = Number(await fs.readFile(path.join(f.root, 'owned.pid'), 'utf8'));
  assert.throws(() => process.kill(timedOutPid, 0), { code: 'ESRCH' });
});

test('ownership failure prevents replay submission; request timeout terminates only owned child', async t => {
  const f = await fixture(t);
  let invoked = false;
  await assert.rejects(withReplayServer({ binary: process.execPath, runtime: f.root, args: [f.mock, '--reject-all-keys'], startupTimeoutMs: 3000 }, async () => { invoked = true; }), /ownership check failed/);
  assert.equal(invoked, false);
  let pid;
  await assert.rejects(withReplayServer({ binary: process.execPath, runtime: f.root, args: [f.mock, '--hang-request'], startupTimeoutMs: 3000, requestTimeoutMs: 100 }, async info => { pid = info.pid; await info.request('/api/optimize/replay-seed', { method: 'POST', body: '{"ship":"synthetic"}' }); }), { name: 'TimeoutError' });
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
});

test('observation hash and selected battle time are bound independently of the authored file hash', async t => {
  const f = await fixture(t);
  const observed = { interval: [0.1, 0.2], source_json_sha256: '0'.repeat(64), battle_time: f.journal.battle_time };
  async function saveObservation() {
    const bytes = JSON.stringify(observed); await fs.writeFile(path.join(f.root, 'observed.json'), bytes); f.manifest.observation.sha256 = sha256(bytes); await f.saveManifest();
  }
  await saveObservation();
  await assert.rejects(runSnapshotReplay(f.options), /Observation source battlelog hash/);
  observed.source_json_sha256 = f.manifest.snapshot_files['battlelogs.imported.json']; observed.battle_time = 'different'; await saveObservation();
  await assert.rejects(runSnapshotReplay(f.options), /Observation selected battle_time/);
});

test('CLI rejects unknown flags without opening files or leaking source paths', async () => {
  const child = spawn(process.execPath, [path.join(repo, 'scripts/replay_snapshot.mjs'), '--unknown', 'private-path'], { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let output = ''; child.stdout.on('data', value => { output += value; }); child.stderr.on('data', value => { output += value; });
  const code = await new Promise(resolve => child.once('exit', resolve));
  assert.equal(code, 1); assert.match(output, /Unknown or duplicate/); assert(!output.includes('private-path'));
});
