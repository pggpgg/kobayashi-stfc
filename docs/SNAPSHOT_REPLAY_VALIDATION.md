# Snapshot-bound replay diagnostics

The replay tool recompiles a frozen capture through the normal server scenario
builder. It does not select a live profile by name. This makes a combat discrepancy
reproducible without overwriting player files or publishing a complete account.

## Run a frozen capture

Use Node with `JSON.parse` reviver source context support. Build the simulator
from the revision being evaluated, and keep the original capture outside the
public repository contents. From the checkout root:

```powershell
cargo build --locked --bin kobayashi
node scripts/replay_snapshot.mjs `
  --manifest tests/fixtures/recorded_fights/realta_jemhadar_baseline_20261024_replay.json `
  --snapshot 'C:/private/frozen-capture' `
  --data data `
  --binary target/debug/kobayashi.exe `
  --output 'C:/private/reports/new-replay.json'
```

The output parent must exist. The tool refuses to overwrite a report. Use
`--startup-timeout-ms 180000` if catalog loading needs more time.

The command prints `report_written` when it creates a diagnostic report. A zero
exit status means the report was produced, **not** that the simulator matches
combat. Inspect each comparison's `status`.

## What is bound

Each reviewed manifest names eight capture files by SHA-256:

- `profile.json`, `roster.imported.json`, `research.imported.json`;
- `buffs.imported.json`, `buildings.imported.json`, `ships.imported.json`;
- `forbidden_tech.imported.json`, `battlelogs.imported.json`.

The runner checks every hash before it parses or copies the data. It selects
exactly one journal by literal battle time. It checks full starting HP, ship
tier and level, ordered components including duplicates, all ten crew positions,
officer ranks and levels, and the target. The request must agree with these
inputs and the public ship, hostile and officer catalogs.

The observation file has its own digest and must identify the same raw journal
file and battle time. Comparison intervals come from that observation file;
they are not copied from a previous simulator prediction.

Selected catalog files are pinned in the manifest. The report also fingerprints
the complete public data tree and simulator executable. These identify the
evaluated model and catalogs; they do not establish historical server parity.

## Isolation and privacy

The runner creates a temporary runtime beside the report. It copies the explicit
public data tree and seven verified profile files into a synthetic profile. It
does not copy a real profile index, sync token or battle journal into the runtime.
It uses an ephemeral loopback port and verifies ownership with a fresh API key
before submitting the replay. Inherited `KOBAYASHI_*` overrides are cleared.

The tool stops its own child process and removes its temporary runtime. It never
uses the live capture port, 3000. If child termination cannot be confirmed, it
retains the runtime and reports an error. Confirm that the child has exited before
removing that directory; it contains copied private input files.

Reports omit raw imports, account identifiers, local source paths and process
logs. They retain input digests and selected public combat identifiers. Keep the
full capture and private reports out of commits. Only redacted observations,
reviewed bindings and explicitly reviewed result summaries belong in the repo.

## Read the comparisons

The current manifests measure **incoming base-stat mitigation**, not complete
fight accuracy. Jem'Hadar observations identify a weapon component. Exchange
observations pool clean hits across weapons and rounds; their comparison is one
pooled diagnostic, not two independent weapon validations.

- `within_observed_interval`: the complete predicted interval lies inside the
  independently recorded interval for this metric.
- `mismatch`: the prediction disagrees with the recorded interval.
- `insufficient_precision`: legacy trace rounding prevents a definite comparison.
- `insufficient`: required weapon evidence is absent or the trace is truncated.

New traces preserve `base_stat_mitigation_unrounded`. Older six-decimal traces
carry display uncertainty; the tool must not treat their rounded points as exact
measurements. Observed bands remain unchanged. See [COMBAT_TRACE.md](COMBAT_TRACE.md).

Seed-zero outcome and duration are diagnostic outputs. The game server's RNG
state is unknown, so one seeded result is not an exact historical replay.
Sequential input reads, unsupported buffs, parser-version differences and other
limits remain explicit in the manifests and reports. A scoped match does not
establish complete player-buff compilation or a verified full-fight result.

See [the reviewed October 2026 replay results](SNAPSHOT_REPLAY_RESULTS.md) for
the four completed no-Tom/Tom comparisons and instrumentation parity checks.

The historical profile-based suite in
`tests/fixtures/recorded_fights/recorded_fight_suite.json` remains separate.
These diagnostics do not inflate its recorded-fight pass count. Promote broader
accuracy cases only after their required inputs and observations are complete.

The [fresh captain-swap controls](TOM_PARIS_CAPTAIN_SWAP_20261026.md) add two
source-bound observations and replay manifests. Their measured hypothesis
compatibility is separate from the current simulator's replay results.

## Regression checks

```powershell
node --test scripts/test/snapshot_replay.test.mjs
cargo test --locked --test native_combat_defense
```

Run filesystem-writing tests in an isolated checkout. The Node suite uses local
mock servers; a denied loopback connection is a test-environment failure, not a
passing result. No real game or live capture server is needed.
