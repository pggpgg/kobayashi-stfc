#!/usr/bin/env node

import { chmod, cp, mkdir, readdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..");

const EXCLUDED_DATA_ENTRIES = new Set(["import", "import_logs", "raw", "upstream"]);
const RUNTIME_UPSTREAM_ENTRIES = [
  "hostile_ability_catalog.json",
  "officers",
  "ships",
  "ship_ability_catalog.json",
  "ship_id_registry.json",
  "summary-officer.json",
  "translations-navigation.json",
  "translations-officer_buffs.json",
  "translations-officer_names.json",
  "translations-ships.json",
  "translations-starbase_modules.json",
];

function parseArgs(argv) {
  const args = new Map();
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    const value = argv[i + 1];
    if (!key?.startsWith("--") || value == null) {
      throw new Error(
        "usage: stage-release-bundle.mjs --stage-dir <dir> --binary <path>",
      );
    }
    args.set(key, value);
  }
  const stageDir = args.get("--stage-dir");
  const binary = args.get("--binary");
  if (!stageDir || !binary) {
    throw new Error(
      "usage: stage-release-bundle.mjs --stage-dir <dir> --binary <path>",
    );
  }
  return {
    stageDir: path.resolve(REPO_ROOT, stageDir),
    binary: path.resolve(REPO_ROOT, binary),
  };
}

async function assertFile(filePath) {
  const info = await stat(filePath).catch(() => null);
  if (!info?.isFile()) throw new Error(`required file missing: ${filePath}`);
}

async function assertDirectory(directoryPath) {
  const info = await stat(directoryPath).catch(() => null);
  if (!info?.isDirectory()) {
    throw new Error(`required directory missing: ${directoryPath}`);
  }
}

async function copyRuntimeData(stageDir) {
  const sourceData = path.join(REPO_ROOT, "data");
  const targetData = path.join(stageDir, "data");
  await mkdir(targetData, { recursive: true });

  for (const entry of await readdir(sourceData, { withFileTypes: true })) {
    if (EXCLUDED_DATA_ENTRIES.has(entry.name)) continue;
    await cp(path.join(sourceData, entry.name), path.join(targetData, entry.name), {
      recursive: entry.isDirectory(),
      force: true,
    });
  }

  const sourceUpstream = path.join(sourceData, "upstream", "data-stfc-space");
  const targetUpstream = path.join(targetData, "upstream", "data-stfc-space");
  await mkdir(targetUpstream, { recursive: true });
  for (const entry of RUNTIME_UPSTREAM_ENTRIES) {
    await cp(path.join(sourceUpstream, entry), path.join(targetUpstream, entry), {
      recursive: true,
      force: true,
    });
  }
}

async function verifyBundle(stageDir, binaryName) {
  const requiredFiles = [
    binaryName,
    "README.txt",
    "LICENSE",
    "frontend/dist/index.html",
    "data/officers/officers.canonical.json",
    "data/ships_extended/index.json",
    "data/hostiles/index.json",
    "data/buildings/index.json",
    "data/research_catalog.json",
    "data/support_buffs.json",
    "data/upstream/data-stfc-space/hostile_ability_catalog.json",
    "data/upstream/data-stfc-space/ship_ability_catalog.json",
    "data/upstream/data-stfc-space/summary-officer.json",
    "data/upstream/data-stfc-space/translations-officer_buffs.json",
    "profiles/demo/profile.json",
  ];
  await Promise.all(
    requiredFiles.map((relative) => assertFile(path.join(stageDir, relative))),
  );
  await assertDirectory(path.join(stageDir, "frontend", "dist", "assets"));
  await assertDirectory(
    path.join(stageDir, "data", "upstream", "data-stfc-space", "officers"),
  );
  await assertDirectory(
    path.join(stageDir, "data", "upstream", "data-stfc-space", "ships"),
  );

  for (const excluded of EXCLUDED_DATA_ENTRIES) {
    if (excluded === "upstream") continue;
    const info = await stat(path.join(stageDir, "data", excluded)).catch(() => null);
    if (info) throw new Error(`maintenance-only data leaked into bundle: data/${excluded}`);
  }

  const indexHtml = await readFile(
    path.join(stageDir, "frontend", "dist", "index.html"),
    "utf8",
  );
  const assetRefs = [...indexHtml.matchAll(/["']\/assets\/([^"']+)["']/g)].map(
    (match) => match[1],
  );
  if (assetRefs.length === 0) {
    throw new Error("frontend/dist/index.html contains no built asset references");
  }
  await Promise.all(
    assetRefs.map((relative) =>
      assertFile(path.join(stageDir, "frontend", "dist", "assets", relative)),
    ),
  );
}

export async function stageReleaseBundle({ stageDir, binary }) {
  await assertFile(binary);
  await assertDirectory(path.join(REPO_ROOT, "frontend", "dist", "assets"));
  await mkdir(stageDir, { recursive: true });
  if ((await readdir(stageDir)).length !== 0) {
    throw new Error(`stage directory must be empty: ${stageDir}`);
  }

  const binaryName = path.basename(binary);
  const stagedBinary = path.join(stageDir, binaryName);
  await cp(binary, stagedBinary, { force: true });
  if (process.platform !== "win32") await chmod(stagedBinary, 0o755);

  await cp(
    path.join(REPO_ROOT, "frontend", "dist"),
    path.join(stageDir, "frontend", "dist"),
    { recursive: true, force: true },
  );
  await copyRuntimeData(stageDir);
  await mkdir(path.join(stageDir, "profiles"), { recursive: true });
  await cp(path.join(REPO_ROOT, "profiles", "demo"), path.join(stageDir, "profiles", "demo"), {
    recursive: true,
    force: true,
  });
  await cp(
    path.join(REPO_ROOT, "packaging", "RELEASE-BUNDLE-README.txt"),
    path.join(stageDir, "README.txt"),
  );
  await cp(path.join(REPO_ROOT, "LICENSE"), path.join(stageDir, "LICENSE"));

  await verifyBundle(stageDir, binaryName);
  const manifest = {
    schema_version: 1,
    binary: binaryName,
    includes: ["frontend/dist", "data", "profiles/demo"],
    excludes: ["data/import", "data/import_logs", "data/raw", "upstream detail caches"],
  };
  await writeFile(
    path.join(stageDir, "bundle-manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const options = parseArgs(process.argv.slice(2));
  await stageReleaseBundle(options);
  process.stdout.write(`Staged self-contained release bundle at ${options.stageDir}\n`);
}
