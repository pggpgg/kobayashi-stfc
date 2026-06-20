#!/usr/bin/env node

import { spawn } from "node:child_process";
import net from "node:net";
import { mkdtemp, readFile, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

function parseBundleDir(argv) {
  if (argv.length !== 2 || argv[0] !== "--bundle-dir") {
    throw new Error("usage: smoke-release-bundle.mjs --bundle-dir <dir>");
  }
  return path.resolve(argv[1]);
}

async function reservePort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  const port = typeof address === "object" && address ? address.port : null;
  await new Promise((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
  if (!port) throw new Error("failed to reserve smoke-test port");
  return port;
}

async function waitForJson(url, child, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    if (child.exitCode != null) {
      throw new Error(`bundle server exited early with code ${child.exitCode}`);
    }
    try {
      const response = await fetch(url);
      if (response.ok) return response.json();
      lastError = new Error(`${response.status} ${response.statusText}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 150));
  }
  throw new Error(`timed out waiting for ${url}: ${lastError}`);
}

async function assertNonEmptyCollection(url, key, child) {
  const value = await waitForJson(url, child);
  if (!Array.isArray(value[key]) || value[key].length === 0) {
    throw new Error(`${url} did not return a non-empty ${key} array`);
  }
}

async function main() {
  const bundleDir = parseBundleDir(process.argv.slice(2));
  const binaryName = process.platform === "win32" ? "kobayashi.exe" : "kobayashi";
  const binary = path.join(bundleDir, binaryName);
  const binaryInfo = await stat(binary).catch(() => null);
  if (!binaryInfo?.isFile()) throw new Error(`bundle binary missing: ${binary}`);

  const outsideCwd = await mkdtemp(path.join(os.tmpdir(), "kobayashi-release-smoke-"));
  const port = await reservePort();
  const baseUrl = `http://127.0.0.1:${port}`;
  let stderr = "";
  const child = spawn(binary, ["serve"], {
    cwd: outsideCwd,
    env: { ...process.env, KOBAYASHI_BIND: `127.0.0.1:${port}` },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });

  try {
    const health = await waitForJson(`${baseUrl}/api/health`, child);
    if (health.status !== "ok") throw new Error(`unexpected health payload: ${JSON.stringify(health)}`);
    await assertNonEmptyCollection(`${baseUrl}/api/officers`, "officers", child);
    await assertNonEmptyCollection(`${baseUrl}/api/ships`, "ships", child);
    await assertNonEmptyCollection(`${baseUrl}/api/hostiles`, "hostiles", child);

    const coverage = await waitForJson(`${baseUrl}/api/mechanics/coverage`, child);
    if (coverage.status !== "ok" || coverage.lcars_officers_files <= 0) {
      throw new Error(`mechanics coverage did not load bundled assets: ${JSON.stringify(coverage)}`);
    }

    const page = await fetch(baseUrl);
    if (!page.ok || !(await page.text()).includes("Kobayashi")) {
      throw new Error("bundled SPA root did not load");
    }
    const indexHtml = await readFile(path.join(bundleDir, "frontend", "dist", "index.html"), "utf8");
    const asset = indexHtml.match(/["'](\/assets\/[^"']+)["']/)?.[1];
    if (!asset || !(await fetch(`${baseUrl}${asset}`)).ok) {
      throw new Error("bundled SPA asset did not load");
    }
  } catch (error) {
    throw new Error(`${error.message}\nserver stderr:\n${stderr}`);
  } finally {
    child.kill("SIGTERM");
    await Promise.race([
      new Promise((resolve) => child.once("exit", resolve)),
      new Promise((resolve) => setTimeout(resolve, 5_000)),
    ]);
    if (child.exitCode == null) child.kill("SIGKILL");
    await rm(outsideCwd, { recursive: true, force: true });
  }
  process.stdout.write(`Self-contained bundle smoke test passed (${bundleDir})\n`);
}

await main();
