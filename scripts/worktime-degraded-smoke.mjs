#!/usr/bin/env node
import childProcess from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function jsonString(value) {
  return JSON.stringify(value, null, 2);
}

async function randomLocalPort() {
  return await new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = typeof address === "object" && address ? address.port : 0;
      server.close(() => resolve(port));
    });
  });
}

function cargoTargetDirectory() {
  try {
    const workspace = path.join(root, "adk-rust");
    const output = childProcess.execFileSync(
      "cargo",
      ["metadata", "--format-version", "1", "--no-deps", "--manifest-path", path.join(workspace, "Cargo.toml")],
      { cwd: workspace, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] },
    );
    const metadata = JSON.parse(output);
    return metadata.target_directory || "";
  } catch {
    return "";
  }
}

function resolveWorktimeBinary() {
  const explicit = process.env.WORKTIME_API_BIN || "";
  const targetDir = cargoTargetDirectory();
  const candidates = [
    explicit,
    targetDir ? path.join(targetDir, "debug/worktime-api") : "",
    targetDir ? path.join(targetDir, "release/worktime-api") : "",
    path.join(root, "adk-rust/target/debug/worktime-api"),
    path.join(root, "adk-rust/target/release/worktime-api"),
  ].filter(Boolean);
  const found = candidates.find((candidate) => fs.existsSync(candidate) && fs.statSync(candidate).mode & 0o111);
  if (!found) {
    throw new Error("worktime-api binary not found; run: cd adk-rust && cargo build -p worktime-api");
  }
  return found;
}

function startActivityWatchStub() {
  const now = new Date().toISOString();
  const sockets = new Set();
  const event = {
    timestamp: now,
    duration: 30,
    data: {
      username: "demo-user",
      userId: "HOST-EXAMPLE\\demo-user",
      sessionId: 1,
      active: true,
      state: "active",
      sampleSeconds: 30,
    },
  };
  const server = http.createServer((request, response) => {
    const parsed = new URL(request.url || "/", "http://127.0.0.1");
    response.setHeader("Content-Type", "application/json; charset=utf-8");
    if (parsed.pathname === "/api/0/buckets") {
      response.end(jsonString({ "aw-worktime-sessions_HOST-EXAMPLE": { metadata: { end: now } } }));
      return;
    }
    if (parsed.pathname === "/api/0/buckets/aw-worktime-sessions_HOST-EXAMPLE/events") {
      response.end(jsonString([event]));
      return;
    }
    if (parsed.pathname === "/api/0/buckets/aw-worktime-sessions_HOST-EXAMPLE") {
      response.end(jsonString({ metadata: { end: now } }));
      return;
    }
    response.statusCode = 404;
    response.end(jsonString({ error: "not_found" }));
  });
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      resolve({
        server,
        port: typeof address === "object" && address ? address.port : 0,
        close: () =>
          new Promise((done) => {
            for (const socket of sockets) socket.destroy();
            server.close(done);
          }),
      });
    });
  });
}

function spawnWorktimeApi(binary, port, awPort, stateDir) {
  const env = {
    ...process.env,
    AW_SERVER_URL: `http://127.0.0.1:${awPort}`,
    AW_WORKTIME_LISTEN_HOST: "127.0.0.1",
    AW_WORKTIME_PORT: String(port),
    AW_WORKTIME_HOST: "HOST-EXAMPLE",
    AW_WORKTIME_AW_HTTP_TIMEOUT_SECONDS: "0.5",
    AW_WORKTIME_SOURCE_HTTP_TIMEOUT_SECONDS: "0.25",
    AW_WORKTIME_EVENTS_LIMIT: "250",
    AW_WORKTIME_EVENTS_CACHE_TTL_SECONDS: "0",
    AW_WORKTIME_REPORT_CACHE_TTL_SECONDS: "0",
    AW_WORKTIME_REPORT_STALE_TTL_SECONDS: "600",
    AW_WORKTIME_REPORT_DISK_STALE_TTL_SECONDS: "600",
    AW_WORKTIME_REPORT_DISK_CACHE_DIR: path.join(stateDir, "cache"),
    AW_WORKTIME_MANAGEMENT_HISTORY_DIR: path.join(stateDir, "history"),
  };
  fs.mkdirSync(env.AW_WORKTIME_REPORT_DISK_CACHE_DIR, { recursive: true });
  fs.mkdirSync(env.AW_WORKTIME_MANAGEMENT_HISTORY_DIR, { recursive: true });

  const child = childProcess.spawn(binary, [], {
    cwd: root,
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const logs = [];
  child.stdout.on("data", (chunk) => logs.push(String(chunk)));
  child.stderr.on("data", (chunk) => logs.push(String(chunk)));
  return { child, logs };
}

async function stopChild(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill("SIGTERM");
  await new Promise((resolve) => {
    const timeout = setTimeout(() => {
      if (child.exitCode === null) child.kill("SIGKILL");
      resolve();
    }, 1500);
    child.once("exit", () => {
      clearTimeout(timeout);
      resolve();
    });
  });
}

async function getJson(url, timeoutMs = 2500) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  const started = Date.now();
  try {
    const response = await fetch(url, { signal: controller.signal });
    const text = await response.text();
    let json = null;
    try {
      json = JSON.parse(text);
    } catch {
      // Keep the raw body in the error below.
    }
    return {
      ok: response.ok,
      status: response.status,
      headers: response.headers,
      json,
      text,
      elapsedMs: Date.now() - started,
    };
  } finally {
    clearTimeout(timeout);
  }
}

async function waitForHealth(port, timeoutMs = 5000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const response = await getJson(`http://127.0.0.1:${port}/health`, 500);
      if (response.status === 200) return response;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`worktime-api did not become ready: ${lastError?.message || "timeout"}`);
}

async function staleFallbackCheck(binary, aw) {
  const port = await randomLocalPort();
  const stateDir = fs.mkdtempSync(path.join(os.tmpdir(), "aw-worktime-smoke-stale-"));
  const api = spawnWorktimeApi(binary, port, aw.port, stateDir);
  try {
    await waitForHealth(port);
    const reportUrl = `http://127.0.0.1:${port}/reports/worktime/management?format=json&host=HOST-EXAMPLE`;
    const fresh = await getJson(reportUrl);
    assert(fresh.status === 200, `fresh report status=${fresh.status} body=${fresh.text}`);
    assert(fresh.json?.status !== "DEGRADED", `fresh report is unexpectedly degraded: ${fresh.text}`);

    await aw.close();
    const stale = await getJson(reportUrl);
    assert(stale.status === 200, `stale report status=${stale.status} body=${stale.text}`);
    assert(stale.elapsedMs < 2500, `stale report exceeded bounded response time: ${stale.elapsedMs}ms`);
    assert(stale.json?.status === "DEGRADED", `stale report did not return DEGRADED: ${stale.text}`);
    assert(stale.json?.stale === true, `stale report did not mark stale=true: ${stale.text}`);
    assert(stale.json?.runtime?.report_stale_served === true, `runtime stale flag missing: ${stale.text}`);
    assert(stale.headers.get("x-aw-worktime-cache") === "stale", "stale cache header missing");

    const health = await getJson(`http://127.0.0.1:${port}/health`);
    assert(health.status === 200, `health status=${health.status} body=${health.text}`);
    assert(health.json?.ok === false, `health must not be fully healthy during degraded mode: ${health.text}`);
    assert(health.json?.status === "DEGRADED", `health did not expose DEGRADED: ${health.text}`);
    assert(typeof health.json?.runtime?.worktime_events_limit === "number", "health runtime events limit missing");
    assert(typeof health.json?.runtime?.aw_query_timeout_count === "number", "health timeout counter missing");

    return {
      freshStatus: fresh.json?.status || "OK",
      staleStatus: stale.json?.status,
      staleCacheHeader: stale.headers.get("x-aw-worktime-cache"),
      staleElapsedMs: stale.elapsedMs,
      healthStatus: health.json?.status,
    };
  } finally {
    await stopChild(api.child);
    fs.rmSync(stateDir, { recursive: true, force: true });
  }
}

async function noCacheDegradedCheck(binary, closedAwPort) {
  const port = await randomLocalPort();
  const stateDir = fs.mkdtempSync(path.join(os.tmpdir(), "aw-worktime-smoke-degraded-"));
  const api = spawnWorktimeApi(binary, port, closedAwPort, stateDir);
  try {
    await waitForHealth(port);
    const reportUrl = `http://127.0.0.1:${port}/reports/worktime/management?format=json&host=HOST-EXAMPLE`;
    const degraded = await getJson(reportUrl);
    assert(degraded.status === 200, `degraded report status=${degraded.status} body=${degraded.text}`);
    assert(degraded.elapsedMs < 2500, `degraded report exceeded bounded response time: ${degraded.elapsedMs}ms`);
    assert(degraded.json?.status === "DEGRADED", `no-cache report did not return DEGRADED: ${degraded.text}`);
    assert(degraded.json?.stale === false, `no-cache report must mark stale=false: ${degraded.text}`);
    assert(degraded.json?.reason === "report_unavailable", `no-cache reason mismatch: ${degraded.text}`);
    assert(degraded.headers.get("x-aw-worktime-cache") === "degraded", "degraded cache header missing");

    const health = await getJson(`http://127.0.0.1:${port}/health`);
    assert(health.json?.ok === false, `health must be degraded without cache: ${health.text}`);
    assert(health.json?.status === "DEGRADED", `health status mismatch without cache: ${health.text}`);
    return {
      degradedStatus: degraded.json?.status,
      stale: degraded.json?.stale,
      cacheHeader: degraded.headers.get("x-aw-worktime-cache"),
      degradedElapsedMs: degraded.elapsedMs,
      healthStatus: health.json?.status,
    };
  } finally {
    await stopChild(api.child);
    fs.rmSync(stateDir, { recursive: true, force: true });
  }
}

async function main() {
  const binary = resolveWorktimeBinary();
  const aw = await startActivityWatchStub();
  const closedAwPort = aw.port;
  const staleResult = await staleFallbackCheck(binary, aw);
  const degradedResult = await noCacheDegradedCheck(binary, closedAwPort);
  console.log(
    jsonString({
      binary,
      staleFallback: staleResult,
      noCacheDegraded: degradedResult,
    }),
  );
  console.log("worktime degraded smoke OK");
}

main().catch((error) => {
  console.error(`worktime degraded smoke FAILED: ${error.stack || error.message}`);
  process.exit(1);
});
