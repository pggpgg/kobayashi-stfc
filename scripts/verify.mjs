#!/usr/bin/env node
/**
 * Post-sync verification: runs Rust tests/build/clippy and frontend tests/build.
 * Mirrors .github/workflows/ci.yml. Run from project root after git pull.
 *
 * Usage: node scripts/verify.mjs
 *   or:  npm run verify
 */

import { execSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const FRONTEND = path.join(ROOT, "frontend");

function run(cmd, opts = {}) {
  const cwd = opts.cwd ?? ROOT;
  console.log(`\n> ${cmd}\n`);
  execSync(cmd, { stdio: "inherit", cwd, shell: true });
}

try {
  console.log("=== Post-sync verification ===\n");

  run("cargo test");
  run("cargo build --release");
  run("cargo clippy --all-targets");

  run("npm ci", { cwd: FRONTEND });
  run("npm run test", { cwd: FRONTEND });
  run("npm run build", { cwd: FRONTEND });

  console.log("\n=== Verification complete ===\n");
} catch (err) {
  process.exit(err.status ?? 1);
}
