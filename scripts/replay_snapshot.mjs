#!/usr/bin/env node
import { runSnapshotReplay } from './lib/snapshot_replay.mjs';

const usage = 'node scripts/replay_snapshot.mjs --manifest public.json --snapshot frozen-directory --data public-data-directory --binary kobayashi.exe --output new-report.json [--startup-timeout-ms 120000]';
const flags = new Map([['--manifest', 'manifest'], ['--snapshot', 'snapshot'], ['--data', 'data'], ['--binary', 'binary'], ['--output', 'output']]);
const options = {};
try {
  const args = process.argv.slice(2);
  if (args.length === 1 && ['--help', '-h'].includes(args[0])) {
    console.log(usage);
  } else {
    for (let index = 0; index < args.length; index += 2) {
      const flag = args[index];
      const value = args[index + 1];
      if (!value || value.startsWith('--')) throw new Error('Every option requires one value');
      if (flag === '--startup-timeout-ms') {
        if (options.serverOptions) throw new Error('Duplicate option');
        const timeout = Number(value);
        if (!Number.isSafeInteger(timeout) || timeout <= 0) throw new Error('Startup timeout must be a positive integer');
        options.serverOptions = { startupTimeoutMs: timeout };
      } else {
        const key = flags.get(flag);
        if (!key || Object.hasOwn(options, key)) throw new Error('Unknown or duplicate option');
        options[key] = value;
      }
    }
    if ([...flags.values()].some(key => !options[key])) throw new Error(usage);
    const report = await runSnapshotReplay(options);
    console.log(JSON.stringify({ status: 'report_written', diagnostic_only: true, comparisons: report.results.flatMap(result => result.comparisons.map(comparison => comparison.status)) }));
  }
} catch (error) {
  // Node filesystem/network errors can embed private absolute paths. Never echo their raw details.
  const message = error.code ? `Replay failed (${error.code}); verify input files and permissions.` : error instanceof SyntaxError ? 'Invalid JSON input.' : error.message;
  console.error(message);
  process.exitCode = 1;
}
