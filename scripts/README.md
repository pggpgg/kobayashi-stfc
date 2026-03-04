# Scripts

## Post-sync verification

After pulling changes from another machine, run:

```bash
npm run verify
```

This runs `cargo test`, `cargo build --release`, `cargo clippy`, then `npm ci`, `npm run test`, and `npm run build` in `frontend/`. Mirrors CI. Requires `data/officers/officers.canonical.json` and (recommended) `data/ships/`, `data/hostiles/` indices.

---

# Data pipeline (STFCcommunity baseline)

1. **Fetch upstream:** From repo root, run  
   `powershell -ExecutionPolicy Bypass -File scripts/fetch_stfc_data.ps1`  
   Downloads STFCcommunity/data and extracts hostiles + ships to `data/upstream/stfccommunity-data/`.

2. **Normalize:**  
   `cargo run --bin normalize_stfc_data`  
   Reads upstream JSON, writes KOBAYASHI-format `data/hostiles/` and `data/ships/` (including `index.json` with `data_version` and `source_note`).

3. **Optional:** Set `STFC_DATA_VERSION` (e.g. a git commit) when running the normalizer to record the source.

Upstream is treated as a read-only baseline (repo is outdated ~3y). Newer entries can be added under `data/hostiles/` and `data/ships/` with the same schema.
</think><｜tool▁call▁begin｜>
Shell
