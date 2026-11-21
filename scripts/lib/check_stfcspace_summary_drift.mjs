import fs from "node:fs";
import path from "node:path";
import {
  atomicWriteFile,
  canonicalJsonSha256,
  fetchJsonResilient,
  runConcurrentPool,
} from "./stfcspace_http.mjs";

const BASE_URL = "https://data.stfc.space";

/** Default CI-gated summary catalogs tracked for upstream drift (ROADMAP task 14). */
export const DOMAINS = [
  { name: "ship", segment: "ship", file: "summary-ship.json", key: (row) => String(row.id) },
  {
    name: "hostile",
    segment: "hostile",
    file: "summary-hostile.json",
    key: (row) => `${row.id}:${row.level}`,
  },
  {
    name: "research",
    segment: "research",
    file: "summary-research.json",
    key: (row) => String(row.id),
  },
];

/** Full combat-relevant summary catalogs for weekly refresh & content-aware drift tracking. */
export const EXTENDED_DOMAINS = [
  ...DOMAINS,
  {
    name: "officer",
    segment: "officer",
    file: "summary-officer.json",
    key: (row) => String(row.id),
  },
  {
    name: "building",
    segment: "building",
    file: "summary-building.json",
    key: (row) => String(row.id),
  },
  {
    name: "forbidden_tech",
    segment: "forbidden_tech",
    file: "summary-forbidden_tech.json",
    key: (row) => String(row.id),
  },
];

/**
 * Resolve a list of domain definitions from names (e.g. `["ship", "hostile"]`) or `allDomains`.
 * @param {{ allDomains?: boolean, domainNames?: string[] | null }} [opts]
 */
export function resolveDomains(opts = {}) {
  if (opts.allDomains) return EXTENDED_DOMAINS;
  if (opts.domainNames && opts.domainNames.length > 0) {
    const byName = new Map(EXTENDED_DOMAINS.map((d) => [d.name, d]));
    const selected = [];
    for (const raw of opts.domainNames) {
      const name = String(raw).trim();
      if (!name) continue;
      const found = byName.get(name);
      if (!found) {
        throw new Error(
          `Unknown drift domain '${name}' (valid: ${EXTENDED_DOMAINS.map((d) => d.name).join(", ")})`,
        );
      }
      if (!selected.includes(found)) selected.push(found);
    }
    if (selected.length === 0) return DOMAINS;
    return selected;
  }
  return DOMAINS;
}

/**
 * Extract numeric entity id from a normalized key (`"123"` or `"123:45"` -> `123`).
 * @param {string} key
 * @returns {number | null}
 */
function numericIdFromKey(key) {
  const head = String(key).split(":")[0];
  const n = Number.parseInt(head, 10);
  return Number.isFinite(n) ? n : null;
}

/**
 * @param {string} dir
 * @param {{ name: string, file: string, key: (row: object) => string }} domain
 */
export function loadDomainKeys(dir, domain) {
  const filePath = path.join(dir, domain.file);
  if (!fs.existsSync(filePath)) {
    throw new Error(`missing summary file: ${filePath}`);
  }
  const raw = fs.readFileSync(filePath, "utf8");
  const data = JSON.parse(raw);
  if (!Array.isArray(data)) {
    throw new Error(`${domain.file} must be a JSON array`);
  }
  const keys = new Set();
  const rowHashes = new Map();
  for (const row of data) {
    const k = domain.key(row);
    keys.add(k);
    rowHashes.set(k, canonicalJsonSha256(row));
  }
  if (keys.size !== data.length) {
    throw new Error(`${domain.file}: duplicate keys after normalization`);
  }
  return { count: data.length, keys, rowHashes };
}

/**
 * Compare summary directories across the requested domains.
 * Detects added keys, removed keys, AND in-place row payload modifications
 * (`modified` + `changedNumericIds`) so detail fetchers can selectively refresh
 * stat-rebalanced entities without full re-downloads.
 *
 * @param {string} beforeDir
 * @param {string} afterDir
 * @param {{ domains?: typeof DOMAINS, includeModifiedInDrifted?: boolean }} [opts]
 */
export function compareSummaryDirs(beforeDir, afterDir, opts = {}) {
  const domains = opts.domains ?? DOMAINS;
  const includeModified = Boolean(opts.includeModifiedInDrifted);
  const reports = [];
  for (const domain of domains) {
    const before = loadDomainKeys(beforeDir, domain);
    const after = loadDomainKeys(afterDir, domain);
    const added = [...after.keys].filter((k) => !before.keys.has(k)).sort();
    const removed = [...before.keys].filter((k) => !after.keys.has(k)).sort();
    const modified = [...after.keys]
      .filter((k) => before.keys.has(k) && before.rowHashes.get(k) !== after.rowHashes.get(k))
      .sort();
    const changedNumericIds = [
      ...new Set(
        [...added, ...modified]
          .map(numericIdFromKey)
          .filter((n) => n !== null),
      ),
    ].sort((a, b) => a - b);
    const keyDrifted = before.count !== after.count || added.length > 0 || removed.length > 0;
    const contentDrifted = modified.length > 0;
    const drifted = includeModified ? keyDrifted || contentDrifted : keyDrifted;
    reports.push({
      name: domain.name,
      file: domain.file,
      beforeCount: before.count,
      afterCount: after.count,
      added,
      removed,
      modified,
      changedNumericIds,
      contentDrifted,
      drifted,
    });
  }
  return reports;
}

/** @param {ReturnType<typeof compareSummaryDirs>} reports */
export function anyDrift(reports) {
  return reports.some((r) => r.drifted);
}

/** @param {ReturnType<typeof compareSummaryDirs>} reports */
export function anyContentOrKeyChange(reports) {
  return reports.some((r) => r.drifted || r.contentDrifted);
}

/**
 * @param {ReturnType<typeof compareSummaryDirs>} reports
 * @param {{ beforeLabel?: string, afterLabel?: string }} [opts]
 */
export function formatTextReport(reports, opts = {}) {
  const beforeLabel = opts.beforeLabel ?? "committed";
  const afterLabel = opts.afterLabel ?? "live";
  const lines = [];
  for (const r of reports) {
    const modCount = r.modified?.length ?? 0;
    if (!r.drifted && modCount === 0) {
      lines.push(`${r.name}: no drift (${r.beforeCount} rows)`);
      continue;
    }
    if (!r.drifted && modCount > 0) {
      lines.push(
        `${r.name}: no drift (${r.beforeCount} rows; ${modCount} modified in-place)`,
      );
      continue;
    }
    lines.push(
      `${r.name}: drift — count ${beforeLabel} ${r.beforeCount} → ${afterLabel} ${r.afterCount}; +${r.added.length} new, -${r.removed.length} removed, ~${modCount} modified`,
    );
    if (r.added.length > 0) {
      const sample = r.added.slice(0, 8).join(", ");
      const more = r.added.length > 8 ? ` … (+${r.added.length - 8} more)` : "";
      lines.push(`  new: ${sample}${more}`);
    }
    if (r.removed.length > 0) {
      const sample = r.removed.slice(0, 8).join(", ");
      const more = r.removed.length > 8 ? ` … (-${r.removed.length - 8} more)` : "";
      lines.push(`  removed: ${sample}${more}`);
    }
    if (modCount > 0) {
      const sample = r.modified.slice(0, 8).join(", ");
      const more = modCount > 8 ? ` … (~${modCount - 8} more)` : "";
      lines.push(`  modified: ${sample}${more}`);
    }
  }
  return lines.join("\n");
}

/**
 * @param {ReturnType<typeof compareSummaryDirs>} reports
 * @param {{ beforeLabel?: string, afterLabel?: string, title?: string }} [opts]
 */
export function formatMarkdownReport(reports, opts = {}) {
  const beforeLabel = opts.beforeLabel ?? "committed";
  const afterLabel = opts.afterLabel ?? "live";
  const title = opts.title ?? "Upstream summary drift";
  const lines = [`## ${title}`, ""];
  if (!anyContentOrKeyChange(reports)) {
    const names = reports.map((r) => r.name).join(", ");
    lines.push(`No drift across ${names} summaries (${beforeLabel} vs ${afterLabel}).`);
    return lines.join("\n");
  }
  lines.push(`| Domain | ${beforeLabel} | ${afterLabel} | +new | -removed | ~modified |`);
  lines.push("| --- | ---: | ---: | ---: | ---: | ---: |");
  for (const r of reports) {
    lines.push(
      `| ${r.name} | ${r.beforeCount} | ${r.afterCount} | ${r.added.length} | ${r.removed.length} | ${r.modified?.length ?? 0} |`,
    );
  }
  for (const r of reports) {
    const mod = r.modified ?? [];
    if (r.added.length === 0 && r.removed.length === 0 && mod.length === 0) continue;
    lines.push("", `### ${r.name} key changes`);
    if (r.added.length > 0) {
      lines.push("", "**New:**", "");
      lines.push("```");
      lines.push(r.added.slice(0, 40).join("\n"));
      if (r.added.length > 40) lines.push(`… (+${r.added.length - 40} more)`);
      lines.push("```");
    }
    if (r.removed.length > 0) {
      lines.push("", "**Removed:**", "");
      lines.push("```");
      lines.push(r.removed.slice(0, 40).join("\n"));
      if (r.removed.length > 40) lines.push(`… (-${r.removed.length - 40} more)`);
      lines.push("```");
    }
    if (mod.length > 0) {
      lines.push("", "**Modified in-place:**", "");
      lines.push("```");
      lines.push(mod.slice(0, 40).join("\n"));
      if (mod.length > 40) lines.push(`… (~${mod.length - 40} more)`);
      lines.push("```");
    }
  }
  return lines.join("\n");
}

/**
 * Fetch summary catalogs from data.stfc.space concurrently with retries and atomic writes.
 * @param {string} destDir
 * @param {{ domains?: typeof DOMAINS, concurrency?: number }} [opts]
 */
export async function fetchLiveSummaries(destDir, opts = {}) {
  const domains = opts.domains ?? DOMAINS;
  const concurrency = opts.concurrency ?? 4;
  fs.mkdirSync(destDir, { recursive: true });
  await runConcurrentPool(domains, concurrency, async (domain) => {
    const url = `${BASE_URL}/${domain.segment}/summary.json`;
    const res = await fetchJsonResilient(url, {
      userAgent: "Kobayashi-upstream-drift-check/2.0",
    });
    const dest = path.join(destDir, domain.file);
    await atomicWriteFile(dest, `${JSON.stringify(res.data)}\n`);
  });
}

export const DRIFT_REMEDIATION =
  "Upstream catalog drift detected. Merge the open `chore(data): automated stfc.space refresh` bot PR or run `cargo xtask check-upstream-drift` locally and refresh.";
