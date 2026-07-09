#!/usr/bin/env node
import { spawn } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import tls from "node:tls";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const defaultOutputDir = path.join(root, "output", "validation");
const maxCapturedBytes = 256 * 1024;
const criticalAlertSeverities = new Set(["critical"]);
const failureStatuses = new Set(["fail"]);

function utcNow() {
  return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
}

function stampForPath(value) {
  return value.replaceAll(":", "").replaceAll("-", "").replace("T", "T").replace("Z", "Z");
}

function parseArgs(argv) {
  const args = {
    profile: process.env.AW_VALIDATION_PROFILE || "full",
    outputDir: process.env.AW_VALIDATION_OUTPUT_DIR || defaultOutputDir,
    retentionDays: Number.parseInt(process.env.AW_VALIDATION_RETENTION_DAYS || "30", 10),
    commandTimeoutSeconds: Number.parseInt(process.env.AW_VALIDATION_COMMAND_TIMEOUT_SECONDS || "900", 10),
    endpointTimeoutSeconds: Number.parseInt(process.env.AW_VALIDATION_ENDPOINT_TIMEOUT_SECONDS || "5", 10),
    productionEvidence: process.env.PRODUCTION_BINARY_PARITY_EVIDENCE || "",
    releaseEvidenceDir: process.env.AW_RELEASE_EVIDENCE_DIR || "",
    releaseAssetDir: process.env.AW_RELEASE_ASSET_DIR || "",
    json: false,
    selfTest: false,
    allowFailures: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      index += 1;
      if (index >= argv.length) throw new Error(`${arg} requires a value`);
      return argv[index];
    };
    if (arg === "--profile") args.profile = next();
    else if (arg === "--output-dir") args.outputDir = path.resolve(next());
    else if (arg === "--retention-days") args.retentionDays = Number.parseInt(next(), 10);
    else if (arg === "--command-timeout-seconds") args.commandTimeoutSeconds = Number.parseInt(next(), 10);
    else if (arg === "--endpoint-timeout-seconds") args.endpointTimeoutSeconds = Number.parseInt(next(), 10);
    else if (arg === "--production-evidence") args.productionEvidence = path.resolve(next());
    else if (arg === "--release-evidence-dir") args.releaseEvidenceDir = path.resolve(next());
    else if (arg === "--release-asset-dir") args.releaseAssetDir = path.resolve(next());
    else if (arg === "--json") args.json = true;
    else if (arg === "--self-test") args.selfTest = true;
    else if (arg === "--allow-failures") args.allowFailures = true;
    else if (arg === "--help" || arg === "-h") args.help = true;
    else throw new Error(`unsupported argument: ${arg}`);
  }

  if (!["quick", "standard", "full", "runtime"].includes(args.profile)) {
    throw new Error(`unsupported --profile=${args.profile}; expected quick, standard, full or runtime`);
  }
  if (!Number.isFinite(args.retentionDays) || args.retentionDays < 1) {
    throw new Error("--retention-days must be a positive integer");
  }
  if (!Number.isFinite(args.commandTimeoutSeconds) || args.commandTimeoutSeconds < 5) {
    throw new Error("--command-timeout-seconds must be at least 5");
  }
  if (!Number.isFinite(args.endpointTimeoutSeconds) || args.endpointTimeoutSeconds < 1) {
    throw new Error("--endpoint-timeout-seconds must be at least 1");
  }
  return args;
}

function usage() {
  return `Usage:
  scripts/run_full_validation.sh [options]
  node scripts/full-validation-orchestrator.mjs [options]

Options:
  --profile quick|standard|full|runtime
      quick: repository, docs, schema and self-test checks
      standard: quick + operational/release validation wrappers
      full: standard + Rust/dependency gates
      runtime: standard + runtime diagnostics focus
  --output-dir <dir>                 default: output/validation
  --retention-days <days>            default: 30
  --production-evidence <json>       production binary parity evidence
  --release-evidence-dir <dir>       release evidence package to verify
  --release-asset-dir <dir>          signed release assets directory to verify
  --command-timeout-seconds <n>      default: 900
  --endpoint-timeout-seconds <n>     default: 5
  --json                             print compact JSON summary
  --allow-failures                   exit 0 after writing reports
  --self-test                        validate the orchestrator itself

Environment:
  AW_VALIDATION_PROFILE, AW_VALIDATION_OUTPUT_DIR,
  AW_VALIDATION_RETENTION_DAYS, PRODUCTION_BINARY_PARITY_EVIDENCE,
  AW_RELEASE_EVIDENCE_DIR, AW_RELEASE_ASSET_DIR,
  AW_VALIDATION_ACTIVITYWATCH_URL, AW_VALIDATION_WORKTIME_URL,
  AW_VALIDATION_GRAFANA_URL, AW_VALIDATION_PROMETHEUS_URL,
  AW_VALIDATION_CLICKHOUSE_URL, AW_VALIDATION_SYSTEMD_UNITS,
  AW_VALIDATION_QUEUE_DIRS, AW_VALIDATION_SIZE_PATHS,
  AW_VALIDATION_TLS_HOSTS
`;
}

function commandExists(name) {
  if (name.includes(path.sep)) {
    try {
      fs.accessSync(name, fs.constants.X_OK);
      return true;
    } catch {
      return false;
    }
  }
  const dirs = String(process.env.PATH || "").split(path.delimiter);
  const extensions = process.platform === "win32" ? ["", ".exe", ".cmd", ".bat"] : [""];
  for (const dir of dirs) {
    for (const ext of extensions) {
      const candidate = path.join(dir, `${name}${ext}`);
      try {
        fs.accessSync(candidate, fs.constants.X_OK);
        return true;
      } catch {
        // continue
      }
    }
  }
  return false;
}

function readTextIfExists(file) {
  try {
    return fs.readFileSync(file, "utf8");
  } catch {
    return "";
  }
}

function truncate(value) {
  const text = String(value || "");
  if (Buffer.byteLength(text, "utf8") <= maxCapturedBytes) return text;
  const buffer = Buffer.from(text, "utf8");
  return `<truncated ${buffer.length - maxCapturedBytes} bytes>\n${buffer.subarray(buffer.length - maxCapturedBytes).toString("utf8")}`;
}

function normalizeRelative(file) {
  return path.relative(root, file).replaceAll(path.sep, "/");
}

function splitEnvList(value) {
  return String(value || "")
    .split(/[,\n]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function sha256Bytes(data) {
  return crypto.createHash("sha256").update(data).digest("hex");
}

function sha256File(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function mkdirp(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function writeJson(file, data) {
  mkdirp(path.dirname(file));
  fs.writeFileSync(file, `${JSON.stringify(data, null, 2)}\n`, "utf8");
}

function writeText(file, data) {
  mkdirp(path.dirname(file));
  fs.writeFileSync(file, data, "utf8");
}

function copyFileCompat(source, target) {
  mkdirp(path.dirname(target));
  try {
    fs.copyFileSync(source, target);
  } catch (error) {
    if (!["EPERM", "ENOSYS", "EXDEV"].includes(error?.code)) throw error;
    fs.writeFileSync(target, fs.readFileSync(source));
  }
}

function runCommand({ id, category, command, args = [], cwd = root, timeoutSeconds, env = {}, optional = false }) {
  const started = Date.now();
  const cmdline = [command, ...args].join(" ");
  if (!commandExists(command)) {
    return Promise.resolve({
      id,
      category,
      status: optional ? "skipped" : "fail",
      ok: false,
      command: cmdline,
      cwd: normalizeRelative(cwd),
      duration_ms: 0,
      skipped_reason: `tool_not_found:${command}`,
      stdout: "",
      stderr: "",
    });
  }

  return new Promise((resolve) => {
    const child = spawn(command, args, {
      cwd,
      env: { ...process.env, ...env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      child.kill("SIGTERM");
      setTimeout(() => {
        if (!child.killed) child.kill("SIGKILL");
      }, 2000).unref();
    }, timeoutSeconds * 1000);

    child.stdout.on("data", (chunk) => {
      stdout = truncate(stdout + chunk.toString("utf8"));
    });
    child.stderr.on("data", (chunk) => {
      stderr = truncate(stderr + chunk.toString("utf8"));
    });
    child.on("error", (error) => {
      clearTimeout(timeout);
      resolve({
        id,
        category,
        status: optional ? "skipped" : "fail",
        ok: false,
        command: cmdline,
        cwd: normalizeRelative(cwd),
        duration_ms: Date.now() - started,
        error: error.message,
        stdout,
        stderr,
      });
    });
    child.on("close", (code, signal) => {
      clearTimeout(timeout);
      const ok = code === 0 && !timedOut;
      resolve({
        id,
        category,
        status: ok ? "pass" : "fail",
        ok,
        command: cmdline,
        cwd: normalizeRelative(cwd),
        exit_code: code,
        signal,
        timed_out: timedOut,
        duration_ms: Date.now() - started,
        stdout: truncate(stdout),
        stderr: truncate(stderr),
      });
    });
  });
}

async function runInternalCheck(id, category, fn) {
  const started = Date.now();
  try {
    const details = (await fn()) || {};
    return {
      id,
      category,
      status: details.status || "pass",
      ok: details.status ? details.status === "pass" : true,
      duration_ms: Date.now() - started,
      ...details,
    };
  } catch (error) {
    return {
      id,
      category,
      status: "fail",
      ok: false,
      duration_ms: Date.now() - started,
      error: error.message,
    };
  }
}

function gitOutput(args, fallback = "") {
  if (!commandExists("git")) return fallback;
  const result = spawnSyncText("git", args, root, 30);
  return result.status === 0 ? result.stdout.trim() : fallback;
}

function spawnSyncText(command, args, cwd, timeoutSeconds) {
  const started = Date.now();
  const result = spawn(command, args, {
    cwd,
    stdio: ["ignore", "pipe", "pipe"],
    env: process.env,
  });
  return new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    const timeout = setTimeout(() => {
      result.kill("SIGTERM");
    }, timeoutSeconds * 1000);
    result.stdout.on("data", (chunk) => {
      stdout = truncate(stdout + chunk.toString("utf8"));
    });
    result.stderr.on("data", (chunk) => {
      stderr = truncate(stderr + chunk.toString("utf8"));
    });
    result.on("close", (status) => {
      clearTimeout(timeout);
      resolve({ status, stdout, stderr, duration_ms: Date.now() - started });
    });
  });
}

async function collectGitContext() {
  const rev = commandExists("git") ? await spawnSyncText("git", ["rev-parse", "HEAD"], root, 30) : null;
  const describe = commandExists("git") ? await spawnSyncText("git", ["describe", "--tags", "--always", "--dirty"], root, 30) : null;
  const branch = commandExists("git") ? await spawnSyncText("git", ["branch", "--show-current"], root, 30) : null;
  return {
    git_sha: rev?.status === 0 ? rev.stdout.trim() : "unknown",
    git_describe: describe?.status === 0 ? describe.stdout.trim() : "unknown",
    git_branch: branch?.status === 0 ? branch.stdout.trim() : "unknown",
  };
}

async function trackedFiles() {
  if (!commandExists("git")) {
    return walk(root)
      .map((file) => normalizeRelative(file))
      .filter((file) => !file.startsWith("output/") && !file.startsWith("dist/"));
  }
  const result = await spawnSyncText("git", ["ls-files", "-z"], root, 60);
  if (result.status !== 0) return [];
  return result.stdout
    .split("\0")
    .filter(Boolean)
    .filter((file) => fs.existsSync(path.join(root, file)));
}

function walk(dir) {
  const found = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if ([".git", "output", "dist", "target", "node_modules"].includes(entry.name)) continue;
    const absolute = path.join(dir, entry.name);
    if (entry.isDirectory()) found.push(...walk(absolute));
    else found.push(absolute);
  }
  return found;
}

async function validateJsonFiles(files) {
  const findings = [];
  let checked = 0;
  for (const file of files.filter((item) => item.endsWith(".json"))) {
    try {
      JSON.parse(fs.readFileSync(path.join(root, file), "utf8"));
      checked += 1;
    } catch (error) {
      findings.push({ file, error: error.message });
    }
  }
  return {
    checked,
    findings,
    status: findings.length === 0 ? "pass" : "fail",
    ok: findings.length === 0,
  };
}

function parseMarkdownLinks(text) {
  const links = [];
  const pattern = /!?\[[^\]]+\]\(([^)]+)\)/g;
  let match = null;
  while ((match = pattern.exec(text)) !== null) {
    links.push(String(match[1] || "").trim());
  }
  return links;
}

async function validateMarkdownLinks(files) {
  const findings = [];
  let checked = 0;
  for (const file of files.filter((item) => item.endsWith(".md"))) {
    const text = fs.readFileSync(path.join(root, file), "utf8");
    for (const raw of parseMarkdownLinks(text)) {
      if (raw.includes("<PROJECT_ROOT>")) continue;
      let target = raw.split(/\s+/)[0].replace(/^<|>$/g, "");
      if (
        !target
        || target.startsWith("#")
        || target.startsWith("http://")
        || target.startsWith("https://")
        || target.startsWith("mailto:")
        || target.includes("<PROJECT_ROOT>")
        || target.startsWith("file:")
      ) {
        continue;
      }
      target = target.split("#", 1)[0];
      if (!target) continue;
      const resolved = path.resolve(root, path.dirname(file), target);
      const wikiResolved = path.resolve(root, path.dirname(file), `${target}.md`);
      checked += 1;
      if (!fs.existsSync(resolved) && !fs.existsSync(wikiResolved)) findings.push({ file, target });
    }
  }
  return {
    checked,
    findings,
    status: findings.length === 0 ? "pass" : "fail",
    ok: findings.length === 0,
  };
}

async function validateYamlFiles(files, timeoutSeconds) {
  const yamlFiles = files.filter((item) => item.endsWith(".yml") || item.endsWith(".yaml"));
  if (yamlFiles.length === 0) return { checked: 0, status: "pass", ok: true };
  if (commandExists("ruby")) {
    const script = "require 'psych'; ARGV.each { |f| Psych.parse_file(f) }";
    return runCommand({
      id: "config.yaml_parse",
      category: "configuration",
      command: "ruby",
      args: ["-e", script, ...yamlFiles],
      timeoutSeconds,
      optional: false,
    });
  }
  return {
    status: "skipped",
    ok: false,
    checked: 0,
    skipped_reason: "ruby_not_available_for_yaml_parse",
  };
}

function commandDefinitions(args) {
  const timeout = args.commandTimeoutSeconds;
  const adkRust = path.join(root, "adk-rust");
  const defs = [
    { id: "repository.git_diff_check", category: "repository", command: "git", args: ["diff", "--check"], timeoutSeconds: timeout },
    { id: "security.secret_pattern_check", category: "security", command: "python3", args: ["scripts/public_secret_pattern_check.py"], timeoutSeconds: timeout },
    { id: "repository.private_config_guard", category: "security", command: "bash", args: ["scripts/check_private_config_guard.sh"], timeoutSeconds: timeout },
    { id: "contracts.portal_contract_sync", category: "configuration", command: "node", args: ["scripts/check_portal_contract_sync.mjs"], timeoutSeconds: timeout },
  ];

  if (["standard", "full", "runtime"].includes(args.profile)) {
    defs.push(
      { id: "operational.operational_maturity", category: "operational", command: "node", args: ["scripts/operational-maturity-check.mjs", "--json"], timeoutSeconds: timeout },
      { id: "operational.deployment_readiness_smoke", category: "deployment", command: "node", args: ["scripts/deployment-readiness-smoke.mjs"], timeoutSeconds: timeout },
      { id: "operational.pilot_validation_smoke", category: "operational", command: "node", args: ["scripts/pilot-validation-smoke.mjs"], timeoutSeconds: timeout },
      { id: "repository.production_inventory_placeholder_self_test", category: "configuration", command: "bash", args: ["scripts/check_production_inventory_placeholders.sh", "--self-test"], timeoutSeconds: timeout, optional: false },
      { id: "release.release_assets_self_test", category: "release", command: "bash", args: ["scripts/verify_release_assets.sh", "--self-test"], timeoutSeconds: timeout, optional: true },
      { id: "release.rust_release_artifacts", category: "release", command: "bash", args: ["scripts/check_detmir_rust_release_artifacts.sh"], timeoutSeconds: timeout, optional: false },
      { id: "release.binary_parity_self_test", category: "release", command: "python3", args: ["scripts/check_production_binary_parity.py", "--self-test"], timeoutSeconds: timeout, optional: false },
    );
    if (process.env.DETMIR_PRODUCTION_CONFIG_PATHS) {
      defs.push({
        id: "repository.production_inventory_placeholders",
        category: "configuration",
        command: "bash",
        args: ["scripts/check_production_inventory_placeholders.sh", "--strict"],
        timeoutSeconds: timeout,
        optional: false,
      });
    }
    if (args.productionEvidence) {
      defs.push({
        id: "release.production_binary_parity",
        category: "release",
        command: "python3",
        args: ["scripts/check_production_binary_parity.py", "--evidence", args.productionEvidence],
        timeoutSeconds: timeout,
        optional: false,
      });
    }
    if (args.releaseEvidenceDir) {
      defs.push({
        id: "release.release_evidence_check",
        category: "release",
        command: "bash",
        args: ["scripts/check_release_evidence.sh", args.releaseEvidenceDir],
        timeoutSeconds: timeout,
        optional: false,
      });
    }
    if (args.releaseAssetDir) {
      defs.push({
        id: "release.release_asset_check",
        category: "release",
        command: "bash",
        args: ["scripts/verify_release_assets.sh", args.releaseAssetDir],
        timeoutSeconds: timeout,
        optional: false,
      });
    }
  }

  if (args.profile === "full") {
    defs.push(
      { id: "rust.cargo_metadata", category: "rust", command: "cargo", args: ["metadata", "--locked", "--format-version", "1"], cwd: adkRust, timeoutSeconds: timeout },
      { id: "rust.cargo_fmt", category: "rust", command: "cargo", args: ["fmt", "--all", "--check"], cwd: adkRust, timeoutSeconds: timeout },
      { id: "rust.cargo_clippy", category: "rust", command: "cargo", args: ["clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"], cwd: adkRust, timeoutSeconds: timeout },
      { id: "rust.cargo_test", category: "rust", command: "cargo", args: ["test", "--workspace", "--locked"], cwd: adkRust, timeoutSeconds: timeout },
      { id: "dependency.cargo_tree_duplicates", category: "dependency", command: "cargo", args: ["tree", "--duplicates", "--locked"], cwd: adkRust, timeoutSeconds: timeout, optional: true },
      { id: "dependency.cargo_audit", category: "dependency", command: "cargo", args: ["audit"], cwd: adkRust, timeoutSeconds: timeout, optional: true },
      { id: "dependency.cargo_deny", category: "dependency", command: "cargo", args: ["deny", "check", "--config", "../deny.toml"], cwd: adkRust, timeoutSeconds: timeout, optional: true },
      { id: "dependency.cargo_machete", category: "dependency", command: "cargo", args: ["machete", "--with-metadata"], cwd: adkRust, timeoutSeconds: timeout, optional: true },
    );
  }

  return defs;
}

function summarizeCheckForEvidence(check) {
  return {
    id: check.id,
    category: check.category,
    status: check.status,
    exit_code: check.exit_code,
    duration_ms: check.duration_ms,
    skipped_reason: check.skipped_reason,
    error: check.error,
  };
}

function collectDiskUsage() {
  const disks = [];
  const lines = readTextIfExists("/proc/mounts").split("\n").filter(Boolean);
  const seen = new Set();
  for (const line of lines) {
    const [, mountPoint, fsType] = line.split(/\s+/);
    if (!mountPoint || seen.has(mountPoint)) continue;
    if (["proc", "sysfs", "tmpfs", "devtmpfs", "devpts", "cgroup", "cgroup2", "overlay", "squashfs"].includes(fsType)) continue;
    if (fsType.startsWith("fuse.")) continue;
    seen.add(mountPoint);
    try {
      const stat = fs.statfsSync(mountPoint);
      const total = Number(stat.blocks) * Number(stat.bsize);
      const free = Number(stat.bavail) * Number(stat.bsize);
      const used = Math.max(total - free, 0);
      disks.push({
        mount: mountPoint,
        fs_type: fsType,
        total_bytes: total,
        used_bytes: used,
        free_bytes: free,
        used_pct: total > 0 ? Number(((used / total) * 100).toFixed(2)) : 0,
      });
    } catch {
      // ignore inaccessible mounts
    }
  }
  return disks.sort((a, b) => b.used_pct - a.used_pct);
}

function collectMemory() {
  const meminfo = {};
  for (const line of readTextIfExists("/proc/meminfo").split("\n")) {
    const match = line.match(/^([^:]+):\s+(\d+)\s+kB/);
    if (match) meminfo[match[1]] = Number(match[2]) * 1024;
  }
  const total = meminfo.MemTotal || os.totalmem();
  const available = meminfo.MemAvailable || os.freemem();
  const used = Math.max(total - available, 0);
  return {
    total_bytes: total,
    available_bytes: available,
    used_bytes: used,
    used_pct: total > 0 ? Number(((used / total) * 100).toFixed(2)) : 0,
    swap_total_bytes: meminfo.SwapTotal || 0,
    swap_free_bytes: meminfo.SwapFree || 0,
  };
}

function collectCpu() {
  const load = os.loadavg();
  return {
    cpu_count: os.cpus().length,
    load_1m: load[0],
    load_5m: load[1],
    load_15m: load[2],
  };
}

function sizePath(target) {
  try {
    const stat = fs.statSync(target);
    if (stat.isFile()) return stat.size;
    if (!stat.isDirectory()) return 0;
    let total = 0;
    for (const entry of fs.readdirSync(target, { withFileTypes: true })) {
      const child = path.join(target, entry.name);
      if (entry.isSymbolicLink()) continue;
      total += sizePath(child);
    }
    return total;
  } catch {
    return null;
  }
}

function collectPathSizes() {
  const defaults = [
    "/var/lib/activitywatch",
    "/var/log/activitywatch",
    "/opt/activitywatch/clickhouse-1c",
    "/opt/activitywatch/clickhouse-workforce",
    "/opt/hayabusa",
  ];
  const configured = splitEnvList(process.env.AW_VALIDATION_SIZE_PATHS);
  const paths = configured.length > 0 ? configured : defaults;
  return paths.map((item) => ({
    path: item,
    exists: fs.existsSync(item),
    size_bytes: sizePath(item),
  }));
}

function countQueueFiles() {
  const defaults = [
    "/var/lib/activitywatch/queues",
    "/var/lib/activitywatch/worktime-report-cache",
    "/opt/activitywatch/aw-rus-ops/drop",
  ];
  const dirs = splitEnvList(process.env.AW_VALIDATION_QUEUE_DIRS);
  const targets = dirs.length > 0 ? dirs : defaults;
  return targets.map((dir) => {
    try {
      const entries = fs.readdirSync(dir, { withFileTypes: true });
      return {
        path: dir,
        exists: true,
        files: entries.filter((entry) => entry.isFile()).length,
        directories: entries.filter((entry) => entry.isDirectory()).length,
      };
    } catch {
      return { path: dir, exists: false, files: null, directories: null };
    }
  });
}

async function probeEndpoint(name, url, timeoutSeconds) {
  if (!url) return { name, status: "skipped", skipped_reason: "not_configured" };
  return new Promise((resolve) => {
    const started = Date.now();
    const parsed = new URL(url);
    const transport = parsed.protocol === "https:" ? https : http;
    const req = transport.request(parsed, { method: "GET", timeout: timeoutSeconds * 1000 }, (res) => {
      res.resume();
      res.on("end", () => {
        resolve({
          name,
          url,
          status: res.statusCode >= 200 && res.statusCode < 500 ? "pass" : "fail",
          http_status: res.statusCode,
          duration_ms: Date.now() - started,
        });
      });
    });
    req.on("timeout", () => {
      req.destroy(new Error("timeout"));
    });
    req.on("error", (error) => {
      resolve({
        name,
        url,
        status: "fail",
        error: error.message,
        duration_ms: Date.now() - started,
      });
    });
    req.end();
  });
}

async function collectSystemd(units, timeoutSeconds) {
  if (!commandExists("systemctl")) return { available: false, units: [] };
  const results = [];
  for (const unit of units) {
    const result = await spawnSyncText(
      "systemctl",
      ["show", unit, "--property=Id,LoadState,ActiveState,SubState,Result,NRestarts,ExecMainStatus"],
      root,
      timeoutSeconds,
    );
    const values = {};
    for (const line of result.stdout.split("\n")) {
      const [key, ...rest] = line.split("=");
      if (key) values[key] = rest.join("=");
    }
    results.push({
      unit,
      command_status: result.status,
      load_state: values.LoadState || "unknown",
      active_state: values.ActiveState || "unknown",
      sub_state: values.SubState || "unknown",
      result: values.Result || "",
      restarts: Number.parseInt(values.NRestarts || "0", 10),
      exec_main_status: values.ExecMainStatus || "",
    });
  }
  return { available: true, units: results };
}

async function probeTlsCertificate(target, timeoutSeconds) {
  const [host, portText] = target.split(":");
  const port = Number.parseInt(portText || "443", 10);
  if (!host || !Number.isFinite(port)) {
    return { target, status: "fail", error: "expected host:port" };
  }
  return new Promise((resolve) => {
    const started = Date.now();
    const socket = tls.connect({
      host,
      port,
      servername: host,
      rejectUnauthorized: false,
      timeout: timeoutSeconds * 1000,
    }, () => {
      const cert = socket.getPeerCertificate();
      socket.end();
      const expiresAt = cert?.valid_to ? new Date(cert.valid_to) : null;
      resolve({
        target,
        status: cert && cert.valid_to ? "pass" : "fail",
        subject: cert?.subject || {},
        issuer: cert?.issuer || {},
        valid_from: cert?.valid_from || null,
        valid_to: cert?.valid_to || null,
        expires_at_utc: expiresAt && !Number.isNaN(expiresAt.valueOf()) ? expiresAt.toISOString() : null,
        days_until_expiry: expiresAt && !Number.isNaN(expiresAt.valueOf())
          ? Math.floor((expiresAt.valueOf() - Date.now()) / (24 * 60 * 60 * 1000))
          : null,
        authorized: socket.authorized,
        authorization_error: socket.authorizationError || null,
        duration_ms: Date.now() - started,
      });
    });
    socket.on("timeout", () => socket.destroy(new Error("timeout")));
    socket.on("error", (error) => {
      resolve({ target, status: "fail", error: error.message, duration_ms: Date.now() - started });
    });
  });
}

async function collectTlsCertificates(timeoutSeconds) {
  const targets = splitEnvList(process.env.AW_VALIDATION_TLS_HOSTS);
  if (targets.length === 0) return [];
  return Promise.all(targets.map((target) => probeTlsCertificate(target, timeoutSeconds)));
}

async function collectRuntimeDiagnostics(args) {
  const endpoints = [
    ["activitywatch", process.env.AW_VALIDATION_ACTIVITYWATCH_URL || "http://127.0.0.1:5600/api/0/info"],
    ["worktime", process.env.AW_VALIDATION_WORKTIME_URL || "http://127.0.0.1:5610/healthz"],
    ["grafana", process.env.AW_VALIDATION_GRAFANA_URL || ""],
    ["prometheus", process.env.AW_VALIDATION_PROMETHEUS_URL || ""],
    ["clickhouse", process.env.AW_VALIDATION_CLICKHOUSE_URL || ""],
  ];
  const systemdUnits = splitEnvList(process.env.AW_VALIDATION_SYSTEMD_UNITS);
  const defaultUnits = [
    "activitywatch-server.service",
    "aw-worktime-api.service",
    "detmir-readiness.service",
    "aw-prune-local-state.service",
    "aw-db-maintenance.service",
    "aw-db-vacuum.service",
  ];

  return {
    host: {
      hostname: os.hostname(),
      platform: os.platform(),
      release: os.release(),
      uptime_seconds: Math.floor(os.uptime()),
    },
    cpu: collectCpu(),
    memory: collectMemory(),
    disks: collectDiskUsage(),
    path_sizes: collectPathSizes(),
    queues: countQueueFiles(),
    endpoints: await Promise.all(
      endpoints.map(([name, url]) => probeEndpoint(name, url, args.endpointTimeoutSeconds)),
    ),
    systemd: await collectSystemd(systemdUnits.length > 0 ? systemdUnits : defaultUnits, args.endpointTimeoutSeconds),
    tls_certificates: await collectTlsCertificates(args.endpointTimeoutSeconds),
  };
}

function buildEvidence(report, checks) {
  const byCategory = (category) => checks.filter((check) => check.category === category).map(summarizeCheckForEvidence);
  return {
    deployment: {
      generated_at_utc: report.generated_at_utc,
      git_sha: report.git_sha,
      checks: [...byCategory("deployment"), ...byCategory("configuration")],
    },
    release: {
      generated_at_utc: report.generated_at_utc,
      git_sha: report.git_sha,
      checks: byCategory("release"),
      production_binary_parity_evidence_configured: Boolean(report.inputs.productionEvidence),
      release_evidence_dir_configured: Boolean(report.inputs.releaseEvidenceDir),
    },
    operational: {
      generated_at_utc: report.generated_at_utc,
      git_sha: report.git_sha,
      checks: byCategory("operational"),
    },
    recovery: {
      generated_at_utc: report.generated_at_utc,
      git_sha: report.git_sha,
      checks: checks
        .filter((check) => ["retention", "recovery"].includes(check.category))
        .map(summarizeCheckForEvidence),
    },
    build: {
      generated_at_utc: report.generated_at_utc,
      git_sha: report.git_sha,
      git_describe: report.git_describe,
      checks: [...byCategory("rust"), ...byCategory("dependency"), ...byCategory("repository")],
      tools: report.tools,
    },
  };
}

async function retentionRecoveryChecks() {
  const checks = [];
  checks.push(await runInternalCheck("retention.policy_documented", "retention", () => {
    const file = path.join(root, "docs", "RETENTION_POLICY_RU.md");
    const text = readTextIfExists(file);
    const required = ["Component", "Data type", "Default retention", "Cleanup method", "Recovery impact"];
    const missing = required.filter((marker) => !text.includes(marker));
    return {
      status: fs.existsSync(file) && missing.length === 0 ? "pass" : "fail",
      file: "docs/RETENTION_POLICY_RU.md",
      missing,
    };
  }));
  checks.push(await runInternalCheck("recovery.audit_documented", "recovery", () => {
    const required = ["RECOVERY_AUDIT.md", "DISASTER_RECOVERY_PROOF.md", "docs/RETENTION_POLICY_RU.md"];
    const missing = required.filter((file) => !fs.existsSync(path.join(root, file)));
    return { status: missing.length === 0 ? "pass" : "fail", missing };
  }));
  return checks;
}

function fileFingerprint(files) {
  const hashes = {};
  for (const file of files) {
    const full = path.join(root, file);
    if (fs.existsSync(full) && fs.statSync(full).isFile()) {
      hashes[file] = sha256File(full);
    }
  }
  return hashes;
}

async function buildFingerprints(files) {
  const configFiles = files.filter((file) =>
    file.endsWith(".json")
    || file.endsWith(".yml")
    || file.endsWith(".yaml")
    || file.endsWith(".toml")
    || file.endsWith(".service")
    || file.endsWith(".timer")
    || file.includes("grafana/")
  );
  const artifactFiles = [
    "adk-rust/target/release/detmir-readiness",
    "adk-rust/target/release/detmir-portal",
    "adk-rust/target/release/worktime-api",
    "adk-rust/target/release/aw-rus-healthd",
    "adk-rust/target/x86_64-pc-windows-gnu/release/aw-windows-telemetry.exe",
  ];
  return {
    config_hash: sha256Bytes(JSON.stringify(fileFingerprint(configFiles), Object.keys(fileFingerprint(configFiles)).sort())),
    artifact_hashes: fileFingerprint(artifactFiles),
    cargo_lock_sha256: fs.existsSync(path.join(root, "adk-rust", "Cargo.lock"))
      ? sha256File(path.join(root, "adk-rust", "Cargo.lock"))
      : null,
  };
}

function latestHistoryReports(historyDir, currentRunDir) {
  if (!fs.existsSync(historyDir)) return [];
  return fs
    .readdirSync(historyDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => path.join(historyDir, entry.name, "validation-report.json"))
    .filter((file) => path.dirname(file) !== currentRunDir && fs.existsSync(file))
    .sort();
}

function loadPreviousReport(historyDir, currentRunDir) {
  const reports = latestHistoryReports(historyDir, currentRunDir);
  if (reports.length === 0) return null;
  try {
    return JSON.parse(fs.readFileSync(reports[reports.length - 1], "utf8"));
  } catch {
    return null;
  }
}

function checkMap(report) {
  const map = new Map();
  for (const check of report?.checks || []) map.set(check.id, check);
  return map;
}

function detectRegressions(current, previous) {
  if (!previous) {
    return {
      baseline: "created",
      new_failures: [],
      recovered_failures: [],
      performance_degradations: [],
      new_alerts: [],
      dependency_changes: [],
      configuration_drift: [],
      artifact_changes: [],
      resource_growth: [],
    };
  }

  const prevChecks = checkMap(previous);
  const newFailures = [];
  const recovered = [];
  const performance = [];
  for (const check of current.checks) {
    const prev = prevChecks.get(check.id);
    if (!prev) continue;
    if (check.status === "fail" && prev.status !== "fail") newFailures.push({ id: check.id, previous: prev.status, current: check.status });
    if (check.status !== "fail" && prev.status === "fail") recovered.push({ id: check.id, previous: prev.status, current: check.status });
    if (
      Number.isFinite(check.duration_ms)
      && Number.isFinite(prev.duration_ms)
      && check.duration_ms > Math.max(prev.duration_ms * 1.5, prev.duration_ms + 5000)
    ) {
      performance.push({ id: check.id, previous_ms: prev.duration_ms, current_ms: check.duration_ms });
    }
  }

  const previousAlertIds = new Set((previous.alerts || []).map((alert) => alert.id));
  const newAlerts = (current.alerts || []).filter((alert) => !previousAlertIds.has(alert.id));
  const dependencyChanges = [];
  if (previous.fingerprints?.cargo_lock_sha256 && current.fingerprints?.cargo_lock_sha256
      && previous.fingerprints.cargo_lock_sha256 !== current.fingerprints.cargo_lock_sha256) {
    dependencyChanges.push({
      file: "adk-rust/Cargo.lock",
      previous_sha256: previous.fingerprints.cargo_lock_sha256,
      current_sha256: current.fingerprints.cargo_lock_sha256,
    });
  }
  const configurationDrift = [];
  if (previous.fingerprints?.config_hash && current.fingerprints?.config_hash
      && previous.fingerprints.config_hash !== current.fingerprints.config_hash) {
    configurationDrift.push({ previous_hash: previous.fingerprints.config_hash, current_hash: current.fingerprints.config_hash });
  }
  const artifactChanges = [];
  for (const [file, hash] of Object.entries(current.fingerprints?.artifact_hashes || {})) {
    const previousHash = previous.fingerprints?.artifact_hashes?.[file];
    if (previousHash && previousHash !== hash) {
      artifactChanges.push({ file, previous_sha256: previousHash, current_sha256: hash });
    }
  }

  const resourceGrowth = [];
  const growthBytes = Number.parseInt(process.env.AW_VALIDATION_PATH_GROWTH_WARN_BYTES || String(1024 * 1024 * 1024), 10);
  const growthRatio = Number.parseFloat(process.env.AW_VALIDATION_PATH_GROWTH_WARN_RATIO || "1.25");
  const previousSizes = new Map((previous.runtime_health?.path_sizes || []).map((item) => [item.path, item.size_bytes]));
  for (const item of current.runtime_health?.path_sizes || []) {
    const previousSize = previousSizes.get(item.path);
    if (!Number.isFinite(previousSize) || !Number.isFinite(item.size_bytes)) continue;
    const delta = item.size_bytes - previousSize;
    if (delta > growthBytes && item.size_bytes > previousSize * growthRatio) {
      resourceGrowth.push({
        path: item.path,
        previous_size_bytes: previousSize,
        current_size_bytes: item.size_bytes,
        growth_bytes: delta,
      });
    }
  }

  return {
    baseline: "compared",
    previous_generated_at_utc: previous.generated_at_utc,
    new_failures: newFailures,
    recovered_failures: recovered,
    performance_degradations: performance,
    new_alerts: newAlerts.map((alert) => ({ id: alert.id, severity: alert.severity, message: alert.message })),
    dependency_changes: dependencyChanges,
    configuration_drift: configurationDrift,
    artifact_changes: artifactChanges,
    resource_growth: resourceGrowth,
  };
}

function buildAlerts(report) {
  const alerts = [];
  const push = (id, severity, category, message, evidence = {}) => {
    alerts.push({ id, severity, category, message, evidence });
  };

  for (const check of report.checks) {
    if (check.status === "fail") {
      push(`validation_failed:${check.id}`, "critical", check.category, `Validation failed: ${check.id}`, {
        exit_code: check.exit_code,
        error: check.error,
        timed_out: check.timed_out,
      });
    }
  }

  const diskWarn = Number.parseFloat(process.env.AW_VALIDATION_DISK_USED_WARN_PCT || "85");
  const diskCrit = Number.parseFloat(process.env.AW_VALIDATION_DISK_USED_CRIT_PCT || "95");
  for (const disk of report.runtime_health.disks || []) {
    if (disk.used_pct >= diskCrit) push(`disk_critical:${disk.mount}`, "critical", "runtime", `Disk ${disk.mount} usage ${disk.used_pct}%`, disk);
    else if (disk.used_pct >= diskWarn) push(`disk_warning:${disk.mount}`, "warning", "runtime", `Disk ${disk.mount} usage ${disk.used_pct}%`, disk);
  }

  const memWarn = Number.parseFloat(process.env.AW_VALIDATION_MEMORY_USED_WARN_PCT || "90");
  const memCrit = Number.parseFloat(process.env.AW_VALIDATION_MEMORY_USED_CRIT_PCT || "97");
  const memory = report.runtime_health.memory || {};
  if (memory.used_pct >= memCrit) push("memory_critical", "critical", "runtime", `Memory usage ${memory.used_pct}%`, memory);
  else if (memory.used_pct >= memWarn) push("memory_warning", "warning", "runtime", `Memory usage ${memory.used_pct}%`, memory);

  for (const endpoint of report.runtime_health.endpoints || []) {
    if (endpoint.status === "fail") {
      const severity = endpoint.url ? "critical" : "warning";
      push(`endpoint_unavailable:${endpoint.name}`, severity, "runtime", `${endpoint.name} endpoint unavailable`, endpoint);
    }
  }

  for (const unit of report.runtime_health.systemd?.units || []) {
    if (unit.load_state === "loaded" && unit.active_state === "failed") {
      push(`systemd_failed:${unit.unit}`, "critical", "runtime", `${unit.unit} is failed`, unit);
    }
    if (unit.restarts > 0) {
      push(`systemd_restarts:${unit.unit}`, "warning", "runtime", `${unit.unit} restart count is ${unit.restarts}`, unit);
    }
  }

  const queueWarn = Number.parseInt(process.env.AW_VALIDATION_QUEUE_FILES_WARN || "1000", 10);
  for (const queue of report.runtime_health.queues || []) {
    if (queue.exists && Number.isFinite(queue.files) && queue.files > queueWarn) {
      push(`queue_backlog:${queue.path}`, "warning", "runtime", `Queue ${queue.path} has ${queue.files} files`, queue);
    }
  }

  const certWarnDays = Number.parseInt(process.env.AW_VALIDATION_CERT_EXPIRY_WARN_DAYS || "14", 10);
  for (const cert of report.runtime_health.tls_certificates || []) {
    if (cert.status === "fail") {
      push(`certificate_probe_failed:${cert.target}`, "warning", "runtime", `TLS certificate probe failed for ${cert.target}`, cert);
    } else if (Number.isFinite(cert.days_until_expiry) && cert.days_until_expiry <= certWarnDays) {
      push(`certificate_expiring:${cert.target}`, "warning", "runtime", `TLS certificate expires in ${cert.days_until_expiry} days for ${cert.target}`, cert);
    }
  }

  const backupEvidencePath = process.env.AW_VALIDATION_BACKUP_EVIDENCE_PATH || "";
  if (backupEvidencePath) {
    const maxAgeDays = Number.parseInt(process.env.AW_VALIDATION_BACKUP_EVIDENCE_MAX_AGE_DAYS || "7", 10);
    try {
      const stat = fs.statSync(backupEvidencePath);
      const ageDays = Math.floor((Date.now() - stat.mtimeMs) / (24 * 60 * 60 * 1000));
      if (ageDays > maxAgeDays) {
        push("backup_evidence_expired", "warning", "recovery", `Backup evidence is ${ageDays} days old`, {
          path: backupEvidencePath,
          age_days: ageDays,
          max_age_days: maxAgeDays,
        });
      }
    } catch (error) {
      push("backup_evidence_missing", "warning", "recovery", `Backup evidence path is unavailable: ${backupEvidencePath}`, {
        path: backupEvidencePath,
        error: error.message,
      });
    }
  }

  if (!report.inputs.productionEvidence) {
    push("binary_parity_evidence_missing", report.profile === "full" ? "warning" : "info", "release", "Production binary parity evidence was not provided");
  }
  if (!report.inputs.releaseEvidenceDir) {
    push("release_evidence_missing", report.profile === "full" ? "warning" : "info", "release", "Release evidence directory was not provided");
  }
  const secretCheck = report.checks.find((check) => check.id === "security.secret_pattern_check");
  if (secretCheck?.status === "fail") {
    push("secret_leak_detected", "critical", "security", "Secret pattern check failed", summarizeCheckForEvidence(secretCheck));
  }

  if (report.regressions) {
    for (const item of report.regressions.new_failures || []) {
      push(`new_failure:${item.id}`, "critical", "regression", `New validation failure: ${item.id}`, item);
    }
    for (const item of report.regressions.configuration_drift || []) {
      push("configuration_drift_detected", "warning", "regression", "Configuration fingerprint changed since previous run", item);
    }
    for (const item of report.regressions.artifact_changes || []) {
      push(`artifact_changed:${item.file}`, "warning", "regression", `Release artifact changed: ${item.file}`, item);
    }
    for (const item of report.regressions.resource_growth || []) {
      push(`resource_growth:${item.path}`, "warning", "regression", `Runtime path grew by ${item.growth_bytes} bytes: ${item.path}`, item);
    }
  }

  return alerts;
}

function calculateScore(report) {
  const checks = report.checks || [];
  if (checks.length === 0) return 0;
  const passed = checks.filter((check) => check.status === "pass").length;
  const skipped = checks.filter((check) => check.status === "skipped").length;
  const base = (passed + skipped * 0.5) / checks.length;
  const criticalPenalty = (report.alerts || []).filter((alert) => alert.severity === "critical").length * 0.08;
  const warningPenalty = (report.alerts || []).filter((alert) => alert.severity === "warning").length * 0.02;
  return Math.max(0, Math.min(100, Math.round((base - criticalPenalty - warningPenalty) * 100)));
}

function renderMarkdown(report) {
  const lines = [];
  lines.push("# AWatch-rus Full Validation Report");
  lines.push("");
  lines.push(`Generated: ${report.generated_at_utc}`);
  lines.push(`Host: ${report.host_identifier}`);
  lines.push(`Git SHA: ${report.git_sha}`);
  lines.push(`Version: ${report.version}`);
  lines.push(`Profile: ${report.profile}`);
  lines.push(`Status: ${report.status}`);
  lines.push(`Validation score: ${report.validation_score}`);
  lines.push(`Release readiness score: ${report.release_readiness_score}`);
  lines.push("");
  lines.push("## Summary");
  lines.push("");
  lines.push(`- Checks: ${report.summary.total}`);
  lines.push(`- Passed: ${report.summary.pass}`);
  lines.push(`- Failed: ${report.summary.fail}`);
  lines.push(`- Skipped: ${report.summary.skipped}`);
  lines.push(`- Alerts: ${report.alerts.length}`);
  lines.push("");
  lines.push("## Alerts");
  lines.push("");
  if (report.alerts.length === 0) {
    lines.push("No active alerts.");
  } else {
    for (const alert of report.alerts) {
      lines.push(`- ${alert.severity}: ${alert.id} - ${alert.message}`);
    }
  }
  lines.push("");
  lines.push("## Regressions");
  lines.push("");
  lines.push(`Baseline: ${report.regressions.baseline}`);
  for (const key of ["new_failures", "performance_degradations", "dependency_changes", "configuration_drift", "artifact_changes"]) {
    lines.push(`- ${key}: ${(report.regressions[key] || []).length}`);
  }
  lines.push(`- resource_growth: ${(report.regressions.resource_growth || []).length}`);
  lines.push("");
  lines.push("## Checks");
  lines.push("");
  lines.push("| Check | Category | Status | Duration ms |");
  lines.push("| --- | --- | --- | ---: |");
  for (const check of report.checks) {
    lines.push(`| ${check.id} | ${check.category} | ${check.status} | ${check.duration_ms ?? 0} |`);
  }
  lines.push("");
  lines.push("## Runtime");
  lines.push("");
  lines.push(`- CPU load 1m: ${report.runtime_health.cpu.load_1m}`);
  lines.push(`- Memory used: ${report.runtime_health.memory.used_pct}%`);
  const topDisk = (report.runtime_health.disks || [])[0];
  if (topDisk) lines.push(`- Highest disk usage: ${topDisk.mount} ${topDisk.used_pct}%`);
  lines.push("");
  lines.push("## Output Files");
  lines.push("");
  for (const [name, file] of Object.entries(report.outputs || {})) {
    lines.push(`- ${name}: ${file}`);
  }
  lines.push("");
  return `${lines.join("\n")}\n`;
}

function pruneHistory(historyDir, retentionDays, nowMs) {
  if (!fs.existsSync(historyDir)) return [];
  const cutoff = nowMs - retentionDays * 24 * 60 * 60 * 1000;
  const removed = [];
  for (const entry of fs.readdirSync(historyDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const full = path.join(historyDir, entry.name);
    const stat = fs.statSync(full);
    if (stat.mtimeMs < cutoff) {
      fs.rmSync(full, { recursive: true, force: true });
      removed.push(entry.name);
    }
  }
  return removed;
}

function buildDashboard(report) {
  return {
    generated_at_utc: report.generated_at_utc,
    host_identifier: report.host_identifier,
    git_sha: report.git_sha,
    version: report.version,
    overall_health: report.status,
    overall_readiness_score: report.validation_score,
    release_readiness_score: report.release_readiness_score,
    system_uptime_seconds: report.runtime_health.host.uptime_seconds,
    active_alerts: report.alerts,
    recent_regressions: report.regressions,
    resource_usage: {
      cpu: report.runtime_health.cpu,
      memory: report.runtime_health.memory,
      disks: report.runtime_health.disks,
    },
    service_state: report.runtime_health.systemd,
    backup_state: {
      release_evidence_configured: Boolean(report.inputs.releaseEvidenceDir),
      recovery_documents_present: report.checks.find((check) => check.id === "recovery.audit_documented")?.status === "pass",
    },
    recovery_readiness: report.evidence.recovery,
  };
}

async function executeValidation(args) {
  const generatedAt = utcNow();
  const outputDir = path.resolve(args.outputDir);
  const historyDir = path.join(outputDir, "history");
  const runDir = path.join(historyDir, stampForPath(generatedAt));
  const latestDir = path.join(outputDir, "latest");
  mkdirp(runDir);
  mkdirp(latestDir);

  const git = await collectGitContext();
  const files = await trackedFiles();
  const tools = {};
  for (const tool of ["git", "node", "python3", "bash", "cargo", "ruby", "shellcheck", "ansible-playbook", "openssl", "systemctl"]) {
    tools[tool] = commandExists(tool);
  }

  const checks = [];
  checks.push(await runInternalCheck("repository.tracked_files_inventory", "repository", () => ({
    status: files.length > 0 ? "pass" : "fail",
    tracked_files: files.length,
  })));
  checks.push(await runInternalCheck("configuration.json_parse", "configuration", () => validateJsonFiles(files)));
  checks.push(await runInternalCheck("documentation.markdown_links", "documentation", () => validateMarkdownLinks(files)));
  checks.push(await runInternalCheck("configuration.yaml_parse", "configuration", () => validateYamlFiles(files, args.commandTimeoutSeconds)));
  checks.push(...await retentionRecoveryChecks());

  for (const def of commandDefinitions(args)) {
    checks.push(await runCommand(def));
  }

  const runtimeHealth = await collectRuntimeDiagnostics(args);
  const fingerprints = await buildFingerprints(files);

  const summary = {
    total: checks.length,
    pass: checks.filter((check) => check.status === "pass").length,
    fail: checks.filter((check) => check.status === "fail").length,
    skipped: checks.filter((check) => check.status === "skipped").length,
  };

  const report = {
    schema_version: 1,
    generated_at_utc: generatedAt,
    product: "AWatch-rus",
    profile: args.profile,
    host_identifier: os.hostname(),
    version: git.git_describe,
    ...git,
    inputs: {
      outputDir,
      retentionDays: args.retentionDays,
      productionEvidence: args.productionEvidence || null,
      releaseEvidenceDir: args.releaseEvidenceDir || null,
      releaseAssetDir: args.releaseAssetDir || null,
    },
    tools,
    summary,
    status: "pending",
    checks,
    runtime_health: runtimeHealth,
    fingerprints,
    regressions: null,
    alerts: [],
    validation_score: 0,
    release_readiness_score: 0,
    outputs: {},
  };

  report.evidence = buildEvidence(report, checks);
  const previous = loadPreviousReport(historyDir, runDir);
  report.regressions = detectRegressions(report, previous);
  report.alerts = buildAlerts(report);
  report.validation_score = calculateScore(report);
  report.release_readiness_score = Math.max(
    0,
    report.validation_score
      - (report.inputs.productionEvidence ? 0 : 5)
      - (report.inputs.releaseEvidenceDir ? 0 : 5),
  );
  const criticalAlerts = report.alerts.filter((alert) => criticalAlertSeverities.has(alert.severity)).length;
  report.status = summary.fail === 0 && criticalAlerts === 0 ? "pass" : "fail";

  const dashboard = buildDashboard(report);
  const outputs = {
    validation_report_json: path.join(runDir, "validation-report.json"),
    validation_report_md: path.join(runDir, "validation-report.md"),
    production_health_json: path.join(runDir, "production-health.json"),
    deployment_health_json: path.join(runDir, "deployment-health.json"),
    runtime_health_json: path.join(runDir, "runtime-health.json"),
    release_evidence_json: path.join(runDir, "release-evidence.json"),
    operational_evidence_json: path.join(runDir, "operational-evidence.json"),
    recovery_evidence_json: path.join(runDir, "recovery-evidence.json"),
    build_evidence_json: path.join(runDir, "build-evidence.json"),
    operational_dashboard_json: path.join(runDir, "operational-dashboard.json"),
    operational_dashboard_md: path.join(runDir, "operational-dashboard.md"),
  };
  report.outputs = Object.fromEntries(Object.entries(outputs).map(([key, value]) => [key, normalizeRelative(value)]));

  writeJson(outputs.validation_report_json, report);
  writeText(outputs.validation_report_md, renderMarkdown(report));
  writeJson(outputs.production_health_json, {
    generated_at_utc: report.generated_at_utc,
    git_sha: report.git_sha,
    host_identifier: report.host_identifier,
    status: report.status,
    alerts: report.alerts,
    runtime_health: report.runtime_health,
  });
  writeJson(outputs.deployment_health_json, report.evidence.deployment);
  writeJson(outputs.runtime_health_json, report.runtime_health);
  writeJson(outputs.release_evidence_json, report.evidence.release);
  writeJson(outputs.operational_evidence_json, report.evidence.operational);
  writeJson(outputs.recovery_evidence_json, report.evidence.recovery);
  writeJson(outputs.build_evidence_json, report.evidence.build);
  writeJson(outputs.operational_dashboard_json, dashboard);
  writeText(outputs.operational_dashboard_md, renderDashboardMarkdown(dashboard));

  fs.rmSync(latestDir, { recursive: true, force: true });
  mkdirp(latestDir);
  for (const [name, file] of Object.entries(outputs)) {
    const latestName = path.basename(file);
    copyFileCompat(file, path.join(latestDir, latestName));
    report.outputs[`latest_${name}`] = normalizeRelative(path.join(latestDir, latestName));
  }
  writeJson(outputs.validation_report_json, report);
  writeText(outputs.validation_report_md, renderMarkdown(report));
  writeJson(path.join(latestDir, "validation-report.json"), report);
  writeText(path.join(latestDir, "validation-report.md"), renderMarkdown(report));

  report.history_retention = {
    history_dir: normalizeRelative(historyDir),
    retention_days: args.retentionDays,
    pruned_runs: pruneHistory(historyDir, args.retentionDays, Date.now()),
  };
  writeJson(outputs.validation_report_json, report);
  writeJson(path.join(latestDir, "validation-report.json"), report);

  return report;
}

function renderDashboardMarkdown(dashboard) {
  const lines = [];
  lines.push("# AWatch-rus Operational Dashboard");
  lines.push("");
  lines.push(`Generated: ${dashboard.generated_at_utc}`);
  lines.push(`Host: ${dashboard.host_identifier}`);
  lines.push(`Git SHA: ${dashboard.git_sha}`);
  lines.push(`Version: ${dashboard.version}`);
  lines.push(`Overall health: ${dashboard.overall_health}`);
  lines.push(`Overall readiness score: ${dashboard.overall_readiness_score}`);
  lines.push(`Release readiness score: ${dashboard.release_readiness_score}`);
  lines.push(`System uptime seconds: ${dashboard.system_uptime_seconds}`);
  lines.push("");
  lines.push("## Active Alerts");
  lines.push("");
  if (dashboard.active_alerts.length === 0) lines.push("No active alerts.");
  for (const alert of dashboard.active_alerts) lines.push(`- ${alert.severity}: ${alert.message}`);
  lines.push("");
  lines.push("## Resources");
  lines.push("");
  lines.push(`- CPU load 1m: ${dashboard.resource_usage.cpu.load_1m}`);
  lines.push(`- Memory used: ${dashboard.resource_usage.memory.used_pct}%`);
  for (const disk of dashboard.resource_usage.disks.slice(0, 5)) {
    lines.push(`- Disk ${disk.mount}: ${disk.used_pct}%`);
  }
  lines.push("");
  return `${lines.join("\n")}\n`;
}

async function selfTest() {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "aw-full-validation-selftest-"));
  try {
    const commandOk = await runCommand({
      id: "self.command_ok",
      category: "self",
      command: process.execPath,
      args: ["-e", "process.stdout.write('ok')"],
      cwd: root,
      timeoutSeconds: 10,
    });
    if (commandOk.status !== "pass" || commandOk.stdout !== "ok") throw new Error("command runner pass case failed");

    const commandFail = await runCommand({
      id: "self.command_fail",
      category: "self",
      command: process.execPath,
      args: ["-e", "process.exit(3)"],
      cwd: root,
      timeoutSeconds: 10,
    });
    if (commandFail.status !== "fail" || commandFail.exit_code !== 3) throw new Error("command runner fail case failed");

    const previous = {
      generated_at_utc: "2026-07-01T00:00:00Z",
      checks: [{ id: "x", status: "pass", duration_ms: 100 }],
      alerts: [],
      fingerprints: {
        config_hash: "a",
        artifact_hashes: { bin: "1" },
        cargo_lock_sha256: "lock-a",
      },
    };
    const current = {
      checks: [{ id: "x", status: "fail", duration_ms: 1000 }],
      alerts: [{ id: "alert-x", severity: "critical", message: "x" }],
      fingerprints: {
        config_hash: "b",
        artifact_hashes: { bin: "2" },
        cargo_lock_sha256: "lock-b",
      },
    };
    const regressions = detectRegressions(current, previous);
    if (regressions.new_failures.length !== 1) throw new Error("new failure regression not detected");
    if (regressions.configuration_drift.length !== 1) throw new Error("configuration drift not detected");
    if (regressions.artifact_changes.length !== 1) throw new Error("artifact change not detected");
    if (regressions.dependency_changes.length !== 1) throw new Error("dependency change not detected");

    const fakeReport = {
      checks: [{ id: "x", category: "self", status: "fail" }],
      runtime_health: {
        disks: [{ mount: "/", used_pct: 99 }],
        memory: { used_pct: 98 },
        endpoints: [{ name: "activitywatch", url: "http://127.0.0.1", status: "fail" }],
        systemd: { units: [] },
      },
      inputs: {},
      profile: "full",
      regressions,
    };
    const alerts = buildAlerts(fakeReport);
    if (!alerts.some((alert) => alert.id === "validation_failed:x")) throw new Error("validation failure alert missing");
    if (!alerts.some((alert) => alert.id.startsWith("disk_critical:"))) throw new Error("disk alert missing");

    const markdown = renderMarkdown({
      generated_at_utc: utcNow(),
      host_identifier: "self",
      git_sha: "0".repeat(40),
      version: "self",
      profile: "quick",
      status: "pass",
      validation_score: 100,
      release_readiness_score: 100,
      summary: { total: 1, pass: 1, fail: 0, skipped: 0 },
      alerts: [],
      regressions: { baseline: "created", new_failures: [], performance_degradations: [], dependency_changes: [], configuration_drift: [], artifact_changes: [] },
      checks: [{ id: "self", category: "self", status: "pass", duration_ms: 1 }],
      runtime_health: { cpu: { load_1m: 0 }, memory: { used_pct: 0 }, disks: [] },
      outputs: {},
    });
    if (!markdown.includes("AWatch-rus Full Validation Report")) throw new Error("markdown renderer failed");

    writeJson(path.join(tmp, "report.json"), { ok: true });
    JSON.parse(fs.readFileSync(path.join(tmp, "report.json"), "utf8"));
    console.log("full_validation_orchestrator_self_test=ok");
    return 0;
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    console.error(usage());
    return 2;
  }
  if (args.help) {
    process.stdout.write(usage());
    return 0;
  }
  if (args.selfTest) return selfTest();

  const report = await executeValidation(args);
  const summary = {
    status: report.status,
    validation_score: report.validation_score,
    release_readiness_score: report.release_readiness_score,
    checks: report.summary,
    alerts: report.alerts.length,
    output_dir: normalizeRelative(path.join(path.resolve(args.outputDir), "latest")),
  };
  if (args.json) {
    process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
  } else {
    process.stdout.write(
      `full_validation=${summary.status} score=${summary.validation_score} release_score=${summary.release_readiness_score} `
      + `checks=${summary.checks.total} failed=${summary.checks.fail} alerts=${summary.alerts} output=${summary.output_dir}\n`,
    );
  }
  if (args.allowFailures) return 0;
  return report.status === "pass" ? 0 : 1;
}

main().then((code) => {
  process.exitCode = code;
}).catch((error) => {
  console.error(`full_validation_orchestrator=fail error=${error.stack || error.message}`);
  process.exitCode = 1;
});
