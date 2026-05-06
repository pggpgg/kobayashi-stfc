#!/usr/bin/env bash
# Run the same checks as .github/workflows/ci.yml (Rust + frontend jobs).
# Usage: from repo root: ./scripts/local-ci.sh
# Requires: Rust stable (rustfmt, clippy), Node 20+, npm. Optional: cargo-audit (cargo install cargo-audit).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "=== cargo fmt --check ==="
cargo fmt --all -- --check

echo "=== cargo clippy ==="
cargo clippy --all-targets -- -D warnings

echo "=== cargo test ==="
cargo test

echo "=== root scripts (node:test import helpers) ==="
npm run test:scripts

echo "=== cargo build --release ==="
cargo build --release

if command -v cargo-audit >/dev/null 2>&1; then
  echo "=== cargo audit ==="
  cargo audit
else
  echo "=== cargo audit (skipped: install with 'cargo install cargo-audit' or use taiki-e/install-action in CI) ==="
fi

echo "=== frontend (npm ci, audit, lint, typecheck, test, build) ==="
(
  cd frontend
  npm ci
  npm audit --audit-level=high
  npm run lint
  npm run typecheck
  npm run test
  npm run build
)

echo "=== local-ci.sh: OK ==="
