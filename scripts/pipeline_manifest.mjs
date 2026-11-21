#!/usr/bin/env node
/**
 * Generate or verify the deterministic data pipeline provenance manifest
 * (`data/pipeline_manifest.json`).
 *
 * Usage (repo root):
 *   node scripts/pipeline_manifest.mjs --check
 *   node scripts/pipeline_manifest.mjs --write
 *   node scripts/pipeline_manifest.mjs --check --json
 */

import path from "node:path";
import url from "node:url";
import {
  DEFAULT_MANIFEST_REL_PATH,
  formatManifestDiffMarkdown,
  verifyPipelineManifest,
  writePipelineManifest,
} from "./lib/pipeline_manifest.mjs";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));

function printHelp() {
  console.log(`Usage: node scripts/pipeline_manifest.mjs [--check | --write] [--json]

Options:
  --check   Verify committed ${DEFAULT_MANIFEST_REL_PATH} matches current data/ contents (default)
  --write   Recompute and atomically write ${DEFAULT_MANIFEST_REL_PATH}
  --json    Emit machine-readable JSON output
`);
}

async function main() {
  const argv = process.argv.slice(2);
  if (argv.includes("--help") || argv.includes("-h")) {
    printHelp();
    process.exit(0);
  }
  const write = argv.includes("--write");
  const json = argv.includes("--json");

  if (write) {
    const { dest, manifest } = await writePipelineManifest(REPO_ROOT);
    if (json) {
      console.log(
        JSON.stringify(
          {
            mode: "write",
            path: path.relative(REPO_ROOT, dest),
            combined_sha256: manifest.combined_sha256,
            upstream_entries: Object.keys(manifest.upstream).length,
            normalized_entries: Object.keys(manifest.normalized).length,
          },
          null,
          2,
        ),
      );
    } else {
      console.log(
        `Wrote ${path.relative(REPO_ROOT, dest)} (combined=${manifest.combined_sha256.slice(0, 12)}, upstream=${Object.keys(manifest.upstream).length}, normalized=${Object.keys(manifest.normalized).length})`,
      );
    }
    return;
  }

  const res = verifyPipelineManifest(REPO_ROOT);
  if (json) {
    console.log(
      JSON.stringify(
        {
          mode: "check",
          ok: res.ok,
          path: path.relative(REPO_ROOT, res.manifestPath),
          combined_sha256: res.actual.combined_sha256,
          diffs: res.diffs,
        },
        null,
        2,
      ),
    );
  } else if (res.ok) {
    console.log(
      `✅ Pipeline manifest matches on-disk catalogs (${res.actual.combined_sha256.slice(0, 12)})`,
    );
  } else {
    console.error(
      `❌ Pipeline manifest drift detected (${res.diffs.length} difference(s)). Run \`node scripts/pipeline_manifest.mjs --write\` after refreshing data:\n`,
    );
    console.error(formatManifestDiffMarkdown(res.diffs));
  }

  if (!res.ok) {
    process.exit(1);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
