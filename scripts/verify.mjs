#!/usr/bin/env node
/**
 * Post-sync verification: mirrors `.github/workflows/ci.yml` (Rust, Frontend, Combat engine Python).
 * Run from repo root after git pull.
 *
 * Usage: node scripts/verify.mjs
 *   or:  npm run verify
 *
 * Optional environment (same checks as CI, but skip when a tool is missing locally):
 *   VERIFY_SKIP_CARGO_FMT=1
 *   VERIFY_SKIP_CARGO_AUDIT=1
 *   VERIFY_SKIP_PYTHON=1
 */

import { execSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const FRONTEND = path.join(ROOT, "frontend");

function truthyEnv(name) {
  const v = process.env[name];
  if (v == null || v === "") return false;
  return !/^(0|false|no)$/i.test(String(v).trim());
}

function run(cmd, opts = {}) {
  const cwd = opts.cwd ?? ROOT;
  console.log(`\n> ${cmd}\n`);
  execSync(cmd, { stdio: "inherit", cwd, shell: true });
}

/** First executable that responds to `--version` (matches CI’s Python 3.12 tests). Override with `PYTHON`. */
function resolvePython() {
  if (process.env.PYTHON?.trim()) {
    return process.env.PYTHON.trim();
  }
  const candidates =
    process.platform === "win32"
      ? ["python", "python3", "py -3.12", "py -3"]
      : ["python3", "python"];
  for (const cmd of candidates) {
    try {
      execSync(`${cmd} --version`, { stdio: "ignore", cwd: ROOT, shell: true });
      return cmd;
    } catch {
      /* try next */
    }
  }
  return null;
}

function runPythonTests(py) {
  run(`${py} -m pip install -r tools/combat_engine/requirements-test.txt`);
  run(`${py} -m pytest tools/combat_engine/tests/ -v`);
}

try {
  console.log("=== Post-sync verification (CI parity) ===\n");

  // --- Rust (matches ci.yml `rust` job order) ---
  if (!truthyEnv("VERIFY_SKIP_CARGO_FMT")) {
    run("cargo fmt --all -- --check");
  } else {
    console.log("\n> (skipped: VERIFY_SKIP_CARGO_FMT)\n");
  }

  run("cargo test");
  run("cargo build --release");
  run("cargo clippy --all-targets -- -D warnings");

  if (!truthyEnv("VERIFY_SKIP_CARGO_AUDIT")) {
    run("cargo audit");
  } else {
    console.log("\n> (skipped: VERIFY_SKIP_CARGO_AUDIT)\n");
  }

  // --- Frontend (matches ci.yml `frontend` job order) ---
  run("npm ci", { cwd: FRONTEND });
  run("npm audit --audit-level=high", { cwd: FRONTEND });
  run("npm run lint", { cwd: FRONTEND });
  run("npm run typecheck", { cwd: FRONTEND });
  run("npm run test", { cwd: FRONTEND });
  run("npm run build", { cwd: FRONTEND });

  // --- Python combat_engine (matches ci.yml `combat_engine_python` job) ---
  if (!truthyEnv("VERIFY_SKIP_PYTHON")) {
    const py = resolvePython();
    if (py == null) {
      console.error(
        "\nverify: No Python interpreter found (tried python, python3, py -3.12 / py -3 on Windows).\n" +
          "Install Python 3.12+, add it to PATH, set PYTHON to the executable, or run with VERIFY_SKIP_PYTHON=1.\n",
      );
      process.exit(1);
    }
    console.log(`\n(using Python: ${py})\n`);
    runPythonTests(py);
  } else {
    console.log("\n> (skipped: VERIFY_SKIP_PYTHON)\n");
  }

  console.log("\n=== Verification complete ===\n");
} catch (err) {
  process.exit(err.status ?? 1);
}
