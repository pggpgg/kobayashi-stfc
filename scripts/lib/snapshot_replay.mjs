import { createHash, randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import * as fs from 'node:fs/promises';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';

export const PROFILE_FILES = Object.freeze(['profile.json', 'roster.imported.json', 'research.imported.json', 'buffs.imported.json', 'buildings.imported.json', 'ships.imported.json', 'forbidden_tech.imported.json']);
export const SNAPSHOT_FILES = Object.freeze([...PROFILE_FILES, 'battlelogs.imported.json']);
export const SYNTHETIC_PROFILE_ID = 'snapshot-replay';
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

// JSON.parse context.source preserves unrelated u64 account/battle identifiers without rounding.
export function parseJsonLossless(bytes) {
  let supported = false;
  JSON.parse('9007199254740993', (_key, _value, context) => { supported = context?.source === '9007199254740993'; });
  if (!supported) throw new Error('Node with JSON.parse reviver context.source is required');
  return JSON.parse(bytes.toString(), (_key, value, context) => {
    if (typeof value !== 'number') return value;
    if (!Number.isFinite(value)) throw new Error('Non-finite JSON number');
    if (Number.isInteger(value) && !Number.isSafeInteger(value)) {
      if (!/^-?\d+$/.test(context.source)) throw new Error('Unsafe JSON number encoding');
      return context.source;
    }
    return value;
  });
}

function assert(condition, message) { if (!condition) throw new Error(message); }
function inside(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
}
function equal(actual, expected, label) { assert(isDeepStrictEqual(actual, expected), `${label} does not match manifest`); }
function digest(value) { assert(typeof value === 'string' && /^[a-f0-9]{64}$/.test(value), 'Expected lowercase SHA-256'); }
async function readRegular(file) {
  const stat = await fs.lstat(file);
  assert(stat.isFile() && !stat.isSymbolicLink(), 'Input must be a regular file');
  return fs.readFile(file);
}
function publicPath(root, relative) {
  assert(typeof relative === 'string' && relative.length > 0 && !relative.includes('\\') && !relative.includes(':') && !relative.split('/').some(s => !s || s === '.' || s === '..'), 'Unsafe relative path');
  const file = path.resolve(root, relative);
  assert(inside(root, file), 'Path escapes its root');
  return file;
}

export async function verifySnapshot(snapshotDirectory, hashes, { parse = true } = {}) {
  equal(Object.keys(hashes ?? {}).sort(), [...SNAPSHOT_FILES].sort(), 'snapshot_files keys');
  const root = await fs.realpath(snapshotDirectory);
  const buffers = {};
  // Verify every byte hash before parsing any source or preparing a runtime.
  for (const filename of SNAPSHOT_FILES) {
    digest(hashes[filename]);
    const bytes = await readRegular(path.join(root, filename));
    assert(sha256(bytes) === hashes[filename], `Snapshot hash mismatch: ${filename}`);
    buffers[filename] = bytes;
  }
  const sources = parse ? Object.fromEntries(SNAPSHOT_FILES.map(name => [name, parseJsonLossless(buffers[name])])) : undefined;
  return { root, buffers, sources };
}

export function selectJournal(journals, battleTime) {
  assert(typeof battleTime === 'string' && battleTime.length > 0, 'battle_time must be a literal string');
  assert(Array.isArray(journals), 'Journal adapter must return an array');
  const matches = journals.filter(journal => journal.battle_time === battleTime);
  assert(matches.length === 1, 'battle_time must select exactly one journal');
  return matches[0];
}

export function validateJournalBinding(actual, expected) {
  equal(actual.player, expected.player, 'Player ship and crew');
  equal(actual.target, expected.target, 'Target ship and components');
}

function singleton(values, label) {
  assert(Array.isArray(values) && values.length === 1, `${label} must select exactly one ship`);
  return values[0];
}
function fleetBinding(fleet, player) {
  assert(fleet && typeof fleet === 'object', 'Missing selected fleet');
  const deployed = fleet.deployed_fleet;
  const shipId = singleton(deployed?.ship_ids, 'Deployed fleet');
  const hull = singleton(fleet.hull_ids, 'Fleet hull');
  const maxHull = singleton(fleet.max_ship_hps, 'Maximum hull health');
  const maxShield = singleton(fleet.max_ship_shps, 'Maximum shield health');
  equal(singleton(fleet.initial_ship_hps, 'Initial hull health'), maxHull, 'Full initial hull health');
  equal(singleton(fleet.initial_ship_shps, 'Initial shield health'), maxShield, 'Full initial shield health');
  assert(Array.isArray(deployed.ship_components?.[shipId]), 'Missing selected ship components');
  const result = { hull_id: hull, level: singleton(fleet.initial_ship_levels, 'Fleet level'), max_hull_health: maxHull, max_shield_health: maxShield, components: deployed.ship_components[shipId] };
  if (player) {
    assert(Number.isSafeInteger(deployed.ship_tiers?.[shipId]), 'Missing selected ship tier');
    result.tier = deployed.ship_tiers[shipId];
    assert(Array.isArray(fleet.bridge_officers) && fleet.bridge_officers.length === 10, 'Expected all ten recorded crew slots');
    const slots = fleet.bridge_officers.map(officer => officer === null ? null : { officer_id: officer.id, rank: officer.rank, level: officer.level });
    result.captain = slots[0]; result.bridge = slots.slice(1, 3); result.below_decks = slots.slice(3);
  } else result.faction = fleet.faction_id;
  return result;
}

export const defaultAdapter = {
  journals(sources) {
    assert(Array.isArray(sources['battlelogs.imported.json']?.battlelogs), 'Missing frozen battlelog journals');
    return sources['battlelogs.imported.json'].battlelogs.map(entry => entry.journal);
  },
  binding(journal) {
    return { player: fleetBinding(journal.initiator_fleet_data, true), target: fleetBinding(journal.target_fleet_data, false) };
  },
  async request(expected, _observations, { sources, canonical, dataRoot }) {
    const req = expected.request;
    assert(req && typeof req === 'object', 'Manifest requires explicit replay request');
    const allowed = ['ship', 'hostile', 'ship_tier', 'ship_level', 'seed', 'sim_index', 'max_trace_events', 'crew', 'support_buffs', 'defender_opponent'];
    assert(Object.keys(req).every(key => allowed.includes(key)), 'Unsupported replay request field');
    assert(typeof req.ship === 'string' && /^[a-z0-9_-]+$/.test(req.ship), 'Ship request must use a catalog ID');
    assert(typeof req.hostile === 'string' && /^[a-z0-9_-]+$/.test(req.hostile), 'Hostile request must use a catalog ID');
    equal(req.ship_tier, expected.player.tier, 'Request ship tier');
    equal(req.ship_level, expected.player.level, 'Request ship level');
    for (const key of ['seed', 'sim_index', 'max_trace_events']) assert(Number.isSafeInteger(req[key]) && req[key] >= 0, `Request ${key} requires a safe nonnegative integer`);
    assert(req.max_trace_events >= 1 && req.max_trace_events <= 2000, 'Request trace cap must be 1..2000');
    assert(req.defender_opponent === undefined || req.defender_opponent === 'hostile', 'Recorded hostile replay requires hostile defender');
    assert(req.support_buffs === undefined || (Array.isArray(req.support_buffs) && req.support_buffs.length === 0), 'Unbound support buffs are unsupported');
    assert(req.crew && Object.keys(req.crew).every(key => ['captain', 'bridge', 'below_deck'].includes(key)), 'Unsupported request crew field');
    assert(Array.isArray(req.crew.bridge) && req.crew.bridge.length === 2, 'Request must preserve two bridge slots');
    assert(Array.isArray(req.crew.below_deck) && req.crew.below_deck.length <= 7, 'Request has invalid below-deck slots');
    const actualSlots = [req.crew.captain, ...req.crew.bridge, ...Array.from({ length: 7 }, (_, index) => req.crew.below_deck[index] ?? null)];
    const expectedSlots = [expected.player.captain, ...expected.player.bridge, ...expected.player.below_decks];
    assert(expectedSlots.length === 10, 'Manifest must preserve all ten crew slots');
    for (let index = 0; index < expectedSlots.length; index++) {
      const officer = expectedSlots[index];
      if (officer === null) { equal(actualSlots[index], null, 'Empty request crew slot'); continue; }
      assert(Number.isSafeInteger(officer.officer_id) && Number.isSafeInteger(officer.rank) && Number.isSafeInteger(officer.level), 'Officer ID/rank/level must be safe integers');
      const rows = canonical.officers.filter(row => String(row.source_officer_id) === String(officer.officer_id));
      assert(rows.length === 1, 'Selected officer must resolve uniquely in public catalog');
      equal(actualSlots[index], rows[0].id, 'Canonical request crew slot');
      const roster = sources['roster.imported.json']?.officers?.filter(row => row.canonical_officer_id === rows[0].id);
      assert(roster?.length === 1, 'Selected officer must resolve uniquely in frozen roster');
      equal(roster[0].rank, officer.rank, 'Frozen roster officer rank');
      equal(roster[0].level, officer.level, 'Frozen roster officer level');
    }
    const registry = parseJsonLossless(await readRegular(path.join(dataRoot, 'upstream/data-stfc-space/ship_id_registry.json')));
    const shipRows = registry.ships.filter(row => String(row.numeric_id) === String(expected.player.hull_id));
    assert(shipRows.length === 1, 'Player hull must resolve uniquely in public ship registry');
    equal(req.ship, shipRows[0].id, 'Request player hull');
    const hostile = parseJsonLossless(await readRegular(path.join(dataRoot, 'hostiles', `${req.hostile}.json`)));
    equal(hostile.level, expected.target.level, 'Request hostile level');
    equal(hostile.faction?.id, expected.target.faction, 'Request hostile faction');
    equal(hostile.hull_health, expected.target.max_hull_health, 'Request hostile maximum hull health');
    equal(hostile.shield_health, expected.target.max_shield_health, 'Request hostile maximum shield health');
    const recordedComponents = expected.target.components.filter(id => id > 0);
    equal((hostile.components ?? []).map(component => component.id), recordedComponents, 'Request hostile component order and duplicates');
    return structuredClone(req);
  },
};

export function extractIncomingMitigation(prediction) {
  const groups = new Map();
  for (const event of prediction.trace?.events ?? []) {
    if (event.event_type !== 'mitigation_calc' || event.phase !== 'counter') continue;
    const unrounded = event.values?.base_stat_mitigation_unrounded;
    const raw = Number.isFinite(unrounded);
    const value = raw ? unrounded : event.values?.base_stat_mitigation;
    if (!Number.isFinite(value) || !Number.isSafeInteger(event.weapon_index)) continue;
    // Legacy traces expose six decimal places. Preserve their display uncertainty without
    // widening the independently observed interval. The epsilon covers bound arithmetic.
    const uncertainty = raw ? 0 : 0.5e-6 + 2 * Number.EPSILON * Math.max(1, Math.abs(value));
    const values = groups.get(event.weapon_index) ?? [];
    values.push({ value, minimum: value - uncertainty, maximum: value + uncertainty, raw, uncertainty });
    groups.set(event.weapon_index, values);
  }
  return [...groups].sort(([a], [b]) => a - b).map(([weapon_index, values]) => {
    const fullPrecisionCount = values.filter(value => value.raw).length;
    return { weapon_index, minimum: Math.min(...values.map(value => value.minimum)), maximum: Math.max(...values.map(value => value.maximum)), minimum_upper_bound: Math.min(...values.map(value => value.maximum)), maximum_lower_bound: Math.max(...values.map(value => value.minimum)), reported_minimum: Math.min(...values.map(value => value.value)), reported_maximum: Math.max(...values.map(value => value.value)), count: values.length, precision: fullPrecisionCount === values.length ? 'unrounded_f64' : fullPrecisionCount === 0 ? 'rounded_6_decimal' : 'mixed', full_precision_count: fullPrecisionCount, rounded_count: values.length - fullPrecisionCount, maximum_display_rounding_uncertainty: Math.max(...values.map(value => value.uncertainty)) };
  });
}

export function jsonPointer(value, pointer) {
  assert(typeof pointer === 'string' && pointer.startsWith('/'), 'Observation requires JSON pointer');
  for (const segment of pointer.slice(1).split('/')) {
    assert(!/~(?![01])/.test(segment), 'Invalid JSON pointer escape');
    const key = segment.replaceAll('~1', '/').replaceAll('~0', '~');
    assert(value !== null && typeof value === 'object' && Object.hasOwn(value, key), 'Observed JSON pointer does not resolve');
    value = value[key];
  }
  return value;
}

export function comparePrediction(prediction, comparisons, observations) {
  assert(Array.isArray(comparisons) && comparisons.length > 0, 'At least one comparison is required');
  return comparisons.map(comparison => {
    assert(comparison.metric === 'incoming_base_stat_mitigation', 'Unsupported comparison metric');
    const single = Object.hasOwn(comparison, 'weapon_index');
    const pooled = Object.hasOwn(comparison, 'weapon_indexes');
    assert(single !== pooled, 'Comparison requires exactly one of weapon_index or weapon_indexes');
    const indexes = single ? [comparison.weapon_index] : comparison.weapon_indexes;
    assert(Array.isArray(indexes) && indexes.length > 0 && indexes.every(index => Number.isSafeInteger(index) && index >= 0), 'Invalid comparison weapon indexes');
    assert(new Set(indexes).size === indexes.length, 'Comparison weapon indexes must be unique');
    const evidenceScope = comparison.evidence_scope ?? (single ? 'weapon_component' : 'pooled_fight');
    assert(evidenceScope === (single ? 'weapon_component' : 'pooled_fight'), 'Evidence scope must match the weapon selection');
    assert(!Object.hasOwn(comparison, 'minimum') && !Object.hasOwn(comparison, 'maximum'), 'Observed intervals must come only from observation JSON');
    const interval = jsonPointer(observations, comparison.observed_path);
    assert(Array.isArray(interval) && interval.length === 2 && interval.every(Number.isFinite) && interval[0] <= interval[1], 'Invalid observed interval');
    let scope = comparison.round_scope;
    if (comparison.round_index !== undefined) {
      assert(scope === undefined && Number.isSafeInteger(comparison.round_index) && comparison.round_index >= 0, 'Invalid comparison round_index');
      scope = [comparison.round_index, comparison.round_index];
    }
    if (scope !== undefined) assert(Array.isArray(scope) && scope.length === 2 && scope.every(Number.isSafeInteger) && scope[0] >= 0 && scope[0] <= scope[1], 'Invalid round_scope');
    assert(evidenceScope !== 'pooled_fight' || scope === undefined, 'Pooled fight evidence cannot claim a narrower round scope');
    const scoped = scope ? { trace: { events: (prediction.trace?.events ?? []).filter(event => event.round_index >= scope[0] && event.round_index <= scope[1]) } } : prediction;
    const incoming = extractIncomingMitigation(scoped);
    const matching = incoming.filter(group => indexes.includes(group.weapon_index));
    const missing = indexes.filter(index => !matching.some(group => group.weapon_index === index));
    const actual = single ? matching[0] : matching.length === 0 ? undefined : { weapon_indexes: indexes, minimum: Math.min(...matching.map(group => group.minimum)), maximum: Math.max(...matching.map(group => group.maximum)), minimum_upper_bound: Math.min(...matching.map(group => group.minimum_upper_bound)), maximum_lower_bound: Math.max(...matching.map(group => group.maximum_lower_bound)), count: matching.reduce((sum, group) => sum + group.count, 0), precision: matching.every(group => group.precision === 'unrounded_f64') ? 'unrounded_f64' : matching.every(group => group.precision === 'rounded_6_decimal') ? 'rounded_6_decimal' : 'mixed', full_precision_count: matching.reduce((sum, group) => sum + group.full_precision_count, 0), rounded_count: matching.reduce((sum, group) => sum + group.rounded_count, 0), maximum_display_rounding_uncertainty: Math.max(...matching.map(group => group.maximum_display_rounding_uncertainty)), weapon_ranges: matching };
    let status = 'insufficient';
    if (prediction.trace?.truncated === false && missing.length === 0 && actual) {
      if (actual.minimum_upper_bound < interval[0] || actual.maximum_lower_bound > interval[1]) status = 'mismatch';
      else if (actual.minimum >= interval[0] && actual.maximum <= interval[1]) status = 'within_observed_interval';
      else status = 'insufficient_precision';
    }
    return { metric: comparison.metric, ...(single ? { weapon_index: comparison.weapon_index } : { weapon_indexes: indexes }), evidence_scope: evidenceScope, observed_path: comparison.observed_path, observed_interval: interval, round_scope: scope ?? null, predicted: actual ?? null, missing_weapon_indexes: missing, status };
  });
}

async function catalogTree(root) {
  const hashes = {};
  async function walk(dir, prefix = '') {
    for (const entry of (await fs.readdir(dir, { withFileTypes: true })).sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0)) {
      assert(!entry.isSymbolicLink(), 'Data catalog must not contain symbolic links');
      const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (entry.isDirectory()) await walk(path.join(dir, entry.name), relative);
      else {
        assert(entry.isFile(), 'Data catalog contains a nonregular entry');
        hashes[relative] = sha256(await readRegular(path.join(root, relative)));
      }
    }
  }
  await walk(root);
  for (const required of ['officers/officers.canonical.json', 'ships_extended/index.json', 'hostiles/index.json']) assert(hashes[required], `Missing required public catalog: ${required}`);
  return hashes;
}

export function isolatedEnvironment(runtime, port, scaling = false, inherited = process.env) {
  assert(typeof scaling === 'boolean', 'Forbidden-tech scaling setting must be boolean');
  const environment = Object.fromEntries(Object.entries(inherited).filter(([name]) => !name.toUpperCase().startsWith('KOBAYASHI_')));
  return { ...environment, KOBAYASHI_HOME: runtime, KOBAYASHI_BIND: `127.0.0.1:${port}`, KOBAYASHI_RAYON_THREADS: '1', KOBAYASHI_MAX_CONCURRENT_CPU_JOBS: '1', KOBAYASHI_FT_LEVEL_TIER_SCALING: scaling ? '1' : '0' };
}

async function unusedPort() {
  const server = createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const port = server.address().port;
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  assert(port !== 3000, 'Refusing reserved live server port');
  return port;
}
const delay = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
const fields = (object, names) => Object.fromEntries(names.filter(name => Object.hasOwn(object ?? {}, name)).map(name => [name, object[name]]));

export function publicPrediction(prediction) {
  assert(prediction.status === undefined || prediction.status === 'ok', 'Replay returned unsuccessful status');
  assert(!prediction.using_placeholder_combatants && !prediction.scenario?.using_placeholder_combatants && !prediction.summary?.using_placeholder_combatants && !(prediction.warnings ?? []).some(warning => /placeholder/i.test(String(warning))), 'Replay used placeholder combatants');
  return {
    scenario: fields(prediction.scenario, ['ship', 'hostile', 'ship_tier', 'ship_level', 'scenario_seed', 'sim_index', 'base_seed', 'iteration_seed', 'effective_defender_hull']),
    summary: fields(prediction.summary, ['attacker_won', 'winner_by_round_limit', 'rounds_simulated', 'total_damage', 'total_isolytic_damage', 'attacker_hull_remaining', 'defender_hull_remaining', 'defender_shield_remaining']),
    incoming_base_stat_mitigation: extractIncomingMitigation(prediction),
    trace_event_count: prediction.trace?.event_count,
    trace_truncated: prediction.trace?.truncated,
    server_warning_count: prediction.warnings?.length ?? 0,
  };
}

export async function withReplayServer({ binary, runtime, scaling = false, startupTimeoutMs = 120000, requestTimeoutMs = 60000, args = ['serve'] }, operation) {
  assert(Number.isSafeInteger(startupTimeoutMs) && startupTimeoutMs > 0 && Number.isSafeInteger(requestTimeoutMs) && requestTimeoutMs > 0, 'Server timeouts must be positive safe integers');
  const port = await unusedPort();
  const key = randomUUID();
  const env = { ...isolatedEnvironment(runtime, port, scaling), KOBAYASHI_API_KEY: key, KOBAYASHI_API_KEY_TRUST_LOOPBACK: '0' };
  const child = spawn(binary, args, { cwd: runtime, env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  // Never include process output: imported source data and filesystem paths can be private.
  child.stdout.resume(); child.stderr.resume();
  let spawnError;
  child.on('error', error => { spawnError = error; });
  const exited = new Promise(resolve => child.once('exit', resolve));
  const cancelled = new AbortController();
  const interrupt = () => { cancelled.abort(); child.kill(); };
  process.on('SIGINT', interrupt); process.on('SIGTERM', interrupt);
  function alive() { assert(!spawnError && child.exitCode === null && child.signalCode === null, 'Replay child failed or exited'); }
  const base = `http://127.0.0.1:${port}`;
  async function request(route, options = {}) {
    alive();
    const response = await fetch(`${base}${route}`, { ...options, redirect: 'error', headers: { 'content-type': 'application/json', 'x-api-key': key, 'x-profile-id': SYNTHETIC_PROFILE_ID, ...options.headers }, signal: AbortSignal.any([cancelled.signal, AbortSignal.timeout(requestTimeoutMs)]) });
    alive();
    return response;
  }
  try {
    const deadline = Date.now() + startupTimeoutMs;
    let health;
    while (Date.now() < deadline) {
      alive();
      try {
        const response = await fetch(`${base}/api/health`, { signal: AbortSignal.timeout(Math.min(500, requestTimeoutMs)), redirect: 'error' });
        if (response.ok) { health = parseJsonLossless(await response.text()); break; }
      } catch { /* A not-yet-listening owned child is expected during startup. */ }
      await delay(50);
    }
    assert(health?.status === 'ok', 'Replay server health startup timeout');
    // Prove the listener knows our fresh per-process secret before sending any source-derived input.
    const bad = await request('/api/optimize/replay-seed', { method: 'POST', headers: { 'x-api-key': `wrong-${key}` }, body: '{}' });
    assert(bad.status === 401, 'Listener ownership check failed');
    const good = await request('/api/optimize/replay-seed', { method: 'POST', body: '{}' });
    assert(good.status === 400, 'Listener ownership check failed');
    alive();
    return await operation({ request, health, pid: child.pid });
  } finally {
    if (child.exitCode === null && child.signalCode === null) child.kill();
    await Promise.race([exited, delay(3000)]);
    if (child.exitCode === null && child.signalCode === null) {
      child.kill('SIGKILL');
      await Promise.race([exited, delay(3000)]);
    }
    process.off('SIGINT', interrupt); process.off('SIGTERM', interrupt);
    if (!(child.exitCode !== null || child.signalCode !== null || spawnError)) {
      const error = new Error('Replay child termination could not be confirmed; isolated runtime retained');
      error.code = 'CHILD_TERMINATION_UNCONFIRMED';
      throw error;
    }
  }
}

// Adapter hooks deliberately return only public bindings and API DTOs, never raw source records.
export async function runSnapshotReplay(options, adapter = defaultAdapter) {
  const { manifest: manifestPath, snapshot, data, binary, output } = options;
  assert(adapter && typeof adapter.journals === 'function' && typeof adapter.binding === 'function' && typeof adapter.request === 'function', 'A supported snapshot adapter is required');
  const parent = await fs.realpath(path.dirname(path.resolve(output)));
  const outputPath = path.join(parent, path.basename(output));
  assert(path.extname(outputPath) === '.json', 'Output must be a new .json file');
  try { await fs.lstat(outputPath); throw new Error('Refusing to overwrite output'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  const manifestBytes = await readRegular(manifestPath);
  const manifest = parseJsonLossless(manifestBytes);
  assert(manifest.schema_version === 1, 'Unsupported manifest schema_version');
  assert(Array.isArray(manifest.journals) && manifest.journals.length > 0, 'Manifest requires journals');
  const verified = await verifySnapshot(snapshot, manifest.snapshot_files, { parse: false });
  assert(!inside(verified.root, outputPath), 'Output must be outside frozen snapshot');
  const observationPath = publicPath(await fs.realpath(path.dirname(path.resolve(manifestPath))), manifest.observation.path);
  assert(inside(await fs.realpath(path.dirname(path.resolve(manifestPath))), await fs.realpath(observationPath)), 'Observation escapes manifest directory');
  const observationBytes = await readRegular(observationPath);
  digest(manifest.observation.sha256);
  assert(sha256(observationBytes) === manifest.observation.sha256, 'Observation hash mismatch');
  verified.sources = Object.fromEntries(SNAPSHOT_FILES.map(name => [name, parseJsonLossless(verified.buffers[name])]));
  const observations = parseJsonLossless(observationBytes);
  equal(observations.source_json_sha256, manifest.snapshot_files['battlelogs.imported.json'], 'Observation source battlelog hash');
  const binaryPath = await fs.realpath(binary);
  const binaryHash = sha256(await readRegular(binaryPath));
  const dataRoot = await fs.realpath(data);
  assert(!inside(verified.root, dataRoot), 'Public data must be outside frozen snapshot');
  assert(!inside(dataRoot, outputPath), 'Output must be outside public data');
  const catalogs = await catalogTree(dataRoot);
  for (const [relative, expectedHash] of Object.entries(manifest.catalog_files ?? {})) {
    publicPath(dataRoot, relative); digest(expectedHash);
    assert(catalogs[relative] === expectedHash, 'Pinned public catalog hash mismatch or missing file');
  }
  const canonical = parseJsonLossless(await readRegular(path.join(dataRoot, 'officers', 'officers.canonical.json')));
  const selected = [];
  for (const expected of manifest.journals) {
    equal(observations.battle_time, expected.battle_time, 'Observation selected battle_time');
    const journal = selectJournal(adapter.journals(verified.sources), expected.battle_time);
    validateJournalBinding(adapter.binding(journal, verified.sources), expected);
    const request = await adapter.request(expected, observations, { sources: verified.sources, canonical, dataRoot });
    comparePrediction({ trace: { events: [] } }, expected.comparisons, observations);
    selected.push({ expected, request });
  }
  const runtime = await fs.mkdtemp(path.join(parent, '.snapshot-replay-'));
  assert(runtime !== verified.root && !inside(verified.root, runtime), 'Runtime must be outside frozen snapshot');
  let preserveRuntime = false;
  try {
    // Copy the explicit catalog tree, then verify the actual runtime bytes, avoiding live profile discovery.
    await fs.cp(dataRoot, path.join(runtime, 'data'), { recursive: true, errorOnExist: true, force: false });
    equal(await catalogTree(path.join(runtime, 'data')), catalogs, 'Copied public catalog hashes');
    const profile = path.join(runtime, 'profiles', SYNTHETIC_PROFILE_ID);
    await fs.mkdir(profile, { recursive: true });
    for (const name of PROFILE_FILES) await fs.writeFile(path.join(profile, name), verified.buffers[name], { flag: 'wx' });
    await fs.writeFile(path.join(runtime, 'profiles', 'index.json'), JSON.stringify({ profiles: [{ id: SYNTHETIC_PROFILE_ID, name: 'Frozen snapshot replay', sync_token: randomUUID(), is_default: true }], default_id: SYNTHETIC_PROFILE_ID }), { flag: 'wx' });
    const report = await withReplayServer({ binary: binaryPath, runtime, scaling: manifest.settings?.forbidden_tech_level_tier_scaling ?? false, ...options.serverOptions }, async ({ request, health }) => {
      const results = [];
      for (const entry of selected) {
        const response = await request('/api/optimize/replay-seed', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(entry.request) });
        assert(response.ok, `Replay API rejected request (HTTP ${response.status})`);
        const prediction = parseJsonLossless(await response.text());
        const comparisons = adapter.comparisons ? adapter.comparisons(entry.expected, observations) : entry.expected.comparisons;
        results.push({ battle_time: entry.expected.battle_time, coverage: 'scoped_mitigation_not_full_fight', outcome_coverage: 'diagnostic_seed_not_game_seed', request: entry.request, prediction: publicPrediction(prediction), comparisons: comparePrediction(prediction, comparisons, observations), assumptions: entry.expected.assumptions ?? [], unsupported: entry.expected.unsupported ?? [] });
      }
      return { schema_version: 1, diagnostic_only: true, rng_claim: 'Seeded simulator prediction; source game RNG state is unknown. No exact RNG match is claimed.', binary_sha256: binaryHash, manifest_sha256: sha256(manifestBytes), snapshot_files: manifest.snapshot_files, observation_sha256: manifest.observation.sha256, data: { coverage: 'current_public_catalogs_not_historical_binding', files: catalogs, fingerprint_sha256: sha256(JSON.stringify(catalogs)) }, build: fields(health.build, ['cargo_pkg_version', 'git_sha_short']), settings: { forbidden_tech_level_tier_scaling: manifest.settings?.forbidden_tech_level_tier_scaling ?? false }, results, assumptions: manifest.assumptions ?? [], unsupported: manifest.unsupported ?? [] };
    });
    await fs.writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
    return report;
  } catch (error) {
    preserveRuntime = error.code === 'CHILD_TERMINATION_UNCONFIRMED';
    throw error;
  } finally {
    // Only remove our own newly created directory, after withReplayServer confirmed child termination.
    assert(inside(parent, runtime) && path.basename(runtime).startsWith('.snapshot-replay-'), 'Unsafe runtime cleanup path');
    if (!preserveRuntime) await fs.rm(runtime, { recursive: true, force: true });
  }
}
