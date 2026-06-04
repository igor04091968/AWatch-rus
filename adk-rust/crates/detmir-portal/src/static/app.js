const state = { tab: "operator", links: null, readiness: null, reports: null };

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
  if (s === "ok" || s === "true") return "status-ok";
  if (s === "warn" || s === "warning") return "status-warn";
  if (s === "fail" || s === "false") return "status-fail";
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
    .replaceAll("yes", "да")
    .replaceAll("no", "нет")
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

function metricCard(labelText, value, status, context) {
  return `
    <article class="metric-card">
      <span class="badge ${statusClass(status)}">${escapeHtml(status || "INFO")}</span>
      <h3>${ui(labelText)}</h3>
      <p class="metric-value">${ui(value ?? "Нет данных")}</p>
      <p class="muted">${ui(context || "")}</p>
    </article>
  `;
}

function renderExecutiveMetrics(report, incidents) {
  const activity = findKpi(report, "Индекс активности");
  const employees = findKpi(report, "Сотрудники");
  const risk = findKpi(report, "UEBA риск");
  const open = findKpi(report, "Открытые вопросы");
  const departments = findKpi(report, "Подразделения");
  const evidence = findKpi(report, "Evidence");
  const openCount = Array.isArray(incidents) ? incidents.filter(item => !item.acknowledged).length : 0;
  return `
    <section class="executive-grid" aria-label="Управленческая сводка">
      ${metricCard("Индекс активности за день", activity?.value, activity?.status, activity?.context || "расчет: активное время / плановое время")}
      ${metricCard("Активные сотрудники", employees?.value, employees?.status, employees?.context)}
      ${metricCard("Отклонения от нормы", open?.value ?? openCount, open?.status || incidentStatusFromCount(openCount), "вопросы, требующие реакции")}
      ${metricCard("Подразделения с просадкой", departments?.value, departments?.status, departments?.context || "сравнение текущего дня")}
      ${metricCard("Риски ИБ", risk?.value, risk?.status, risk?.context || "оценка по правилам")}
      ${metricCard("Новые инциденты", open?.value ?? openCount, open?.status || incidentStatusFromCount(openCount), "не взятые в работу")}
      ${metricCard("Готовность отчета", reportReadinessText(report), report?.operator_ok ? "OK" : report?.severity, "daily / weekly / monthly")}
      ${metricCard("Доказательная база", evidence?.value, evidence?.status, evidence?.context)}
    </section>
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
    ${renderExecutiveMetrics(report, incidents)}
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

function renderIncidents(data) {
  const links = state.links || {};
  const incidents = Array.isArray(data) ? data : data.incidents;
  const evidence = Array.isArray(data) ? null : data.evidence;
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
    <section class="card evidence-card">
      <h3>Материалы: скриншоты, хеши, файлы</h3>
      ${renderDlpEvidence(evidence)}
    </section>
  `;
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

function renderReports(data) {
  return `
    <div class="page-head">
      <div>
        <h2 class="section-title">Отчеты</h2>
        <p class="muted">Ежедневные, недельные, месячные, по подразделению, сотруднику, инциденту и пилотной эксплуатации.</p>
      </div>
      <span class="badge ${statusClass(data.severity)}">${escapeHtml(data.severity)}</span>
    </div>
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

async function refresh() {
  if (!state.links) state.links = await loadJson("/links");
  const summary = await loadJson("/summary");
  state.readiness = {
    bundle: await loadJson("/readiness/bundle").catch(error => ({ ok: false, error: error.message })),
    verify: state.readiness?.verify || null
  };
  renderSummary(summary, state.readiness);
  const content = document.getElementById("content");
  const data = await loadJson(`/${state.tab}`);
  if (state.tab === "operator") {
    state.reports = await loadJson("/reports").catch(() => state.reports);
    content.innerHTML = renderOperator(data, state.reports);
    updateFilters(state.reports);
  }
  if (state.tab === "manager") {
    const policyExplain = await loadJson("/workforce/policy/explain").catch(() => null);
    content.innerHTML = renderManager(data, policyExplain);
  }
  if (state.tab === "owner") content.innerHTML = renderOwner(data);
  if (state.tab === "incidents") {
    const evidence = await loadJson("/dlp/evidence").catch(error => ({ ok: false, error: error.message, items: [] }));
    content.innerHTML = renderIncidents({ incidents: data, evidence });
  }
  if (state.tab === "reports") {
    state.reports = data;
    content.innerHTML = renderReports(data);
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
  document.body.classList.toggle("security-mode", tab === "owner" || tab === "incidents");
}

function showError(error) {
  document.getElementById("content").innerHTML = `<pre>${escapeHtml(error.stack || error.message || error)}</pre>`;
}

document.querySelectorAll(".tab").forEach(btn => {
  btn.addEventListener("click", () => setTab(btn.dataset.tab));
});

document.addEventListener("click", event => {
  const button = event.target.closest("[data-incident-action]");
  if (!button) return;
  incidentAction(button).catch(showError);
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
  document.getElementById("content").innerHTML = renderReports(data);
}

async function exportMarkdown() {
  const data = state.reports || await loadJson("/reports");
  const markdown = displayText(data.markdown || "");
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

applySecurityMode(state.tab);
refresh().catch(showError);
setInterval(() => refresh().catch(showError), 60000);
