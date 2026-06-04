#!/usr/bin/env node
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function loadPlaywright() {
  const home = process.env.HOME || "";
  const candidates = [
    "playwright",
    "playwright-core",
    process.env.PLAYWRIGHT_CORE_MODULE,
    home ? path.join(home, ".agents/skills/playwright/node_modules/playwright-core") : "",
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

function env(name, fallback = "") {
  const value = process.env[name];
  return value && value.trim() ? value.trim() : fallback;
}

function authHeaders() {
  const explicit = env("DETMIR_PORTAL_SMOKE_AUTH_HEADER");
  if (explicit) return { Authorization: explicit };
  const basic = env("DETMIR_PORTAL_SMOKE_BASIC_AUTH", env("DETMIR_BASIC_AUTH"));
  if (basic) return { Authorization: `Basic ${basic}` };
  return {};
}

function extractDataTabs(html) {
  return [...html.matchAll(/data-tab=["']([^"']+)["']/g)].map((match) => match[1]);
}

function containsText(text, marker) {
  return String(text || "").toLocaleLowerCase("ru-RU").includes(String(marker || "").toLocaleLowerCase("ru-RU"));
}

function assertStaticTabHandlers() {
  const index = fs.readFileSync(path.join(root, "adk-rust/crates/detmir-portal/src/static/index.html"), "utf8");
  const app = fs.readFileSync(path.join(root, "adk-rust/crates/detmir-portal/src/static/app.js"), "utf8");
  const tabs = [...new Set(extractDataTabs(index))];
  const missing = tabs.filter((tab) => !app.includes(`state.tab === "${tab}"`));
  if (missing.length) {
    throw new Error(`data-tab without app.js handler: ${missing.join(", ")}`);
  }
  return tabs;
}

const expectedTabs = [
  { tab: "operator", label: "Обзор", marker: "Представление руководителя" },
  { tab: "employees", label: "Сотрудники", marker: "Карточки сотрудников" },
  { tab: "departments", label: "Подразделения", marker: "Рейтинг подразделений" },
  { tab: "owner", label: "Риски", marker: "Контроль безопасности" },
  { tab: "incidents", label: "Расследования", marker: "Расследования и доказательная база" },
  { tab: "perimeter", label: "Сетевой периметр", marker: "Состояние доступных сигналов" },
  { tab: "reports", label: "Отчеты", marker: "Срезы отчета" },
  { tab: "settings", label: "Настройки", marker: "Параметры расчета" },
];

let smokeStep = "start";

async function main() {
  const staticTabs = assertStaticTabHandlers();
  const missingExpected = expectedTabs.filter((item) => !staticTabs.includes(item.tab));
  if (missingExpected.length) {
    throw new Error(`expected tabs absent in index.html: ${missingExpected.map((item) => item.tab).join(", ")}`);
  }

  const { chromium } = loadPlaywright();
  const url = env("DETMIR_PORTAL_SMOKE_URL", "http://127.0.0.1:8720/portal/");
  const timeout = Number(env("DETMIR_PORTAL_SMOKE_TIMEOUT_MS", "30000"));
  const browser = await chromium.launch({
    headless: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage"],
  });
  const context = await browser.newContext({
    ignoreHTTPSErrors: true,
    viewport: { width: 1440, height: 950 },
    extraHTTPHeaders: authHeaders(),
  });
  const page = await context.newPage();
  const errors = [];
  const badResponses = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("response", (response) => {
    if (response.status() >= 400) badResponses.push({ url: response.url(), status: response.status() });
  });

  const results = [];
  const checks = [];
  try {
    smokeStep = "open_portal";
    await page.goto(url, { waitUntil: "domcontentloaded", timeout });
    smokeStep = "wait_tabs";
    await page.waitForSelector(".tab", { timeout });
    smokeStep = "wait_initial_ready";
    await page.waitForFunction(
      () => document.querySelector("#loadingStateText")?.textContent === "READY",
      null,
      { timeout },
    );
    checks.push({
      name: "loading_refresh_status_ready",
      ok:
        (await page.locator("#loadingStatus").count()) === 1
        && (await page.locator("#loadingStageText").innerText({ timeout })).includes("Данные загружены")
        && !(await page.locator("body").innerText({ timeout })).includes("Данные загружаются"),
    });
    for (const item of expectedTabs) {
      smokeStep = `tab:${item.tab}`;
      await page.click(`button[data-tab="${item.tab}"]`, { timeout });
      await page.waitForFunction(
        (marker) => document.body && document.body.innerText.includes(marker),
        item.marker,
        { timeout },
      );
      const activeTab = await page.locator(".tab.is-active").innerText({ timeout });
      const bodyText = await page.locator("#content").innerText({ timeout });
      const stillLoading = bodyText.includes("Загрузка") || bodyText.includes("Обновление периода");
      const ok = activeTab.trim() === item.label && bodyText.includes(item.marker) && !stillLoading;
      results.push({ tab: item.tab, label: item.label, marker: item.marker, ok });
      if (item.tab === "operator") {
        const readyBodyText = await page.locator("#content").innerText({ timeout });
        const requiredExecutive = [
          "Главный вывод",
          "Достоверность показателей",
          "Полнота данных",
          "Главная причина риска",
          "Подтверждающие слои",
          "Карта рисков",
          "Риски подразделений",
          "Рабочая активность сотрудников",
          "Контроль безопасности",
          "Что требует внимания",
        ];
        checks.push({
          name: "executive_dashboard_layer",
          ok: requiredExecutive.every((marker) => containsText(readyBodyText, marker)),
          required: requiredExecutive,
        });
        const cardHeadings = await page.$$eval("#content section.card h3, #content h3.section-title", (nodes) =>
          nodes.map((node) => node.textContent.trim()),
        );
        const riskNarrativeIndex = cardHeadings.indexOf("Главный вывод");
        const executiveIndex = cardHeadings.indexOf("Сводка руководителя");
        const businessRiskIndex = cardHeadings.indexOf("Риски подразделений");
        const heatmapIndex = cardHeadings.indexOf("Карта рисков");
        checks.push({
          name: "management_block_order",
          ok:
            riskNarrativeIndex >= 0 &&
            executiveIndex > riskNarrativeIndex &&
            businessRiskIndex > executiveIndex &&
            heatmapIndex > businessRiskIndex,
          order: [
            "Главный вывод",
            "Сводка руководителя",
            "Риски подразделений",
            "Карта рисков",
          ],
        });
        checks.push({
          name: "role_view_switcher_present",
          ok:
            (await page.locator('[data-view-mode="executive"]').count()) === 1
            && (await page.locator('[data-view-mode="security"]').count()) === 1
            && (await page.locator('[data-view-mode="operations"]').count()) === 1,
        });

        smokeStep = "role:security";
        await page.click('[data-view-mode="security"]', { timeout });
        await page.waitForFunction(
          () => document.querySelector("#loadingStateText")?.textContent === "READY"
            && document.body.innerText.includes("Пакеты расследований"),
          null,
          { timeout },
        );
        const securityText = await page.locator("#content").innerText({ timeout });
        checks.push({
          name: "security_role_view",
          ok: [
            "Представление безопасности",
            "Требует проверки",
            "Расследования",
            "Аудит",
            "Пакеты расследований",
          ].every((marker) => containsText(securityText, marker)),
        });

        smokeStep = "role:operations";
        await page.click('[data-view-mode="operations"]', { timeout });
        await page.waitForFunction(
          () => document.querySelector("#loadingStateText")?.textContent === "READY"
            && document.body.innerText.includes("Телеметрия"),
          null,
          { timeout },
        );
        const operationsText = await page.locator("#content").innerText({ timeout });
        checks.push({
          name: "operations_role_view",
          ok: [
            "Представление эксплуатации",
            "Полнота данных",
            "Качество данных",
            "Качество данных по рабочим местам",
            "Ошибки",
            "Телеметрия",
          ].every((marker) => containsText(operationsText, marker)),
        });
      }
      if (item.tab === "settings") {
        const requiredSettings = [
          "Период расчета",
          "Рабочий день",
          "Порог WARN",
          "Порог FAIL",
          "Источник правил",
          "Дата последнего пересчета",
        ];
        checks.push({
          name: "settings_readonly_rows",
          ok: requiredSettings.every((marker) => containsText(bodyText, marker)),
          required: requiredSettings,
        });
      }
    }
    smokeStep = "security:investigation_navigation";
    await page.click('button[data-tab="operator"]', { timeout });
    await page.click('[data-view-mode="security"]', { timeout });
    await page.waitForFunction(
      () => document.querySelector("#loadingStateText")?.textContent === "READY"
        && document.body.innerText.includes("Требует проверки"),
      null,
      { timeout },
    );
    const investigationButtons = await page.locator("[data-open-investigation]").count();
    if (investigationButtons > 0) {
      await page.locator("[data-open-investigation]").first().click({ timeout });
      await page.waitForFunction(
        () => document.body && document.body.innerText.includes("Расследования и доказательная база"),
        null,
        { timeout },
      );
      const activeTab = await page.locator(".tab.is-active").innerText({ timeout });
      const incidentText = await page.locator("#content").innerText({ timeout });
      checks.push({
        name: "risk_open_investigation_readonly_navigation",
        ok: activeTab.trim() === "Расследования"
          && containsText(incidentText, "Расследование сформировано автоматически")
          && containsText(incidentText, "Номер")
          && containsText(incidentText, "Риск")
          && containsText(incidentText, "Материалы расследования"),
        buttons: investigationButtons,
      });
    } else {
      checks.push({
        name: "risk_open_investigation_readonly_navigation",
        ok: true,
        buttons: 0,
        skipped: "no risk rows in current report",
      });
    }
  } finally {
    await browser.close();
  }

  const ok = results.every((item) => item.ok) && checks.every((item) => item.ok) && errors.length === 0 && badResponses.length === 0;
  const result = {
    ok,
    generated_at_utc: new Date().toISOString(),
    url,
    static_tabs: staticTabs,
    tabs: results,
    checks,
    errors,
    bad_responses: badResponses,
  };
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  process.exitCode = ok ? 0 : 2;
}

main().catch((error) => {
  process.stdout.write(`${JSON.stringify({ ok: false, step: smokeStep, error: error.message, stack: error.stack }, null, 2)}\n`);
  process.exitCode = 2;
});
