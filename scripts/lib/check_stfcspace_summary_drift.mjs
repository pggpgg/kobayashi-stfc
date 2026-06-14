import fs from "node:fs";
import path from "node:path";

const BASE_URL = "https://data.stfc.space";

/** Combat-relevant summary catalogs tracked for upstream drift (ROADMAP task 14). */
export const DOMAINS = [
  { name: "ship", segment: "ship", file: "summary-ship.json", key: (row) => String(row.id) },
  {
    name: "hostile",
    segment: "hostile",
    file: "summary-hostile.json",
    key: (row) => `${row.id}:${row.level}`,
  },
  { name: "research", segment: "research", file: "summary-research.json", key: (row) => String(row.id) },
];

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
  for (const row of data) {
    keys.add(domain.key(row));
  }
  if (keys.size !== data.length) {
    throw new Error(`${domain.file}: duplicate keys after normalization`);
  }
  return { count: data.length, keys };
}

/**
 * @param {string} beforeDir
 * @param {string} afterDir
 * @returns {Array<{ name: string, file: string, beforeCount: number, afterCount: number, added: string[], removed: string[], drifted: boolean }>}
 */
export function compareSummaryDirs(beforeDir, afterDir) {
  const reports = [];
  for (const domain of DOMAINS) {
    const before = loadDomainKeys(beforeDir, domain);
    const after = loadDomainKeys(afterDir, domain);
    const added = [...after.keys].filter((k) => !before.keys.has(k)).sort();
    const removed = [...before.keys].filter((k) => !after.keys.has(k)).sort();
    const drifted = before.count !== after.count || added.length > 0 || removed.length > 0;
    reports.push({
      name: domain.name,
      file: domain.file,
      beforeCount: before.count,
      afterCount: after.count,
      added,
      removed,
      drifted,
    });
  }
  return reports;
}

/** @param {ReturnType<typeof compareSummaryDirs>} reports */
export function anyDrift(reports) {
  return reports.some((r) => r.drifted);
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
    if (!r.drifted) {
      lines.push(`${r.name}: no drift (${r.beforeCount} rows)`);
      continue;
    }
    lines.push(
      `${r.name}: drift — count ${beforeLabel} ${r.beforeCount} → ${afterLabel} ${r.afterCount}; +${r.added.length} new, -${r.removed.length} removed`,
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
  if (!anyDrift(reports)) {
    lines.push(`No drift across ship, hostile, and research summaries (${beforeLabel} vs ${afterLabel}).`);
    return lines.join("\n");
  }
  lines.push(`| Domain | ${beforeLabel} | ${afterLabel} | +new | -removed |`);
  lines.push("| --- | ---: | ---: | ---: | ---: |");
  for (const r of reports) {
    lines.push(
      `| ${r.name} | ${r.beforeCount} | ${r.afterCount} | ${r.added.length} | ${r.removed.length} |`,
    );
  }
  for (const r of reports) {
    if (r.added.length === 0 && r.removed.length === 0) continue;
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
  }
  return lines.join("\n");
}

/**
 * Fetch ship/hostile/research summaries from data.stfc.space into `destDir`.
 * @param {string} destDir
 */
export async function fetchLiveSummaries(destDir) {
  fs.mkdirSync(destDir, { recursive: true });
  for (const domain of DOMAINS) {
    const url = `${BASE_URL}/${domain.segment}/summary.json`;
    const res = await fetch(url, {
      headers: { Accept: "application/json", "User-Agent": "Kobayashi-upstream-drift-check/1.0" },
    });
    if (!res.ok) {
      throw new Error(`${url} -> HTTP ${res.status}`);
    }
    const data = await res.json();
    const dest = path.join(destDir, domain.file);
    fs.writeFileSync(dest, `${JSON.stringify(data)}\n`, "utf8");
  }
}

export const DRIFT_REMEDIATION =
  "Upstream catalog drift detected. Merge the open `chore(data): automated stfc.space refresh` bot PR or run `cargo xtask check-upstream-drift` locally and refresh.";
