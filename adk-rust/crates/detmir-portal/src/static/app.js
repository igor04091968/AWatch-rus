const state = { tab: "operator", period: "today", links: null, readiness: null, reports: null };

function apiBase() {
  const path = window.location.pathname;
  return path.startsWith("/portal") ? "/portal/api" : "/api";
}

async function loadJson(path) {
  const response = await fetch(`${apiBase()}${path}`, { cache: "no-store" });
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
  return response.json();
}

async function postJson(path, payload) {
  const response = await fetch(`${apiBase()}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
  return response.json();
}

function statusClass(status) {
  const s = String(status || "UNKNOWN").toLowerCase();
  if (s === "ok" || s === "true" || s === "low" || s === "false_positive" || s === "resolved") return "status-ok";
  if (s === "warn" || s === "warning" || s === "fallback" || s === "stale" || s === "medium" || s === "in_review" || s === "postponed" || s === "open" || s === "in_progress") return "status-warn";
  if (s === "degraded" || s === "high" || s === "confirmed" || s === "rejected" || s === "archived") return "status-degraded";
  if (s === "fail" || s === "false" || s === "error" || s === "critical" || s === "missing") return "status-fail";
  return "status-unknown";
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function displayText(value) {
  return String(value ?? "")
    .replaceAll("Workforce", "Работа сотрудников")
    .replaceAll("workforce", "работа сотрудников")
    .replaceAll("Security", "Безопасность")
    .replaceAll("Forensics", "Расследования")
    .replaceAll("UEBA риск", "Индекс риска")
    .replaceAll("UEBA", "оценка риска")
    .replaceAll("DLP/ИБ", "ИБ")
    .replaceAll("DLP", "Проверки ИБ")
    .replaceAll("Evidence metadata", "Материалы проверки")
    .replaceAll("Evidence", "Материалы")
    .replaceAll("evidence", "материалы")
    .replaceAll("Grafana dashboards", "графики")
    .replaceAll("Grafana data", "данные графиков")
    .replaceAll("Grafana", "Графики")
    .replaceAll("DetMir ActivityWatch", "Журнал активности")
    .replaceAll("AW UI", "Журнал активности")
    .replaceAll("bundle", "пакет проверки")
    .replaceAll("Checksum", "Контрольная сумма")
    .replaceAll("Markdown", "Текстовый отчет")
    .replaceAll("JSON", "Данные")
    .replaceAll("proxy:", "расчет:")
    .replaceAll("rule-based", "по правилам")
    .replaceAll("Read-only risk score", "Оценка риска без автоматического воздействия")
    .replaceAll("Confidence", "Достоверность")
    .replaceAll("sources", "источники")
    .replaceAll("baseline", "обычный профиль")
    .replaceAll("policy", "правила")
    .replaceAll("default_weight", "нет явного правила")
    .replaceAll("App time", "Время приложений")
    .replaceAll("Weighted", "С учетом правил")
    .replaceAll("Worktime", "Рабочее время")
    .replaceAll("Management report", "Сводка руководителя")
    .replaceAll("Drill-down", "Разбор")
    .replaceAll("KPI", "показатель")
    .replaceAll("items", "записи")
    .replaceAll("medium risk", "средний риск")
    .replaceAll("low risk", "низкий риск")
    .replaceAll("high risk", "высокий риск")
    .replaceAll("critical risk", "критический риск")
    .replaceAll("weighted_seconds", "время с учетом правил")
    .replaceAll("planned_seconds", "плановое время")
    .replaceAll("active_seconds", "активное время")
    .replaceAll("app-weight", "вес приложений")
    .replaceAll("per-user", "по сотруднику")
    .replaceAll("user:", "сотрудник:")
    .replaceAll("dept:", "подразделение:")
    .replace(/\byes\b/g, "да")
    .replace(/\bno\b/g, "нет")
    .replaceAll("daily", "день")
    .replaceAll("weekly", "неделя")
    .replaceAll("monthly", "месяц")
    .replaceAll("OK", "OK")
    .replaceAll("FAIL", "FAIL")
    .replaceAll("WARN", "WARN");
}

function ui(value) {
  return escapeHtml(displayText(value));
}

function renderSummary(summary, readiness) {
  const global = document.getElementById("globalStatus");
  global.className = `status-pill ${statusClass(summary.severity)}`;
  global.textContent = `Сбор данных ${summary.operator_ok ? "OK" : "NO"} · ${summary.severity}`;
  const blocks = Object.entries(summary.blocks || {});
  const readinessCard = renderReadinessSummaryCard(readiness);
  document.getElementById("summary").innerHTML = readinessCard + blocks.map(([name, block]) => `
    <article class="card">
      <span class="badge ${statusClass(block.status)}">${escapeHtml(block.status)}</span>
      <h3>${ui(label(name))}</h3>
      <p class="muted">${ui(block.text)}</p>
    </article>
  `).join("");
}

function renderReadinessSummaryCard(readiness) {
  const bundle = readiness?.bundle || {};
  const verify = readiness?.verify || {};
  const status = bundle.status?.status || bundle.status || (verify.ok ? "OK" : "UNKNOWN");
  const generated = bundle.status?.generated_at_utc || bundle.generated_at_utc || "-";
  const signature = bundle.status?.signature || {};
  const signatureOk = Boolean(verify.signature_verified || signature.verified);
  const checksumOk = Boolean(verify.checksum_verified || bundle.status?.checksum_verified || verify.ok);
  const fingerprint = signature.public_key_fingerprint_sha256 || "-";
  const verificationText = verify.generated_at_utc
    ? `Проверено: ${verify.generated_at_utc}`
    : "Проверка не запускалась";
  return `
    <article class="card readiness-card">
      <div class="readiness-head">
        <div>
          <span class="badge ${statusClass(status)}">${escapeHtml(status)}</span>
          <h3>Готовность системы</h3>
        </div>
        <button class="small-button" data-readiness-verify="true">Проверить пакет</button>
      </div>
      <div class="readiness-metrics">
        <div><span class="muted">Дата</span><strong>${escapeHtml(generated)}</strong></div>
        <div><span class="muted">Подпись</span><strong class="${signatureOk ? "text-ok" : "text-fail"}">${signatureOk ? "OK" : "FAIL"}</strong></div>
        <div><span class="muted">Контрольная сумма</span><strong class="${checksumOk ? "text-ok" : "text-fail"}">${checksumOk ? "OK" : "FAIL"}</strong></div>
      </div>
      <p class="muted small">Отпечаток ключа: <code>${escapeHtml(shortFingerprint(fingerprint))}</code></p>
      <p id="readinessVerifyStatus" class="muted small">${escapeHtml(verificationText)}</p>
    </article>
  `;
}

function shortFingerprint(value) {
  const text = String(value || "-");
  if (text.length <= 24) return text;
  return `${text.slice(0, 12)}…${text.slice(-12)}`;
}

function label(name) {
  return {
    collection: "Сбор данных",
    grafana: "Графики",
    dlp: "Проверки ИБ",
    worktime: "Работа сегодня",
    one_c: "1С",
    work: "Работа сотрудников",
    security: "Безопасность"
  }[name] || name;
}

function findKpi(report, needle) {
  const text = String(needle || "").toLowerCase();
  return (report?.kpis || []).find(item => String(item.label || "").toLowerCase().includes(text));
}

function findSection(report, title) {
  const text = String(title || "").toLowerCase();
  return (report?.sections || []).find(item => String(item.title || "").toLowerCase().includes(text));
}

function periodConfig(period = state.period) {
  if (period === "week") return { key: "week", label: "неделя", days: 7 };
  if (period === "month") return { key: "month", label: "месяц", days: 30 };
  return { key: "today", label: "сегодня", days: 1 };
}

function periodFromSelectValue(value) {
  const text = String(value || "").toLowerCase();
  if (text === "week") return "week";
  if (text === "month") return "month";
  if (text.includes("нед") || text.includes("7")) return "week";
  if (text.includes("мес") || text.includes("30")) return "month";
  return "today";
}

function trendPoints(report, days) {
  const trend = Array.isArray(report?.workforce?.trend) ? report.workforce.trend : [];
  return trend.slice(Math.max(0, trend.length - days));
}

function averageCoverage(points) {
  const values = points
    .map(item => Number(item.portfolio_coverage_pct))
    .filter(value => Number.isFinite(value));
  if (values.length === 0) return null;
  return Math.round(values.reduce((sum, value) => sum + value, 0) / values.length);
}

function periodStatus(points, config) {
  if (config.key === "today") return "OK";
  if (points.length >= config.days) return "OK";
  if (points.length > 0) return "WARN";
  return "UNKNOWN";
}

function periodReadinessText(report, config = periodConfig()) {
  if (config.key === "today") return "дневной срез";
  const points = trendPoints(report, config.days);
  if (points.length >= config.days) return `готово: ${config.label}`;
  return `накоплено ${points.length}/${config.days} дней`;
}

function periodReport(report) {
  if (!report || state.period === "today") return report;
  const config = periodConfig();
  const points = trendPoints(report, config.days);
  const average = averageCoverage(points);
  const next = JSON.parse(JSON.stringify(report));
  next.period = `управленческий срез за период: ${config.label}`;
  next.period_view = {
    key: config.key,
    label: config.label,
    required_days: config.days,
    available_days: points.length,
    status: periodStatus(points, config),
    average_coverage_pct: average
  };
  next.kpis = (next.kpis || []).map(item => {
    if (item.label === "Индекс активности") {
      return {
        ...item,
        value: average === null ? "нет данных" : `${average}%`,
        status: periodStatus(points, config),
        context: `${config.label}: среднее по накопленной истории, ${points.length}/${config.days} дней`
      };
    }
    if (item.label === "Активное время") {
      return {
        ...item,
        value: state.period === "week" ? "по дням" : "по истории",
        status: periodStatus(points, config),
        context: "агрегация времени требует накопленных дневных срезов"
      };
    }
    if (item.label === "Подразделения") {
      return {
        ...item,
        context: `${config.label}: текущий состав групп, тренд по истории`
      };
    }
    return item;
  });
  return next;
}

function renderPeriodBanner(report) {
  const config = periodConfig();
  if (config.key === "today") return "";
  const view = periodReport(report)?.period_view || {};
  const status = view.status || "UNKNOWN";
  const avg = Number.isFinite(Number(view.average_coverage_pct)) ? `${view.average_coverage_pct}%` : "нет данных";
  return `
    <section class="period-banner">
      <div>
        <strong>Период: ${ui(config.label)}</strong>
        <span class="muted">Накоплено ${escapeHtml(view.available_days ?? 0)} из ${escapeHtml(config.days)} дневных срезов. Средняя активность: ${escapeHtml(avg)}.</span>
      </div>
      <span class="badge ${statusClass(status)}">${ui(periodReadinessText(report, config))}</span>
    </section>
  `;
}

function renderDailyDetailNotice() {
  const config = periodConfig();
  if (config.key === "today") return "";
  return `
    <section class="period-banner">
      <div>
        <strong>Период: ${ui(config.label)}</strong>
        <span class="muted">Этот детальный экран показывает последний дневной срез. Сводная интерпретация за период доступна в разделах "Пульс организации", "Подразделения" и "Отчеты".</span>
      </div>
      <span class="badge status-warn">дневная детализация</span>
    </section>
  `;
}

function metricCard(labelText, value, status, context, trend) {
  const trendText = trend || context || "";
  const trendClass = statusClass(status).replace("status-", "");
  return `
    <article class="metric-card">
      <span class="badge ${statusClass(status)}">${escapeHtml(status || "INFO")}</span>
      <span class="kpi-label">${ui(labelText)}</span>
      <strong class="metric-value">${ui(value ?? "Нет данных")}</strong>
      <span class="kpi-trend ${trendClass}">${ui(trendText)}</span>
    </article>
  `;
}

function renderExecutiveMetrics(report, incidents) {
  void incidents;
  return `
    <section class="executive-grid" aria-label="Управленческая сводка">
      ${executiveDashboardKpis(report)}
    </section>
  `;
}

function statusRiskLabel(status) {
  const value = String(status || "INFO").toUpperCase();
  if (value === "FAIL") return "high";
  if (value === "WARN") return "medium";
  if (value === "OK") return "low";
  return "info";
}

function trendArrow(status) {
  const value = String(status || "INFO").toUpperCase();
  if (value === "OK") return "→";
  if (value === "WARN") return "↓";
  if (value === "FAIL") return "↓";
  return "→";
}

function firstPercent(value) {
  const match = String(value || "").match(/-?\d+(?:[.,]\d+)?\s*%/);
  return match ? match[0].replace(",", ".").replace(/\s+/g, "") : "-";
}

function percentNumber(value) {
  const text = firstPercent(value);
  if (text === "-") return null;
  const number = Number(text.replace("%", ""));
  return Number.isFinite(number) ? number : null;
}

function signedPercent(value) {
  if (!Number.isFinite(value)) return "нет истории";
  const rounded = Math.round(value);
  if (rounded > 0) return `+${rounded}%`;
  return `${rounded}%`;
}

function parseActivityValue(value) {
  const text = String(value || "");
  const activeMatch = text.match(/active\s+(\d+)\s*\/\s*(\d+)/i);
  const hhmmMatch = text.match(/(\d{1,3}:\d{2})/);
  return {
    activity: percentNumber(text),
    active: activeMatch ? Number(activeMatch[1]) : null,
    total: activeMatch ? Number(activeMatch[2]) : null,
    hhmm: hhmmMatch ? hhmmMatch[1] : "00:00",
  };
}

function statusWeight(status) {
  const value = String(status || "INFO").toUpperCase();
  if (value === "FAIL") return 3;
  if (value === "WARN") return 2;
  if (value === "OK") return 1;
  return 0;
}

function departmentDeviation(item) {
  const status = String(item?.status || "INFO").toUpperCase();
  const value = item?.value || "";
  if (status === "OK") return "в норме";
  if (status === "WARN") return `отклонение: ${firstPercent(value)}`;
  if (status === "FAIL") return `критическая просадка: ${firstPercent(value)}`;
  return "требует данных";
}

function departmentResponsible(owners, index) {
  const item = Array.isArray(owners) ? owners[index] : null;
  return item?.label || item?.title || "не назначен";
}

function latestTrend(report) {
  const trend = Array.isArray(report?.workforce?.trend) ? report.workforce.trend : [];
  return trend.length ? trend[trend.length - 1] : {};
}

function weeklyTrendPct(report) {
  const trend = Array.isArray(report?.workforce?.trend) ? report.workforce.trend : [];
  if (trend.length < 2) return null;
  const first = Number(trend[0]?.portfolio_coverage_pct);
  const last = Number(trend[trend.length - 1]?.portfolio_coverage_pct);
  if (!Number.isFinite(first) || !Number.isFinite(last)) return null;
  return last - first;
}

function departmentTrendPct(report, name) {
  const trend = Array.isArray(report?.workforce?.trend) ? report.workforce.trend : [];
  const points = trend
    .map(day => (day.department_rollups || []).find(item => item.name === name))
    .filter(Boolean);
  if (points.length < 2) return null;
  const first = Number(points[0]?.portfolio_coverage_pct);
  const last = Number(points[points.length - 1]?.portfolio_coverage_pct);
  if (!Number.isFinite(first) || !Number.isFinite(last)) return null;
  return last - first;
}

function departmentRows(report) {
  const departments = report?.workforce?.department_comparison || [];
  const owners = report?.workforce?.owner_comparison || [];
  return departments.map((item, index) => {
    const parsed = parseActivityValue(item.value);
    const activity = item.index_activity ? percentNumber(item.index_activity) : parsed.activity;
    const trend = departmentTrendPct(report, item.label);
    const status = item.status || "INFO";
    const responsible = item.responsible || departmentResponsible(owners, index);
    return {
      label: item.label || "Без подразделения",
      activity,
      activityText: Number.isFinite(activity) ? `${Math.round(activity)}%` : firstPercent(item.value),
      deviation: item.deviation || departmentDeviation(item),
      status,
      responsible,
      active: parsed.active,
      total: parsed.total,
      hhmm: parsed.hhmm,
      trend,
      risk: status === "FAIL" ? "FAIL — требуется действие" : status === "WARN" ? "WARN — требуется внимание" : "LOW — все нормально",
      reason: departmentRiskReason({ item, parsed, activity, status, trend }),
      check: departmentCheckText({ parsed, status, trend }),
    };
  });
}

function departmentRiskReason({ parsed, activity, status, trend }) {
  const reasons = [];
  if (Number.isFinite(activity)) reasons.push(`индекс активности ${Math.round(activity)}%`);
  if (Number.isFinite(parsed.active) && Number.isFinite(parsed.total)) reasons.push(`активны ${parsed.active}/${parsed.total} сотрудников`);
  if (status === "WARN") reasons.push("подразделение ниже рабочего порога");
  if (status === "FAIL") reasons.push("требуется управленческое действие");
  if (Number.isFinite(trend) && trend < 0) reasons.push(`недельный тренд ${signedPercent(trend)}`);
  if (parsed.hhmm === "00:00") reasons.push("нет подтвержденного рабочего времени");
  return reasons.join("; ") || "недостаточно данных для уверенного вывода";
}

function departmentCheckText({ parsed, status, trend }) {
  if (status === "OK") return "держать под наблюдением, действий не требуется";
  if (parsed.hhmm === "00:00") return "проверить входы в рабочие системы, RDP/1C активность и свежесть данных коллектора";
  if (Number.isFinite(trend) && trend < 0) return "сверить план задач, нагрузку ответственного и причины падения за неделю";
  return "поручить ответственному проверить план работ и первичные события ActivityWatch";
}

function executiveDashboardKpis(report) {
  const rows = departmentRows(report);
  const latest = latestTrend(report);
  const activityValues = rows.map(row => row.activity).filter(Number.isFinite);
  const average = activityValues.length ? Math.round(activityValues.reduce((sum, value) => sum + value, 0) / activityValues.length) : null;
  const criticalRisks = (report?.ueba_risk?.reasons || []).filter(item => String(item.status || item.severity).toUpperCase() === "FAIL" || Number(item.points || 0) >= 25).length;
  return [
    metricCard("Сотрудников в работе", latest.active_users ?? findKpi(report, "Сотрудники")?.value ?? "нет данных", "OK", "активны сегодня"),
    metricCard("Средний индекс активности", Number.isFinite(average) ? `${average}%` : findKpi(report, "Индекс активности")?.value, average === null ? "UNKNOWN" : workforceIndexStatus(average), "по подразделениям"),
    metricCard("WARN подразделений", rows.filter(row => row.status === "WARN").length, rows.some(row => row.status === "WARN") ? "WARN" : "OK", "требуется внимание"),
    metricCard("FAIL подразделений", rows.filter(row => row.status === "FAIL").length, rows.some(row => row.status === "FAIL") ? "FAIL" : "OK", "требуется действие"),
    metricCard("Критических рисков", criticalRisks, criticalRisks ? "FAIL" : "OK", "приоритетный разбор"),
    metricCard("Тренд недели", signedPercent(weeklyTrendPct(report)), Number(weeklyTrendPct(report)) < 0 ? "WARN" : "OK", "динамика активности"),
  ].join("");
}

function renderDepartmentTable(report) {
  const rows = departmentRows(report).slice(0, 12);
  if (!rows.length) return `<p class="muted">Подразделения пока не рассчитаны.</p>`;
  return `
    <div class="table-scroll">
    <table class="data-table department-table">
      <thead><tr><th>Подразделение</th><th>Индекс активности</th><th>Отклонение</th><th>Статус</th><th>Ответственный</th></tr></thead>
      <tbody>${rows.map(row => `
        <tr>
          <td><strong>${ui(row.label)}</strong></td>
          <td>${ui(row.activityText || "-")}</td>
          <td>${ui(row.deviation)}</td>
          <td><span class="badge ${statusClass(row.status)}">${ui(row.status || "INFO")}</span></td>
          <td>${ui(row.responsible)}</td>
        </tr>
      `).join("")}</tbody>
    </table>
    </div>
  `;
}

function anomalyExplanation(item) {
  const title = item?.label || item?.title || "Аномалия";
  const evidence = item?.value || item?.subject || "система отметила отклонение от обычного среза";
  const status = String(item?.status || "INFO").toUpperCase();
  const text = `${title} ${evidence}`.toLowerCase();
  let why = "это может указывать на изменение загрузки, дисциплины процесса или качества сбора данных";
  let check = "проверить первичные события ActivityWatch, рабочий план и контекст подразделения";
  if (text.includes("копирован") || text.includes("dlp") || text.includes("файл")) {
    why = "есть риск неконтролируемого движения данных или подготовки выгрузки";
    check = "открыть материалы проверки, сверить файл, пользователя, время и разрешенную бизнес-задачу";
  } else if (text.includes("ноч") || text.includes("вне рабочего")) {
    why = "активность вне обычного окна может быть переработкой, удаленным доступом или нарушением регламента";
    check = "сверить RDP-сессию, задачу сотрудника и журнал входов";
  } else if (text.includes("недогруз") || text.includes("просад") || text.includes("coverage")) {
    why = "просадка активности может означать простой, неверный план работ или сбой сбора";
    check = "сравнить план задач, присутствие в системе и свежесть данных коллектора";
  } else if (status === "FAIL") {
    why = "событие имеет высокий приоритет и может влиять на безопасность или управляемость";
    check = "назначить ручную проверку и сопоставить событие с журналами источников";
  }
  return { title, evidence, why, check };
}

function renderAnomalies(report) {
  const insights = Array.isArray(report?.workforce?.insights) ? report.workforce.insights : [];
  const items = insights.filter(item => item.status !== "OK").slice(0, 8);
  if (!items.length) return `<p class="muted">Существенных аномалий за выбранный период нет.</p>`;
  return `<ul class="rank-list anomaly-list">${items.map(item => {
    const explanation = anomalyExplanation(item);
    return `
    <li>
      <span class="badge ${statusClass(item.status)}">${ui(statusRiskLabel(item.status))}</span>
      <div>
        <strong>${ui(explanation.title)}</strong>
        <p><b>Что произошло:</b> ${ui(explanation.evidence)}</p>
        <p><b>Почему это риск:</b> ${ui(explanation.why)}</p>
        <p><b>Что проверить:</b> ${ui(explanation.check)}</p>
      </div>
    </li>
  `; }).join("")}</ul>`;
}

function renderTopRisks(report) {
  const reasons = Array.isArray(report?.ueba_risk?.reasons) ? report.ueba_risk.reasons : [];
  if (!reasons.length) return `<p class="muted">Существенных риск-сигналов нет.</p>`;
  return `<ol class="rank-list risk-list">${reasons.slice(0, 8).map((item, index) => `
    <li>
      <strong>${index + 1}</strong>
      <span>${ui(item.label || item.code || "Риск")}<small>${ui(item.recommendation || item.value || "")}</small></span>
      <span class="badge ${statusClass(item.status || item.severity)}">+${escapeHtml(item.points || 0)}</span>
      <button class="small-button" data-open-investigation="true">Открыть расследование</button>
    </li>
  `).join("")}</ol>`;
}

function renderDepartmentRanking(report) {
  const rows = departmentRows(report);
  const best = [...rows]
    .sort((a, b) => (b.activity ?? -1) - (a.activity ?? -1) || statusWeight(a.status) - statusWeight(b.status))
    .slice(0, 5);
  const problem = [...rows]
    .sort((a, b) => statusWeight(b.status) - statusWeight(a.status) || (a.activity ?? 101) - (b.activity ?? 101))
    .slice(0, 5);
  const renderRows = items => `<div class="list compact-list">${items.map(row => `
    <div class="row compact-row">
      <strong>${ui(row.label)}</strong>
      <span class="muted">${ui(row.activityText)} · ${ui(row.deviation)} · ${ui(row.responsible)}</span>
      <span class="badge ${statusClass(row.status)}">${ui(row.status)}</span>
    </div>
  `).join("")}</div>`;
  const emptyRows = `<div class="list compact-list"><div class="row compact-row"><strong>Нет данных</strong><span class="muted">Подразделения пока не рассчитаны.</span><span class="badge status-unknown">UNKNOWN</span></div></div>`;
  return `
    <section class="ranking-grid">
      <article class="card">
        <h3>ТОП-5 лучших подразделений</h3>
        ${best.length ? renderRows(best) : emptyRows}
      </article>
      <article class="card">
        <h3>ТОП-5 проблемных подразделений</h3>
        ${problem.length ? renderRows(problem) : emptyRows}
      </article>
    </section>
  `;
}

function attentionItems(report) {
  const rows = departmentRows(report);
  const items = [];
  rows
    .filter(row => row.status === "FAIL" || row.status === "WARN")
    .slice(0, 3)
    .forEach(row => items.push({
      title: `${row.status === "FAIL" ? "Требуется действие" : "Требуется внимание"}: ${row.label}`,
      why: row.reason,
      check: row.check,
      action: `Поручить разбор: ${row.responsible}. Открыть расследование и сверить первичные события.`,
      status: row.status,
    }));
  (report?.workforce?.insights || [])
    .filter(item => item.status !== "OK")
    .slice(0, Math.max(0, 5 - items.length))
    .forEach(item => {
      const explanation = anomalyExplanation(item);
      items.push({
        title: explanation.title,
        why: explanation.why,
        check: explanation.check,
        action: "Назначить владельца проверки и сверить с планом работ.",
        status: item.status || "WARN",
      });
    });
  return items.slice(0, 5);
}

function renderAttentionBlock(report) {
  const items = attentionItems(report);
  if (!items.length) return "";
  return `
    <section class="attention-panel">
      <div class="band-head"><h3>Требует внимания</h3><span class="muted">что руководителю нужно поручить сегодня</span></div>
      <div class="attention-list">${items.map(item => `
        <article class="attention-item">
          <span class="badge ${statusClass(item.status)}">${ui(item.status || "INFO")}</span>
          <div>
            <strong>${ui(item.title)}</strong>
            <p><b>Почему это важно:</b> ${ui(item.why)}</p>
            <p><b>Что проверить:</b> ${ui(item.check)}</p>
            <p><b>Рекомендуемое действие:</b> ${ui(item.action)}</p>
          </div>
          <button class="small-button" data-open-investigation="true">Открыть расследование</button>
        </article>
      `).join("")}</div>
    </section>
  `;
}

function renderDepartmentHeatMap(report) {
  const rows = departmentRows(report);
  return `
    <section class="dashboard-band">
      <div class="band-head"><h3>Heat Map подразделений</h3><span class="muted">риск понятным языком для руководителя</span></div>
      <div class="table-scroll">
        <table class="data-table heatmap-table">
          <thead><tr><th>Подразделение</th><th>Активность</th><th>Отклонение</th><th>Риск</th><th>Ответственный</th><th>Действие</th></tr></thead>
          <tbody>${rows.length ? rows.map(row => `
            <tr>
              <td><strong>${ui(row.label)}</strong><small>${ui(row.reason)}</small></td>
              <td>${ui(row.activityText)}</td>
              <td>${ui(row.deviation)}</td>
              <td><span class="badge ${statusClass(row.status)}">${ui(row.risk)}</span></td>
              <td>${ui(row.responsible)}</td>
              <td><button class="small-button" data-open-investigation="true">Открыть расследование</button></td>
            </tr>
          `).join("") : `
            <tr>
              <td><strong>Нет данных</strong><small>Подразделения пока не рассчитаны.</small></td>
              <td>-</td>
              <td>-</td>
              <td><span class="badge status-unknown">UNKNOWN</span></td>
              <td>-</td>
              <td><button class="small-button" data-open-investigation="true">Открыть расследование</button></td>
            </tr>
          `}</tbody>
        </table>
      </div>
    </section>
  `;
}

function renderDepartmentLeaderCard(report) {
  const rows = departmentRows(report);
  const row = rows.length
    ? [...rows].sort((a, b) => statusWeight(b.status) - statusWeight(a.status) || (a.activity ?? 101) - (b.activity ?? 101))[0]
    : {
        label: "Нет данных",
        responsible: "-",
        total: "-",
        active: "-",
        activityText: "-",
        trend: null,
        status: "UNKNOWN",
        risk: "UNKNOWN",
        check: "Дождаться расчета подразделений и проверить источник worktime management."
      };
  const best = rows.length ? [...rows].sort((a, b) => (b.activity ?? -1) - (a.activity ?? -1))[0] : row;
  const worst = rows.length ? [...rows].sort((a, b) => (a.activity ?? 101) - (b.activity ?? 101))[0] : row;
  return `
    <section class="card leader-card">
      <div class="section-head">
        <div>
          <h3>Карточка руководителя подразделения</h3>
          <p class="muted">Read-only срез для поручения разбора ответственному.</p>
        </div>
        <span class="badge ${statusClass(row.status)}">${ui(row.risk)}</span>
      </div>
      <div class="leader-grid">
        <div><span class="muted">Подразделение</span><strong>${ui(row.label)}</strong></div>
        <div><span class="muted">Ответственный</span><strong>${ui(row.responsible)}</strong></div>
        <div><span class="muted">Сотрудников всего</span><strong>${ui(row.total ?? "-")}</strong></div>
        <div><span class="muted">Активны сегодня</span><strong>${ui(row.active ?? "-")}</strong></div>
        <div><span class="muted">Средний индекс</span><strong>${ui(row.activityText)}</strong></div>
        <div><span class="muted">Лучший показатель</span><strong>${ui(best?.activityText || "-")}</strong></div>
        <div><span class="muted">Худший показатель</span><strong>${ui(worst?.activityText || "-")}</strong></div>
        <div><span class="muted">Недельный тренд</span><strong>${ui(signedPercent(row.trend))}</strong></div>
      </div>
      <p class="muted"><b>Что проверить:</b> ${ui(row.check)}</p>
    </section>
  `;
}

function renderOverviewAnalytics(report) {
  return `
    ${renderDepartmentRanking(report)}
    ${renderAttentionBlock(report)}
    <section class="analytics-grid">
      <article class="analytics-panel department-panel">
        <h3>Подразделения сегодня</h3>
        ${renderDepartmentTable(report)}
      </article>
      <article class="analytics-panel anomaly-panel">
        <h3>Аномалии</h3>
        ${renderAnomalies(report)}
      </article>
      <article class="analytics-panel risk-panel">
        <h3>Топ рисков</h3>
        ${renderTopRisks(report)}
      </article>
    </section>
    ${renderDepartmentHeatMap(report)}
    ${renderDepartmentLeaderCard(report)}
  `;
}

function incidentStatusFromCount(count) {
  const n = Number(count || 0);
  if (n === 0) return "OK";
  if (n <= 3) return "WARN";
  return "FAIL";
}

function reportReadinessText(report) {
  const trend = report?.workforce?.trend_status || "daily_only";
  if (trend === "monthly_ready") return "день / неделя / месяц";
  if (trend === "weekly_ready") return "день / неделя";
  return "день";
}

function renderSectionItems(section, emptyText) {
  const items = Array.isArray(section?.items) ? section.items : [];
  if (items.length === 0) {
    return `<p class="muted">${escapeHtml(emptyText || "Данных для среза пока нет.")}</p>`;
  }
  return `<div class="list compact-list">${items.slice(0, 12).map(item => `
    <div class="row compact-row">
      <strong>${ui(item.label || "-")}</strong>
      <span class="muted">${ui(item.value || "")}</span>
      <span class="badge ${statusClass(item.status)}">${escapeHtml(item.status || "INFO")}</span>
    </div>
  `).join("")}</div>`;
}

function renderReportTypeCards() {
  const types = [
    ["Ежедневный отчет", "Оперативный срез за сегодня", "OK"],
    ["Недельный отчет", "Динамика после накопления истории по дням", "INFO"],
    ["Месячный отчет", "Готов для управленческой аналитики после накопления истории", "INFO"],
    ["По подразделению", "Сравнение загрузки и просадок", "OK"],
    ["По сотруднику", "Разбор показателя с осторожной трактовкой", "WARN"],
    ["По инциденту", "Материалы для служебной проверки", "OK"],
    ["Акт пилота", "Итоги опытной эксплуатации", "OK"]
  ];
  return `<div class="report-type-grid">${types.map(([title, text, status]) => `
    <article class="report-type">
      <span class="badge ${statusClass(status)}">${escapeHtml(status)}</span>
      <h3>${ui(title)}</h3>
      <p class="muted">${ui(text)}</p>
    </article>
  `).join("")}</div>`;
}

function renderLinks(links) {
  const link = (text, href) => `<a class="button" href="${escapeHtml(href)}" target="_blank" rel="noopener noreferrer">${escapeHtml(text)}</a>`;
  return `<div class="links">
    ${link("Журнал активности", links.detmir_activitywatch)}
    ${link("Графики", links.grafana_dashboards)}
    ${link("События рабочих мест", links.aw_ui)}
    ${link("Рабочее время", links.worktime_report)}
    ${link("1С сводка", links.file1c_brief)}
    ${link("1С действия", links.file1c_actions)}
  </div>`;
}

function renderDlpLinks(links) {
  const link = (text, href) => `<a class="button" href="${escapeHtml(href)}" target="_blank" rel="noopener noreferrer">${escapeHtml(text)}</a>`;
  return `<div class="links">
    ${link("ИБ-графики", links.dlp_security_dashboard)}
    ${link("ИБ для руководства", links.dlp_management_dashboard)}
    ${link("Обзор рисков данных", links.dlp_overview_dashboard)}
    ${link("Все графики", links.grafana_dashboards)}
  </div>`;
}

function renderSourceList(data) {
  const sources = [
    ["DetMir", data.detmir_status],
    ["Проверки", data.detmir_check],
    ["Сервисы", data.failed_units],
    ["Данные графиков", data.grafana_data]
  ];
  return `<div class="list">${sources.map(([name, source]) => `
    <div class="row">
      <strong>${ui(name)}</strong>
      <span class="muted">${ui(source?.summary || source?.error || "нет данных")}</span>
      <span class="badge ${statusClass(source?.status || source?.ok)}">${escapeHtml(source?.status || (source?.ok ? "OK" : "FAIL"))}</span>
    </div>
  `).join("")}</div>`;
}

function renderOperator(data, report) {
  report = periodReport(report);
  const incidents = Array.isArray(data.incidents) ? data.incidents : [];
  const workforce = findSection(report, "Работа");
  const insights = findSection(report, "Выводы Workforce");
  const security = findSection(report, "ИБ");
  const actions = findSection(report, "Действия");
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Управленческая сводка</h2>
        <p class="muted">За 10 секунд: работают ли люди, где просадка, где риск и что требует внимания.</p>
      </div>
      <span class="badge ${statusClass(report?.severity)}">${escapeHtml(report?.headline || "Оперативный срез")}</span>
    </div>
    ${renderPeriodBanner(report)}
    ${renderExecutiveMetrics(report, incidents)}
    ${renderAgentQuality(report?.agent_quality, report?.agent_quality_explain)}
    ${renderAgentQualityHistory(report?.agent_quality_history, report?.agent_quality_history_summary)}
    ${renderAgentQualityNodes(report?.agent_quality_nodes, report?.agent_quality_nodes_summary)}
    ${renderAgentCoverageSla(report?.agent_coverage_sla)}
    ${renderBusinessRisk(report?.business_risk)}
    ${renderBusinessRiskTimeline(report?.business_risk_history, report?.business_risk_history_summary)}
    ${renderRiskIncidentCandidates(report?.risk_incident_candidates)}
    ${renderOverviewAnalytics(report)}
    <section class="dashboard-band">
      <div class="band-head"><h3>Рабочая активность сотрудников</h3><span class="muted">загрузка, простои, перегруз и дисциплина процессов</span></div>
      ${renderSectionItems(workforce, "Срез по работе сотрудников пока не сформирован.")}
      ${renderSectionItems(insights, "Отклонений по работе сотрудников пока не найдено.")}
    </section>
    <section class="dashboard-band security-band">
      <div class="band-head"><h3>Контроль безопасности</h3><span class="muted">подозрительные события и приоритет реакции</span></div>
      ${renderSectionItems(security, "ИБ-событий для реакции пока нет.")}
    </section>
    <section class="dashboard-band">
      <div class="band-head"><h3>Что требует внимания</h3><span class="muted">операторские действия и открытые вопросы</span></div>
      ${renderIncidentsList(incidents)}
      ${renderSectionItems(actions, "Рекомендаций нет.")}
    </section>
    <section class="dashboard-band technical-band">
      <div class="band-head"><h3>Технический статус источников</h3><span class="muted">вторичный слой для оператора</span></div>
      ${renderSourceList(data)}
    </section>
  `;
}

function renderManager(data, policyExplain) {
  const workforceIndex = workforceIndexText(data.users_count, data.total_active_seconds);
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Рабочая активность сотрудников</h2>
        <p class="muted">Оценка загрузки, простоев, перегруза и использования рабочих приложений.</p>
      </div>
      <span class="badge ${statusClass(data.status?.status)}">${escapeHtml(data.status?.status || "INFO")}</span>
    </div>
    ${renderDailyDetailNotice()}
    <div class="grid-2">
      <section class="card">
        <h3>Индекс активности</h3>
        <p class="kpi-value">${escapeHtml(workforceIndex)}</p>
        <p class="muted">расчет: активное время / плановое рабочее время</p>
        <p class="muted">Сотрудников: ${data.users_count}; активных часов: ${Number(data.total_active_hours || 0).toFixed(1)}</p>
        <p class="muted">${escapeHtml(data.status?.text || "")}</p>
      </section>
      <section class="card">
        <h3>RDP / 1C / рабочие приложения</h3>
        <div class="list">${(data.applications || []).slice(0, 8).map(app => `
          <div class="row">
            <strong>${escapeHtml(app.application)}</strong>
            <span class="muted">${escapeHtml(app.proved_work_human || "")}</span>
            <span class="badge status-ok">${escapeHtml(app.evidence_events || 0)}</span>
          </div>
        `).join("")}</div>
      </section>
    </div>
    ${renderWorkforceIndexExplanation(policyExplain)}
    <h3 class="section-title">Сотрудники без активности и с аномалиями</h3>
    <div class="list">${(data.users || []).map(user => `
      <div class="row">
        <strong>${escapeHtml(user.user)}</strong>
        <span class="muted">Активно: ${escapeHtml(user.active_hhmm || "00:00")} · последнее: ${escapeHtml(user.last_activity || "-")}</span>
        <span class="badge status-ok">${escapeHtml(user.sessions_count || 0)} сесс.</span>
      </div>
    `).join("")}</div>
  `;
}

function renderDepartments(report) {
  report = periodReport(report);
  const departments = report?.workforce?.department_comparison || [];
  const owners = report?.workforce?.owner_comparison || [];
  const insights = report?.workforce?.insights || [];
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Подразделения</h2>
        <p class="muted">Сравнение активности, тренда, риска и ответственных по группам.</p>
      </div>
      <span class="badge ${statusClass(report?.severity)}">${ui(report?.severity || "INFO")}</span>
    </div>
    ${renderPeriodBanner(report)}
    <div class="grid-2">
      <section class="card">
        <h3>Рейтинг подразделений</h3>
        ${renderSimpleItems(departments, "Подразделения пока не рассчитаны.")}
      </section>
      <section class="card">
        <h3>Ответственные</h3>
        ${renderSimpleItems(owners, "Ответственные пока не рассчитаны.")}
      </section>
    </div>
    <section class="dashboard-band">
      <div class="band-head"><h3>Просадки и отклонения</h3><span class="muted">что требует управленческого внимания</span></div>
      ${renderSimpleItems(insights, "Существенных отклонений по подразделениям пока нет.")}
    </section>
  `;
}

function renderEmployees(data, policyExplain) {
  const employees = Array.isArray(policyExplain?.employee_details) ? policyExplain.employee_details : [];
  const users = Array.isArray(data.users) ? data.users : [];
  const selected = users.slice(0, 12);
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Карточки сотрудников</h2>
        <p class="muted">Рабочий день, приложения, активность, последнее событие и риск-сигналы.</p>
      </div>
      <span class="badge ${statusClass(data.status?.status)}">${escapeHtml(data.users_count || 0)} сотрудников</span>
    </div>
    ${renderDailyDetailNotice()}
    <div class="employee-card-grid">${selected.map(user => renderEmployeeCard(user, employees)).join("") || `<p class="muted">Сотрудники пока не найдены.</p>`}</div>
    ${renderWorkforceIndexExplanation(policyExplain)}
  `;
}

function renderEmployeeCard(user, employeeDetails) {
  const detail = employeeDetails.find(item => item.user === user.user) || {};
  return `
    <article class="employee-card">
      <div class="section-head">
        <div>
          <h3>${ui(user.user || "Сотрудник")}</h3>
          <p class="muted small">Последняя активность: ${ui(user.last_activity || "-")}</p>
        </div>
        <span class="badge ${statusClass(detail.status || "INFO")}">${ui(workforceIndexTextFromValue(detail.index))}</span>
      </div>
      <div class="index-metrics employee-metrics">
        <div><span class="muted">Активность</span><strong>${ui(user.active_hhmm || "00:00")}</strong></div>
        <div><span class="muted">Сессии</span><strong>${escapeHtml(user.sessions_count || 0)}</strong></div>
        <div><span class="muted">План</span><strong>${escapeHtml(humanSeconds(detail.planned_seconds))}</strong></div>
      </div>
      <p class="muted">${ui(detail.reason || "Персональный разбор по приложениям будет точнее после накопления данных по ролям.")}</p>
    </article>
  `;
}

function renderSimpleItems(items, emptyText) {
  if (!Array.isArray(items) || items.length === 0) return `<p class="muted">${ui(emptyText)}</p>`;
  return `<div class="list compact-list">${items.slice(0, 16).map(item => `
    <div class="row compact-row">
      <strong>${ui(item.label || item.title || "-")}</strong>
      <span class="muted">${ui(item.value || item.subject || "")}</span>
      <span class="badge ${statusClass(item.status)}">${ui(item.status || "INFO")}</span>
    </div>
  `).join("")}</div>`;
}

function workforceIndexText(usersCount, activeSeconds) {
  const users = Number(usersCount || 0);
  const seconds = Number(activeSeconds || 0);
  if (users <= 0 || seconds <= 0) return "Нет данных";
  const pct = Math.max(0, Math.min(100, Math.round(seconds / (users * 8 * 3600) * 100)));
  return `${pct}%`;
}

function humanSeconds(seconds) {
  const value = Math.max(0, Number(seconds || 0));
  const totalMinutes = Math.round(value / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return `${hours}:${String(minutes).padStart(2, "0")}`;
}

function pctText(value) {
  const n = Number(value);
  if (!Number.isFinite(n)) return "0%";
  return `${Math.round(n * 100)}%`;
}

function renderWorkforceIndexExplanation(policy) {
  if (!policy || !policy.configured) {
    return `
      <section class="card index-explain-card">
        <h3>Почему такой индекс?</h3>
        <p class="muted">Правила ролей и приложений не настроены. Доступен только нейтральный индекс активности.</p>
      </section>
    `;
  }
  const details = Array.isArray(policy.app_details) ? policy.app_details.slice(0, 12) : [];
  const employees = Array.isArray(policy.employee_details) ? policy.employee_details.slice(0, 12) : [];
  const weightedTotal = Math.max(1, Number(policy.weighted_seconds || 0));
  const appRows = details.length === 0
    ? `<div class="row compact-row"><strong>Нет приложений</strong><span class="muted">Нет разбора по приложениям для взвешенного показателя</span><span></span></div>`
    : details.map(item => {
        const contribution = Math.round(Number(item.weighted_seconds || 0) / weightedTotal * 100);
        return `
          <div class="row app-weight-row">
            <strong>${escapeHtml(item.application || "-")}</strong>
            <span class="muted">${escapeHtml(humanSeconds(item.seconds))} · вес ${escapeHtml(pctText(item.weight))} · правило ${ui(item.matched_rule || "нет явного правила")}</span>
            <span class="badge ${Number(item.weight || 0) > 0 ? "status-ok" : "status-unknown"}">${escapeHtml(humanSeconds(item.weighted_seconds))} · ${contribution}%</span>
          </div>
        `;
      }).join("");
  return `
    <section class="card index-explain-card">
      <div class="section-head">
        <div>
          <h3>Почему такой индекс?</h3>
          <p class="muted">${ui(policy.explanation || "Индекс = взвешенное время приложений / плановое время роли.")}</p>
          <p class="muted small">Формула: индекс = время с учетом правил / плановое время × 100.</p>
        </div>
        <span class="badge ${statusClass(workforceIndexStatus(policy.index))}">${escapeHtml(workforceIndexTextFromValue(policy.index))}</span>
      </div>
      <div class="index-metrics">
        <div><span class="muted">Роль</span><strong>${escapeHtml(policy.role_label || policy.role || "-")}</strong></div>
        <div><span class="muted">План</span><strong>${escapeHtml(humanSeconds(policy.planned_seconds))}</strong></div>
        <div><span class="muted">Время приложений</span><strong>${escapeHtml(humanSeconds(policy.app_seconds))}</strong></div>
        <div><span class="muted">С учетом правил</span><strong>${escapeHtml(humanSeconds(policy.weighted_seconds))}</strong></div>
      </div>
      <div class="list compact-list app-weight-list">${appRows}</div>
      ${renderPolicyAudit(policy.policy_audit)}
      ${renderEmployeeIndexDetails(employees)}
    </section>
  `;
}

function renderPolicyAudit(audit) {
  const items = Array.isArray(audit?.needs_review) ? audit.needs_review.slice(0, 12) : [];
  if (items.length === 0) {
    return `<div class="audit-note"><strong>Проверка правил</strong><span class="muted">Ключевые приложения попали под явные правила или данных для проверки нет.</span></div>`;
  }
  return `
    <div class="audit-block">
      <h4>Проверка правил: приложения без явного правила</h4>
      <p class="muted small">Эти приложения не нашли явного правила и требуют проверки классификации.</p>
      <div class="list compact-list">${items.map(item => `
        <div class="row compact-row">
          <strong>${escapeHtml(item.application || "-")}</strong>
          <span class="muted">${escapeHtml(humanSeconds(item.seconds))} · вес по умолчанию ${escapeHtml(pctText(item.weight))}</span>
          <span class="badge status-warn">проверить</span>
        </div>
      `).join("")}</div>
    </div>
  `;
}

function renderEmployeeIndexDetails(items) {
  if (!items.length) return "";
  return `
    <div class="employee-drilldown">
      <h4>Разбор по сотрудникам</h4>
      <p class="muted small">Это не персональный взвешенный показатель: индекс сотрудника сейчас считается по активному времени; разбор по весам приложений доступен только на уровне общего среза.</p>
      <div class="list compact-list">${items.map(item => `
        <div class="row employee-index-row">
          <strong>${escapeHtml(item.user || "-")}</strong>
          <span class="muted">${ui(item.reason || `индекс сотрудника = активное время / плановое время × 100 · активно ${humanSeconds(item.active_seconds)} / план ${humanSeconds(item.planned_seconds)}`)}</span>
          <span class="badge ${statusClass(item.status)}">${escapeHtml(workforceIndexTextFromValue(item.index))}</span>
        </div>
      `).join("")}</div>
    </div>
  `;
}

function workforceIndexTextFromValue(value) {
  const n = Number(value);
  return Number.isFinite(n) ? `${Math.round(n)}%` : "Нет данных";
}

function workforceIndexStatus(value) {
  const n = Number(value);
  if (!Number.isFinite(n)) return "UNKNOWN";
  if (n >= 80) return "OK";
  if (n >= 45) return "WARN";
  return "FAIL";
}

function renderOwner(data) {
  const cards = Object.entries(data.cards || {});
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Контроль безопасности</h2>
        <p class="muted">Приоритеты ИБ: подозрительные подключения, сигналы по данным, RDP-аномалии и состояние периметра.</p>
      </div>
      <span class="badge ${statusClass(data.summary?.severity)}">${escapeHtml(data.summary?.severity || "INFO")}</span>
    </div>
    <div class="summary-grid">${cards.map(([name, block]) => `
      <article class="card">
        <span class="badge ${statusClass(block.status)}">${escapeHtml(block.status)}</span>
        <h3>${ui(label(name))}</h3>
        <p class="muted">${ui(block.text)}</p>
      </article>
    `).join("")}</div>
    <section class="card">
      <h3>Риски с приоритетом</h3>
      <div class="list">${(data.recommendations || []).map(item => `<div class="row"><strong>Рекомендация</strong><span class="muted">${ui(item)}</span><span></span></div>`).join("")}</div>
    </section>
    <section class="card">
      <h3>Графики безопасности</h3>
      ${renderLinks(data.links)}
    </section>
  `;
}

function renderPerimeter(data, report) {
  report = periodReport(report);
  const risk = report?.ueba_risk || {};
  const cards = Object.entries(data.cards || {});
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Сетевой периметр</h2>
        <p class="muted">Режим наблюдения: состояние внешних сигналов, необычные направления и связь с рисками сотрудников. Управление сетью отсюда не выполняется.</p>
      </div>
      <span class="badge status-unknown">наблюдение</span>
    </div>
    ${renderPeriodBanner(report)}
    <div class="summary-grid">
      ${metricCard("Нетипичные направления", "нет данных", "UNKNOWN", "источник периметра не подключен к порталу")}
      ${metricCard("Сетевые события", "нет данных", "UNKNOWN", "ожидается интеграционный слой")}
      ${metricCard("Связь с рисками", `${risk.score ?? 0}/100`, risk.status || "UNKNOWN", risk.summary || "оценка по правилам")}
      ${metricCard("Автоматическое воздействие", "выключено", "OK", "портал работает только на чтение")}
    </div>
    <section class="dashboard-band security-band">
      <div class="band-head"><h3>Состояние доступных сигналов</h3><span class="muted">без изменения правил периметра</span></div>
      ${renderSimpleItems(cards.map(([name, block]) => ({ label: label(name), value: block.text, status: block.status })), "Сигналы периметра пока не подключены.")}
    </section>
  `;
}

function renderIncidentsList(items) {
  if (!items || items.length === 0) return `<p class="muted">Активных проблем нет.</p>`;
  return `<div class="list">${items.map(item => `
    <div class="row incident-row">
      <div>
        <strong>${ui(item.source)}</strong>
        <div class="muted small">${ui(item.kind)} · ${escapeHtml(item.id)}</div>
      </div>
      <div>
        <span class="muted">${ui(item.summary)}</span>
        ${item.acknowledged ? `<div class="muted small">В работе: ${ui(item.assigned_to || item.actor || "оператор")} · ${ui(item.comment || "")}</div>` : ""}
      </div>
      <span class="badge ${statusClass(item.status)}">${escapeHtml(item.status)}</span>
      <div class="actions">
        <button class="small-button" data-incident-action="ack" data-incident-id="${escapeHtml(item.id)}"${item.acknowledged ? " disabled" : ""}>В работу</button>
        <button class="small-button" data-incident-action="assign" data-incident-id="${escapeHtml(item.id)}">Назначить</button>
      </div>
    </div>
  `).join("")}</div>`;
}

function isDlpIncident(item) {
  const text = `${item?.kind || ""} ${item?.source || ""} ${item?.summary || ""}`.toLowerCase();
  return text.includes("dlp") || text.includes("incident") || text.includes("case") || text.includes("иб");
}

function renderDlpIncidentsList(items) {
  const dlpItems = (items || []).filter(isDlpIncident);
  if (dlpItems.length === 0) return `<p class="muted">Активных ИБ-инцидентов по данным нет.</p>`;
  return renderIncidentsList(dlpItems);
}

function renderDlpEvidence(evidence) {
  if (!evidence) return `<p class="muted">Материалы проверки загружаются.</p>`;
  if (!evidence.ok) return `<p class="muted">Материалы недоступны: ${ui(evidence.error || "ошибка чтения")}</p>`;
  const items = evidence.items || [];
  if (items.length === 0) return `<p class="muted">Материалы по ИБ-инцидентам пока не найдены.</p>`;
  return `<div class="list evidence-list">${items.map(item => `
    <div class="row evidence-row">
      <div>
        <strong>${escapeHtml(item.signal_type || item.source || item.stream_type)}</strong>
        <div class="muted small">${escapeHtml(item.event_ts)} · ${escapeHtml(item.hostname)}${item.username ? " · " + escapeHtml(item.username) : ""}</div>
      </div>
      <div>
        <span class="muted">${escapeHtml(item.message || item.file_path || item.rule_id || item.event_id)}</span>
        <div class="muted small">${item.source_file ? "Файл: " + escapeHtml(item.source_file) + " · " : ""}${item.screenshot_sha256 ? "Контрольный хеш: " + escapeHtml(item.screenshot_sha256) : ui(item.blocked_reason || "без скрина")}</div>
      </div>
      <span class="badge ${item.screenshot_available ? "status-ok" : "status-warn"}">${item.screenshot_available ? "СКРИН" : "МЕТА"}</span>
      <div class="actions">
        ${item.preview_url ? `<a class="small-button" href="${escapeHtml(item.preview_url)}" target="_blank" rel="noopener noreferrer">Открыть</a>` : ""}
        ${item.download_url ? `<a class="small-button" href="${escapeHtml(item.download_url)}" target="_blank" rel="noopener noreferrer">Скачать</a>` : ""}
      </div>
    </div>
  `).join("")}</div>`;
}

function buildAutoInvestigation(report) {
  report = periodReport(report || state.reports || {});
  const rows = departmentRows(report);
  const risky = [...rows].sort((a, b) => statusWeight(b.status) - statusWeight(a.status) || (a.activity ?? 101) - (b.activity ?? 101))[0];
  const reasons = Array.isArray(report?.ueba_risk?.reasons) ? report.ueba_risk.reasons : [];
  const risk = reasons[0] || {};
  const status = risky?.status || risk.status || report?.ueba_risk?.status || "INFO";
  const department = risky?.label || "Портфель";
  const owner = risky?.responsible || "ответственный не назначен";
  const generated = report?.generated_at_utc || new Date().toISOString();
  const summary = risk.label || risk.code || risky?.reason || "Система сформировала риск-сигнал для ручной проверки";
  return {
    incident_id: `auto-${String(department).toLowerCase().replace(/[^a-zа-я0-9]+/gi, "-").replace(/^-|-$/g, "") || "risk"}`,
    risk_id: risk.code || risk.label || "workforce-ueba-risk",
    department,
    owner,
    activity_index: risky?.activityText || "нет данных",
    deviation: risky?.deviation || "нет данных",
    status,
    summary,
    why_it_is_risk: risky?.reason || risk.value || "риск может указывать на просадку активности, отклонение от нормы или событие безопасности",
    what_to_check: risky?.check || risk.recommendation || "проверить первичные события ActivityWatch, RDP/1C активность, процессы и сетевые сигналы",
    recommended_actions: [
      "назначить ответственного за ручную проверку",
      "сопоставить риск с журналами активности и бизнес-задачей",
      "зафиксировать вывод в отчете по инциденту"
    ],
    evidence: [
      "RDP activity: проверяется по событиям рабочего времени",
      "process activity: проверяется по процессам и приложениям",
      "network activity: проверяется по сетевым сигналам",
      "proxy activity: подключается как внешний источник",
      "pfSense events: учитываются при наличии интеграционного слоя"
    ],
    generated_at: generated
  };
}

function renderAutoInvestigationCard(report) {
  const card = buildAutoInvestigation(report);
  return `
    <section class="card investigation-card">
      <div class="section-head">
        <div>
          <h3>Автоматическая карточка расследования</h3>
          <p class="muted">Расследование сформировано автоматически. Решение принимает ответственный сотрудник.</p>
        </div>
        <span class="badge ${statusClass(card.status)}">${ui(card.status)}</span>
      </div>
      <div class="investigation-grid">
        <div><span class="muted">incident_id</span><strong>${ui(card.incident_id)}</strong></div>
        <div><span class="muted">risk_id</span><strong>${ui(card.risk_id)}</strong></div>
        <div><span class="muted">department</span><strong>${ui(card.department)}</strong></div>
        <div><span class="muted">owner</span><strong>${ui(card.owner)}</strong></div>
        <div><span class="muted">activity_index</span><strong>${ui(card.activity_index)}</strong></div>
        <div><span class="muted">deviation</span><strong>${ui(card.deviation)}</strong></div>
        <div><span class="muted">generated_at</span><strong>${ui(card.generated_at)}</strong></div>
      </div>
      <div class="list compact-list">
        <div class="row compact-row"><strong>summary</strong><span class="muted">${ui(card.summary)}</span><span></span></div>
        <div class="row compact-row"><strong>why_it_is_risk</strong><span class="muted">${ui(card.why_it_is_risk)}</span><span></span></div>
        <div class="row compact-row"><strong>what_to_check</strong><span class="muted">${ui(card.what_to_check)}</span><span></span></div>
        <div class="row compact-row"><strong>recommended_actions</strong><span class="muted">${ui(card.recommended_actions.join("; "))}</span><span></span></div>
        <div class="row compact-row"><strong>evidence</strong><span class="muted">${ui(card.evidence.join("; "))}</span><span></span></div>
      </div>
    </section>
  `;
}

function renderIncidents(data) {
  const links = state.links || {};
  const incidents = Array.isArray(data) ? data : data.incidents;
  const evidence = Array.isArray(data) ? null : data.evidence;
  const reports = Array.isArray(data) ? state.reports : data.reports;
  const cases = Array.isArray(data?.cases?.cases) ? data.cases.cases : [];
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Расследования и доказательная база</h2>
        <p class="muted">Кто, когда, откуда, куда, связанные хосты, материалы и выгрузка для проверки.</p>
      </div>
      <span class="badge ${statusClass(incidentStatusFromCount((incidents || []).length))}">${escapeHtml((incidents || []).length)} записей</span>
    </div>
    <div class="grid-2">
      <section class="card">
        <h3>Карточки инцидентов</h3>
        ${renderDlpIncidentsList(incidents)}
      </section>
      <section class="card">
        <h3>Связанные графики</h3>
        ${renderDlpLinks(links)}
      </section>
    </div>
    ${renderAutoInvestigationCard(reports)}
    ${renderCases(cases)}
    <section class="card evidence-card">
      <h3>Материалы: скриншоты, хеши, файлы</h3>
      ${renderDlpEvidence(evidence)}
    </section>
  `;
}

function renderCases(cases) {
  const rows = Array.isArray(cases) ? cases.slice(0, 20) : [];
  return `
    <section class="card cases-card">
      <div class="section-head">
        <div>
          <h3>Дела</h3>
          <p class="muted">Ручные дела, созданные из подтвержденных кандидатов. Автоматически дела не создаются.</p>
        </div>
        <span class="badge ${statusClass(rows.length ? "OPEN" : "OK")}">${rows.length}</span>
      </div>
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>Дело</th>
              <th>Кандидат</th>
              <th>Статус</th>
              <th>Ответственный</th>
              <th>Решение</th>
              <th>Действие</th>
            </tr>
          </thead>
          <tbody>
            ${rows.length ? rows.map(item => `
              <tr>
                <td><strong>${ui(item.title || item.case_id)}</strong><br><code>${ui(item.case_id || "-")}</code><br><span class="muted small">${ui(item.created_at_utc || "-")} · ${ui(item.updated_at_utc || "-")}</span></td>
                <td><code>${ui(item.candidate_id || "-")}</code><br><span class="muted small">${ui(item.summary || "резюме не задано")}</span></td>
                <td><span class="badge ${statusClass(item.status)}">${ui(caseStatusText(item.status))}</span></td>
                <td>${ui(item.owner || "ответственный не указан")}</td>
                <td>${ui(item.decision || "решение не зафиксировано")}</td>
                <td>${renderCaseActions(item)}</td>
              </tr>
            `).join("") : `
              <tr>
                <td>Нет дел</td>
                <td>-</td>
                <td><span class="badge status-ok">OK</span></td>
                <td>-</td>
                <td>Создайте дело из подтвержденного кандидата.</td>
                <td>-</td>
              </tr>
            `}
          </tbody>
        </table>
      </div>
    </section>
  `;
}

function renderCaseActions(item) {
  const id = item?.case_id || "";
  const packUrl = `/portal/api/cases/${encodeURIComponent(id)}?format=markdown`;
  return `
    <div class="button-row compact-actions">
      <button class="small-button" data-case-status="IN_PROGRESS" data-case-id="${escapeHtml(id)}">В работу</button>
      <button class="small-button" data-case-status="RESOLVED" data-case-id="${escapeHtml(id)}">Решено</button>
      <button class="small-button" data-case-status="REJECTED" data-case-id="${escapeHtml(id)}">Отклонить</button>
      <button class="small-button" data-case-status="ARCHIVED" data-case-id="${escapeHtml(id)}">В архив</button>
    </div>
    <a class="small-button investigation-pack-button" href="${escapeHtml(packUrl)}" download>Скачать карточку дела</a>
  `;
}

function caseStatusText(status) {
  const value = String(status || "OPEN").toUpperCase();
  if (value === "IN_PROGRESS") return "В работе";
  if (value === "RESOLVED") return "Решено";
  if (value === "REJECTED") return "Отклонено";
  if (value === "ARCHIVED") return "Архив";
  return "Открыто";
}

function renderKpiCards(items) {
  return `<div class="summary-grid kpi-grid">${(items || []).map(item => `
    <article class="card kpi-card">
      <span class="badge ${statusClass(item.status)}">${escapeHtml(item.status || "INFO")}</span>
      <h3>${ui(item.label)}</h3>
      <p class="kpi-value">${ui(item.value)}</p>
      <p class="muted">${ui(item.context || "")}</p>
    </article>
  `).join("")}</div>`;
}

function renderReportSections(sections) {
  return `<div class="grid-2">${(sections || []).map(section => `
    <section class="card report-section">
      <h3>${ui(section.title)}</h3>
      <div class="list compact-list">${(section.items || []).map(item => `
        <div class="row compact-row">
          <strong>${ui(item.label)}</strong>
          <span class="muted">${ui(item.value)}</span>
          <span class="badge ${statusClass(item.status)}">${escapeHtml(item.status || "INFO")}</span>
        </div>
      `).join("")}</div>
    </section>
  `).join("")}</div>`;
}

function renderUebaRisk(risk) {
  if (!risk) return "";
  const reasons = Array.isArray(risk.reasons) ? risk.reasons.slice(0, 12) : [];
  const sources = Array.isArray(risk.risk_sources) ? risk.risk_sources.join(", ") : "-";
  const confidence = Number.isFinite(Number(risk.confidence)) ? `${Math.round(Number(risk.confidence) * 100)}%` : "0%";
  const baselineReady = `user: ${risk.user_baseline_available ? "yes" : "no"} · dept: ${risk.department_baseline_available ? "yes" : "no"}`;
  return `
    <section class="card ueba-risk-card">
      <div class="section-head">
        <div>
          <h3>Индекс риска</h3>
          <p class="muted">${ui(risk.note || "Оценка риска без автоматического воздействия.")}</p>
          <p class="muted small">Формула: сумма факторов риска, максимум 100.</p>
          <p class="muted small">Достоверность: ${escapeHtml(confidence)} · источники: ${ui(sources)} · обычный профиль: ${ui(risk.baseline_status || "-")} · версия правил: ${ui(risk.policy_version || "-")}</p>
          <p class="muted small">Окно сравнения: ${escapeHtml(risk.baseline_window_days || "-")} дн. · доступно: ${ui(baselineReady)} · отклонение: ${escapeHtml(risk.deviation_score ?? 0)}</p>
        </div>
        <span class="badge ${statusClass(risk.status)}">${escapeHtml(risk.level || "unknown")} · ${escapeHtml(risk.score ?? 0)}/100</span>
      </div>
      <div class="list compact-list">${reasons.length ? reasons.map(item => `
        <div class="row compact-row">
          <strong>${ui(item.label || item.code || "-")}</strong>
          <span class="muted">${ui(item.value || "")} · ${ui(item.recommendation || "")}</span>
          <span class="badge ${statusClass(item.status || item.severity)}">+${escapeHtml(item.points || 0)}</span>
        </div>
      `).join("") : `<div class="row compact-row"><strong>Сигналы</strong><span class="muted">Существенных риск-сигналов в текущем срезе нет.</span><span class="badge status-ok">OK</span></div>`}</div>
    </section>
  `;
}

function fallbackAgentQualityExplain(quality) {
  const q = quality || {};
  const source = q.collector_source || "unknown";
  const hasError = Boolean(q.collector_error);
  if (source === "wts_api" && !hasError) {
    return {
      status: "OK",
      title: "Данные агента подтверждают KPI",
      summary: "Сессии собраны основным способом через Windows WTS API; индекс активности можно использовать как рабочий управленческий KPI.",
      recommendation: "Использовать отчет как подтвержденный оперативный срез.",
      kpi_accepted: true
    };
  }
  if (source === "local_fallback") {
    return {
      status: "DEGRADED",
      title: "Диагностический режим агента",
      summary: "Диагностический режим, данные не засчитываются в KPI.",
      recommendation: "Проверить доступность WTS API и права запуска агента.",
      kpi_accepted: false
    };
  }
  if (hasError) {
    return {
      status: "DEGRADED",
      title: "Достоверность данных снижена",
      summary: `Коллектор передал ошибку: ${q.collector_error}`,
      recommendation: "Восстановить основной путь WTS API перед использованием отчета как доказательной базы.",
      kpi_accepted: false
    };
  }
  return {
    status: "UNKNOWN",
    title: "Достоверность данных неизвестна",
    summary: "Агент не передал диагностику качества данных.",
    recommendation: "Обновить Rust agent до версии с diagnostics и проверить telemetry.jsonl.",
    kpi_accepted: false
  };
}

function renderAgentQuality(quality, explain) {
  const q = quality || {};
  const e = explain || fallbackAgentQualityExplain(q);
  const status = e.status || q.quality_status || "UNKNOWN";
  const source = q.collector_source || "unknown";
  const warn = ["warning", "fallback", "degraded", "error"].includes(String(status).toLowerCase());
  const accepted = Boolean(e.kpi_accepted);
  return `
    <section class="card agent-quality-card">
      <div class="section-head">
        <div>
          <h3>Достоверность данных агента</h3>
          <p class="muted">${ui(e.title || "Оценка доверия к данным агента")}</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <p class="quality-summary">${ui(e.summary || "")}</p>
      <div class="quality-decision">
        <div><span class="muted">Принято в KPI</span><strong>${accepted ? "да" : "нет"}</strong></div>
        <div><span class="muted">Источник</span><strong>${ui(source)}</strong></div>
      </div>
      ${warn ? `<div class="quality-warning">Внимание. Данные активности собраны не основным способом. Точность определения активности и RDP-сессий может быть снижена.</div>` : ""}
      <p class="muted">${ui(e.recommendation || "")}</p>
      <details class="quality-details">
        <summary>Технические детали</summary>
        <div class="quality-grid">
          <div><span class="muted">Источник коллектора</span><strong>${ui(source)}</strong></div>
          <div><span class="muted">Всего сессий</span><strong>${escapeHtml(q.sessions_collected_total ?? 0)}</strong></div>
          <div><span class="muted">Активных сессий</span><strong>${escapeHtml(q.active_sessions_total ?? 0)}</strong></div>
          <div><span class="muted">RDP-сессий</span><strong>${escapeHtml(q.rdp_sessions_total ?? 0)}</strong></div>
          <div><span class="muted">Ошибка коллектора</span><strong>${q.collector_error ? ui(q.collector_error) : "нет"}</strong></div>
        </div>
      </details>
    </section>
  `;
}

function renderAgentQualityHistory(history, summary) {
  const items = Array.isArray(history) ? history : [];
  const s = summary || {};
  const unstableDays = Number(s.warning_days || 0) + Number(s.degraded_days || 0) + Number(s.unknown_days || 0);
  const status = items.length === 0 ? "UNKNOWN" : (Number(s.ok_days || 0) >= 5 ? "OK" : "WARNING");
  return `
    <section class="card agent-quality-history-card">
      <div class="section-head">
        <div>
          <h3>Стабильность агента за 7 дней</h3>
          <p class="muted">Показывает, можно ли доверять недельному KPI, а не только текущему срезу.</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <div class="quality-decision">
        <div><span class="muted">OK дней</span><strong>${escapeHtml(s.ok_days ?? 0)}</strong></div>
        <div><span class="muted">Проблемных дней</span><strong>${escapeHtml(unstableDays)}</strong></div>
        <div><span class="muted">KPI принят</span><strong>${escapeHtml(s.kpi_accepted_pct ?? 0)}%</strong></div>
      </div>
      ${Number(s.ok_days || 0) < 5 ? `<div class="quality-warning">KPI требует валидации: нестабильный сбор данных агента.</div>` : ""}
      <details class="quality-details">
        <summary>История по дням</summary>
        <div class="list compact-list">${items.length ? items.map(item => `
          <div class="row compact-row">
            <strong>${escapeHtml(item.date || "-")}</strong>
            <span class="muted">source=${ui(item.source || "unknown")} · KPI=${item.kpi_accepted ? "да" : "нет"}${item.collector_error ? ` · ${ui(item.collector_error)}` : ""}</span>
            <span class="badge ${statusClass(item.status)}">${ui(item.status || "UNKNOWN")}</span>
          </div>
        `).join("") : `<div class="row compact-row"><strong>История</strong><span class="muted">История качества агента за период отсутствует.</span><span class="badge status-unknown">UNKNOWN</span></div>`}</div>
      </details>
    </section>
  `;
}

function renderAgentQualityNodes(nodes, summary) {
  const items = Array.isArray(nodes) ? nodes : [];
  const s = summary || {};
  const problematic = items.filter(item => String(item.status || "UNKNOWN") !== "OK" || !item.kpi_accepted);
  const rows = (problematic.length ? problematic : items).slice(0, 10);
  const status = Number(s.total_nodes || 0) === 0
    ? "UNKNOWN"
    : (Number(s.accepted_kpi_nodes_pct || 0) >= 80 ? "OK" : "WARNING");
  return `
    <section class="card agent-quality-nodes-card">
      <div class="section-head">
        <div>
          <h3>Качество данных по рабочим местам</h3>
          <p class="muted">Какие узлы подтверждают KPI, а какие снижают доверие к управленческой аналитике.</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <div class="quality-decision">
        <div><span class="muted">Всего узлов</span><strong>${escapeHtml(s.total_nodes ?? 0)}</strong></div>
        <div><span class="muted">OK</span><strong>${escapeHtml(s.ok_nodes ?? 0)}</strong></div>
        <div><span class="muted">Проблемных</span><strong>${escapeHtml(Number(s.degraded_nodes || 0) + Number(s.unknown_nodes || 0))}</strong></div>
        <div><span class="muted">KPI принят</span><strong>${escapeHtml(s.accepted_kpi_nodes_pct ?? 0)}%</strong></div>
      </div>
      ${Number(s.total_nodes || 0) > 0 && Number(s.accepted_kpi_nodes_pct || 0) < 80 ? `<div class="quality-warning">KPI требует проверки: менее 80% узлов дают подтвержденные данные.</div>` : ""}
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>Узел</th>
              <th>Статус</th>
              <th>Источник</th>
              <th>Последняя телеметрия</th>
              <th>KPI</th>
              <th>Рекомендация</th>
            </tr>
          </thead>
          <tbody>
            ${rows.length ? rows.map(item => `
              <tr>
                <td>${ui(item.hostname || "unknown")}</td>
                <td><span class="badge ${statusClass(item.status)}">${ui(item.status || "UNKNOWN")}</span></td>
                <td>${ui(item.source || "unknown")}</td>
                <td>${escapeHtml(item.last_seen_utc || "-")}</td>
                <td>${item.kpi_accepted ? "да" : "нет"}</td>
                <td>${ui(item.recommendation || "")}</td>
              </tr>
            `).join("") : `
              <tr>
                <td>Нет данных</td>
                <td><span class="badge status-unknown">UNKNOWN</span></td>
                <td>unknown</td>
                <td>-</td>
                <td>нет</td>
                <td>История качества по рабочим местам отсутствует.</td>
              </tr>
            `}
          </tbody>
        </table>
      </div>
      ${problematic.length > 10 ? `<p class="muted small">Показаны первые 10 проблемных узлов из ${escapeHtml(problematic.length)}.</p>` : ""}
    </section>
  `;
}

function renderAgentCoverageSla(sla) {
  const s = sla || {};
  const rows = Array.isArray(s.problem_nodes) ? s.problem_nodes.slice(0, 10) : [];
  const status = s.sla_status || "UNKNOWN";
  return `
    <section class="card agent-coverage-card">
      <div class="section-head">
        <div>
          <h3>Покрытие агентов</h3>
          <p class="muted">Показывает, насколько KPI репрезентативен по всему парку рабочих мест.</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <div class="quality-decision">
        <div><span class="muted">Ожидается узлов</span><strong>${escapeHtml(s.expected_nodes ?? 0)}</strong></div>
        <div><span class="muted">Данные за 24 часа</span><strong>${escapeHtml(s.reporting_nodes_24h ?? 0)}</strong></div>
        <div><span class="muted">Устаревшие</span><strong>${escapeHtml(s.stale_nodes ?? 0)}</strong></div>
        <div><span class="muted">Отсутствующие</span><strong>${escapeHtml(s.missing_nodes ?? 0)}</strong></div>
        <div><span class="muted">Покрытие</span><strong>${escapeHtml(s.coverage_pct ?? 0)}%</strong></div>
        <div><span class="muted">Свежесть</span><strong>${escapeHtml(s.freshness_pct ?? 0)}%</strong></div>
      </div>
      ${status === "CRITICAL" ? `<div class="quality-warning">Покрытие агентов критически недостаточно: KPI не может считаться репрезентативным.</div>` : ""}
      ${status === "WARNING" ? `<div class="quality-warning">KPI требует проверки: часть рабочих мест не присылает свежую телеметрию.</div>` : ""}
      ${status === "UNKNOWN" ? `<p class="muted">Список ожидаемых рабочих мест не настроен.</p>` : ""}
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>Узел</th>
              <th>Подразделение</th>
              <th>Ответственный</th>
              <th>Последняя телеметрия</th>
              <th>Статус</th>
              <th>Рекомендация</th>
            </tr>
          </thead>
          <tbody>
            ${rows.length ? rows.map(item => `
              <tr>
                <td>${ui(item.hostname || "unknown")}</td>
                <td>${ui(item.department || "-")}</td>
                <td>${ui(item.owner || "-")}</td>
                <td>${escapeHtml(item.last_seen_utc || "-")}</td>
                <td><span class="badge ${statusClass(item.status)}">${ui(item.status || "UNKNOWN")}</span></td>
                <td>${ui(item.recommendation || "")}</td>
              </tr>
            `).join("") : `
              <tr>
                <td>${Number(s.expected_nodes || 0) === 0 ? "Нет списка" : "Проблем нет"}</td>
                <td>-</td>
                <td>-</td>
                <td>-</td>
                <td><span class="badge ${statusClass(status)}">${ui(status)}</span></td>
                <td>${Number(s.expected_nodes || 0) === 0 ? "Настроить expected_nodes.json." : "Действий не требуется."}</td>
              </tr>
            `}
          </tbody>
        </table>
      </div>
      ${Array.isArray(s.problem_nodes) && s.problem_nodes.length > 10 ? `<p class="muted small">Показаны первые 10 проблемных узлов из ${escapeHtml(s.problem_nodes.length)}.</p>` : ""}
    </section>
  `;
}

function renderBusinessRisk(items) {
  const rows = Array.isArray(items) ? items.slice(0, 10) : [];
  const worst = rows[0]?.risk_level || "UNKNOWN";
  return `
    <section class="card business-risk-card">
      <div class="section-head">
        <div>
          <h3>Риски подразделений</h3>
          <p class="muted">Организационные зоны риска по доверию к KPI, активности, тренду и проблемным узлам.</p>
        </div>
        <span class="badge ${statusClass(worst)}">${ui(worst)}</span>
      </div>
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>Подразделение</th>
              <th>Риск</th>
              <th>Причины</th>
              <th>Рекомендация</th>
            </tr>
          </thead>
          <tbody>
            ${rows.length ? rows.map(item => `
              <tr>
                <td><strong>${ui(item.department || "Без подразделения")}</strong></td>
                <td><span class="badge ${statusClass(item.risk_level)}">${ui(item.risk_level || "UNKNOWN")}</span></td>
                <td>${ui(businessRiskReasons(item))}</td>
                <td>${ui(item.recommendation || "Проверить первичные данные подразделения.")}</td>
              </tr>
            `).join("") : `
              <tr>
                <td>Нет данных</td>
                <td><span class="badge status-unknown">UNKNOWN</span></td>
                <td>нет данных</td>
                <td>Дождаться расчета подразделений.</td>
              </tr>
            `}
          </tbody>
        </table>
      </div>
    </section>
  `;
}

function businessRiskReasons(item) {
  const reasons = Array.isArray(item?.reasons) && item.reasons.length
    ? item.reasons.join("; ")
    : "существенных причин не найдено";
  return `${reasons}. Trust ${item?.trust_score ?? 0}%, активность ${item?.activity_score ?? 0}%, тренд ${businessTrendText(item?.trend)}, проблемных узлов ${item?.problem_nodes_count ?? 0}`;
}

function renderBusinessRiskTimeline(history, summary) {
  const s = summary || {};
  const rows = Array.isArray(history) ? history.slice(-10).reverse() : [];
  const status = Number(s.stable_high_risk || 0) > 0 || Number(s.new_high_risk || 0) > 0
    ? "WARN"
    : "OK";
  const counters = [
    ["Ухудшились", s.departments_worsened ?? 0, "рост уровня риска за период"],
    ["Улучшились", s.departments_improved ?? 0, "снижение уровня риска"],
    ["Стабильно высокий риск", s.stable_high_risk ?? 0, "3+ дня HIGH/CRITICAL"],
    ["Новый высокий риск", s.new_high_risk ?? 0, "последняя точка стала HIGH/CRITICAL"],
  ];
  return `
    <section class="card business-risk-timeline-card">
      <div class="section-head">
        <div>
          <h3>Динамика рисков</h3>
          <p class="muted">Как менялся бизнес-риск подразделений по накопленной daily history.</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <div class="summary-grid kpi-grid compact-kpis">
        ${counters.map(([label, value, hint]) => `
          <article class="mini-kpi">
            <span>${ui(label)}</span>
            <strong>${escapeHtml(value)}</strong>
            <small>${ui(hint)}</small>
          </article>
        `).join("")}
      </div>
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>Дата</th>
              <th>Подразделение</th>
              <th>Риск</th>
              <th>Причины</th>
            </tr>
          </thead>
          <tbody>
            ${rows.length ? rows.map(item => `
              <tr>
                <td>${ui(item.date || "-")}</td>
                <td><strong>${ui(item.department || "Без подразделения")}</strong></td>
                <td><span class="badge ${statusClass(item.risk_level)}">${ui(item.risk_level || "UNKNOWN")}</span></td>
                <td>${ui(Array.isArray(item.reasons) && item.reasons.length ? item.reasons.join("; ") : "нет существенных причин")}</td>
              </tr>
            `).join("") : `
              <tr>
                <td>-</td>
                <td>История не накоплена</td>
                <td><span class="badge status-unknown">UNKNOWN</span></td>
                <td>Нужно дождаться daily history.</td>
              </tr>
            `}
          </tbody>
        </table>
      </div>
    </section>
  `;
}

function renderRiskIncidentCandidates(items) {
  const rows = Array.isArray(items) ? items.slice(0, 10) : [];
  const worst = rows[0]?.risk_level || "UNKNOWN";
  return `
    <section class="card risk-candidates-card">
      <div class="section-head">
        <div>
          <h3>Кандидаты в инциденты</h3>
          <p class="muted">Read-only очередь ручной проверки. Реальные инциденты автоматически не создаются.</p>
        </div>
        <span class="badge ${statusClass(worst)}">${ui(worst)}</span>
      </div>
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Подразделение</th>
              <th>Узел</th>
              <th>Риск</th>
              <th>Проверка</th>
              <th>Причина</th>
              <th>Рекомендация</th>
              <th>Действие</th>
            </tr>
          </thead>
          <tbody>
            ${rows.length ? rows.map(item => `
              <tr>
                <td><code>${ui(item.id || "-")}</code></td>
                <td>${ui(item.department || "-")}<br><span class="muted small">${ui(item.owner || "ответственный не указан")}</span></td>
                <td>${ui(item.hostname || "-")}<br><span class="muted small">${ui(candidateSeenText(item))}</span></td>
                <td><span class="badge ${statusClass(item.risk_level)}">${ui(item.risk_level || "UNKNOWN")}</span></td>
                <td>${renderCandidateReview(item)}</td>
                <td>${ui(candidateReasonText(item))}</td>
                <td>${ui(item.recommendation || "Назначить ответственную ручную проверку.")}</td>
                <td>${renderCandidateReviewActions(item)}</td>
              </tr>
            `).join("") : `
              <tr>
                <td>-</td>
                <td>Нет кандидатов</td>
                <td>-</td>
                <td><span class="badge status-ok">OK</span></td>
                <td><span class="badge status-unknown">NEW</span></td>
                <td>очередь проверки пуста</td>
                <td>Действий не требуется.</td>
                <td>-</td>
              </tr>
            `}
          </tbody>
        </table>
      </div>
      ${rows.length ? `<p class="muted small">Показаны кандидаты для проверки, а не автоматически подтвержденные инциденты.</p>` : ""}
    </section>
  `;
}

function renderCandidateReview(item) {
  const review = item?.incident_review || {};
  const status = review.status || "NEW";
  const comment = review.comment || "комментария нет";
  const reviewer = review.reviewer || "проверяющий не указан";
  const updated = review.updated_at || "не обновлялось";
  const audit = Array.isArray(item?.incident_review_audit) ? item.incident_review_audit.slice(-4).reverse() : [];
  const history = audit.length ? `
    <details class="review-history">
      <summary>История изменений</summary>
      <ul>
        ${audit.map(entry => `
          <li>
            <span>${ui(reviewStatusText(entry.old_status))} → ${ui(reviewStatusText(entry.new_status))}</span><br>
            <span class="muted small">${ui(entry.reviewer || "проверяющий не указан")} · ${ui(entry.changed_at_utc || "-")}</span><br>
            <span class="muted small">${ui(entry.comment || "комментария нет")}</span>
          </li>
        `).join("")}
      </ul>
    </details>
  ` : `<span class="muted small">История изменений отсутствует</span>`;
  return `
    <span class="badge ${statusClass(status)}">${ui(reviewStatusText(status))}</span><br>
    <span class="muted small">Изменил: ${ui(reviewer)}</span><br>
    <span class="muted small">Когда: ${ui(updated)}</span><br>
    <span class="muted small">Комментарий: ${ui(comment)}</span>
    ${history}
  `;
}

function renderCandidateReviewActions(item) {
  const id = item?.id || "";
  const reviewStatus = String(item?.incident_review?.status || "NEW").toUpperCase();
  const actions = [
    ["IN_REVIEW", "В проверку"],
    ["CONFIRMED", "Подтвердить"],
    ["FALSE_POSITIVE", "Ложный"],
    ["POSTPONED", "Отложить"],
  ];
  const packUrl = `/portal/api/investigation-pack/${encodeURIComponent(id)}?format=markdown`;
  const createCase = reviewStatus === "CONFIRMED"
    ? `<button class="small-button investigation-pack-button primary" data-create-case="true" data-candidate-id="${escapeHtml(id)}">Создать дело</button>`
    : "";
  return `
    <div class="button-row compact-actions">${actions.map(([status, label]) => `
      <button class="small-button" data-review-status="${escapeHtml(status)}" data-candidate-id="${escapeHtml(id)}">${ui(label)}</button>
    `).join("")}</div>
    <a class="small-button investigation-pack-button" href="${escapeHtml(packUrl)}" download>Скачать пакет расследования</a>
    ${createCase}
  `;
}

function reviewStatusText(status) {
  const value = String(status || "NEW").toUpperCase();
  if (value === "IN_REVIEW") return "В проверке";
  if (value === "CONFIRMED") return "Подтвержден";
  if (value === "FALSE_POSITIVE") return "Ложный";
  if (value === "POSTPONED") return "Отложен";
  return "Новый";
}

function candidateReasonText(item) {
  const evidence = Array.isArray(item?.evidence) && item.evidence.length
    ? ` Evidence: ${item.evidence.join("; ")}`
    : "";
  return `${item?.reason || "требуется проверка"}.${evidence}`;
}

function candidateSeenText(item) {
  const first = item?.first_seen_utc || "-";
  const last = item?.last_seen_utc || "-";
  return `первое: ${first}; последнее: ${last}`;
}

function businessTrendText(value) {
  const trend = String(value || "UNKNOWN").toUpperCase();
  if (trend === "FALLING") return "падает";
  if (trend === "RISING") return "растет";
  if (trend === "STABLE") return "стабильно";
  return "нет данных";
}

function renderReports(data) {
  data = periodReport(data);
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Отчеты</h2>
        <p class="muted">Ежедневные, недельные, месячные, по подразделению, сотруднику, инциденту и пилотной эксплуатации.</p>
      </div>
      <span class="badge ${statusClass(data.severity)}">${escapeHtml(data.severity)}</span>
    </div>
    ${renderPeriodBanner(data)}
    ${renderReportTypeCards()}
    <div class="grid-2">
      <section class="card report-hero">
        <span class="badge ${statusClass(data.severity)}">${escapeHtml(data.severity)}</span>
        ${data.anonymized ? `<span class="badge status-warn">обезличено</span>` : ""}
        <h3>${ui(data.headline)}</h3>
        <p class="muted">${ui(data.period || "")} · обновлено ${escapeHtml(data.generated_at_utc || "")}</p>
      </section>
      <section class="card">
        <h3>Для руководителя</h3>
        <div class="list compact-list">${(data.executive_points || []).map(point => `
          <div class="row compact-row"><strong>Итог</strong><span class="muted">${ui(point)}</span><span></span></div>
        `).join("")}</div>
      </section>
    </div>
    <div class="report-actions">
      <button class="small-button" data-anonymize-report="true">Демо без имен</button>
      <button class="small-button" data-export-markdown="true">Текстовый отчет</button>
      <button class="small-button" data-print-report="true">Печать / PDF</button>
      <a class="small-button" href="${apiBase()}/reports" target="_blank" rel="noopener noreferrer">Скачать данные</a>
    </div>
    <h3 class="section-title">Ключевые показатели</h3>
    ${renderKpiCards(data.kpis)}
    ${renderAgentQuality(data.agent_quality, data.agent_quality_explain)}
    ${renderAgentQualityHistory(data.agent_quality_history, data.agent_quality_history_summary)}
    ${renderAgentQualityNodes(data.agent_quality_nodes, data.agent_quality_nodes_summary)}
    ${renderAgentCoverageSla(data.agent_coverage_sla)}
    ${renderBusinessRisk(data.business_risk)}
    ${renderBusinessRiskTimeline(data.business_risk_history, data.business_risk_history_summary)}
    ${renderRiskIncidentCandidates(data.risk_incident_candidates)}
    ${renderUebaRisk(data.ueba_risk)}
    ${renderWorkforceIndexExplanation(data.workforce_policy)}
    <h3 class="section-title">Срезы отчета</h3>
    ${renderReportSections(data.sections)}
    <section class="card markdown-card">
      <h3>Текст для отчета</h3>
      <pre>${ui(data.markdown || "")}</pre>
    </section>
  `;
}

function basename(value) {
  const text = String(value || "").trim();
  if (!text) return "-";
  return text.split(/[\\/]/).filter(Boolean).pop() || text;
}

function settingRows(report) {
  const policy = report?.workforce_policy || {};
  const risk = report?.ueba_risk || {};
  const roles = Array.isArray(policy.available_roles) ? policy.available_roles : [];
  const activeRole = roles.find(item => item.role === policy.default_role || item.role === policy.role) || roles[0] || {};
  const workdayHours = activeRole.planned_hours_per_day || (Number(policy.planned_seconds || 0) > 0 ? (Number(policy.planned_seconds) / 3600).toFixed(1) : 8);
  const period = periodConfig();
  return [
    ["Период расчета", period.label, period.key === "today" ? "оперативный дневной срез" : `${period.days} календарных дней`],
    ["Рабочий день", `${workdayHours} ч`, `роль: ${policy.role_label || activeRole.label || policy.default_role || "default"}`],
    ["Порог WARN", ">= 15 баллов", "любой ненормальный риск попадает в очередь проверки"],
    ["Порог FAIL", ">= 70 баллов", "высокий риск требует приоритетного разбора"],
    ["Источник правил", policy.configured ? basename(policy.path) : "встроенные правила", risk.policy_configured ? `UEBA policy: ${basename(risk.policy_path)}` : "UEBA policy: встроенная модель"],
    ["Дата последнего пересчета", report?.generated_at_utc || "-", `версия политики: ${risk.policy_version || "ueba-rule-v1"}`],
  ];
}

function renderSettings(report) {
  const rows = settingRows(periodReport(report || state.reports || {}));
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Настройки</h2>
        <p class="muted">Read-only параметры расчета и границы интерпретации данных.</p>
      </div>
      <span class="badge status-ok">только чтение</span>
    </div>
    <section class="card">
      <h3>Параметры расчета</h3>
      <table class="data-table settings-table">
        <thead><tr><th>Параметр</th><th>Значение</th><th>Комментарий</th></tr></thead>
        <tbody>${rows.map(([name, value, note]) => `
          <tr><td><strong>${ui(name)}</strong></td><td>${ui(value)}</td><td>${ui(note)}</td></tr>
        `).join("")}</tbody>
      </table>
    </section>
  `;
}

async function refresh() {
  if (!state.links) state.links = await loadJson("/links");
  const summary = await loadJson("/summary");
  state.readiness = {
    bundle: await loadJson("/readiness/bundle").catch(error => ({ ok: false, error: error.message })),
    verify: state.readiness?.verify || null
  };
  renderSummary(summary, state.readiness);
  const content = document.getElementById("content");
  if (state.tab === "operator") {
    const data = await loadJson("/operator");
    state.reports = await loadJson("/reports").catch(() => state.reports);
    content.innerHTML = renderOperator(data, state.reports);
    updateFilters(state.reports);
  }
  if (state.tab === "manager") {
    const data = await loadJson("/manager");
    const policyExplain = await loadJson("/workforce/policy/explain").catch(() => null);
    content.innerHTML = renderManager(data, policyExplain);
  }
  if (state.tab === "departments") {
    const data = await loadJson("/reports");
    state.reports = data;
    content.innerHTML = renderDepartments(data);
    updateFilters(data);
  }
  if (state.tab === "employees") {
    const data = await loadJson("/manager");
    const policyExplain = await loadJson("/workforce/policy/explain").catch(() => null);
    content.innerHTML = renderEmployees(data, policyExplain);
  }
  if (state.tab === "owner") {
    const data = await loadJson("/owner");
    content.innerHTML = renderOwner(data);
  }
  if (state.tab === "incidents") {
    const data = await loadJson("/incidents");
    const evidence = await loadJson("/dlp/evidence").catch(error => ({ ok: false, error: error.message, items: [] }));
    const reports = await loadJson("/reports").catch(() => state.reports || {});
    const cases = await loadJson("/cases").catch(error => ({ ok: false, error: error.message, cases: [] }));
    state.reports = reports;
    content.innerHTML = renderIncidents({ incidents: data, evidence, reports, cases });
  }
  if (state.tab === "perimeter") {
    const data = await loadJson("/owner");
    const reports = await loadJson("/reports").catch(() => state.reports);
    state.reports = reports;
    content.innerHTML = renderPerimeter(data, reports);
  }
  if (state.tab === "reports") {
    const data = await loadJson("/reports");
    state.reports = data;
    content.innerHTML = renderReports(data);
    updateFilters(data);
  }
  if (state.tab === "settings") {
    const data = await loadJson("/reports").catch(() => state.reports || {});
    state.reports = data;
    content.innerHTML = renderSettings(data);
    updateFilters(data);
  }
}

function setTab(tab) {
  state.tab = tab;
  applySecurityMode(tab);
  document.querySelectorAll(".tab").forEach(btn => {
    btn.classList.toggle("is-active", btn.dataset.tab === tab);
  });
  document.getElementById("content").innerHTML = `<p class="muted">Загрузка...</p>`;
  refresh().catch(showError);
}

function applySecurityMode(tab) {
  document.body.classList.toggle("security-mode", tab === "owner" || tab === "incidents" || tab === "perimeter");
}

function showError(error) {
  document.getElementById("content").innerHTML = `<pre>${escapeHtml(error.stack || error.message || error)}</pre>`;
}

document.querySelectorAll(".tab").forEach(btn => {
  btn.addEventListener("click", () => setTab(btn.dataset.tab));
});

document.getElementById("periodFilter")?.addEventListener("change", event => {
  state.period = periodFromSelectValue(event.target.value);
  document.getElementById("content").innerHTML = `<p class="muted">Обновление периода...</p>`;
  refresh().catch(showError);
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-open-reports]");
  if (!button) return;
  setTab("reports");
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-open-investigation]");
  if (!button) return;
  state.investigationRequested = true;
  setTab("incidents");
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-incident-action]");
  if (!button) return;
  incidentAction(button).catch(showError);
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-review-status]");
  if (!button) return;
  candidateReviewAction(button).catch(showError);
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-create-case]");
  if (!button) return;
  createCaseAction(button).catch(showError);
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-case-status]");
  if (!button) return;
  caseStatusAction(button).catch(showError);
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-print-report]");
  if (!button) return;
  window.print();
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-anonymize-report]");
  if (!button) return;
  anonymizeReport(button).catch(showError);
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-export-markdown]");
  if (!button) return;
  exportMarkdown().catch(showError);
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-readiness-verify]");
  if (!button) return;
  verifyReadinessBundle(button).catch(showError);
});

async function verifyReadinessBundle(button) {
  button.disabled = true;
  button.textContent = "Проверка...";
  const verify = await loadJson("/readiness/verify");
  state.readiness = {
    ...(state.readiness || {}),
    verify
  };
  const status = document.getElementById("readinessVerifyStatus");
  if (status) {
    status.textContent = `Проверено: контрольная сумма ${verify.checksum_verified ? "OK" : "FAIL"} · подпись ${verify.signature_verified ? "OK" : "FAIL"}`;
  }
  button.disabled = false;
  button.textContent = "Проверить пакет";
}

async function anonymizeReport(button) {
  button.disabled = true;
  const data = await loadJson("/reports?anonymize=1");
  state.reports = data;
  document.getElementById("content").innerHTML = renderReports(data);
}

async function exportMarkdown() {
  const data = state.reports || await loadJson("/reports");
  const markdown = displayText(periodReport(data)?.markdown || data.markdown || "");
  const blob = new Blob([markdown], { type: "text/markdown;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "awatch-rus-report.md";
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

function updateFilters(report) {
  updateSelect("departmentFilter", (report?.workforce?.department_comparison || []).map(item => item.label));
  updateSelect("ownerFilter", (report?.workforce?.owner_comparison || []).map(item => item.label));
}

function updateSelect(id, values) {
  const select = document.getElementById(id);
  if (!select || select.dataset.loaded === "true") return;
  const unique = [...new Set((values || []).filter(Boolean))].slice(0, 50);
  select.innerHTML = `<option>Все</option>` + unique.map(item => `<option>${escapeHtml(item)}</option>`).join("");
  select.dataset.loaded = "true";
}

async function incidentAction(button) {
  const id = button.dataset.incidentId;
  const action = button.dataset.incidentAction;
  const payload = { id, action };
  if (action === "ack") {
    const comment = window.prompt("Комментарий к взятию в работу", "");
    if (comment === null) return;
    payload.comment = comment;
  }
  if (action === "assign") {
    const assignedTo = window.prompt("Кому назначить", "");
    if (assignedTo === null || assignedTo.trim() === "") return;
    payload.assigned_to = assignedTo;
    const comment = window.prompt("Комментарий", "");
    if (comment === null) return;
    payload.comment = comment;
  }
  button.disabled = true;
  await postJson("/incidents/action", payload);
  await refresh();
}

async function candidateReviewAction(button) {
  const candidateId = button.dataset.candidateId;
  const status = button.dataset.reviewStatus;
  const reviewer = window.prompt("Проверяющий", "");
  if (reviewer === null || reviewer.trim() === "") return;
  const comment = window.prompt("Комментарий к проверке", "");
  if (comment === null) return;
  button.disabled = true;
  await postJson("/incident-review", {
    candidate_id: candidateId,
    status,
    reviewer,
    comment,
  });
  await refresh();
}

async function createCaseAction(button) {
  const candidateId = button.dataset.candidateId;
  const title = window.prompt("Название дела", `Дело по кандидату ${candidateId}`);
  if (title === null || title.trim() === "") return;
  const owner = window.prompt("Ответственный по делу", "");
  if (owner === null) return;
  const summary = window.prompt("Краткое резюме дела", "");
  if (summary === null) return;
  button.disabled = true;
  await postJson("/cases", {
    candidate_id: candidateId,
    title,
    owner,
    summary,
  });
  state.tab = "incidents";
  await refresh();
}

async function caseStatusAction(button) {
  const caseId = button.dataset.caseId;
  const status = button.dataset.caseStatus;
  const decision = window.prompt("Решение или комментарий по делу", "");
  if (decision === null) return;
  button.disabled = true;
  await postJson(`/cases/${encodeURIComponent(caseId)}/status`, {
    status,
    decision,
  });
  await refresh();
}

applySecurityMode(state.tab);
refresh().catch(showError);
setInterval(() => refresh().catch(showError), 60000);
