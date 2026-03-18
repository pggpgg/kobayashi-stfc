#!/usr/bin/env node
/**
 * Lightweight validator for data/upstream/data-stfc-space/mapping/*.json.
 *
 * Checks:
 * - mapping/index.json exists and lists known domains.
 * - Each domain's mapping_file exists.
 * - Each declared "files[].path" exists under data/upstream/data-stfc-space/.
 * - Each "files[].glob" pattern matches at least one file (unless allow_empty === true).
 *
 * This script is intentionally conservative and focused on filesystem-level
 * validation so it can be run safely as part of data sanity checks.
 *
 * Usage (from repo root):
 *   node scripts/validate_stfcspace_mapping.mjs
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

const REPO_ROOT = path.dirname(path.dirname(url.fileURLToPath(import.meta.url)));
const MAPPING_DIR = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
  "mapping",
);
const INDEX_PATH = path.join(MAPPING_DIR, "index.json");
const UPSTREAM_ROOT = path.join(
  REPO_ROOT,
  "data",
  "upstream",
  "data-stfc-space",
);

async function readJson(filePath) {
  const raw = await fs.readFile(filePath, "utf8");
  return JSON.parse(raw);
}

async function pathExists(p) {
  try {
    await fs.stat(p);
    return true;
  } catch {
    return false;
  }
}

/**
 * Very small glob helper for patterns like "ships/*.json" or "translations-*.json".
 * We intentionally support only one directory segment and a simple "*" wildcard
 * in the filename for this validator.
 */
async function resolveSimpleGlob(relPattern) {
  const parts = relPattern.split("/");
  if (parts.length === 1) {
    // Pattern like "translations-*.json"
    const dir = MAPPING_DIR.replace(/[\\/]mapping$/, "");
    return matchInDir(dir, parts[0]);
  }
  if (parts.length === 2) {
    const [dirPart, filePattern] = parts;
    const baseDir = path.join(
      REPO_ROOT,
      "data",
      "upstream",
      "data-stfc-space",
      dirPart,
    );
    return matchInDir(baseDir, filePattern);
  }
  // Fallback: treat as no matches for unsupported patterns.
  return [];
}

async function matchInDir(dir, filePattern) {
  const results = [];
  let entries;
  try {
    entries = await fs.readdir(dir, { withFileTypes: true });
  } catch {
    return results;
  }
  const [prefix, suffix] = filePattern.split("*");
  for (const entry of entries) {
    if (!entry.isFile()) continue;
    const name = entry.name;
    if (prefix && !name.startsWith(prefix)) continue;
    if (suffix && !name.endsWith(suffix)) continue;
    results.push(path.join(dir, name));
  }
  return results;
}

/**
 * Ensure every path listed in mapping/upstream_catalog.json exists on disk.
 */
async function validateUpstreamCatalog(errors) {
  const catalogPath = path.join(MAPPING_DIR, "upstream_catalog.json");
  if (!(await pathExists(catalogPath))) {
    errors.push("mapping/upstream_catalog.json is missing.");
    return;
  }
  let cat;
  try {
    cat = await readJson(catalogPath);
  } catch (err) {
    errors.push(`upstream_catalog.json: ${err.message}`);
    return;
  }
  const check = async (rel, label) => {
    const full = path.join(UPSTREAM_ROOT, rel);
    if (!(await pathExists(full))) {
      errors.push(`upstream_catalog ${label}: missing ${rel}`);
    }
  };
  for (const m of cat.markdown_docs ?? []) {
    if (m.path) await check(m.path, "markdown_docs");
  }
  for (const r of cat.registry_files ?? []) {
    if (r.path) await check(r.path, "registry_files");
  }
  for (const s of cat.summaries ?? []) {
    if (s.path) await check(s.path, "summaries");
  }
  for (const t of cat.translations ?? []) {
    if (t.path) await check(t.path, "translations");
  }
  for (const b of cat.bulk_patterns_not_individually_documented ?? []) {
    if (!b.glob) continue;
    const matches = await resolveSimpleGlob(b.glob);
    if (matches.length === 0) {
      errors.push(
        `upstream_catalog bulk pattern ${b.glob}: no files found (expected cached upstream data).`,
      );
    }
  }

  const summaryOnDisk = new Set();
  const translationOnDisk = new Set();
  try {
    const entries = await fs.readdir(UPSTREAM_ROOT, { withFileTypes: true });
    for (const e of entries) {
      if (!e.isFile() || !e.name.endsWith(".json")) continue;
      if (e.name.startsWith("summary-")) summaryOnDisk.add(e.name);
      if (e.name.startsWith("translations-")) translationOnDisk.add(e.name);
    }
  } catch (_) {
    /* ignore */
  }
  const summaryCatalog = new Set(
    (cat.summaries ?? []).map((s) => s.path).filter(Boolean),
  );
  for (const name of summaryOnDisk) {
    if (!summaryCatalog.has(name)) {
      errors.push(
        `Upstream file ${name} is not listed in upstream_catalog.json summaries[] — add a semantic entry.`,
      );
    }
  }
  const translationCatalog = new Set(
    (cat.translations ?? []).map((t) => t.path).filter(Boolean),
  );
  for (const name of translationOnDisk) {
    if (!translationCatalog.has(name)) {
      errors.push(
        `Upstream file ${name} is not listed in upstream_catalog.json translations[] — add a semantic entry.`,
      );
    }
  }
}

async function main() {
  const errors = [];
  const warnings = [];

  if (!(await pathExists(MAPPING_DIR))) {
    console.error(
      "Mapping directory data/upstream/data-stfc-space/mapping/ does not exist.",
    );
    process.exit(1);
  }

  if (!(await pathExists(INDEX_PATH))) {
    console.error(
      "Mapping index.json is missing under data/upstream/data-stfc-space/mapping/.",
    );
    process.exit(1);
  }

  let index;
  try {
    index = await readJson(INDEX_PATH);
  } catch (err) {
    console.error("Failed to read mapping index.json:", err.message);
    process.exit(1);
  }

  if (!Array.isArray(index.domains) || index.domains.length === 0) {
    errors.push("mapping/index.json.domains must be a non-empty array.");
  }

  for (const domain of index.domains ?? []) {
    const name = domain.domain ?? "<unknown>";
    const mappingFile = domain.mapping_file;
    if (typeof mappingFile !== "string" || !mappingFile) {
      errors.push(`Domain ${name}: missing or invalid mapping_file.`);
      continue;
    }
    const mappingPath = path.join(MAPPING_DIR, mappingFile);
    if (!(await pathExists(mappingPath))) {
      errors.push(
        `Domain ${name}: mapping file ${path.relative(
          REPO_ROOT,
          mappingPath,
        )} does not exist.`,
      );
      continue;
    }

    let mapping;
    try {
      mapping = await readJson(mappingPath);
    } catch (err) {
      errors.push(
        `Domain ${name}: failed to parse ${mappingFile} as JSON: ${err.message}`,
      );
      continue;
    }

    if (mapping.domain !== name) {
      warnings.push(
        `Domain ${name}: mapping.domain is ${JSON.stringify(
          mapping.domain,
        )} (expected ${JSON.stringify(name)}).`,
      );
    }

    if (name === "upstream_catalog") {
      await validateUpstreamCatalog(errors);
      continue;
    }

    if (!Array.isArray(mapping.files)) {
      warnings.push(
        `Domain ${name}: mapping.files is missing or not an array; skipping file checks for this domain.`,
      );
      continue;
    }

    if (mapping.files.length === 0) {
      warnings.push(`Domain ${name}: mapping.files is empty; skipping file checks.`);
      continue;
    }

    for (const f of mapping.files) {
      const role = f.role ?? "<unknown>";
      if (f.path) {
        const upstreamPath = path.join(UPSTREAM_ROOT, f.path);
        if (!(await pathExists(upstreamPath))) {
          errors.push(
            `Domain ${name}: file path ${f.path} (role=${role}) does not exist under data/upstream/data-stfc-space/.`,
          );
        }
      } else if (f.glob) {
        const matches = await resolveSimpleGlob(f.glob);
        if (matches.length === 0 && !f.allow_empty) {
          warnings.push(
            `Domain ${name}: glob ${f.glob} (role=${role}) did not match any files. This may be fine if the upstream has not been fetched yet.`,
          );
        }
      } else {
        warnings.push(
          `Domain ${name}: file entry missing both path and glob (role=${role}).`,
        );
      }
    }
  }

  if (errors.length > 0) {
    console.error("❌ Mapping validation failed with errors:");
    for (const e of errors) console.error("  -", e);
    if (warnings.length > 0) {
      console.error("\nWarnings:");
      for (const w of warnings) console.error("  -", w);
    }
    process.exit(1);
  }

  console.log("✅ Mapping files passed basic validation.");
  if (warnings.length > 0) {
    console.log("\nWarnings:");
    for (const w of warnings) console.log("  -", w);
  }
}

main().catch((err) => {
  console.error("validate_stfcspace_mapping.mjs crashed:", err);
  process.exit(1);
});

