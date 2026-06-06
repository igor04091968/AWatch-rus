const state = {
  tab: "operator",
  viewMode: initialViewMode(),
  period: "today",
  links: null,
  readiness: null,
  operatorData: null,
  reports: null,
  cases: null,
  pendingScrollSelector: null,
  load: {
    status: "LOADING",
    stage: "Инициализация портала",
    progress: 0,
    lastUpdatedAt: null,
    lastError: null,
  },
};

const VIEW_MODES = {
  executive: {
    label: "Руководитель",
    title: "Роль руководителя",
    heading: "Представление руководителя",
    description: "Главный вывод, сводка руководителя, риски подразделений и карта рисков.",
    stage: "Формирование главного вывода",
  },
  manager: {
    label: "Менеджер",
    title: "Роль менеджера",
    heading: "Представление менеджера",
    description: "Workforce, сравнение подразделений, ответственные, тренды и отчет.",
    stage: "Расчет Workforce и подразделений",
  },
  security: {
    label: "Безопасность",
    title: "Роль безопасности",
    heading: "Представление безопасности",
    description: "Очередь проверки, расследования, аудит решений и пакеты расследований.",
    stage: "Подготовка разделов проверки и расследований",
  },
  forensics: {
    label: "Расследования",
    title: "Роль расследований",
    heading: "Представление расследований",
    description: "Карточки расследований, timeline, связка user / host / app / network event и экспорт.",
    stage: "Подготовка timeline расследований",
  },
  admin: {
    label: "Администратор",
    title: "Роль администратора",
    heading: "Представление администратора",
    description: "Настройки, качество данных, источники и эксплуатационные ошибки.",
    stage: "Проверка настроек и источников",
  },
  operations: {
    label: "Эксплуатация",
    title: "Роль эксплуатации",
    heading: "Представление эксплуатации",
    description: "Полнота данных, качество сбора, ошибки и телеметрия рабочих мест.",
    stage: "Подготовка разделов эксплуатации",
  },
};

function initialViewMode() {
  try {
    const stored = window.localStorage?.getItem("detmir.portal.viewMode");
    return ["executive", "manager", "security", "forensics", "admin", "operations"].includes(stored) ? stored : "executive";
  } catch {
    return "executive";
  }
}

function apiBase() {
  const path = window.location.pathname;
  return path.startsWith("/portal") ? "/portal/api" : "/api";
}

function apiRole() {
  if (state.tab === "employees" || state.tab === "departments") return "manager";
  if (state.tab === "owner" || state.tab === "perimeter") return "security";
  if (state.tab === "incidents") return "forensics";
  if (state.tab === "settings") return "admin";
  const mode = currentViewMode();
  if (mode === "operations") return "admin";
  return ["executive", "manager", "security", "forensics", "admin"].includes(mode) ? mode : "executive";
}

function roleHeaders() {
  return { "X-AWatch-Role": apiRole() };
}

function withRole(path, role = apiRole()) {
  const separator = path.includes("?") ? "&" : "?";
  return `${path}${separator}role=${encodeURIComponent(role)}`;
}

async function loadJson(path) {
  const response = await fetch(`${apiBase()}${path}`, { cache: "no-store", headers: roleHeaders() });
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
  return response.json();
}

async function postJson(path, payload) {
  const response = await fetch(`${apiBase()}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...roleHeaders() },
    body: JSON.stringify(payload)
  });
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`);
  return response.json();
}

function statusClass(status) {
  const s = String(status || "UNKNOWN").toLowerCase();
  if (s === "ok" || s === "ready" || s === "true" || s === "normal" || s === "low" || s === "false_positive" || s === "resolved") return "status-ok";
  if (s === "loading" || s === "warn" || s === "warning" || s === "attention" || s === "fallback" || s === "stale" || s === "medium" || s === "in_review" || s === "postponed" || s === "open" || s === "in_progress") return "status-warn";
  if (s === "degraded" || s === "high" || s === "high_risk" || s === "confirmed" || s === "rejected" || s === "archived") return "status-degraded";
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
    .replaceAll("Risk Narrative", "Главный вывод")
    .replaceAll("Risk Heatmap", "Карта рисков")
    .replaceAll("Business Risk", "Риски подразделений")
    .replaceAll("Security Events", "События безопасности")
    .replaceAll("Security Correlation", "Связь рисков и активности")
    .replaceAll("Incident Candidates", "Требует проверки")
    .replaceAll("Incident Candidate", "Требует проверки")
    .replaceAll("Critical Candidates", "Срочно проверить")
    .replaceAll("Open Cases", "Активные расследования")
    .replaceAll("Agent Coverage", "Полнота данных")
    .replaceAll("Coverage SLA", "Полнота данных")
    .replaceAll("Agent Quality", "Качество данных")
    .replaceAll("Trust Score", "Уровень доверия")
    .replaceAll("Trust KPI", "Достоверность показателей")
    .replaceAll("Correlation Score", "Уровень взаимосвязи")
    .replaceAll("Forensics Readiness", "Готовность к расследованию")
    .replaceAll("Executive Dashboard", "Сводка руководителя")
    .replaceAll("Executive Summary", "Краткий вывод")
    .replaceAll("Cases", "Расследования")
    .replaceAll("Workforce", "Работа сотрудников")
    .replaceAll("workforce", "работа сотрудников")
    .replaceAll("Security", "Безопасность")
    .replaceAll("Forensics", "Материалы расследования")
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
    .replaceAll("SECURITY_EVENTS_BACKEND", "настройка источника событий")
    .replaceAll("CLICKHOUSE_*", "параметры источника событий")
    .replaceAll("ClickHouse", "источник событий")
    .replaceAll("local_fallback", "резервный локальный источник")
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
    .replaceAll("SLA", "полнота данных")
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
    .replace(/\bsource\b/g, "источник")
    .replace(/\bunknown\b/g, "неизвестно")
    .replace(/\baccepted\b/g, "принято")
    .replace(/\bopen_cases\b/g, "активные расследования")
    .replace(/\bcritical_candidates\b/g, "срочно проверить")
    .replaceAll("daily", "день")
    .replaceAll("weekly", "неделя")
    .replaceAll("monthly", "месяц")
    .replace(/\bCRITICAL\b/g, "критично")
    .replace(/\bWARNING\b/g, "требует внимания")
    .replace(/\bUNKNOWN\b/g, "нет данных")
    .replace(/\bDISABLED\b/g, "отключено")
    .replace(/\bREADY\b/g, "готово")
    .replace(/\bOPEN\b/g, "открыто")
    .replace(/\bINFO\b/g, "информация")
    .replace(/\bHIGH\b/g, "высокий риск")
    .replace(/\bMEDIUM\b/g, "средний риск")
    .replace(/\bLOW\b/g, "низкий риск")
    .replace(/\bFAIL\b/g, "критично")
    .replace(/\bWARN\b/g, "требует внимания")
    .replace(/\bNO\b/g, "нет")
    .replace(/\bOK\b/g, "в норме");
}

function ui(value) {
  return escapeHtml(displayText(value));
}

function tooltip(value) {
  return `title="${ui(value)}"`;
}

function currentViewMode() {
  return VIEW_MODES[state.viewMode] ? state.viewMode : "executive";
}

function currentViewMeta() {
  return VIEW_MODES[currentViewMode()];
}

function updateViewModeButtons() {
  document.querySelectorAll("[data-view-mode]").forEach(button => {
    button.classList.toggle("is-active", button.dataset.viewMode === currentViewMode());
  });
}

function setViewMode(mode) {
  if (!VIEW_MODES[mode]) return;
  state.viewMode = mode;
  try {
    window.localStorage?.setItem("detmir.portal.viewMode", mode);
  } catch {
    // View mode is a UI preference only; ignore storage failures.
  }
  updateViewModeButtons();
  if (state.tab !== "operator") {
    setTab("operator");
    return;
  }
  if (state.operatorData && state.reports) {
    const content = document.getElementById("content");
    if (content) {
      content.innerHTML = renderOperator(state.operatorData, state.reports, { cases: state.cases });
    }
    setLoadStatus("READY", "Данные готовы", 100);
    consumePendingScroll();
    if (mode === "security" && !state.cases) {
      loadJson("/cases")
        .then(cases => {
          state.cases = cases;
          if (currentViewMode() === "security" && state.tab === "operator" && content) {
            content.innerHTML = renderOperator(state.operatorData, state.reports, { cases: state.cases });
          }
        })
        .catch(() => {
          state.cases = { ok: false, cases: [] };
        });
    }
    return;
  }
  refresh({ stage: VIEW_MODES[mode].stage });
}

function renderSummary(summary, readiness) {
  const global = document.getElementById("globalStatus");
  global.className = `status-pill ${statusClass(summary.severity)}`;
  global.textContent = `Сбор данных ${summary.operator_ok ? "в норме" : "нет данных"} · ${displayText(summary.severity)}`;
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
        <div><span class="muted">Подпись</span><strong class="${signatureOk ? "text-ok" : "text-fail"}">${signatureOk ? "подтверждена" : "не подтверждена"}</strong></div>
        <div><span class="muted">Контрольная сумма</span><strong class="${checksumOk ? "text-ok" : "text-fail"}">${checksumOk ? "подтверждена" : "не подтверждена"}</strong></div>
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

function setLoadStatus(status, stage, progress, options = {}) {
  state.load.status = status;
  state.load.stage = stage || state.load.stage || "Обновление данных";
  state.load.progress = Math.max(0, Math.min(100, Number(progress) || 0));
  if (status === "READY" || status === "EMPTY") {
    state.load.lastUpdatedAt = new Date().toISOString();
    state.load.lastError = null;
  }
  if (status === "ERROR" || status === "STALE") {
    state.load.lastError = options.error || state.load.lastError || null;
  }
  renderLoadStatus();
}

function loadStatusLabel(status) {
  return {
    LOADING: "Загрузка данных",
    READY: "Данные готовы",
    EMPTY: "Данные отсутствуют",
    STALE: "Данные устарели",
    ERROR: "Ошибка получения данных",
    UNKNOWN: "Состояние неизвестно",
  }[status] || "Состояние неизвестно";
}

function renderLoadStatus() {
  const box = document.getElementById("loadingStatus");
  if (!box) return;
  const status = state.load.status || "UNKNOWN";
  const stage = state.load.stage || "Состояние неизвестно";
  const progress = Math.max(0, Math.min(100, Number(state.load.progress) || 0));
  const stateText = document.getElementById("loadingStateText");
  const stageText = document.getElementById("loadingStageText");
  const updatedText = document.getElementById("loadingUpdatedText");
  const bar = document.getElementById("loadingProgressBar");
  box.className = `loading-status loading-status-${status.toLowerCase()}`;
  box.dataset.loadStatus = status;
  if (stateText) stateText.textContent = loadStatusLabel(status);
  if (stageText) stageText.textContent = stage;
  if (updatedText) updatedText.textContent = `последнее обновление: ${formatLoadTime(state.load.lastUpdatedAt)}`;
  if (bar) bar.style.width = `${progress}%`;
}

function formatLoadTime(value) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function renderLoadingContent(stage = "Данные загружаются") {
  return `
    <div class="loading-shell" data-loading-state="LOADING">
      <div class="loading-message">
        <strong>Загрузка данных</strong>
        <p class="muted small">${ui(stage)}</p>
      </div>
      <section class="skeleton-grid" aria-label="Загрузка основных показателей">
        ${Array.from({ length: 6 }).map(() => `
          <article class="skeleton-card">
            <span class="skeleton-line short"></span>
            <span class="skeleton-line medium"></span>
            <span class="skeleton-line long"></span>
          </article>
        `).join("")}
      </section>
      <section class="grid-2">
        <article class="skeleton-panel">
          <span class="skeleton-line medium"></span>
          <span class="skeleton-line long"></span>
          <span class="skeleton-line long"></span>
          <span class="skeleton-line medium"></span>
        </article>
        <article class="skeleton-panel">
          <span class="skeleton-line short"></span>
          <span class="skeleton-line long"></span>
          <span class="skeleton-line medium"></span>
          <span class="skeleton-line long"></span>
        </article>
      </section>
    </div>
  `;
}

function renderEmptyState(stage = "Данных за выбранный период нет") {
  return `
    <section class="empty-state" data-loading-state="EMPTY">
      <span class="badge status-unknown">Данные отсутствуют</span>
      <h3>Данных пока нет</h3>
      <p class="muted">${ui(stage)}</p>
    </section>
  `;
}

function renderErrorState(error) {
  const message = error?.message || error?.stack || String(error || "Неизвестная ошибка");
  return `
    <section class="error-state" data-loading-state="ERROR">
      <span class="badge status-fail">Ошибка получения данных</span>
      <h3>Ошибка получения данных</h3>
      <p class="muted">Портал не получил актуальные данные. Подробности ниже.</p>
      <pre>${escapeHtml(message)}</pre>
    </section>
  `;
}

function staleBanner(error) {
  const message = error?.message || String(error || "ошибка обновления");
  return `
    <div class="stale-banner" data-loading-state="STALE">
      <span class="badge status-warn">Данные устарели</span>
      <strong>Показаны ранее загруженные данные.</strong>
      <p class="muted small">Последнее обновление: ${escapeHtml(formatLoadTime(state.load.lastUpdatedAt))}. Новая загрузка не завершилась: ${escapeHtml(message)}</p>
    </div>
  `;
}

function hasTabData(tab, payload) {
  if (!payload) return false;
  if (tab === "operator") {
    const report = payload.report || {};
    return (Array.isArray(report.kpis) && report.kpis.length > 0)
      || (Array.isArray(report.executive_points) && report.executive_points.length > 0)
      || (Array.isArray(payload.data?.workforce) && payload.data.workforce.length > 0);
  }
  if (tab === "employees") {
    return Array.isArray(payload.data?.workforce) && payload.data.workforce.length > 0;
  }
  if (tab === "departments") {
    return Array.isArray(payload.data?.workforce?.department_comparison)
      && payload.data.workforce.department_comparison.length > 0;
  }
  if (tab === "owner" || tab === "perimeter") {
    return Boolean(payload.data && Object.keys(payload.data).length > 0);
  }
  if (tab === "incidents") {
    return (Array.isArray(payload.data?.incidents) && payload.data.incidents.length > 0)
      || (Array.isArray(payload.data?.reports?.risk_incident_candidates) && payload.data.reports.risk_incident_candidates.length > 0)
      || (Array.isArray(payload.data?.cases?.cases) && payload.data.cases.cases.length > 0);
  }
  if (tab === "reports" || tab === "settings") return Boolean(payload.data && Object.keys(payload.data).length > 0);
  return true;
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

function renderExecutiveDashboard(report) {
  const dashboard = report?.executive_dashboard;
  if (!dashboard) return "";
  const highRisk = Array.isArray(dashboard.high_risk_departments) ? dashboard.high_risk_departments : [];
  const candidates = Array.isArray(dashboard.critical_candidates) ? dashboard.critical_candidates : [];
  const summary = dashboard.summary || {};
  return `
    <section class="card executive-dashboard-card">
      <div class="section-head">
        <div>
          <h3 ${tooltip("Главная сводка для руководителя: что происходит в организации прямо сейчас.")}>Сводка руководителя</h3>
          <p class="muted">Что происходит в организации прямо сейчас: достоверность показателей, полнота данных, риски, активные расследования и готовность к расследованию.</p>
        </div>
        <span class="badge ${statusClass(executiveDashboardStatus(dashboard))}">${ui(dashboard.forensics_readiness || "UNKNOWN")}</span>
      </div>
      <div class="quality-grid">
        <div ${tooltip("Можно ли доверять рассчитанным показателям активности.")}><span class="muted">Достоверность показателей</span><strong>${ui(optionalPercent(dashboard.trust_kpi_score))}</strong></div>
        <div ${tooltip("Какая часть рабочих мест прислала свежие подтвержденные данные.")}><span class="muted">Полнота данных</span><strong>${ui(optionalPercent(dashboard.agent_coverage_pct))}</strong></div>
        <div><span class="muted">Высокий риск</span><strong>${ui(highRisk.length)}</strong></div>
        <div ${tooltip("Сколько записей нужно срочно проверить вручную.")}><span class="muted">Срочно проверить</span><strong>${ui(candidates.length)}</strong></div>
        <div ${tooltip("Сколько расследований сейчас в работе.")}><span class="muted">Активные расследования</span><strong>${ui(dashboard.open_cases ?? 0)}</strong></div>
        <div><span class="muted">Закрыто за 30 дней</span><strong>${ui(dashboard.resolved_cases_30d ?? 0)}</strong></div>
        <div ${tooltip("Агрегированная сводка событий безопасности за последние 24 часа, без сырых логов.")}><span class="muted">События безопасности</span><strong>${ui(dashboard.security_events_24h ?? "откл.")}</strong></div>
      </div>
      <div class="list compact-list executive-summary-list">
        <div class="row compact-row"><strong>Главный риск</strong><span class="muted">${ui(summary.main_risk || "нет данных")}</span><span></span></div>
        <div class="row compact-row"><strong>Главное улучшение</strong><span class="muted">${ui(summary.main_improvement || "нет данных")}</span><span></span></div>
        <div class="row compact-row"><strong>Пробел в данных</strong><span class="muted">${ui(summary.main_data_gap || "нет данных")}</span><span></span></div>
      </div>
    </section>
  `;
}

function renderRiskNarrative(report) {
  const dashboard = report?.executive_dashboard;
  const summary = dashboard?.summary || {};
  const narrativeStatus = summary.risk_narrative_status || executiveDashboardStatus(dashboard || {});
  return `
    <section class="card risk-narrative-card">
      <div class="section-head">
        <div>
          <h3 ${tooltip("Главный управленческий вывод: что случилось, почему это риск и чем это подтверждается.")}>Главный вывод</h3>
          <p class="muted">Главный управленческий вывод: что происходит, почему это риск и какие слои это подтверждают.</p>
        </div>
        <span class="badge ${statusClass(narrativeStatus)}">${ui(narrativeStatus)}</span>
      </div>
      <div class="list compact-list executive-summary-list">
        <div class="row compact-row"><strong>Главная причина риска</strong><span class="muted">${ui(summary.main_risk_cause || "связанный риск не выражен")}</span><span class="badge ${statusClass(narrativeStatus)}">${ui(narrativeStatus)}</span></div>
        <div class="row compact-row"><strong>Подтверждающие слои</strong><span class="muted">Достоверность показателей · Полнота данных · Риски подразделений · Карта рисков · Связь рисков и активности · Требует проверки · Расследования</span><span></span></div>
      </div>
    </section>
  `;
}

function optionalPercent(value) {
  const number = Number(value);
  return Number.isFinite(number) ? `${Math.round(number)}%` : "нет данных";
}

function executiveDashboardStatus(dashboard) {
  if ((dashboard.critical_candidates || []).length > 0) return "WARN";
  if ((dashboard.high_risk_departments || []).length > 0) return "WARN";
  if (Number(dashboard.agent_coverage_pct) < 75) return "FAIL";
  return "OK";
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
      risk: status === "FAIL" ? "критично — требуется действие" : status === "WARN" ? "требует внимания" : "низкий риск — все нормально",
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
    metricCard("Требуют внимания", rows.filter(row => row.status === "WARN").length, rows.some(row => row.status === "WARN") ? "WARN" : "OK", "подразделения"),
    metricCard("Критичные подразделения", rows.filter(row => row.status === "FAIL").length, rows.some(row => row.status === "FAIL") ? "FAIL" : "OK", "требуется действие"),
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
  const emptyRows = `<div class="list compact-list"><div class="row compact-row"><strong>Нет данных</strong><span class="muted">Подразделения пока не рассчитаны.</span><span class="badge status-unknown">нет данных</span></div></div>`;
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
              <td><span class="badge status-unknown">нет данных</span></td>
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
          <p class="muted">Срез только для чтения, чтобы поручить разбор ответственному.</p>
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

function renderOperatorDetailBands(report) {
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
      <span class="badge ${statusClass(source?.status || source?.ok)}">${ui(source?.status || (source?.ok ? "OK" : "FAIL"))}</span>
    </div>
  `).join("")}</div>`;
}

function renderOperator(data, report, extras = {}) {
  report = periodReport(report);
  const incidents = Array.isArray(data.incidents) ? data.incidents : [];
  const meta = currentViewMeta();
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">${ui(meta.heading)}</h2>
        <p class="muted">${ui(meta.description)}</p>
      </div>
      <span class="badge ${statusClass(report?.severity)}">${ui(meta.label)}</span>
    </div>
    ${renderPeriodBanner(report)}
    ${renderRoleViewSummary(meta)}
    ${renderOperatorRoleContent(currentViewMode(), data, report, { ...extras, incidents })}
  `;
}

function renderRoleViewSummary(meta) {
  return `
    <section class="card role-view-card">
      <div class="section-head">
        <div>
          <h3>${ui(meta.heading)}</h3>
          <p class="muted">${ui(meta.description)}</p>
        </div>
        <span class="badge status-ok">${ui(meta.label)}</span>
      </div>
    </section>
  `;
}

function renderOperatorRoleContent(mode, data, report, extras = {}) {
  if (mode === "manager") return renderManagerView(report);
  if (mode === "security") return renderSecurityView(data, report, extras);
  if (mode === "forensics") return renderForensicsView(data, report, extras);
  if (mode === "admin") return renderOperationsView(data, report);
  if (mode === "operations") return renderOperationsView(data, report);
  return renderExecutiveView(report, extras.incidents || []);
}

function renderExecutiveView(report, incidents) {
  return `
    ${renderRiskNarrative(report)}
    ${renderExecutiveDashboard(report)}
    ${renderSecurityEventsSummary(report?.security_events_summary, { compact: true })}
    ${renderBusinessRisk(report?.business_risk)}
    ${renderRiskHeatmap(report?.risk_heatmap)}
    ${renderOverviewAnalytics(report)}
  `;
}

function renderSecurityView(data, report, extras = {}) {
  const cases = Array.isArray(extras.cases?.cases) ? extras.cases.cases : [];
  return `
    ${renderSecurityEventsSummary(report?.security_events_summary)}
    ${renderRiskIncidentCandidates(report?.risk_incident_candidates)}
    ${renderSecurityCorrelation(report?.security_correlation)}
    ${renderCases(cases)}
    ${renderIncidentReviewAuditSummary(report)}
    ${renderInvestigationPacks(report?.risk_incident_candidates)}
    <section class="dashboard-band security-band">
      <div class="band-head"><h3>Расследования</h3><span class="muted">ручная проверка, решения и материалы</span></div>
      ${renderDlpIncidentsList(extras.incidents || data.incidents || [])}
    </section>
  `;
}

function renderManagerView(report) {
  return `
    ${renderExecutiveDashboard(report)}
    ${renderDepartmentRanking(report)}
    ${renderDepartmentHeatMap(report)}
    ${renderOverviewAnalytics(report)}
    <section class="dashboard-band">
      <div class="band-head"><h3>Markdown-отчет</h3><span class="muted">экспорт управленческого среза Workforce</span></div>
      <pre class="markdown-preview">${ui((report?.markdown || "").slice(0, 2000))}</pre>
    </section>
  `;
}

function renderForensicsView(data, report, extras = {}) {
  const cases = Array.isArray(extras.cases?.cases) ? extras.cases.cases : [];
  return `
    ${renderRiskIncidentCandidates(report?.risk_incident_candidates)}
    ${renderCases(cases)}
    ${renderInvestigationTimeline(report)}
    ${renderInvestigationPacks(report?.risk_incident_candidates)}
    ${renderIncidentReviewAuditSummary(report)}
  `;
}

function renderOperationsView(data, report) {
  return `
    ${renderAgentCoverageSla(report?.agent_coverage_sla)}
    ${renderAgentQuality(report?.agent_quality, report?.agent_quality_explain)}
    ${renderSecurityEventsSummary(report?.security_events_summary, { operations: true })}
    ${renderOperationsErrors(data, report)}
    <section class="dashboard-band technical-band">
      <div class="band-head"><h3>Телеметрия</h3><span class="muted">свежесть, стабильность и источники данных</span></div>
      ${renderAgentQualityHistory(report?.agent_quality_history, report?.agent_quality_history_summary)}
      ${renderAgentQualityNodes(report?.agent_quality_nodes, report?.agent_quality_nodes_summary)}
      ${renderSourceList(data)}
    </section>
  `;
}

function renderOperationsErrors(data, report) {
  const rows = [];
  const collectorError = report?.agent_quality?.collector_error;
  if (collectorError) {
    rows.push(["Ошибка коллектора", collectorError, report?.agent_quality?.quality_status || "DEGRADED"]);
  }
  if (report?.security_events_summary?.fallback_used) {
    rows.push([
      "События безопасности",
      report.security_events_summary.error || "Источник событий недоступен",
      "WARN",
    ]);
  }
  const problemNodes = Array.isArray(report?.agent_coverage_sla?.problem_nodes)
    ? report.agent_coverage_sla.problem_nodes
    : [];
  for (const node of problemNodes.slice(0, 8)) {
    rows.push([
      node.hostname || "unknown",
      `${node.status || "UNKNOWN"} · ${node.recommendation || "Проверить рабочее место"}`,
      node.status || "WARNING",
    ]);
  }
  for (const [name, source] of Object.entries(data || {})) {
    if (!source || typeof source !== "object") continue;
    const status = source.status || (source.ok === false ? "FAIL" : "");
    if (!status || ["OK", "READY", "INFO", "DISABLED"].includes(String(status).toUpperCase())) continue;
    rows.push([label(name), source.summary || source.error || "требуется проверка", status]);
  }
  return `
    <section class="card operations-errors-card">
      <div class="section-head">
        <div>
          <h3>Ошибки</h3>
          <p class="muted">Что мешает достоверному сбору и эксплуатационной готовности.</p>
        </div>
        <span class="badge ${statusClass(rows.length ? "WARNING" : "OK")}">${rows.length}</span>
      </div>
      <div class="list compact-list">${rows.length ? rows.map(([name, text, status]) => `
        <div class="row compact-row">
          <strong>${ui(name)}</strong>
          <span class="muted">${ui(text)}</span>
          <span class="badge ${statusClass(status)}">${ui(status)}</span>
        </div>
      `).join("") : `
        <div class="row compact-row">
          <strong>Критичных ошибок нет</strong>
          <span class="muted">Портал не видит ошибок коллектора или проблемных рабочих мест в текущем срезе.</span>
          <span class="badge status-ok">в норме</span>
        </div>
      `}</div>
    </section>
  `;
}

function renderIncidentReviewAuditSummary(report) {
  const summary = report?.incident_review_audit_summary || {};
  const rows = [
    ["Всего изменений", summary.total_changes ?? 0],
    ["Подтверждено", summary.confirmed_count ?? 0],
    ["Ложные срабатывания", summary.false_positive_count ?? 0],
    ["Отложено", summary.postponed_count ?? 0],
    ["Последнее изменение", summary.last_change_utc || "-"],
  ];
  return `
    <section class="card incident-audit-card">
      <div class="section-head">
        <div>
          <h3>Аудит</h3>
          <p class="muted">Кто и когда менял статус записей, требующих проверки.</p>
        </div>
        <span class="badge ${statusClass(Number(summary.total_changes || 0) > 0 ? "INFO" : "UNKNOWN")}">${ui(summary.total_changes ?? 0)}</span>
      </div>
      <div class="quality-grid">${rows.map(([name, value]) => `
        <div><span class="muted">${ui(name)}</span><strong>${ui(value)}</strong></div>
      `).join("")}</div>
    </section>
  `;
}

function renderInvestigationPacks(candidates) {
  const rows = Array.isArray(candidates) ? candidates.slice(0, 10) : [];
  return `
    <section class="card investigation-packs-card">
      <div class="section-head">
        <div>
          <h3>Материалы расследования</h3>
          <p class="muted">Выгружаемые пакеты материалов по каждой записи для ручной проверки.</p>
        </div>
        <span class="badge ${statusClass(rows.length ? "INFO" : "UNKNOWN")}">${rows.length}</span>
      </div>
      <div class="list compact-list">${rows.length ? rows.map(item => `
        <div class="row compact-row">
          <strong>${ui(item.department || "Без подразделения")}</strong>
          <span class="muted">${ui(item.reason || "требуется проверка")} · ${ui(item.hostname || "-")}</span>
          <a class="small-button" href="${apiBase()}${withRole(`/investigation-pack/${encodeURIComponent(item.id || "")}?format=markdown`, "forensics")}" download>Скачать</a>
        </div>
      `).join("") : `
        <div class="row compact-row">
          <strong>Пакетов нет</strong>
          <span class="muted">Нет записей, требующих выгрузки материалов.</span>
          <span class="badge status-ok">в норме</span>
        </div>
      `}</div>
    </section>
  `;
}

function renderInvestigationTimeline(report) {
  const investigations = Array.isArray(report?.forensics?.investigations) ? report.forensics.investigations : [];
  const timelines = investigations.flatMap(item => Array.isArray(item.timeline)
    ? item.timeline.map(event => ({ ...event, investigation_id: item.investigation_id }))
    : []);
  return `
    <section class="card investigation-timeline-card">
      <div class="section-head">
        <div>
          <h3>Timeline событий</h3>
          <p class="muted">Связка user / host / app / network event для ручного расследования.</p>
        </div>
        <span class="badge ${statusClass(timelines.length ? "INFO" : "UNKNOWN")}">${timelines.length}</span>
      </div>
      <div class="list compact-list">${timelines.length ? timelines.slice(0, 20).map(event => `
        <div class="row compact-row">
          <strong>${ui(event.timestamp || "-")}</strong>
          <span class="muted">${ui(event.kind || "event")} · ${ui(event.entity || "-")} · ${ui(event.summary || "")}</span>
          <span class="badge status-ok">${ui(event.source || "portal")}</span>
        </div>
      `).join("") : `
        <div class="row compact-row">
          <strong>Timeline пуст</strong>
          <span class="muted">Нет кандидатов для расследования в текущем срезе.</span>
          <span class="badge status-ok">в норме</span>
        </div>
      `}</div>
    </section>
  `;
}

function renderOverviewAnalytics(report) {
  const workforce = findSection(report, "Работа");
  const insights = findSection(report, "Выводы по активности") || findSection(report, "Выводы Workforce");
  const security = findSection(report, "ИБ");
  const actions = findSection(report, "Действия");
  return `
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
      ${renderSectionItems(actions, "Рекомендаций нет.")}
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
      <span class="badge ${item.screenshot_available ? "status-ok" : "status-warn"}">${item.screenshot_available ? "скриншот" : "метаданные"}</span>
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
    risk_id: risk.code || risk.label || "risk-check",
    department,
    owner,
    activity_index: risky?.activityText || "нет данных",
    deviation: risky?.deviation || "нет данных",
    status,
    summary,
    why_it_is_risk: risky?.reason || risk.value || "риск может указывать на просадку активности, отклонение от нормы или событие безопасности",
    what_to_check: risky?.check || risk.recommendation || "проверить первичные события ActivityWatch, удаленный доступ, активность в 1С, процессы и сетевые сигналы",
    recommended_actions: [
      "назначить ответственного за ручную проверку",
      "сопоставить риск с журналами активности и бизнес-задачей",
      "зафиксировать вывод в отчете по инциденту"
    ],
    evidence: [
      "удаленные сеансы: проверяются по событиям рабочего времени",
      "процессы: проверяются по приложениям",
      "сеть: проверяется по сетевым сигналам",
      "внешний шлюз: подключается как дополнительный источник",
      "периметр: учитывается при наличии интеграционного слоя"
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
        <div><span class="muted">Номер</span><strong>${ui(card.incident_id)}</strong></div>
        <div><span class="muted">Риск</span><strong>${ui(card.risk_id)}</strong></div>
        <div><span class="muted">Подразделение</span><strong>${ui(card.department)}</strong></div>
        <div><span class="muted">Ответственный</span><strong>${ui(card.owner)}</strong></div>
        <div><span class="muted">Индекс активности</span><strong>${ui(card.activity_index)}</strong></div>
        <div><span class="muted">Отклонение</span><strong>${ui(card.deviation)}</strong></div>
        <div><span class="muted">Сформировано</span><strong>${ui(card.generated_at)}</strong></div>
      </div>
      <div class="list compact-list">
        <div class="row compact-row"><strong>Краткое описание</strong><span class="muted">${ui(card.summary)}</span><span></span></div>
        <div class="row compact-row"><strong>Почему это риск</strong><span class="muted">${ui(card.why_it_is_risk)}</span><span></span></div>
        <div class="row compact-row"><strong>Что проверить</strong><span class="muted">${ui(card.what_to_check)}</span><span></span></div>
        <div class="row compact-row"><strong>Рекомендуемые действия</strong><span class="muted">${ui(card.recommended_actions.join("; "))}</span><span></span></div>
        <div class="row compact-row"><strong>Материалы расследования</strong><span class="muted">${ui(card.evidence.join("; "))}</span><span></span></div>
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
    <section class="card cases-card" id="cases-section">
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
                <td><span class="badge status-ok">в норме</span></td>
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
  const packUrl = `/portal/api/cases/${encodeURIComponent(id)}?format=markdown&role=forensics`;
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
      `).join("") : `<div class="row compact-row"><strong>Сигналы</strong><span class="muted">Существенных риск-сигналов в текущем срезе нет.</span><span class="badge status-ok">в норме</span></div>`}</div>
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
      title: "Данные агента подтверждают показатели",
      summary: "Сессии собраны основным способом Windows; индекс активности можно использовать как рабочий управленческий показатель.",
      recommendation: "Использовать отчет как подтвержденный оперативный срез.",
      kpi_accepted: true
    };
  }
  if (source === "local_fallback") {
    return {
      status: "DEGRADED",
      title: "Диагностический режим агента",
      summary: "Диагностический режим, данные не засчитываются в показатели активности.",
      recommendation: "Проверить доступность WTS API и права запуска агента.",
      kpi_accepted: false
    };
  }
  if (hasError) {
    return {
      status: "DEGRADED",
      title: "Достоверность данных снижена",
      summary: `Коллектор передал ошибку: ${q.collector_error}`,
      recommendation: "Восстановить основной способ сбора Windows перед использованием отчета как доказательной базы.",
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
    <section class="card agent-quality-card" id="trust-kpi-section">
      <div class="section-head">
        <div>
          <h3 ${tooltip("Показывает, можно ли использовать данные агента для управленческих показателей.")}>Качество данных</h3>
          <p class="muted">${ui(e.title || "Оценка доверия к данным агента")}</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <p class="quality-summary">${ui(e.summary || "")}</p>
      <div class="quality-decision">
        <div><span class="muted">Участвует в показателях</span><strong>${accepted ? "да" : "нет"}</strong></div>
        <div><span class="muted">Источник</span><strong>${ui(source)}</strong></div>
      </div>
      ${warn ? `<div class="quality-warning">Внимание. Данные активности собраны не основным способом. Точность определения активности и удаленных сеансов может быть снижена.</div>` : ""}
      <p class="muted">${ui(e.recommendation || "")}</p>
      <details class="quality-details">
        <summary>Технические детали</summary>
        <div class="quality-grid">
          <div><span class="muted">Источник коллектора</span><strong>${ui(source)}</strong></div>
          <div><span class="muted">Всего сессий</span><strong>${escapeHtml(q.sessions_collected_total ?? 0)}</strong></div>
          <div><span class="muted">Активных сессий</span><strong>${escapeHtml(q.active_sessions_total ?? 0)}</strong></div>
          <div><span class="muted">Удаленных сеансов</span><strong>${escapeHtml(q.rdp_sessions_total ?? 0)}</strong></div>
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
          <p class="muted">Показывает, можно ли доверять недельным показателям, а не только текущему срезу.</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <div class="quality-decision">
        <div><span class="muted">Дней в норме</span><strong>${escapeHtml(s.ok_days ?? 0)}</strong></div>
        <div><span class="muted">Проблемных дней</span><strong>${escapeHtml(unstableDays)}</strong></div>
        <div><span class="muted">Показатели подтверждены</span><strong>${escapeHtml(s.kpi_accepted_pct ?? 0)}%</strong></div>
      </div>
      ${Number(s.ok_days || 0) < 5 ? `<div class="quality-warning">Показатели требуют проверки: нестабильный сбор данных агента.</div>` : ""}
      <details class="quality-details">
        <summary>История по дням</summary>
        <div class="list compact-list">${items.length ? items.map(item => `
          <div class="row compact-row">
            <strong>${escapeHtml(item.date || "-")}</strong>
            <span class="muted">источник=${ui(item.source || "unknown")} · показатели=${item.kpi_accepted ? "да" : "нет"}${item.collector_error ? ` · ${ui(item.collector_error)}` : ""}</span>
            <span class="badge ${statusClass(item.status)}">${ui(item.status || "UNKNOWN")}</span>
          </div>
        `).join("") : `<div class="row compact-row"><strong>История</strong><span class="muted">История качества агента за период отсутствует.</span><span class="badge status-unknown">нет данных</span></div>`}</div>
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
          <p class="muted">Какие рабочие места подтверждают показатели, а какие снижают доверие к управленческой аналитике.</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <div class="quality-decision">
        <div><span class="muted">Всего узлов</span><strong>${escapeHtml(s.total_nodes ?? 0)}</strong></div>
        <div><span class="muted">В норме</span><strong>${escapeHtml(s.ok_nodes ?? 0)}</strong></div>
        <div><span class="muted">Проблемных</span><strong>${escapeHtml(Number(s.degraded_nodes || 0) + Number(s.unknown_nodes || 0))}</strong></div>
        <div><span class="muted">Показатели подтверждены</span><strong>${escapeHtml(s.accepted_kpi_nodes_pct ?? 0)}%</strong></div>
      </div>
      ${Number(s.total_nodes || 0) > 0 && Number(s.accepted_kpi_nodes_pct || 0) < 80 ? `<div class="quality-warning">Показатели требуют проверки: менее 80% рабочих мест дают подтвержденные данные.</div>` : ""}
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>Узел</th>
              <th>Статус</th>
              <th>Источник</th>
              <th>Последняя телеметрия</th>
              <th>Показатели</th>
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
                <td><span class="badge status-unknown">нет данных</span></td>
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
    <section class="card agent-coverage-card" id="agent-coverage-section">
      <div class="section-head">
        <div>
          <h3 ${tooltip("Показывает, какая часть рабочих мест присылает свежие данные.")}>Полнота данных</h3>
          <p class="muted">Показывает, насколько показатели репрезентативны по всему парку рабочих мест.</p>
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
      ${status === "CRITICAL" ? `<div class="quality-warning">Полнота данных критически недостаточна: показатели нельзя считать репрезентативными.</div>` : ""}
      ${status === "WARNING" ? `<div class="quality-warning">Показатели требуют проверки: часть рабочих мест не присылает свежую телеметрию.</div>` : ""}
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

function renderSecurityEventsSummary(summary, options = {}) {
  const s = summary || {};
  const backend = s.backend || "disabled";
  const disabled = backend === "disabled" || s.status === "disabled" || !summary;
  const fallback = Boolean(s.fallback_used);
  const status = disabled ? "UNKNOWN" : fallback ? "WARN" : Number(s.events_24h || 0) > 0 ? "WARN" : "OK";
  const stateText = disabled
    ? "Источник событий безопасности отключён"
    : fallback
      ? "События безопасности временно недоступны"
      : "События безопасности доступны";
  const localModeText = disabled ? "Локальный режим" : "";
  const title = options.compact
    ? "События безопасности"
    : "События безопасности за 24 часа";
  const subtitle = disabled
    ? `${stateText}. ${localModeText}.`
    : fallback
      ? "Портал работает в резервном режиме без событий безопасности."
      : "Агрегированная сводка без сырых журналов и без автоматического создания инцидентов.";
  const top = Array.isArray(s.top_departments) ? s.top_departments.slice(0, 5) : [];
  const warning = fallback
    ? `<div class="quality-warning">События безопасности временно недоступны. Проверьте подключение источника событий.</div>`
    : "";
  if (options.compact) {
    return `
      <section class="card security-events-card">
        <div class="section-head">
          <div>
            <h3 ${tooltip("Краткая управленческая сводка доступности событий безопасности за последние 24 часа.")}>${ui(title)}</h3>
            <p class="muted">${ui(stateText)}</p>
          </div>
          <span class="badge ${statusClass(status)}">${ui(status)}</span>
        </div>
        <div class="quality-grid">
          <div><span class="muted">Состояние</span><strong>${ui(stateText)}</strong></div>
          <div><span class="muted">Событий за 24 часа</span><strong>${ui(s.events_24h ?? 0)}</strong></div>
          <div><span class="muted">Режим</span><strong>${ui(disabled ? localModeText : "Агрегированная сводка")}</strong></div>
        </div>
      </section>
    `;
  }
  return `
    <section class="card security-events-card">
      <div class="section-head">
        <div>
          <h3 ${tooltip("Краткая агрегированная сводка событий безопасности за последние 24 часа. Это не SIEM-журнал.")}>${ui(title)}</h3>
          <p class="muted">${ui(subtitle)}</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <div class="quality-grid">
        <div><span class="muted">Состояние</span><strong>${ui(stateText)}</strong></div>
        <div><span class="muted">Источник</span><strong>${ui(backend)}</strong></div>
        <div><span class="muted">Событий</span><strong>${ui(s.events_24h ?? 0)}</strong></div>
        <div><span class="muted">Неуспешные входы</span><strong>${ui(s.failed_logins_24h ?? 0)}</strong></div>
        <div><span class="muted">Подозрительные входы</span><strong>${ui(s.suspicious_logins_24h ?? 0)}</strong></div>
        <div><span class="muted">RDP-сессии</span><strong>${ui(s.rdp_sessions_24h ?? 0)}</strong></div>
        <div><span class="muted">Ошибки агентов</span><strong>${ui(s.agent_errors_24h ?? 0)}</strong></div>
      </div>
      ${warning}
      ${s.error ? `<p class="muted small">Причина: ${ui(s.error)}</p>` : ""}
      ${disabled ? `<p class="muted small">Источник событий безопасности не включен в текущем режиме.</p>` : ""}
      ${options.compact ? "" : `
        <div class="table-scroll">
          <table class="data-table">
            <thead>
              <tr>
                <th>Подразделение</th>
                <th>События</th>
              </tr>
            </thead>
            <tbody>
              ${top.length ? top.map(item => `
                <tr>
                  <td>${ui(item.department || "Без подразделения")}</td>
                  <td>${ui(item.events ?? 0)}</td>
                </tr>
              `).join("") : `
                <tr>
                  <td>${disabled ? "Источник отключён" : "Нет данных"}</td>
                  <td>0</td>
                </tr>
              `}
            </tbody>
          </table>
        </div>
        <p class="muted small">Последнее событие: ${ui(s.last_event_utc || "нет данных")} · запрос ${ui(s.query_ms ?? 0)} ms.</p>
      `}
    </section>
  `;
}

function renderBusinessRisk(items) {
  const rows = Array.isArray(items) ? items.slice(0, 10) : [];
  const worst = rows[0]?.risk_level || "UNKNOWN";
  return `
    <section class="card business-risk-card" id="business-risk-section">
      <div class="section-head">
        <div>
          <h3>Риски подразделений</h3>
          <p class="muted">Организационные зоны риска по достоверности показателей, активности, тренду и проблемным рабочим местам.</p>
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
              <th>События</th>
              <th>Рекомендация</th>
            </tr>
          </thead>
          <tbody>
            ${rows.length ? rows.map(item => `
              <tr>
                <td><strong>${ui(item.department || "Без подразделения")}</strong></td>
                <td><span class="badge ${statusClass(item.risk_level)}">${ui(item.risk_level || "UNKNOWN")}</span></td>
                <td>${ui(businessRiskReasons(item))}</td>
                <td>${ui(item.security_events_24h ?? 0)}</td>
                <td>${ui(item.recommendation || "Проверить первичные данные подразделения.")}</td>
              </tr>
            `).join("") : `
              <tr>
                <td>Нет данных</td>
                <td><span class="badge status-unknown">нет данных</span></td>
                <td>нет данных</td>
                <td>0</td>
                <td>Дождаться расчета подразделений.</td>
              </tr>
            `}
          </tbody>
        </table>
      </div>
    </section>
  `;
}

function renderRiskHeatmap(items) {
  const rows = Array.isArray(items) ? items.slice(0, 10) : [];
  const worst = rows[0]?.heat_level || "UNKNOWN";
  return `
    <section class="card risk-heatmap-card">
      <div class="section-head">
        <div>
          <h3 ${tooltip("Таблица показывает подразделения, где одновременно есть несколько признаков риска.")}>Карта рисков</h3>
          <p class="muted">Где одновременно снижена достоверность показателей, низкая активность, неполные данные, записи на проверку и активные расследования.</p>
        </div>
        <span class="badge ${statusClass(worst)}">${ui(worst)}</span>
      </div>
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>Подразделение</th>
              <th>Достоверность</th>
              <th>Активность</th>
              <th>Покрытие</th>
              <th>Риск</th>
              <th>События</th>
              <th>Расследования</th>
              <th>Связи</th>
            </tr>
          </thead>
          <tbody>
            ${rows.length ? rows.map(item => `
              <tr class="heatmap-row heatmap-${escapeHtml(String(item.heat_level || "unknown").toLowerCase())}">
                <td><strong>${ui(item.department || "Без подразделения")}</strong></td>
                <td>${ui(riskPercentText(item.trust_kpi_score))}</td>
                <td>${ui(riskPercentText(item.activity_score))}</td>
                <td>${ui(riskPercentText(item.agent_coverage_pct))}</td>
                <td>
                  <span class="badge ${statusClass(item.heat_level)}">${ui(item.heat_level || "UNKNOWN")}</span><br>
                  <span class="muted small">${ui(item.business_risk_level || "UNKNOWN")} · проверить ${ui(item.critical_candidates ?? 0)}</span>
                </td>
                <td>${ui(item.security_events_24h ?? 0)}</td>
                <td>${ui(item.open_cases ?? 0)}</td>
                <td>${renderRiskLayerLinks(item.links)}</td>
              </tr>
            `).join("") : `
              <tr>
                <td>Нет данных</td>
                <td>нет данных</td>
                <td>нет данных</td>
                <td>нет данных</td>
                <td><span class="badge status-unknown">нет данных</span></td>
                <td>0</td>
                <td>0</td>
                <td>-</td>
              </tr>
            `}
          </tbody>
        </table>
      </div>
    </section>
  `;
}

function renderRiskLayerLinks(links) {
  const items = Array.isArray(links) ? links : [];
  if (!items.length) return "-";
  return `<div class="button-row compact-actions">${items.map(link => {
    const target = riskLayerTarget(link.target);
    return `<button class="small-button" data-risk-layer-tab="${ui(target.tab)}" data-risk-layer-selector="${ui(target.selector)}" title="${ui(link.summary || link.label || "")}">${ui(link.label || link.target || "слой")}</button>`;
  }).join("")}</div>`;
}

function riskLayerTarget(target) {
  const value = String(target || "");
  if (value === "risk_heatmap") return { tab: "operator", selector: ".risk-heatmap-card" };
  if (value === "business_risk") return { tab: "operator", selector: "#business-risk-section" };
  if (value === "security_correlation") return { tab: "operator", selector: ".security-correlation-card" };
  if (value === "security_events") return { tab: "operator", selector: ".security-events-card" };
  if (value === "incident_candidates") return { tab: "operator", selector: "#risk-candidates-section" };
  if (value === "cases") return { tab: "incidents", selector: "#cases-section" };
  if (value === "agent_coverage") return { tab: "operator", selector: "#agent-coverage-section" };
  return { tab: "operator", selector: "#trust-kpi-section" };
}

function riskPercentText(value) {
  const number = Number(value);
  return Number.isFinite(number) ? `${Math.round(number)}%` : "UNKNOWN";
}

function renderSecurityCorrelation(items) {
  const rows = Array.isArray(items) ? items.slice(0, 10) : [];
  const score = Number(rows[0]?.correlation_score || 0);
  const status = score >= 80 ? "CRITICAL" : score >= 60 ? "HIGH" : score >= 35 ? "MEDIUM" : rows.length ? "LOW" : "UNKNOWN";
  return `
    <section class="card security-correlation-card">
      <div class="section-head">
        <div>
          <h3 ${tooltip("Показывает, где проблемы активности совпадают с рисками проверки.")}>Связь рисков и активности</h3>
          <p class="muted">Связь между падением активности, достоверностью показателей, записями на проверку и активными расследованиями. Инциденты автоматически не создаются.</p>
        </div>
        <span class="badge ${statusClass(status)}">${ui(status)}</span>
      </div>
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>Подразделение</th>
              <th>Достоверность</th>
              <th>Активность</th>
              <th>Риск подразделения</th>
              <th>Проверки</th>
              <th>События</th>
              <th>Взаимосвязь</th>
              <th>Причина</th>
            </tr>
          </thead>
          <tbody>
            ${rows.length ? rows.map(item => `
              <tr>
                <td><strong>${ui(item.department || "Без подразделения")}</strong></td>
                <td>${ui(riskPercentText(item.trust_kpi_score))}</td>
                <td>${ui(riskPercentText(item.activity_score))}</td>
                <td><span class="badge ${statusClass(item.business_risk_level)}">${ui(item.business_risk_level || "UNKNOWN")}</span></td>
                <td>проверить ${ui(item.critical_candidates ?? 0)} · расследования ${ui(item.open_cases ?? 0)}</td>
                <td>${ui(item.security_events_24h ?? 0)}</td>
                <td><strong>${ui(Number(item.correlation_score || 0))}/100</strong></td>
                <td>${ui(item.explanation || item.correlation_reason || "связь не выражена")}</td>
              </tr>
            `).join("") : `
              <tr>
                <td>Нет данных</td>
                <td>нет данных</td>
                <td>нет данных</td>
                <td><span class="badge status-unknown">нет данных</span></td>
                <td>проверить 0 · расследования 0</td>
                <td>0</td>
                <td>0/100</td>
                <td>недостаточно данных по подразделениям</td>
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
  return `${reasons}. Доверие ${item?.trust_score ?? 0}%, активность ${item?.activity_score ?? 0}%, тренд ${businessTrendText(item?.trend)}, проблемных рабочих мест ${item?.problem_nodes_count ?? 0}, событий безопасности за 24 часа ${item?.security_events_24h ?? 0}`;
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
          <p class="muted">Как менялись риски подразделений по накопленной ежедневной истории.</p>
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
                <td><span class="badge status-unknown">нет данных</span></td>
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
    <section class="card risk-candidates-card" id="risk-candidates-section">
      <div class="section-head">
        <div>
          <h3 ${tooltip("Очередь ситуаций, которые нужно проверить вручную перед созданием расследования.")}>Требует проверки</h3>
          <p class="muted">Очередь ручной проверки. Реальные инциденты автоматически не создаются.</p>
        </div>
        <span class="badge ${statusClass(worst)}">${ui(worst)}</span>
      </div>
      <div class="table-scroll">
        <table class="data-table">
          <thead>
            <tr>
              <th>Номер</th>
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
                <td><span class="badge status-ok">в норме</span></td>
                <td><span class="badge status-unknown">NEW</span></td>
                <td>очередь проверки пуста</td>
                <td>Действий не требуется.</td>
                <td>-</td>
              </tr>
            `}
          </tbody>
        </table>
      </div>
      ${rows.length ? `<p class="muted small">Показаны записи для проверки, а не автоматически подтвержденные инциденты.</p>` : ""}
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
  const packUrl = `/portal/api/investigation-pack/${encodeURIComponent(id)}?format=markdown&role=forensics`;
  const createCase = reviewStatus === "CONFIRMED"
    ? `<button class="small-button investigation-pack-button primary" data-create-case="true" data-candidate-id="${escapeHtml(id)}">Создать дело</button>`
    : "";
  return `
    <div class="button-row compact-actions">${actions.map(([status, label]) => `
      <button class="small-button" data-review-status="${escapeHtml(status)}" data-candidate-id="${escapeHtml(id)}">${ui(label)}</button>
    `).join("")}</div>
    <button class="small-button" data-open-investigation="true">Открыть расследование</button>
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
    ? ` Материалы: ${item.evidence.join("; ")}`
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
    ${renderRiskNarrative(data)}
    ${renderExecutiveDashboard(data)}
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
    ${renderSecurityEventsSummary(data.security_events_summary)}
    ${renderRiskHeatmap(data.risk_heatmap)}
    ${renderSecurityCorrelation(data.security_correlation)}
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
    ["Порог внимания", ">= 15 баллов", "любой ненормальный риск попадает в очередь проверки"],
    ["Порог критического риска", ">= 70 баллов", "высокий риск требует приоритетного разбора"],
    ["Источник правил", policy.configured ? basename(policy.path) : "встроенные правила", risk.policy_configured ? `правила оценки риска: ${basename(risk.policy_path)}` : "правила оценки риска: встроенная модель"],
    ["Дата последнего пересчета", report?.generated_at_utc || "-", `версия политики: ${risk.policy_version || "ueba-rule-v1"}`],
  ];
}

function renderSettings(report) {
  const rows = settingRows(periodReport(report || state.reports || {}));
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Настройки</h2>
        <p class="muted">Параметры расчета и границы интерпретации данных. Экран открыт только для чтения.</p>
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

async function refresh(options = {}) {
  const content = document.getElementById("content");
  const background = Boolean(options.background);
  const stage = options.stage || "Получение данных";
  const progress = (status, label, value) => {
    if (!background) setLoadStatus(status, label, value);
  };
  try {
    progress("LOADING", stage, 8);
    if (!background && content) content.innerHTML = renderLoadingContent(stage);
    if (!state.links) {
      progress("LOADING", "Получение данных", 18);
      state.links = await loadJson("/links");
    }
    progress("LOADING", "Расчёт показателей", 34);
    const summary = await loadJson("/summary");
    progress("LOADING", "Расчёт показателей", 46);
    state.readiness = {
      bundle: await loadJson("/readiness/bundle").catch(error => ({ ok: false, error: error.message })),
      verify: state.readiness?.verify || null
    };
    renderSummary(summary, state.readiness);
    progress("LOADING", "Формирование главного вывода", 68);
    const tabResult = await loadCurrentTab();
    progress("LOADING", "Подготовка разделов", 88);
    if (!hasTabData(state.tab, tabResult)) {
      setLoadStatus("EMPTY", "Данные отсутствуют", 100);
      if (content) {
        content.innerHTML = `${renderEmptyState("Источники ответили, но полезные записи для текущего раздела пока не найдены.")}${tabResult.html || ""}`;
      }
      return;
    }
    if (content) content.innerHTML = tabResult.html;
    setLoadStatus("READY", "Данные готовы", 100);
    consumePendingScroll();
  } catch (error) {
    showError(error);
  }
}

async function loadCurrentTab() {
  if (state.tab === "operator") {
    const data = await loadJson("/operator");
    state.operatorData = data;
    state.reports = await loadJson("/reports").catch(() => state.reports);
    if (currentViewMode() === "security" || currentViewMode() === "forensics") {
      state.cases = await loadJson("/cases").catch(error => ({ ok: false, error: error.message, cases: [] }));
    }
    updateFilters(state.reports);
    return { data, report: state.reports, cases: state.cases, html: renderOperator(data, state.reports, { cases: state.cases }) };
  }
  if (state.tab === "manager") {
    const data = await loadJson("/manager");
    const policyExplain = await loadJson("/workforce/policy/explain").catch(() => null);
    return { data, policyExplain, html: renderManager(data, policyExplain) };
  }
  if (state.tab === "departments") {
    const data = await loadJson("/reports");
    state.reports = data;
    updateFilters(data);
    return { data, html: renderDepartments(data) };
  }
  if (state.tab === "employees") {
    const data = await loadJson("/manager");
    const policyExplain = await loadJson("/workforce/policy/explain").catch(() => null);
    return { data, policyExplain, html: renderEmployees(data, policyExplain) };
  }
  if (state.tab === "owner") {
    const data = await loadJson("/owner");
    return { data, html: renderOwner(data) };
  }
  if (state.tab === "incidents") {
    const data = await loadJson("/incidents");
    const evidence = await loadJson("/dlp/evidence").catch(error => ({ ok: false, error: error.message, items: [] }));
    const reports = await loadJson("/reports").catch(() => state.reports || {});
    const cases = await loadJson("/cases").catch(error => ({ ok: false, error: error.message, cases: [] }));
    state.reports = reports;
    return { data: { incidents: data, evidence, reports, cases }, html: renderIncidents({ incidents: data, evidence, reports, cases }) };
  }
  if (state.tab === "perimeter") {
    const data = await loadJson("/owner");
    const reports = await loadJson("/reports").catch(() => state.reports);
    state.reports = reports;
    return { data, report: reports, html: renderPerimeter(data, reports) };
  }
  if (state.tab === "reports") {
    const data = await loadJson("/reports");
    state.reports = data;
    updateFilters(data);
    return { data, html: renderReports(data) };
  }
  if (state.tab === "settings") {
    const data = await loadJson("/reports").catch(() => state.reports || {});
    state.reports = data;
    updateFilters(data);
    return { data, html: renderSettings(data) };
  }
  return { data: {}, html: renderEmptyState("Раздел не найден.") };
}

function tabLoadingStage(tab) {
  if (tab === "operator") return currentViewMeta().stage;
  return {
    employees: "Получение данных сотрудников",
    departments: "Расчёт показателей подразделений",
    owner: "Формирование главного вывода по рискам",
    incidents: "Подготовка разделов расследований",
    perimeter: "Подготовка разделов сетевого периметра",
    reports: "Подготовка разделов отчета",
    settings: "Подготовка раздела настроек",
  }[tab] || "Подготовка разделов";
}

function setTab(tab) {
  state.tab = tab;
  applySecurityMode(tab);
  document.querySelectorAll(".tab").forEach(btn => {
    btn.classList.toggle("is-active", btn.dataset.tab === tab);
  });
  refresh({ stage: tabLoadingStage(tab) });
}

function applySecurityMode(tab) {
  document.body.classList.toggle("security-mode", tab === "owner" || tab === "incidents" || tab === "perimeter");
}

function consumePendingScroll() {
  const selector = state.pendingScrollSelector;
  if (!selector) return;
  state.pendingScrollSelector = null;
  window.setTimeout(() => {
    const target = document.querySelector(selector);
    if (target) target.scrollIntoView({ behavior: "smooth", block: "start" });
  }, 0);
}

function showError(error) {
  const hasPreviousData = Boolean(state.load.lastUpdatedAt);
  setLoadStatus(
    hasPreviousData ? "STALE" : "ERROR",
    hasPreviousData ? "Не удалось обновить данные" : "Ошибка загрузки данных",
    hasPreviousData ? 100 : 0,
    { error: error?.message || String(error) },
  );
  const content = document.getElementById("content");
  if (!content) return;
  if (hasPreviousData) {
    if (!content.querySelector("[data-loading-state='STALE']")) {
      content.insertAdjacentHTML("afterbegin", staleBanner(error));
    }
    return;
  }
  content.innerHTML = renderErrorState(error);
}

document.querySelectorAll(".tab").forEach(btn => {
  btn.addEventListener("click", () => setTab(btn.dataset.tab));
});

document.querySelectorAll("[data-view-mode]").forEach(btn => {
  btn.addEventListener("click", () => setViewMode(btn.dataset.viewMode));
});

document.getElementById("periodFilter")?.addEventListener("change", event => {
  state.period = periodFromSelectValue(event.target.value);
  refresh({ stage: "Получение данных за выбранный период" });
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
  const button = event.target.closest("[data-risk-layer-tab]");
  if (!button) return;
  const selector = button.dataset.riskLayerSelector || "";
  state.pendingScrollSelector = selector;
  const tab = button.dataset.riskLayerTab || state.tab;
  if (tab === state.tab) {
    consumePendingScroll();
  } else {
    setTab(tab);
  }
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
    status.textContent = `Проверено: контрольная сумма ${verify.checksum_verified ? "подтверждена" : "не подтверждена"} · подпись ${verify.signature_verified ? "подтверждена" : "не подтверждена"}`;
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
  await refresh({ stage: "Обновление статуса инцидента" });
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
  await refresh({ stage: "Обновление проверки кандидата" });
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
  await refresh({ stage: "Создание дела и обновление расследований" });
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
  await refresh({ stage: "Обновление статуса дела" });
}

updateViewModeButtons();
applySecurityMode(state.tab);
renderLoadStatus();
refresh({ stage: "Первичная загрузка портала" });
setInterval(() => refresh({ background: true, stage: "Фоновое обновление данных" }), 60000);
