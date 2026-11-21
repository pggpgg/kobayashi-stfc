/**
 * Declarative DAG orchestrator for Kobayashi's data refresh pipeline.
 *
 * Replaces ad-hoc `shell: true` command chains with:
 * - Explicit stage definitions (`id`, `group`, `dependsOn`, `softFailure`, `optional`)
 * - Deterministic topological ordering with `--stages` / `--skip` filtering
 * - Direct `spawnSync(program, args)` execution (no shell quoting hazards)
 * - Atomic stdout capture when a stage writes generated markdown/json files
 * - Structured per-stage timing, soft-failure classification, and Markdown/JSON reporting
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { atomicWriteFile } from "./stfcspace_http.mjs";

/**
 * Topologically sort selected stages while preserving declaration order as tie-breaker.
 * Throws if an unknown dependency or cycle exists.
 *
 * @template {{ id: string, dependsOn?: string[] }} T
 * @param {T[]} stages
 * @returns {T[]}
 */
export function resolveStageOrder(stages) {
  const byId = new Map();
  for (const stage of stages) {
    if (byId.has(stage.id)) {
      throw new Error(`Duplicate pipeline stage id: ${stage.id}`);
    }
    byId.set(stage.id, stage);
  }

  const visited = new Set();
  const visiting = new Set();
  const ordered = [];

  function visit(id) {
    if (visited.has(id)) return;
    if (visiting.has(id)) {
      throw new Error(`Cycle detected in pipeline DAG involving stage '${id}'`);
    }
    const stage = byId.get(id);
    if (!stage) {
      throw new Error(`Unknown dependency stage id: ${id}`);
    }
    visiting.add(id);
    for (const dep of stage.dependsOn ?? []) {
      if (byId.has(dep)) {
        visit(dep);
      }
    }
    visiting.delete(id);
    visited.add(id);
    ordered.push(stage);
  }

  for (const stage of stages) {
    visit(stage.id);
  }
  return ordered;
}

/**
 * Select stages matching active groups and optional explicit `--stages` / `--skip` lists.
 *
 * @template {{ id: string, group: string, dependsOn?: string[] }} T
 * @param {T[]} allStages
 * @param {{
 *   groups: Set<string>,
 *   onlyStages?: string[] | null,
 *   skipStages?: string[] | null,
 * }} opts
 * @returns {T[]}
 */
export function selectStages(allStages, opts) {
  const { groups, onlyStages = null, skipStages = null } = opts;
  const knownIds = new Set(allStages.map((s) => s.id));
  const skipSet = new Set(skipStages ?? []);

  for (const id of skipSet) {
    if (!knownIds.has(id)) {
      throw new Error(`Unknown stage in --skip: '${id}' (valid: ${[...knownIds].join(", ")})`);
    }
  }

  let candidates;
  if (onlyStages && onlyStages.length > 0) {
    const onlySet = new Set(onlyStages);
    for (const id of onlySet) {
      if (!knownIds.has(id)) {
        throw new Error(`Unknown stage in --stages: '${id}' (valid: ${[...knownIds].join(", ")})`);
      }
    }
    candidates = allStages.filter((s) => onlySet.has(s.id) && !skipSet.has(s.id));
  } else {
    candidates = allStages.filter((s) => groups.has(s.group) && !skipSet.has(s.id));
  }

  return resolveStageOrder(candidates);
}

/**
 * Default command runner using direct `spawnSync(program, args)` without shell interpretation.
 * Supports optional `captureStdoutTo` (atomic write) for generators that print to stdout.
 */
export async function defaultExecuteCommand(spec, cwd) {
  const { program, args = [], env = null, captureStdoutTo = null } = spec;
  const mergedEnv = env ? { ...process.env, ...env } : process.env;

  if (captureStdoutTo) {
    const res = spawnSync(program, args, {
      cwd,
      encoding: "utf8",
      env: mergedEnv,
      maxBuffer: 64 * 1024 * 1024,
      stdio: ["inherit", "pipe", "inherit"],
    });
    if (res.error) {
      return { exitCode: 1, error: res.error.message };
    }
    if (res.status !== 0) {
      return { exitCode: res.status ?? 1, error: `exited with code ${res.status}` };
    }
    const destPath = path.isAbsolute(captureStdoutTo)
      ? captureStdoutTo
      : path.join(cwd, captureStdoutTo);
    await atomicWriteFile(destPath, res.stdout ?? "");
    return { exitCode: 0, error: null };
  }

  const res = spawnSync(program, args, {
    cwd,
    env: mergedEnv,
    stdio: "inherit",
  });
  if (res.error) {
    return { exitCode: 1, error: res.error.message };
  }
  return {
    exitCode: res.status ?? (res.signal ? 1 : 0),
    error: res.status === 0 ? null : `exited with code ${res.status ?? res.signal}`,
  };
}

/**
 * Execute an ordered list of pipeline stages with dry-run, precondition gating,
 * soft-failure continuation, and structured telemetry.
 *
 * @param {Array<{
 *   id: string,
 *   group: string,
 *   title: string,
 *   dependsOn?: string[],
 *   optional?: boolean,
 *   softFailure?: boolean,
 *   checkPrecondition?: () => { ok: boolean, reason?: string, hardFail?: boolean },
 *   buildCommand?: () => { program: string, args?: string[], env?: Record<string, string>, captureStdoutTo?: string },
 *   runCustom?: () => Promise<{ ok: boolean, note?: string, error?: string }>,
 * }>} orderedStages
 * @param {{
 *   cwd: string,
 *   dryRun?: boolean,
 *   continueOnSoftFailure?: boolean,
 *   executeCommand?: typeof defaultExecuteCommand,
 *   logger?: (msg: string) => void,
 * }} opts
 */
export async function runPipelineDag(orderedStages, opts) {
  const {
    cwd,
    dryRun = false,
    continueOnSoftFailure = false,
    executeCommand = defaultExecuteCommand,
    logger = (msg) => console.log(msg),
  } = opts;

  const startedAt = new Date().toISOString();
  const startedMs = Date.now();
  const results = [];
  const failedHardIds = new Set();

  for (const stage of orderedStages) {
    const blockedBy = (stage.dependsOn ?? []).filter((dep) => failedHardIds.has(dep));
    if (blockedBy.length > 0) {
      results.push({
        id: stage.id,
        group: stage.group,
        title: stage.title,
        status: "skipped",
        duration_ms: 0,
        note: `blocked by failed dependency: ${blockedBy.join(", ")}`,
      });
      continue;
    }

    if (stage.checkPrecondition) {
      const pre = stage.checkPrecondition();
      if (!pre.ok) {
        if (pre.hardFail && !dryRun) {
          failedHardIds.add(stage.id);
          results.push({
            id: stage.id,
            group: stage.group,
            title: stage.title,
            status: "failed",
            duration_ms: 0,
            error: pre.reason ?? "precondition failed",
          });
          break;
        }
        logger(`\n── [${stage.id}] ${stage.title} ──\n(skip: ${pre.reason ?? "precondition unmet"})\n`);
        results.push({
          id: stage.id,
          group: stage.group,
          title: stage.title,
          status: "skipped",
          duration_ms: 0,
          note: pre.reason ?? "precondition unmet",
        });
        continue;
      }
    }

    const cmdSpec = stage.buildCommand ? stage.buildCommand() : null;
    const cmdText = cmdSpec
      ? `${cmdSpec.program} ${(cmdSpec.args ?? []).join(" ")}${cmdSpec.captureStdoutTo ? ` > ${cmdSpec.captureStdoutTo}` : ""}`.trim()
      : "(built-in stage)";

    if (dryRun) {
      logger(`[dry-run] ${stage.id} (${stage.group}): ${stage.title} → ${cmdText}`);
      results.push({
        id: stage.id,
        group: stage.group,
        title: stage.title,
        status: "dry_run",
        duration_ms: 0,
        command: cmdText,
      });
      continue;
    }

    logger(`\n── [${stage.id}] ${stage.title} ──\n${cmdText}\n`);
    const stageStart = Date.now();

    let exitCode = 0;
    let error = null;
    let note = null;

    if (stage.runCustom) {
      try {
        const customRes = await stage.runCustom();
        exitCode = customRes.ok ? 0 : 1;
        error = customRes.error ?? null;
        note = customRes.note ?? null;
      } catch (err) {
        exitCode = 1;
        error = err instanceof Error ? err.message : String(err);
      }
    } else if (cmdSpec) {
      const execRes = await executeCommand(cmdSpec, cwd);
      exitCode = execRes.exitCode;
      error = execRes.error;
    }

    const durationMs = Date.now() - stageStart;

    if (exitCode === 0) {
      results.push({
        id: stage.id,
        group: stage.group,
        title: stage.title,
        status: "ok",
        duration_ms: durationMs,
        command: cmdText,
        ...(note ? { note } : {}),
      });
      continue;
    }

    if (stage.optional || (stage.softFailure && continueOnSoftFailure)) {
      const kind = stage.softFailure ? "soft_failed" : "optional_failed";
      logger(
        `(${kind}: [${stage.id}] exited with ${exitCode}${error ? ` — ${error}` : ""}; continuing)\n`,
      );
      results.push({
        id: stage.id,
        group: stage.group,
        title: stage.title,
        status: kind,
        duration_ms: durationMs,
        command: cmdText,
        error: error ?? `exit ${exitCode}`,
      });
      continue;
    }

    failedHardIds.add(stage.id);
    results.push({
      id: stage.id,
      group: stage.group,
      title: stage.title,
      status: "failed",
      duration_ms: durationMs,
      command: cmdText,
      error: error ?? `exit ${exitCode}`,
    });
    break;
  }

  const totalDurationMs = Date.now() - startedMs;
  const hardFailures = results.filter((r) => r.status === "failed");
  const softFailures = results.filter((r) => r.status === "soft_failed");

  return {
    schema_version: 1,
    started_at: startedAt,
    finished_at: new Date().toISOString(),
    duration_ms: totalDurationMs,
    dry_run: dryRun,
    ok: hardFailures.length === 0,
    needs_triage: softFailures.length > 0,
    hard_failure_count: hardFailures.length,
    soft_failure_count: softFailures.length,
    stages: results,
  };
}

/**
 * Format a pipeline DAG run report as a compact Markdown table for PR bodies and CI job summaries.
 * @param {Awaited<ReturnType<typeof runPipelineDag>>} report
 */
export function formatPipelineReportMarkdown(report) {
  const icon = (status) => {
    switch (status) {
      case "ok":
        return "✅ ok";
      case "skipped":
        return "⏭️ skipped";
      case "dry_run":
        return "🔎 dry-run";
      case "soft_failed":
        return "⚠️ coverage gap";
      case "optional_failed":
        return "⚠️ optional failed";
      default:
        return "❌ failed";
    }
  };

  const lines = [
    "## Data pipeline stages",
    "",
    "| Stage | Status | Duration | Notes |",
    "| --- | --- | ---: | --- |",
  ];
  for (const s of report.stages) {
    const dur = s.duration_ms >= 1000 ? `${(s.duration_ms / 1000).toFixed(1)}s` : `${s.duration_ms}ms`;
    const detail = s.error ?? s.note ?? "";
    lines.push(`| \`${s.id}\` | ${icon(s.status)} | ${dur} | ${detail} |`);
  }
  lines.push("");
  lines.push(
    `Total pipeline time: **${(report.duration_ms / 1000).toFixed(1)}s** · Hard failures: **${report.hard_failure_count}** · Soft coverage gaps: **${report.soft_failure_count}**`,
  );
  return lines.join("\n");
}

/**
 * Helper to check whether a directory has at least one `.json` file.
 * @param {string} dir
 */
export function countJsonFilesInDir(dir) {
  if (!fs.existsSync(dir)) return 0;
  return fs.readdirSync(dir).filter((f) => f.endsWith(".json")).length;
}
