/**
 * Deterministic, OS-independent (LF-normalized SHA-256, zero timestamps) provenance
 * manifest for Kobayashi's upstream snapshots and normalized combat catalogs.
 *
 * Committed to `data/pipeline_manifest.json` so CI and automated data refreshes
 * can verify that every normalized catalog corresponds cleanly to its upstream
 * snapshot and detect silent drift across ships, hostiles, buildings, research,
 * officers, forbidden tech, and ability catalogs.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { atomicWriteJson, sha256NormalizedText } from "./stfcspace_http.mjs";

export const MANIFEST_SCHEMA_VERSION = 1;
export const DEFAULT_MANIFEST_REL_PATH = "data/pipeline_manifest.json";

/**
 * Upstream summary & translation files tracked individually.
 */
export const UPSTREAM_FILE_ENTRIES = [
  { key: "upstream.summary_ship", path: "data/upstream/data-stfc-space/summary-ship.json", kind: "json_array" },
  { key: "upstream.summary_hostile", path: "data/upstream/data-stfc-space/summary-hostile.json", kind: "json_array" },
  { key: "upstream.summary_research", path: "data/upstream/data-stfc-space/summary-research.json", kind: "json_array" },
  { key: "upstream.summary_building", path: "data/upstream/data-stfc-space/summary-building.json", kind: "json_array" },
  { key: "upstream.summary_officer", path: "data/upstream/data-stfc-space/summary-officer.json", kind: "json_array" },
  {
    key: "upstream.summary_forbidden_tech",
    path: "data/upstream/data-stfc-space/summary-forbidden_tech.json",
    kind: "json_array",
  },
  {
    key: "upstream.translations_ships",
    path: "data/upstream/data-stfc-space/translations-ships.json",
    kind: "json_array",
  },
  {
    key: "upstream.translations_officers",
    path: "data/upstream/data-stfc-space/translations-officers.json",
    kind: "json_array",
  },
  {
    key: "upstream.translations_officer_buffs",
    path: "data/upstream/data-stfc-space/translations-officer_buffs.json",
    kind: "json_array",
  },
  {
    key: "upstream.translations_research",
    path: "data/upstream/data-stfc-space/translations-research.json",
    kind: "json_array",
  },
  {
    key: "upstream.translations_starbase_modules",
    path: "data/upstream/data-stfc-space/translations-starbase_modules.json",
    kind: "json_array",
  },
  {
    key: "upstream.translations_factions",
    path: "data/upstream/data-stfc-space/translations-factions.json",
    kind: "json_array",
  },
  {
    key: "upstream.translations_forbidden_tech",
    path: "data/upstream/data-stfc-space/translations-forbidden_tech.json",
    kind: "json_array",
  },
];

/**
 * Upstream per-entity JSON detail directories tracked as deterministic directory trees.
 */
export const UPSTREAM_DIR_ENTRIES = [
  { key: "upstream.details_ships", path: "data/upstream/data-stfc-space/ships" },
  { key: "upstream.details_hostiles", path: "data/upstream/data-stfc-space/hostiles" },
  { key: "upstream.details_research", path: "data/upstream/data-stfc-space/research" },
  { key: "upstream.details_officers", path: "data/upstream/data-stfc-space/officers" },
  { key: "upstream.details_forbidden_tech", path: "data/upstream/data-stfc-space/forbidden_tech" },
];

/**
 * Normalized engine catalogs & registries produced by the data pipeline.
 */
export const NORMALIZED_FILE_ENTRIES = [
  { key: "normalized.ship_id_registry", path: "data/upstream/data-stfc-space/ship_id_registry.json", kind: "json_object" },
  { key: "normalized.hull_id_registry", path: "data/hull_id_registry.json", kind: "json_object" },
  {
    key: "normalized.hostile_ability_catalog",
    path: "data/upstream/data-stfc-space/hostile_ability_catalog.json",
    kind: "json_object",
  },
  {
    key: "normalized.ship_ability_catalog",
    path: "data/upstream/data-stfc-space/ship_ability_catalog.json",
    kind: "json_object",
  },
  { key: "normalized.research_catalog", path: "data/research_catalog.json", kind: "json_object" },
  { key: "normalized.officers_canonical", path: "data/officers/officers.canonical.json", kind: "json_object" },
  { key: "normalized.forbidden_chaos_tech", path: "data/forbidden_chaos_tech.json", kind: "json_object" },
  { key: "normalized.syndicate_reputation", path: "data/syndicate_reputation.json", kind: "json_object" },
  { key: "normalized.support_buffs", path: "data/support_buffs.json", kind: "json_object" },
];

export const NORMALIZED_DIR_ENTRIES = [
  { key: "normalized.ships_extended", path: "data/ships_extended" },
  { key: "normalized.hostiles", path: "data/hostiles" },
  { key: "normalized.buildings", path: "data/buildings" },
];

function extractRecordCount(parsed) {
  if (Array.isArray(parsed)) return parsed.length;
  if (parsed && typeof parsed === "object") {
    for (const field of ["items", "officers", "ships", "hostiles", "buildings", "entries", "records", "abilities", "levels", "buffs"]) {
      if (Array.isArray(parsed[field])) return parsed[field].length;
    }
    return Object.keys(parsed).length;
  }
  return null;
}

function hashSingleFile(repoRoot, relPath) {
  const fullPath = path.join(repoRoot, relPath);
  if (!fs.existsSync(fullPath)) {
    return { path: relPath, present: false, sha256: null, records: null };
  }
  const raw = fs.readFileSync(fullPath, "utf8");
  const sha256 = sha256NormalizedText(raw);
  let records = null;
  let dataVersion = null;
  try {
    const parsed = JSON.parse(raw);
    records = extractRecordCount(parsed);
    if (parsed && typeof parsed === "object" && typeof parsed.data_version === "string") {
      dataVersion = parsed.data_version;
    }
  } catch {
    // Non-JSON or malformed file still hashes deterministically.
  }
  const entry = { path: relPath, present: true, sha256, records };
  if (dataVersion !== null) entry.data_version = dataVersion;
  return entry;
}

function hashJsonDirectory(repoRoot, relPath) {
  const fullDir = path.join(repoRoot, relPath);
  if (!fs.existsSync(fullDir)) {
    return { path: relPath, present: false, file_count: 0, sha256: null };
  }
  const names = fs
    .readdirSync(fullDir, { withFileTypes: true })
    .filter((d) => d.isFile() && d.name.endsWith(".json"))
    .map((d) => d.name)
    .sort();

  const hasher = crypto.createHash("sha256");
  let dataVersion = null;
  for (const name of names) {
    const raw = fs.readFileSync(path.join(fullDir, name), "utf8");
    const fileSha = sha256NormalizedText(raw);
    hasher.update(`${name}:${fileSha}\n`, "utf8");
    if (name === "index.json") {
      try {
        const parsed = JSON.parse(raw);
        if (parsed && typeof parsed.data_version === "string") {
          dataVersion = parsed.data_version;
        }
      } catch {
        // Ignore parse error here; validate_data handles schema errors.
      }
    }
  }
  const entry = {
    path: relPath,
    present: true,
    file_count: names.length,
    sha256: hasher.digest("hex"),
  };
  if (dataVersion !== null) entry.data_version = dataVersion;
  return entry;
}

/**
 * Compute the complete deterministic manifest for `repoRoot`.
 * Contains zero timestamps so repeated runs on identical inputs yield byte-identical JSON.
 *
 * @param {string} repoRoot
 */
export function computePipelineManifest(repoRoot) {
  const upstream = {};
  for (const item of UPSTREAM_FILE_ENTRIES) {
    upstream[item.key.replace(/^upstream\./, "")] = hashSingleFile(repoRoot, item.path);
  }
  for (const item of UPSTREAM_DIR_ENTRIES) {
    upstream[item.key.replace(/^upstream\./, "")] = hashJsonDirectory(repoRoot, item.path);
  }

  const normalized = {};
  for (const item of NORMALIZED_FILE_ENTRIES) {
    normalized[item.key.replace(/^normalized\./, "")] = hashSingleFile(repoRoot, item.path);
  }
  for (const item of NORMALIZED_DIR_ENTRIES) {
    normalized[item.key.replace(/^normalized\./, "")] = hashJsonDirectory(repoRoot, item.path);
  }

  const sortedUpstream = Object.fromEntries(
    Object.keys(upstream)
      .sort()
      .map((k) => [k, upstream[k]]),
  );
  const sortedNormalized = Object.fromEntries(
    Object.keys(normalized)
      .sort()
      .map((k) => [k, normalized[k]]),
  );

  const combinedDigest = crypto
    .createHash("sha256")
    .update(JSON.stringify({ upstream: sortedUpstream, normalized: sortedNormalized }), "utf8")
    .digest("hex");

  return {
    schema_version: MANIFEST_SCHEMA_VERSION,
    combined_sha256: combinedDigest,
    upstream: sortedUpstream,
    normalized: sortedNormalized,
  };
}

/**
 * Compare two manifests and return a deterministic list of changed keys.
 * @param {ReturnType<typeof computePipelineManifest> | null} expected
 * @param {ReturnType<typeof computePipelineManifest>} actual
 */
export function diffPipelineManifests(expected, actual) {
  const diffs = [];
  if (!expected) {
    return [{ section: "manifest", key: "*", kind: "missing_committed_manifest" }];
  }
  if (expected.schema_version !== actual.schema_version) {
    diffs.push({
      section: "manifest",
      key: "schema_version",
      kind: "changed",
      before: expected.schema_version,
      after: actual.schema_version,
    });
  }
  for (const section of ["upstream", "normalized"]) {
    const expMap = expected[section] ?? {};
    const actMap = actual[section] ?? {};
    const keys = [...new Set([...Object.keys(expMap), ...Object.keys(actMap)])].sort();
    for (const key of keys) {
      const b = expMap[key] ?? null;
      const a = actMap[key] ?? null;
      if (JSON.stringify(b) !== JSON.stringify(a)) {
        diffs.push({
          section,
          key,
          path: a?.path ?? b?.path ?? null,
          kind: !b ? "added" : !a ? "removed" : "changed",
          beforeSha: b?.sha256 ?? null,
          afterSha: a?.sha256 ?? null,
          beforeCount: b?.file_count ?? b?.records ?? null,
          afterCount: a?.file_count ?? a?.records ?? null,
        });
      }
    }
  }
  return diffs;
}

/**
 * Write `data/pipeline_manifest.json` atomically.
 * @param {string} repoRoot
 * @param {string} [manifestRelPath]
 */
export async function writePipelineManifest(repoRoot, manifestRelPath = DEFAULT_MANIFEST_REL_PATH) {
  const manifest = computePipelineManifest(repoRoot);
  const dest = path.join(repoRoot, manifestRelPath);
  await atomicWriteJson(dest, manifest, 2);
  return { dest, manifest };
}

/**
 * Verify committed `data/pipeline_manifest.json` matches current on-disk data.
 * @param {string} repoRoot
 * @param {string} [manifestRelPath]
 */
export function verifyPipelineManifest(repoRoot, manifestRelPath = DEFAULT_MANIFEST_REL_PATH) {
  const dest = path.join(repoRoot, manifestRelPath);
  const actual = computePipelineManifest(repoRoot);
  if (!fs.existsSync(dest)) {
    return {
      ok: false,
      manifestPath: dest,
      expected: null,
      actual,
      diffs: diffPipelineManifests(null, actual),
    };
  }
  let expected;
  try {
    expected = JSON.parse(fs.readFileSync(dest, "utf8"));
  } catch (err) {
    return {
      ok: false,
      manifestPath: dest,
      expected: null,
      actual,
      diffs: [
        {
          section: "manifest",
          key: "*",
          kind: "parse_error",
          error: err instanceof Error ? err.message : String(err),
        },
      ],
    };
  }
  const diffs = diffPipelineManifests(expected, actual);
  return {
    ok: diffs.length === 0 && expected.combined_sha256 === actual.combined_sha256,
    manifestPath: dest,
    expected,
    actual,
    diffs,
  };
}

/**
 * Format manifest diffs into a clean Markdown table for PR bodies & step summaries.
 * @param {ReturnType<typeof diffPipelineManifests>} diffs
 */
export function formatManifestDiffMarkdown(diffs) {
  if (!diffs || diffs.length === 0) {
    return "No catalog digest changes detected.";
  }
  const lines = [
    "| Section | Artifact | Count (before → after) | Digest (before → after) |",
    "| --- | --- | ---: | --- |",
  ];
  for (const d of diffs) {
    const countCol =
      d.beforeCount != null || d.afterCount != null
        ? `${d.beforeCount ?? "—"} → ${d.afterCount ?? "—"}`
        : "—";
    const shaCol =
      d.beforeSha || d.afterSha
        ? `\`${(d.beforeSha ?? "none").slice(0, 10)}\` → \`${(d.afterSha ?? "none").slice(0, 10)}\``
        : d.kind;
    lines.push(`| ${d.section} | \`${d.key}\` | ${countCol} | ${shaCol} |`);
  }
  return lines.join("\n");
}
