#!/usr/bin/env node
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function env(name, fallback = "") {
  const value = process.env[name];
  return value && value.trim() ? value.trim() : fallback;
}

function loadPlaywright() {
  const home = process.env.HOME || "";
  const candidates = [
    "playwright",
    "playwright-core",
    process.env.PLAYWRIGHT_CORE_MODULE,
    home ? path.join(home, ".agents/skills/playwright/node_modules/playwright-core") : "",
    home ? path.join(home, ".codex/skills/playwright/node_modules/playwright-core") : "",
  ].filter(Boolean);
  for (const candidate of candidates) {
    try {
      return require(candidate);
    } catch {
      // Try the next local Playwright installation.
    }
  }
  throw new Error("Playwright module not found");
}

function authHeaders() {
  const explicit = env("AWATCH_BROWSER_SMOKE_AUTH_HEADER", env("DETMIR_PORTAL_SMOKE_AUTH_HEADER"));
  if (explicit) return { Authorization: explicit };
  const basic = env("AWATCH_BROWSER_SMOKE_BASIC_AUTH", env("DETMIR_PORTAL_SMOKE_BASIC_AUTH", env("DETMIR_BASIC_AUTH")));
  if (basic) return { Authorization: `Basic ${basic}` };
  return {};
}

function normalizeBaseUrl(raw) {
  const value = raw.endsWith("/") ? raw : `${raw}/`;
  return new URL(value).toString();
}

function missingMarkers(text, markers) {
  const normalized = String(text || "").toLocaleLowerCase("ru-RU");
  return markers.filter((marker) => !normalized.includes(String(marker).toLocaleLowerCase("ru-RU")));
}

async function waitForPortalReady(page, timeout) {
  await page.waitForSelector("#content", { timeout });
  await page.waitForSelector("[data-view-mode]", { timeout });
  await page.waitForFunction(
    () => {
      const status = document.querySelector("#loadingStatus");
      return !status || status.dataset.loadStatus === "READY" || document.body.innerText.includes("Данные готовы");
    },
    null,
    { timeout },
  );
}

async function switchView(page, selector, timeout) {
  await page.click(selector, { timeout });
  await page.waitForFunction(
    (viewMode) => document.querySelector(`[data-view-mode="${viewMode}"]`)?.classList.contains("is-active"),
    selector.match(/data-view-mode="([^"]+)"/)?.[1] || "",
    { timeout },
  );
}

async function waitForViewContent(page, spec, timeout) {
  await page.waitForFunction(
    ({ markers, minLength }) => {
      const content = document.querySelector("#content")?.innerText || "";
      if (content.trim().length < minLength) return false;
      const normalized = content.toLocaleLowerCase("ru-RU");
      return markers.every((marker) => normalized.includes(String(marker).toLocaleLowerCase("ru-RU")));
    },
    { markers: spec.required, minLength: 120 },
    { timeout },
  );
}

async function checkView(page, spec, artifactDir, timeout) {
  await switchView(page, `[data-view-mode="${spec.viewMode}"]`, timeout);
  await waitForViewContent(page, spec, timeout);
  const content = await page.locator("#content").innerText({ timeout });
  const body = await page.locator("body").innerText({ timeout });
  const missing = missingMarkers(content, spec.required);
  const forbidden = [
    "Internal Server Error",
    "HTTP 500",
    "Ошибка рендера",
    "undefined",
    "NaN",
  ].filter((marker) => body.includes(marker));
  const notEmpty = content.trim().length >= 120;
  const screenshot = path.join(artifactDir, spec.screenshot);
  await page.screenshot({ path: screenshot, fullPage: true });
  return {
    name: spec.name,
    view_mode: spec.viewMode,
    ok: missing.length === 0 && forbidden.length === 0 && notEmpty,
    screenshot: path.relative(root, screenshot),
    missing,
    forbidden,
    content_length: content.trim().length,
  };
}

async function main() {
  const { chromium } = loadPlaywright();
  const baseUrl = normalizeBaseUrl(env("AWATCH_BROWSER_SMOKE_URL", env("DETMIR_PORTAL_SMOKE_URL", "http://127.0.0.1:8720/portal/")));
  const timeout = Number(env("AWATCH_BROWSER_SMOKE_TIMEOUT_MS", "30000"));
  const artifactDir = path.resolve(root, env("AWATCH_BROWSER_SMOKE_ARTIFACT_DIR", "artifacts/browser-smoke"));
  fs.mkdirSync(artifactDir, { recursive: true });

  const browser = await chromium.launch({
    headless: !["0", "false", "no"].includes(env("AWATCH_BROWSER_SMOKE_HEADLESS", "true").toLowerCase()),
    args: ["--no-sandbox", "--disable-dev-shm-usage"],
  });
  const context = await browser.newContext({
    ignoreHTTPSErrors: true,
    viewport: { width: 1440, height: 980 },
    extraHTTPHeaders: authHeaders(),
  });
  const page = await context.newPage();
  const consoleErrors = [];
  const pageErrors = [];
  const badResponses = [];
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("response", (response) => {
    if (response.status() >= 500) {
      badResponses.push({ url: response.url(), status: response.status() });
    }
  });

  const specs = [
    {
      name: "executive",
      viewMode: "executive",
      screenshot: "executive.png",
      required: ["Индекс активности", "Почему такой индекс", "Риск-нарратив", "Рекомендуемые действия"],
    },
    {
      name: "workforce",
      viewMode: "manager",
      screenshot: "workforce.png",
      required: ["Индекс активности", "Подразделения", "Тренд", "Почему такой индекс"],
    },
    {
      name: "security",
      viewMode: "security",
      screenshot: "security.png",
      required: ["Рекомендуемые действия ИБ", "События безопасности", "Связь рисков и активности", "Требует проверки"],
    },
    {
      name: "forensics",
      viewMode: "forensics",
      screenshot: "forensics.png",
      required: ["Расследования", "Timeline событий", "Материалы расследования", "Аудит"],
    },
  ];

  let results = [];
  try {
    await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout });
    await waitForPortalReady(page, timeout);
    results = [];
    for (const spec of specs) {
      results.push(await checkView(page, spec, artifactDir, timeout));
    }
  } finally {
    await browser.close();
  }

  const ok = results.every((result) => result.ok)
    && consoleErrors.length === 0
    && pageErrors.length === 0
    && badResponses.length === 0;
  process.stdout.write(`${JSON.stringify({
    ok,
    base_url: baseUrl,
    artifact_dir: path.relative(root, artifactDir),
    checked_at_utc: new Date().toISOString(),
    results,
    console_errors: consoleErrors,
    page_errors: pageErrors,
    server_errors: badResponses,
  }, null, 2)}\n`);
  process.exitCode = ok ? 0 : 2;
}

main().catch((error) => {
  process.stderr.write(`${JSON.stringify({ ok: false, error: error.message }, null, 2)}\n`);
  process.exit(1);
});
