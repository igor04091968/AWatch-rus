#!/usr/bin/env node
import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = "configs/operational-maturity-contract.json";

function readText(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), "utf8");
}

function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

function exists(relativePath) {
  return fs.existsSync(path.join(root, relativePath));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function getPath(value, dotted) {
  return dotted.split(".").reduce((current, key) => {
    if (current === null || current === undefined) return undefined;
    return current[key];
  }, value);
}

function result(name, fn) {
  const started = Date.now();
  try {
    const details = fn() || {};
    return { name, ok: true, duration_ms: Date.now() - started, ...details };
  } catch (error) {
    return { name, ok: false, duration_ms: Date.now() - started, error: error.message };
  }
}

async function asyncResult(name, fn) {
  const started = Date.now();
  try {
    const details = (await fn()) || {};
    return { name, ok: true, duration_ms: Date.now() - started, ...details };
  } catch (error) {
    return { name, ok: false, duration_ms: Date.now() - started, error: error.message };
  }
}

function walk(dir, predicate = () => true) {
  const base = path.join(root, dir);
  if (!fs.existsSync(base)) return [];
  const files = [];
  for (const entry of fs.readdirSync(base, { withFileTypes: true })) {
    const absolute = path.join(base, entry.name);
    const relative = path.relative(root, absolute).replaceAll(path.sep, "/");
    if (entry.isDirectory()) {
      files.push(...walk(relative, predicate));
    } else if (predicate(relative)) {
      files.push(relative);
    }
  }
  return files.sort();
}

function parseArgs() {
  const args = new Set(process.argv.slice(2));
  return {
    json: args.has("--json"),
    live: args.has("--live"),
    failOnDuplicateDebt: args.has("--fail-on-duplicate-debt"),
  };
}

function randomLocalPort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = typeof address === "object" && address ? address.port : 0;
      server.close(() => resolve(port));
    });
  });
}

function fixtureResponse(fixture) {
  if (fixture.contentType === "text/plain") {
    return readText(fixture.fixture);
  }
  return JSON.stringify(readJson(fixture.fixture));
}

async function startFixtureServer(manifest) {
  const fixturesByPath = new Map();
  for (const fixture of manifest.integrationFixtures) {
    fixturesByPath.set(fixture.path, fixture);
  }
  fixturesByPath.set("/metrics", {
    fixture: manifest.observability.metricsFixture,
    status: 200,
    contentType: "text/plain",
  });

  const sockets = new Set();
  const server = http.createServer((request, response) => {
    const parsed = new URL(request.url || "/", "http://127.0.0.1");
    const fixture = fixturesByPath.get(`${parsed.pathname}${parsed.search}`) || fixturesByPath.get(parsed.pathname);
    if (!fixture) {
      response.statusCode = 404;
      response.setHeader("Content-Type", "application/json; charset=utf-8");
      response.end(JSON.stringify({ error: "not_found", path: parsed.pathname }));
      return;
    }
    response.statusCode = fixture.status || 200;
    response.setHeader("Content-Type", fixture.contentType === "text/plain" ? "text/plain; charset=utf-8" : "application/json; charset=utf-8");
    response.setHeader("X-Request-Id", request.headers["x-request-id"] || "operational-maturity-fixture");
    response.setHeader("X-Correlation-Id", request.headers["x-request-id"] || "operational-maturity-fixture");
    response.end(fixtureResponse(fixture));
  });
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });

  const port = await randomLocalPort();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  return {
    port,
    close: () =>
      new Promise((resolve) => {
        for (const socket of sockets) socket.destroy();
        server.close(resolve);
      }),
  };
}

async function fetchWithTimeout(url, timeoutMs, options = {}) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  const started = Date.now();
  try {
    const response = await fetch(url, { ...options, signal: controller.signal });
    const text = await response.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = null;
    }
    return {
      status: response.status,
      ok: response.ok,
      headers: response.headers,
      text,
      json,
      elapsed_ms: Date.now() - started,
    };
  } finally {
    clearTimeout(timeout);
  }
}

function checkApiCompatibility(manifest) {
  const spec = readJson(manifest.apiCompatibility.openapiPath);
  const missing = [];
  for (const [apiPath, methods] of Object.entries(manifest.apiCompatibility.requiredOpenApiPaths)) {
    for (const method of methods) {
      if (!spec.paths?.[apiPath]?.[method]) missing.push(`${method.toUpperCase()} ${apiPath}`);
    }
  }
  for (const schema of manifest.apiCompatibility.requiredSchemas) {
    if (!spec.components?.schemas?.[schema]) missing.push(`schema ${schema}`);
  }
  const runtimeSource = readText(manifest.apiCompatibility.runtimeSourcePath);
  for (const endpoint of manifest.apiCompatibility.runtimeEndpoints) {
    if (!runtimeSource.includes(`"${endpoint}"`)) missing.push(`runtime endpoint ${endpoint}`);
  }
  assert(missing.length === 0, `missing API compatibility anchors: ${missing.join(", ")}`);
  return {
    required_paths: Object.keys(manifest.apiCompatibility.requiredOpenApiPaths).length,
    required_schemas: manifest.apiCompatibility.requiredSchemas.length,
    runtime_endpoints: manifest.apiCompatibility.runtimeEndpoints.length,
  };
}

function validateFixtureContract(manifest) {
  const checked = [];
  for (const fixture of manifest.integrationFixtures) {
    const payload = readJson(fixture.fixture);
    for (const field of fixture.requiredFields || []) {
      assert(getPath(payload, field) !== undefined, `${fixture.fixture} missing field ${field}`);
    }
    for (const [field, expected] of Object.entries(fixture.equals || {})) {
      assert(getPath(payload, field) === expected, `${fixture.fixture} expected ${field}=${JSON.stringify(expected)}`);
    }
    checked.push(fixture.path);
  }
  return { fixtures: checked.length };
}

async function checkIntegrationHarness(manifest) {
  const server = await startFixtureServer(manifest);
  const checked = [];
  try {
    for (const fixture of manifest.integrationFixtures) {
      const response = await fetchWithTimeout(`http://127.0.0.1:${server.port}${fixture.path}`, 1500, {
        headers: { "X-Request-Id": "operational-maturity" },
      });
      assert(response.status === fixture.status, `${fixture.path} status ${response.status}, expected ${fixture.status}`);
      if (fixture.contentType === "application/json") {
        assert(response.json && typeof response.json === "object", `${fixture.path} did not return JSON`);
      }
      assert(response.headers.get("x-correlation-id") === "operational-maturity", `${fixture.path} did not echo correlation id`);
      checked.push({ path: fixture.path, elapsed_ms: response.elapsed_ms });
    }
    return { endpoints: checked.length, max_elapsed_ms: Math.max(...checked.map((item) => item.elapsed_ms)) };
  } finally {
    await server.close();
  }
}

async function startFaultServer() {
  const sockets = new Set();
  const server = http.createServer((request, response) => {
    const parsed = new URL(request.url || "/", "http://127.0.0.1");
    if (parsed.pathname === "/fault/503") {
      response.statusCode = 503;
      response.end("unavailable");
      return;
    }
    if (parsed.pathname === "/fault/slow") {
      setTimeout(() => {
        response.statusCode = 200;
        response.end("slow-ok");
      }, 1200);
      return;
    }
    if (parsed.pathname === "/fault/reset") {
      request.socket.destroy();
      return;
    }
    response.statusCode = 200;
    response.end("ok");
  });
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });
  const port = await randomLocalPort();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  return {
    port,
    close: () =>
      new Promise((resolve) => {
        for (const socket of sockets) socket.destroy();
        server.close(resolve);
      }),
  };
}

async function checkFaultInjection(manifest) {
  const server = await startFaultServer();
  const timeoutMs = manifest.faultInjection.clientTimeoutMs;
  const maxClassifyMs = manifest.faultInjection.maxFailureClassificationMs;
  const classifications = [];
  try {
    const unavailable = await fetchWithTimeout(`http://127.0.0.1:${server.port}/fault/503`, timeoutMs);
    assert(unavailable.status === 503, "503 fault was not classified as HTTP 503");
    classifications.push("http_503");

    const slowStart = Date.now();
    try {
      await fetchWithTimeout(`http://127.0.0.1:${server.port}/fault/slow`, timeoutMs);
      throw new Error("slow fault did not timeout");
    } catch (error) {
      const elapsed = Date.now() - slowStart;
      assert(elapsed < maxClassifyMs, `timeout classification exceeded ${maxClassifyMs}ms`);
      classifications.push("timeout");
    }

    try {
      await fetchWithTimeout(`http://127.0.0.1:${server.port}/fault/reset`, timeoutMs);
      throw new Error("connection reset fault did not fail");
    } catch {
      classifications.push("connection_reset");
    }
    return { classifications };
  } finally {
    await server.close();
  }
}

function percentile(values, pct) {
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.ceil((pct / 100) * sorted.length) - 1);
  return sorted[index] || 0;
}

async function mapLimit(items, limit, fn) {
  const results = [];
  let next = 0;
  async function worker() {
    while (next < items.length) {
      const index = next;
      next += 1;
      results[index] = await fn(items[index], index);
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  return results;
}

async function checkBoundedLoad(manifest) {
  const server = await startFixtureServer(manifest);
  const cfg = manifest.boundedLoad;
  const heapBefore = process.memoryUsage().heapUsed;
  try {
    const work = Array.from({ length: cfg.requests }, (_, index) => cfg.paths[index % cfg.paths.length]);
    const responses = await mapLimit(work, cfg.concurrency, async (apiPath) => {
      const response = await fetchWithTimeout(`http://127.0.0.1:${server.port}${apiPath}`, 1500);
      assert(response.status >= 200 && response.status < 300, `${apiPath} failed with ${response.status}`);
      return response.elapsed_ms;
    });
    const heapGrowth = Math.max(0, process.memoryUsage().heapUsed - heapBefore);
    const p95 = percentile(responses, 95);
    assert(p95 <= cfg.p95MaxMs, `bounded load p95 ${p95}ms exceeds ${cfg.p95MaxMs}ms`);
    assert(heapGrowth <= cfg.heapGrowthMaxBytes, `heap growth ${heapGrowth} exceeds ${cfg.heapGrowthMaxBytes}`);
    return { requests: cfg.requests, concurrency: cfg.concurrency, p95_ms: p95, heap_growth_bytes: heapGrowth };
  } finally {
    await server.close();
  }
}

function validateJsonAndYaml(manifest) {
  const checked = [];
  for (const file of manifest.configValidation.jsonFiles) {
    assert(exists(file), `missing JSON config/fixture ${file}`);
    readJson(file);
    checked.push(file);
  }
  for (const file of manifest.configValidation.yamlFiles) {
    assert(exists(file), `missing YAML config ${file}`);
    const text = readText(file);
    assert(!text.includes("\t"), `${file} contains tabs`);
    if (!/(\.example\.ya?ml|example\.yml)$/i.test(file)) {
      assert(!/password:\s*(change-me|changeme|password)\b/i.test(text), `${file} contains unsafe password placeholder`);
    }
    checked.push(file);
  }
  return { files: checked.length };
}

function validateSystemdUnits(manifest) {
  const files = manifest.configValidation.systemdDirs.flatMap((dir) =>
    walk(dir, (file) => file.endsWith(".service") || file.endsWith(".timer")),
  );
  assert(files.length > 0, "no systemd files found");
  const findings = [];
  for (const file of files) {
    const text = readText(file);
    if (!text.includes("[Unit]")) findings.push(`${file}: missing [Unit]`);
    if (file.endsWith(".service")) {
      if (!text.includes("[Service]")) findings.push(`${file}: missing [Service]`);
      if (!/^ExecStart=/m.test(text)) findings.push(`${file}: missing ExecStart`);
      if (/^Type=oneshot$/m.test(text) && /^Restart=always$/m.test(text)) {
        findings.push(`${file}: oneshot service must not use Restart=always`);
      }
    }
    if (file.endsWith(".timer")) {
      if (!text.includes("[Timer]")) findings.push(`${file}: missing [Timer]`);
      if (!/^Unit=.*\.service$/m.test(text)) findings.push(`${file}: missing service Unit`);
      if (!text.includes("[Install]")) findings.push(`${file}: missing [Install]`);
    }
  }
  assert(findings.length === 0, findings.join("; "));
  return { files: files.length };
}

function validateClickHouseMigrations(manifest) {
  const dir = manifest.configValidation.clickhouseInitDir;
  const files = walk(dir, (file) => file.endsWith(".sql"));
  assert(files.length > 0, "no ClickHouse init SQL files found");
  const findings = [];
  const order = files.map((file) => path.basename(file));
  const sorted = [...order].sort();
  if (order.join("|") !== sorted.join("|")) findings.push("ClickHouse init filenames are not sorted");
  for (const file of files) {
    const text = readText(file);
    if (/\b(DROP|TRUNCATE)\s+(DATABASE|TABLE)\b/i.test(text)) {
      findings.push(`${file}: destructive DDL is not allowed in init validation`);
    }
    const createStatements = text.match(/\bCREATE\s+(DATABASE|TABLE|VIEW|OR\s+REPLACE\s+VIEW)\b/gi) || [];
    for (const create of createStatements) {
      if (/CREATE\s+(DATABASE|TABLE)/i.test(create) && !/IF\s+NOT\s+EXISTS/i.test(text.slice(text.indexOf(create), text.indexOf(create) + 140))) {
        findings.push(`${file}: CREATE DATABASE/TABLE must use IF NOT EXISTS`);
      }
      if (/CREATE\s+VIEW/i.test(create) && !/CREATE\s+(OR\s+REPLACE\s+)?VIEW/i.test(create)) {
        findings.push(`${file}: CREATE VIEW must be OR REPLACE or otherwise idempotent`);
      }
    }
  }
  assert(findings.length === 0, findings.join("; "));
  return { files: files.length };
}

function checkObservability(manifest) {
  const metricsText = readText(manifest.observability.metricsFixture);
  const sourceText = readText(manifest.observability.sourcePath);
  const missing = [];
  for (const metric of manifest.observability.requiredMetrics) {
    if (!metricsText.includes(metric)) missing.push(`fixture metric ${metric}`);
    if (!sourceText.includes(metric)) missing.push(`source metric ${metric}`);
  }
  const hardeningSmoke = readText("scripts/awatch-production-hardening-smoke.mjs");
  for (const header of manifest.observability.diagnosticHeaders) {
    if (!hardeningSmoke.includes(header)) missing.push(`diagnostic header ${header}`);
  }
  assert(missing.length === 0, `missing observability anchors: ${missing.join(", ")}`);
  return { metrics: manifest.observability.requiredMetrics.length };
}

async function checkLive(manifest) {
  const baseUrl = (process.env.AWATCH_OPS_LIVE_URL || "").replace(/\/+$/, "");
  assert(baseUrl, "AWATCH_OPS_LIVE_URL is required for --live");
  const checked = [];
  for (const endpoint of manifest.apiCompatibility.runtimeEndpoints) {
    const response = await fetchWithTimeout(`${baseUrl}${endpoint}`, 3000, {
      headers: { "X-Request-Id": "operational-maturity-live" },
    });
    assert([200, 503].includes(response.status), `${endpoint} returned uncontrolled status ${response.status}`);
    checked.push({ endpoint, status: response.status, elapsed_ms: response.elapsed_ms });
  }
  return { baseUrl, checked };
}

async function main() {
  const args = parseArgs();
  const manifest = readJson(manifestPath);
  const checks = [
    result("api_compatibility", () => checkApiCompatibility(manifest)),
    result("fixture_contracts", () => validateFixtureContract(manifest)),
    await asyncResult("integration_harness", () => checkIntegrationHarness(manifest)),
    await asyncResult("fault_injection", () => checkFaultInjection(manifest)),
    await asyncResult("bounded_load", () => checkBoundedLoad(manifest)),
    result("json_yaml_config_validation", () => validateJsonAndYaml(manifest)),
    result("systemd_config_validation", () => validateSystemdUnits(manifest)),
    result("clickhouse_migration_validation", () => validateClickHouseMigrations(manifest)),
    result("observability_contract", () => checkObservability(manifest)),
  ];
  if (args.live) {
    checks.push(await asyncResult("live_operational_contract", () => checkLive(manifest)));
  }

  const report = {
    ok: checks.every((check) => check.ok),
    mode: args.live ? "live" : "offline",
    manifest: manifest.version,
    checks,
  };

  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    for (const check of checks) {
      console.log(`${check.ok ? "ok" : "fail"} ${check.name} ${check.duration_ms}ms`);
      if (!check.ok) console.log(`  ${check.error}`);
    }
  }
  if (!report.ok) process.exit(1);
}

main().catch((error) => {
  console.error(JSON.stringify({ ok: false, error: error.message }, null, 2));
  process.exit(1);
});
