#!/usr/bin/env node
import { createRequire } from "node:module";
import { execFile } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { promisify } from "node:util";

const require = createRequire(import.meta.url);
const execFileAsync = promisify(execFile);
const homeDir = process.env.HOME || "";

function loadPlaywright() {
  const candidates = [
    "playwright",
    "playwright-core",
    process.env.PLAYWRIGHT_CORE_MODULE,
    homeDir ? path.join(homeDir, ".agents/skills/playwright/node_modules/playwright-core") : "",
  ].filter(Boolean);
  const errors = [];
  for (const candidate of candidates) {
    try {
      return require(candidate);
    } catch (error) {
      errors.push(`${candidate}: ${error.message}`);
    }
  }
  return null;
}

function firstExisting(candidates) {
  return candidates.find((item) => item && fs.existsSync(item)) || "";
}

function env(name, fallback) {
  const value = process.env[name];
  return value && value.trim() ? value.trim() : fallback;
}

function normalizeBase(url) {
  return url.replace(/\/+$/, "");
}

function safeName(value) {
  return value.replace(/[^a-zA-Z0-9_.-]+/g, "-").replace(/^-|-$/g, "");
}

function envInt(name, fallback) {
  const value = Number(env(name, String(fallback)));
  return Number.isFinite(value) ? value : fallback;
}

function isRunDirectoryName(name) {
  return /^20\d{2}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}-\d{3}Z$/.test(name);
}

function pruneOutputRuns(outRoot, keepRuns) {
  if (keepRuns <= 0 || !fs.existsSync(outRoot)) {
    return [];
  }
  const entries = fs
    .readdirSync(outRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && isRunDirectoryName(entry.name))
    .map((entry) => entry.name)
    .sort();
  const stale = entries.slice(0, Math.max(0, entries.length - keepRuns));
  for (const name of stale) {
    fs.rmSync(path.join(outRoot, name), { recursive: true, force: true });
  }
  return stale;
}

function decodeHtmlEntities(value) {
  return value
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
}

function htmlToText(html) {
  return decodeHtmlEntities(
    html
      .replace(/<script[\s\S]*?<\/script>/gi, " ")
      .replace(/<style[\s\S]*?<\/style>/gi, " ")
      .replace(/<[^>]+>/g, " ")
      .replace(/\s+/g, " ")
      .trim(),
  );
}

function commandInPath(name) {
  const dirs = (process.env.PATH || "").split(path.delimiter);
  for (const dir of dirs) {
    const candidate = path.join(dir, name);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return "";
}

function findChromiumExecutable(explicitPath) {
  return firstExisting([
    explicitPath,
    homeDir ? path.join(homeDir, ".cache/ms-playwright/chromium-1217/chrome-linux64/chrome") : "",
    homeDir ? path.join(homeDir, ".cache/ms-playwright/chromium-1208/chrome-linux64/chrome") : "",
    homeDir ? path.join(homeDir, ".cache/rod/browser/chromium-1321438/chrome") : "",
    commandInPath("chromium"),
    commandInPath("chromium-browser"),
    commandInPath("google-chrome"),
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/usr/bin/google-chrome",
  ]);
}

function isBenignConsoleError(text) {
  return text.includes("Failed to load resource: the server responded with a status of 404 (Not Found)");
}

function isBenignRequestFailure(failure) {
  return failure.error === "net::ERR_ABORTED";
}

async function waitForTextMarkers(page, markers, timeoutMs) {
  if (!markers.length) {
    return;
  }
  try {
    await page.waitForFunction(
      (expected) => {
        const text = `${document.title}\n${document.body ? document.body.innerText : ""}`;
        return expected.every((marker) => text.includes(marker));
      },
      markers,
      { timeout: timeoutMs },
    );
  } catch {
    // The final assertion below reports the exact missing markers.
  }
}

async function runPageCheck(browser, spec, runDir) {
  const page = await browser.newPage({
    viewport: { width: 1366, height: 768 },
    locale: "ru-RU",
    timezoneId: "Europe/Moscow",
  });
  const started = Date.now();
  const consoleErrors = [];
  const pageErrors = [];
  const requestFailures = [];
  const badResponses = [];
  const responses = [];

  page.on("console", (message) => {
    if (message.type() === "error") {
      const text = message.text();
      if (!isBenignConsoleError(text)) {
        consoleErrors.push(text);
      }
    }
  });
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });
  page.on("requestfailed", (request) => {
    const url = request.url();
    if (spec.relevantHosts.some((host) => url.startsWith(host))) {
      const failure = { url, error: request.failure()?.errorText || "unknown" };
      if (!isBenignRequestFailure(failure)) {
        requestFailures.push(failure);
      }
    }
  });
  page.on("response", (response) => {
    const url = response.url();
    if (!spec.relevantHosts.some((host) => url.startsWith(host))) {
      return;
    }
    const item = { url, status: response.status() };
    responses.push(item);
    if (response.status() >= 400) {
      badResponses.push(item);
    }
  });

  let status = null;
  let title = "";
  let bodyText = "";
  let screenshot = "";
  let error = "";
  try {
    const response = await page.goto(spec.url, { waitUntil: "commit", timeout: spec.timeoutMs });
    status = response ? response.status() : null;
    await waitForTextMarkers(page, spec.requiredText, spec.renderTimeoutMs);
    await page.waitForTimeout(spec.settleMs);
    title = await page.title();
    bodyText = await page.locator("body").innerText({ timeout: 5000 });
    screenshot = path.join(runDir, `${safeName(spec.name)}.png`);
    await page.screenshot({ path: screenshot, fullPage: true });
  } catch (caught) {
    error = caught && caught.message ? caught.message : String(caught);
    try {
      screenshot = path.join(runDir, `${safeName(spec.name)}-failed.png`);
      await page.screenshot({ path: screenshot, fullPage: true });
    } catch {
      screenshot = "";
    }
  } finally {
    await page.close();
  }

  const visibleText = `${title}\n${bodyText}`;
  const missingMarkers = spec.requiredText.filter((marker) => !visibleText.includes(marker));
  const ok =
    !error &&
    status !== null &&
    status >= 200 &&
    status < 300 &&
    bodyText.length >= spec.minBodyText &&
    missingMarkers.length === 0 &&
    consoleErrors.length === 0 &&
    pageErrors.length === 0 &&
    requestFailures.length === 0 &&
    badResponses.length === 0;

  return {
    engine: "playwright",
    name: spec.name,
    ok,
    url: spec.url,
    status,
    title,
    body_text_length: bodyText.length,
    missing_markers: missingMarkers,
    console_errors: consoleErrors,
    page_errors: pageErrors,
    request_failures: requestFailures,
    bad_responses: badResponses,
    response_count: responses.length,
    latency_ms: Date.now() - started,
    screenshot,
    error,
  };
}

async function runChromiumCliPageCheck(chromiumPath, spec, runDir) {
  const started = Date.now();
  const screenshot = path.join(runDir, `${safeName(spec.name)}.png`);
  const profileDir = path.join(runDir, `${safeName(spec.name)}-profile`);
  const screenshotRequired = spec.cliScreenshotRequired !== false;
  fs.mkdirSync(profileDir, { recursive: true });
  const browserArgs = [
    "--headless",
    "--no-sandbox",
    "--noerrdialogs",
    "--disable-gpu",
    "--disable-crash-reporter",
    "--disable-crashpad",
    "--disable-dev-shm-usage",
    `--user-data-dir=${profileDir}`,
    `--virtual-time-budget=${spec.renderTimeoutMs + spec.settleMs}`,
  ];

  let status = null;
  let error = "";
  let fetchedText = "";
  try {
    const controller = AbortSignal.timeout(spec.timeoutMs);
    const response = await fetch(spec.url, { signal: controller });
    status = response.status;
    fetchedText = await response.text();
  } catch (caught) {
    error = caught && caught.message ? caught.message : String(caught);
  }

  let html = "";
  try {
    const { stdout } = await execFileAsync(chromiumPath, [...browserArgs, "--dump-dom", spec.url], {
      timeout: spec.timeoutMs + spec.renderTimeoutMs + spec.settleMs + 10000,
      maxBuffer: 10 * 1024 * 1024,
    });
    html = stdout || "";
  } catch (caught) {
    const message = caught && caught.message ? caught.message : String(caught);
    error = error ? `${error}; ${message}` : message;
  }

  if (screenshotRequired) {
    try {
      await execFileAsync(
        chromiumPath,
        [...browserArgs, "--window-size=1366,768", `--screenshot=${screenshot}`, spec.url],
        {
          timeout: spec.timeoutMs + spec.renderTimeoutMs + spec.settleMs + 10000,
          maxBuffer: 1024 * 1024,
        },
      );
    } catch (caught) {
      const message = caught && caught.message ? caught.message : String(caught);
      error = error ? `${error}; screenshot: ${message}` : `screenshot: ${message}`;
    }
  }

  const rawHtml = html || fetchedText;
  const titleMatch = rawHtml.match(/<title[^>]*>([\s\S]*?)<\/title>/i);
  const title = titleMatch ? decodeHtmlEntities(titleMatch[1].trim()) : "";
  const bodyText = htmlToText(rawHtml);
  const visibleText = `${title}\n${bodyText}\n${rawHtml}`;
  const requiredText = spec.cliRequiredText || spec.requiredText;
  const minBodyText = spec.cliMinBodyText || spec.minBodyText;
  const missingMarkers = requiredText.filter((marker) => !visibleText.includes(marker));
  const ok =
    !error &&
    status !== null &&
    status >= 200 &&
    status < 300 &&
    bodyText.length >= minBodyText &&
    missingMarkers.length === 0 &&
    (!screenshotRequired || fs.existsSync(screenshot));

  return {
    engine: "chromium-cli",
    name: spec.name,
    ok,
    url: spec.url,
    status,
    title,
    body_text_length: bodyText.length,
    missing_markers: missingMarkers,
    console_errors: [],
    page_errors: [],
    request_failures: [],
    bad_responses: [],
    response_count: 0,
    latency_ms: Date.now() - started,
    screenshot_required: screenshotRequired,
    screenshot: fs.existsSync(screenshot) ? screenshot : "",
    error,
  };
}

async function runPageCheckWithRetries(runOnce, spec, maxRetries) {
  let result = await runOnce(spec);
  for (let attempt = 1; !result.ok && attempt <= maxRetries; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 1000 * attempt));
    const retry = await runOnce({ ...spec, name: `${spec.name}_retry${attempt}` });
    retry.retry_of = spec.name;
    retry.retry_attempt = attempt;
    if (retry.ok) {
      retry.name = spec.name;
      retry.recovered_after_retry = attempt;
      return retry;
    }
    result = retry;
    result.retry_of = spec.name;
    result.retry_attempt = attempt;
  }
  return result;
}

async function main() {
  const requestedEngine = env("AW_BROWSER_SMOKE_ENGINE", "auto");
  const playwright = requestedEngine === "chromium-cli" ? null : loadPlaywright();
  const awBase = normalizeBase(env("AW_BROWSER_SMOKE_AW_BASE", env("AW_SMOKE_AW_SERVER", "http://127.0.0.1:5600")));
  const worktimeBase = normalizeBase(env("AW_BROWSER_SMOKE_WORKTIME_BASE", env("AW_SMOKE_WORKTIME_API", "http://127.0.0.1:5610")));
  const host = env("AW_BROWSER_SMOKE_HOST", env("AW_SMOKE_SOURCE_HOSTNAME", "HOST-EXAMPLE"));
  const timeoutMs = Number(env("AW_BROWSER_SMOKE_TIMEOUT_MS", "20000"));
  const settleMs = Number(env("AW_BROWSER_SMOKE_SETTLE_MS", "6000"));
  const renderTimeoutMs = Number(env("AW_BROWSER_SMOKE_RENDER_TIMEOUT_MS", "15000"));
  const pageRetries = Number(env("AW_BROWSER_SMOKE_PAGE_RETRIES", "1"));
  const keepRuns = envInt("AW_BROWSER_SMOKE_KEEP_RUNS", 24);
  const outRoot = env("AW_BROWSER_SMOKE_OUTPUT_DIR", path.resolve("output", "browser-smoke"));
  const runId = new Date().toISOString().replace(/[:.]/g, "-");
  const runDir = path.join(outRoot, runId);
  fs.mkdirSync(outRoot, { recursive: true });
  const prunedRuns = pruneOutputRuns(outRoot, keepRuns);
  fs.mkdirSync(runDir, { recursive: true });

  const executablePath = findChromiumExecutable(env(
    "PLAYWRIGHT_CHROMIUM_EXECUTABLE",
    "",
  ));

  const launchOptions = {
    headless: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage"],
  };
  if (executablePath) {
    launchOptions.executablePath = executablePath;
  }

  const relevantHosts = [awBase, worktimeBase];
  const specs = [
    {
      name: "aw_webui_home",
      url: `${awBase}/`,
      relevantHosts,
      requiredText: ["Активность", "Windows RDP", host, "DLP"],
      cliRequiredText: ["ActivityWatch", "ru-patch-v5.js", "aw-report-links"],
      cliMinBodyText: 100,
      cliScreenshotRequired: false,
      minBodyText: 500,
      timeoutMs,
      settleMs,
      renderTimeoutMs,
    },
    {
      name: "worktime_today_html",
      url: `${worktimeBase}/reports/worktime/today?format=html&day=today&host=${encodeURIComponent(host)}&allow_stale=1`,
      relevantHosts,
      requiredText: ["AW-rus", "Отчёт", "RDP"],
      minBodyText: 500,
      timeoutMs,
      settleMs: 1000,
      renderTimeoutMs,
    },
    {
      name: "worktime_management_html",
      url: `${worktimeBase}/reports/worktime/management?format=html&day=today&host=${encodeURIComponent(host)}&allow_stale=1`,
      relevantHosts,
      requiredText: ["AW-rus", "Управленческий", "RDP"],
      minBodyText: 500,
      timeoutMs,
      settleMs: 1000,
      renderTimeoutMs,
    },
  ];

  const pages = [];
  if (playwright) {
    const { chromium } = playwright;
    const browser = await chromium.launch(launchOptions);
    try {
      for (const spec of specs) {
        pages.push(await runPageCheckWithRetries((item) => runPageCheck(browser, item, runDir), spec, pageRetries));
      }
    } finally {
      await browser.close();
    }
  } else {
    if (requestedEngine === "playwright") {
      throw new Error("AW_BROWSER_SMOKE_ENGINE=playwright requested, but Playwright could not be loaded");
    }
    if (!executablePath) {
      throw new Error("Unable to load Playwright and no Chromium executable found");
    }
    for (const spec of specs) {
      pages.push(await runPageCheckWithRetries((item) => runChromiumCliPageCheck(executablePath, item, runDir), spec, pageRetries));
    }
  }

  const ok = pages.every((page) => page.ok);
  const result = {
    ok,
    engine: pages[0]?.engine || "unknown",
    generated_at_utc: new Date().toISOString(),
    aw_base: awBase,
    worktime_base: worktimeBase,
    host,
    output_dir: runDir,
    retention: {
      keep_runs: keepRuns,
      pruned_runs: prunedRuns,
    },
    pages,
  };
  const jsonPath = path.join(runDir, "result.json");
  const latestPath = path.join(outRoot, "latest-result.json");
  result.result_json = jsonPath;
  result.latest_result_json = latestPath;
  fs.writeFileSync(jsonPath, `${JSON.stringify(result, null, 2)}\n`, "utf8");
  fs.writeFileSync(latestPath, `${JSON.stringify(result, null, 2)}\n`, "utf8");
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  return ok ? 0 : 2;
}

main()
  .then((code) => {
    process.exitCode = code;
  })
  .catch((error) => {
    process.stdout.write(
      `${JSON.stringify(
        {
          ok: false,
          generated_at_utc: new Date().toISOString(),
          error: error && error.message ? error.message : String(error),
        },
        null,
        2,
      )}\n`,
    );
    process.exitCode = 2;
  });
