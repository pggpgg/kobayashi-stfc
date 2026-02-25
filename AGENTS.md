# AGENTS.md

## Cursor Cloud specific instructions

### Project overview
KOBAYASHI is a Star Trek Fleet Command Monte Carlo combat simulator and crew optimizer. It consists of a **Rust backend** (CLI + HTTP server) and a **React/Vite frontend**. All game data is file-based (JSON/YAML in `data/`); no external databases or services are required.

### Running services

| Service | Command | Port | Notes |
|---|---|---|---|
| Rust backend | `cargo run --bin kobayashi -- serve` | 3000 | Serves API and built frontend from `frontend/dist/` |
| Frontend dev server | `npm run dev` (in `frontend/`) | 5173 | Proxies `/api` to `http://127.0.0.1:3480`; use port 3000 to test full-stack via the built SPA instead |

**Gotcha:** The Vite dev server proxy targets port 3480, but the Rust backend defaults to port 3000. For full-stack dev with the Vite HMR server, either override the backend bind address with `KOBAYASHI_BIND=127.0.0.1:3480` or use `http://localhost:3000` which serves the pre-built SPA from `frontend/dist/`.

### Key commands

See `README.md` for full docs. Quick reference:

- **Build:** `cargo build` (Rust), `npm run build` (frontend, from `frontend/`)
- **Test:** `cargo test` (92 tests covering combat engine, optimizer, server, CLI, data provenance, calibration)
- **Lint/typecheck:** `npx tsc --noEmit` (from `frontend/`); Rust uses standard `cargo` warnings
- **CLI optimize:** `cargo run --bin kobayashi -- optimize --ship <id> --hostile <id> --sims <n>`
- **CLI validate:** `cargo run --bin kobayashi -- validate --path data/officers`

### Non-obvious notes

- The project has multiple binaries (`Cargo.toml` `[[bin]]` entries). Use `--bin kobayashi` to run the main binary; `cargo run` without `--bin` will fail with "could not determine which binary to run."
- Frontend `npm run build` must be run before the Rust server can serve the SPA at port 3000. Without it, the server still serves the API but the UI pages return 404.
- TypeScript check uses `npx tsc --noEmit` (not `tsc -b --noEmit` which errors on combined flags).
- The `tools/` directory contains optional Python validation scripts; they are not required for normal development.
