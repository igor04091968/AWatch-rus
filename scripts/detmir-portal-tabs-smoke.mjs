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

function securityEventsMode(summary) {
  const s = summary || {};
  if (!summary || s.backend === "disabled" || s.status === "disabled") return "disabled";
  if (s.fallback_used) return "fallback";
  return "available";
}

function expectedSecurityEventsText(mode) {
  if (mode === "disabled") return "Источник событий безопасности отключён";
  if (mode === "fallback") return "События безопасности временно недоступны";
  if (mode === "available") return "События безопасности доступны";
  return "";
}

function forbiddenExecutiveTerms(text) {
  const forbidden = [
    "Trust KPI",
    "Business Risk",
    "Risk Narrative",
    "Security Events",
    "Incident Candidate",
    "Coverage SLA",
    "ERROR",
    "EMPTY",
    "STALE",
    "SECURITY_EVENTS_BACKEND",
    "CLICKHOUSE_*",
    "ClickHouse",
  ];
  return forbidden.filter((term) => containsText(text, term));
}

function assertStaticTabHandlers() {
  const index = fs.readFileSync(path.join(root, "adk-rust/crates/detmir-portal/src/static/index.html"), "utf8");
  const app = fs.readFileSync(path.join(root, "adk-rust/crates/detmir-portal/src/static/app.js"), "utf8");
  const tabs = [...new Set(extractDataTabs(index))];
  const missing = tabs.filter((tab) => !app.includes(`state.tab === "${tab}"`));
  if (missing.length) {
    throw new Error(`data-tab without app.js handler: ${missing.join(", ")}`);
  }
  const loadingTerms = [
    "Загрузка данных",
    "Данные готовы",
    "Данные отсутствуют",
    "Данные устарели",
    "Ошибка получения данных",
    "Получение данных",
    "Расчёт показателей",
    "Формирование главного вывода",
    "Подготовка разделов",
  ];
  const missingLoadingTerms = loadingTerms.filter((term) => !app.includes(term) && !index.includes(term));
  if (missingLoadingTerms.length) {
    throw new Error(`missing loading/localization terms: ${missingLoadingTerms.join(", ")}`);
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
      () => document.querySelector("#loadingStatus")?.dataset.loadStatus === "READY",
      null,
      { timeout },
    );
    checks.push({
      name: "loading_refresh_status_ready",
      ok:
        (await page.locator("#loadingStatus").count()) === 1
        && (await page.locator("#loadingStatus").evaluate((node) => node.dataset.loadStatus)) === "READY"
        && (await page.locator("#loadingStateText").innerText({ timeout })).includes("Данные готовы")
        && (await page.locator("#loadingStageText").innerText({ timeout })).includes("Данные готовы"),
    });
    smokeStep = "api:security_events_summary";
    const reportsPayload = await page.evaluate(async () => {
      const response = await fetch("api/reports?role=security", {
        cache: "no-store",
        headers: { "X-AWatch-Role": "security" },
      });
      return { ok: response.ok, status: response.status, json: await response.json() };
    });
    const securitySummary = reportsPayload.json?.security_events_summary || null;
    const securityMode = securityEventsMode(securitySummary);
    const expectedSecurityMode = env("DETMIR_PORTAL_SMOKE_SECURITY_EVENTS_EXPECT", "auto").toLowerCase();
    checks.push({
      name: "security_events_api_json",
      ok: reportsPayload.ok && Boolean(securitySummary),
      mode: securityMode,
      status: reportsPayload.status,
    });
    checks.push({
      name: "security_events_expected_mode",
      ok: expectedSecurityMode === "auto" || expectedSecurityMode === securityMode,
      expected: expectedSecurityMode,
      actual: securityMode,
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
        const forbiddenExecutive = forbiddenExecutiveTerms(readyBodyText);
        checks.push({
          name: "executive_view_no_technical_terms",
          ok: forbiddenExecutive.length === 0,
          forbidden: forbiddenExecutive,
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
            && (await page.locator('[data-view-mode="manager"]').count()) === 1
            && (await page.locator('[data-view-mode="security"]').count()) === 1
            && (await page.locator('[data-view-mode="forensics"]').count()) === 1
            && (await page.locator('[data-view-mode="admin"]').count()) === 1,
        });
        const expectedSecurityText = expectedSecurityEventsText(securityMode);
        checks.push({
          name: "security_events_executive_text",
          ok:
            !expectedSecurityText
            || (containsText(readyBodyText, expectedSecurityText)
              && !containsText(readyBodyText, "SECURITY_EVENTS_BACKEND")
              && !containsText(readyBodyText, "CLICKHOUSE_*")
              && !containsText(readyBodyText, "ClickHouse")),
          mode: securityMode,
          expected_text: expectedSecurityText,
        });

        smokeStep = "role:manager";
        await page.click('[data-view-mode="manager"]', { timeout });
        await page.waitForFunction(
          () => document.querySelector("#loadingStatus")?.dataset.loadStatus === "READY"
            && document.body.innerText.includes("ТОП-5 лучших подразделений"),
          null,
          { timeout },
        );
        const managerText = await page.locator("#content").innerText({ timeout });
        checks.push({
          name: "manager_role_view",
          ok: [
            "Представление менеджера",
            "Сводка руководителя",
            "ТОП-5 лучших подразделений",
            "ТОП-5 проблемных подразделений",
            "Карта рисков",
            "Markdown-отчет",
          ].every((marker) => containsText(managerText, marker))
            && !containsText(managerText, "Материалы расследования"),
        });

        smokeStep = "role:security";
        await page.click('[data-view-mode="security"]', { timeout });
        await page.waitForFunction(
          () => document.querySelector("#loadingStatus")?.dataset.loadStatus === "READY"
            && document.body.innerText.includes("Материалы расследования"),
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
            "Материалы расследования",
            "События безопасности за 24 часа",
            "Связь рисков и активности",
          ].every((marker) => containsText(securityText, marker)),
        });
        checks.push({
          name: "security_role_hides_technical_health_sources",
          ok:
            !containsText(securityText, "detmir_check")
            && !containsText(securityText, "detmir_status")
            && !containsText(securityText, "command failed")
            && !containsText(securityText, "command returned non-zero"),
        });
        checks.push({
          name: "security_events_security_text",
          ok: !expectedSecurityText || containsText(securityText, expectedSecurityText),
          mode: securityMode,
          expected_text: expectedSecurityText,
        });

        smokeStep = "role:forensics";
        await page.click('[data-view-mode="forensics"]', { timeout });
        await page.waitForFunction(
          () => document.querySelector("#loadingStatus")?.dataset.loadStatus === "READY"
            && document.body.innerText.includes("Timeline событий"),
          null,
          { timeout },
        );
        const forensicsText = await page.locator("#content").innerText({ timeout });
        checks.push({
          name: "forensics_role_view",
          ok: [
            "Представление расследований",
            "Требует проверки",
            "Расследования",
            "Timeline событий",
            "Материалы расследования",
            "Аудит",
          ].every((marker) => containsText(forensicsText, marker))
            && !containsText(forensicsText, "Рейтинг подразделений"),
        });

        smokeStep = "role:admin";
        await page.click('[data-view-mode="admin"]', { timeout });
        await page.waitForFunction(
          () => document.querySelector("#loadingStatus")?.dataset.loadStatus === "READY"
            && document.body.innerText.includes("Телеметрия"),
          null,
          { timeout },
        );
        const operationsText = await page.locator("#content").innerText({ timeout });
        checks.push({
          name: "admin_role_view",
          ok: [
            "Представление администратора",
            "Полнота данных",
            "Качество данных",
            "События безопасности за 24 часа",
            "Качество данных по рабочим местам",
            "Ошибки",
            "Телеметрия",
          ].every((marker) => containsText(operationsText, marker)),
        });
        checks.push({
          name: "security_events_operations_text",
          ok:
            !expectedSecurityText
            || (containsText(operationsText, expectedSecurityText)
              && (securityMode !== "fallback" || containsText(operationsText, "События безопасности временно недоступны"))),
          mode: securityMode,
          expected_text: expectedSecurityText,
        });

        smokeStep = "api:role_gates";
        const apiUrl = (path) => new URL(path, page.url()).toString();
        const [managerSecurity, securityWorkforce, forensicsOk, uebaOk] = await Promise.all([
          context.request.get(apiUrl("api/security"), {
            headers: { ...authHeaders(), "X-AWatch-Role": "manager" },
          }),
          context.request.get(apiUrl("api/workforce"), {
            headers: { ...authHeaders(), "X-AWatch-Role": "security" },
          }),
          context.request.get(apiUrl("api/forensics"), {
            headers: { ...authHeaders(), "X-AWatch-Role": "forensics" },
          }),
          context.request.get(apiUrl("api/ueba"), {
            headers: { ...authHeaders(), "X-AWatch-Role": "security" },
          }),
        ]);
        checks.push({
          name: "server_role_gates",
          ok:
            managerSecurity.status() === 403
            && securityWorkforce.status() === 403
            && forensicsOk.ok()
            && uebaOk.ok(),
          statuses: {
            manager_security: managerSecurity.status(),
            security_workforce: securityWorkforce.status(),
            forensics: forensicsOk.status(),
            ueba: uebaOk.status(),
          },
        });
      }
      if (item.tab === "settings") {
        const requiredSettings = [
          "Период расчета",
          "Рабочий день",
          "Порог внимания",
          "Порог критического риска",
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
      () => document.querySelector("#loadingStatus")?.dataset.loadStatus === "READY"
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
