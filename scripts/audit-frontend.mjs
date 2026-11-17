import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const advisory = "https://github.com/advisories/GHSA-fc5q-39xc-vjpf";

const production = spawnSync(npm, ["audit", "--omit=dev", "--audit-level=high"], {
  stdio: "inherit",
  shell: process.platform === "win32",
});
if (production.status !== 0) process.exit(production.status || 1);

const audit = spawnSync(npm, ["audit", "--audit-level=high", "--json"], {
  encoding: "utf8",
  shell: process.platform === "win32",
});
if (audit.stderr) process.stderr.write(audit.stderr);
if (audit.stdout) process.stdout.write(audit.stdout);
if (audit.status === 0) process.exit(0);
if (audit.status !== 1 || audit.error) process.exit(1);

const report = JSON.parse(audit.stdout);
if (report.auditReportVersion !== 2 || report.error || !report.vulnerabilities) {
  process.exit(1);
}

const { packages } = JSON.parse(readFileSync("package-lock.json", "utf8"));
const blocking = new Map(
  Object.entries(report.vulnerabilities).filter(([, finding]) =>
    ["high", "critical"].includes(finding.severity),
  ),
);
function isExceptedAdvisory(cause) {
  return cause?.name === "picomatch" && cause.dependency === "picomatch" &&
    cause.severity === "high" && cause.url === advisory;
}
if (!blocking.get("picomatch")?.via?.some(isExceptedAdvisory)) process.exit(1);

// Remove this exception once the lockfile can use an upstream patched version.
// npm also reports high-severity findings for packages affected transitively.
for (const [name, finding] of blocking) {
  if (finding.severity !== "high" ||
      !finding.nodes?.length ||
      !finding.nodes.every((path) => packages[path]?.dev === true) ||
      !finding.via.every((cause) => {
        if (typeof cause === "string") return Object.hasOwn(report.vulnerabilities, cause);
        if (["info", "low", "moderate"].includes(cause?.severity)) return true;
        return name === "picomatch" && isExceptedAdvisory(cause);
      })) process.exit(1);
}

// npm can report a cycle between a test runner and its coverage plugin.
const allowed = new Set(["picomatch"]);
for (let changed = true; changed;) {
  changed = false;
  for (const [name, finding] of blocking) {
    if (allowed.has(name) || !finding.via.some((cause) => allowed.has(cause))) continue;
    allowed.add(name);
    changed = true;
  }
}

if (allowed.size !== blocking.size) process.exit(1);
console.warn(`Temporary development-only audit exception applied for ${advisory}`);
