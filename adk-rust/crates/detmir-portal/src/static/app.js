const state = { tab: "operator", links: null };

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

function renderSummary(summary) {
  const global = document.getElementById("globalStatus");
  global.className = `status-pill ${statusClass(summary.severity)}`;
  global.textContent = `${summary.severity} · operator ${summary.operator_ok ? "OK" : "NO"}`;
  const blocks = Object.entries(summary.blocks || {});
  document.getElementById("summary").innerHTML = blocks.map(([name, block]) => `
    <article class="card">
      <span class="badge ${statusClass(block.status)}">${escapeHtml(block.status)}</span>
      <h3>${escapeHtml(label(name))}</h3>
      <p class="muted">${escapeHtml(block.text)}</p>
    </article>
  `).join("");
}

function label(name) {
  return {
    collection: "Сбор данных",
    grafana: "Grafana",
    dlp: "DLP",
    worktime: "Работа сегодня",
    one_c: "1С"
  }[name] || name;
}

function renderLinks(links) {
  const link = (text, href) => `<a class="button" href="${escapeHtml(href)}" target="_blank" rel="noopener noreferrer">${escapeHtml(text)}</a>`;
  return `<div class="links">
    ${link("DetMir ActivityWatch", links.detmir_activitywatch)}
    ${link("Grafana", links.grafana_dashboards)}
    ${link("AW UI", links.aw_ui)}
    ${link("Рабочее время", links.worktime_report)}
    ${link("1С сводка", links.file1c_brief)}
    ${link("1С действия", links.file1c_actions)}
  </div>`;
}

function renderDlpLinks(links) {
  const link = (text, href) => `<a class="button" href="${escapeHtml(href)}" target="_blank" rel="noopener noreferrer">${escapeHtml(text)}</a>`;
  return `<div class="links">
    ${link("ИБ дашборд", links.dlp_security_dashboard)}
    ${link("ИБ для руководства", links.dlp_management_dashboard)}
    ${link("DLP обзор", links.dlp_overview_dashboard)}
    ${link("Все Grafana dashboards", links.grafana_dashboards)}
  </div>`;
}

function renderSourceList(data) {
  const sources = [
    ["DetMir", data.detmir_status],
    ["Проверки", data.detmir_check],
    ["Systemd", data.failed_units],
    ["Grafana data", data.grafana_data]
  ];
  return `<div class="list">${sources.map(([name, source]) => `
    <div class="row">
      <strong>${escapeHtml(name)}</strong>
      <span class="muted">${escapeHtml(source?.summary || source?.error || "нет данных")}</span>
      <span class="badge ${statusClass(source?.status || source?.ok)}">${escapeHtml(source?.status || (source?.ok ? "OK" : "FAIL"))}</span>
    </div>
  `).join("")}</div>`;
}

function renderOperator(data) {
  return `
    <h2 class="section-title">Оператор</h2>
    <div class="grid-2">
      <section class="card">
        <h3>Контур</h3>
        ${renderSourceList(data)}
      </section>
      <section class="card">
        <h3>Быстрые переходы</h3>
        ${renderLinks(data.links)}
      </section>
    </div>
    <h3 class="section-title">Проблемы</h3>
    ${renderIncidentsList(data.incidents)}
  `;
}

function renderManager(data, reports) {
  const workforceIndex = workforceIndexText(data.users_count, data.total_active_seconds);
  return `
    <h2 class="section-title">Руководитель</h2>
    <div class="grid-2">
      <section class="card">
        <h3>Работа сегодня</h3>
        <p class="kpi-value">${escapeHtml(workforceIndex)}</p>
        <p class="muted">proxy: активное время / плановое рабочее время</p>
        <p class="muted">Сотрудников: ${data.users_count}; активных часов: ${Number(data.total_active_hours || 0).toFixed(1)}</p>
        <p class="muted">${escapeHtml(data.status?.text || "")}</p>
      </section>
      <section class="card">
        <h3>Приложения</h3>
        <div class="list">${(data.applications || []).slice(0, 8).map(app => `
          <div class="row">
            <strong>${escapeHtml(app.application)}</strong>
            <span class="muted">${escapeHtml(app.proved_work_human || "")}</span>
            <span class="badge status-ok">${escapeHtml(app.evidence_events || 0)}</span>
          </div>
        `).join("")}</div>
      </section>
    </div>
    ${renderWorkforceIndexExplanation(reports?.workforce_policy)}
    <h3 class="section-title">Сотрудники</h3>
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
        <p class="muted">Role/application policy не настроена. Доступен только нейтральный индекс активности.</p>
      </section>
    `;
  }
  const details = Array.isArray(policy.app_details) ? policy.app_details.slice(0, 12) : [];
  const weightedTotal = Math.max(1, Number(policy.weighted_seconds || 0));
  const appRows = details.length === 0
    ? `<div class="row compact-row"><strong>Нет приложений</strong><span class="muted">Нет top breakdown для weighted KPI</span><span></span></div>`
    : details.map(item => {
        const contribution = Math.round(Number(item.weighted_seconds || 0) / weightedTotal * 100);
        return `
          <div class="row app-weight-row">
            <strong>${escapeHtml(item.application || "-")}</strong>
            <span class="muted">${escapeHtml(humanSeconds(item.seconds))} · вес ${escapeHtml(pctText(item.weight))} · правило ${escapeHtml(item.matched_rule || "default_weight")}</span>
            <span class="badge ${Number(item.weight || 0) > 0 ? "status-ok" : "status-unknown"}">${escapeHtml(humanSeconds(item.weighted_seconds))} · ${contribution}%</span>
          </div>
        `;
      }).join("");
  return `
    <section class="card index-explain-card">
      <div class="section-head">
        <div>
          <h3>Почему такой индекс?</h3>
          <p class="muted">${escapeHtml(policy.explanation || "Индекс = взвешенное время приложений / плановое время роли.")}</p>
        </div>
        <span class="badge ${statusClass(workforceIndexStatus(policy.index))}">${escapeHtml(workforceIndexTextFromValue(policy.index))}</span>
      </div>
      <div class="index-metrics">
        <div><span class="muted">Роль</span><strong>${escapeHtml(policy.role_label || policy.role || "-")}</strong></div>
        <div><span class="muted">План</span><strong>${escapeHtml(humanSeconds(policy.planned_seconds))}</strong></div>
        <div><span class="muted">App time</span><strong>${escapeHtml(humanSeconds(policy.app_seconds))}</strong></div>
        <div><span class="muted">Weighted</span><strong>${escapeHtml(humanSeconds(policy.weighted_seconds))}</strong></div>
      </div>
      <div class="list compact-list app-weight-list">${appRows}</div>
    </section>
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
    <h2 class="section-title">Владелец</h2>
    <div class="summary-grid">${cards.map(([name, block]) => `
      <article class="card">
        <span class="badge ${statusClass(block.status)}">${escapeHtml(block.status)}</span>
        <h3>${escapeHtml(label(name))}</h3>
        <p class="muted">${escapeHtml(block.text)}</p>
      </article>
    `).join("")}</div>
    <section class="card">
      <h3>Что сделать</h3>
      <div class="list">${(data.recommendations || []).map(item => `<div class="row"><strong>Рекомендация</strong><span class="muted">${escapeHtml(item)}</span><span></span></div>`).join("")}</div>
    </section>
    <section class="card">
      <h3>Переходы</h3>
      ${renderLinks(data.links)}
    </section>
  `;
}

function renderIncidentsList(items) {
  if (!items || items.length === 0) return `<p class="muted">Активных проблем нет.</p>`;
  return `<div class="list">${items.map(item => `
    <div class="row incident-row">
      <div>
        <strong>${escapeHtml(item.source)}</strong>
        <div class="muted small">${escapeHtml(item.kind)} · ${escapeHtml(item.id)}</div>
      </div>
      <div>
        <span class="muted">${escapeHtml(item.summary)}</span>
        ${item.acknowledged ? `<div class="muted small">В работе: ${escapeHtml(item.assigned_to || item.actor || "оператор")} · ${escapeHtml(item.comment || "")}</div>` : ""}
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
  if (dlpItems.length === 0) return `<p class="muted">Активных DLP/ИБ-инцидентов нет.</p>`;
  return renderIncidentsList(dlpItems);
}

function renderDlpEvidence(evidence) {
  if (!evidence) return `<p class="muted">Данные evidence загружаются.</p>`;
  if (!evidence.ok) return `<p class="muted">Evidence недоступны: ${escapeHtml(evidence.error || "ошибка чтения")}</p>`;
  const items = evidence.items || [];
  if (items.length === 0) return `<p class="muted">DLP evidence пока не найдены.</p>`;
  return `<div class="list evidence-list">${items.map(item => `
    <div class="row evidence-row">
      <div>
        <strong>${escapeHtml(item.signal_type || item.source || item.stream_type)}</strong>
        <div class="muted small">${escapeHtml(item.event_ts)} · ${escapeHtml(item.hostname)}${item.username ? " · " + escapeHtml(item.username) : ""}</div>
      </div>
      <div>
        <span class="muted">${escapeHtml(item.message || item.file_path || item.rule_id || item.event_id)}</span>
        <div class="muted small">${item.source_file ? "Файл: " + escapeHtml(item.source_file) + " · " : ""}${item.screenshot_sha256 ? "SHA-256: " + escapeHtml(item.screenshot_sha256) : escapeHtml(item.blocked_reason || "без скрина")}</div>
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
    <h2 class="section-title">Инциденты ИБ</h2>
    <div class="grid-2">
      <section class="card">
        <h3>DLP-инциденты</h3>
        ${renderDlpIncidentsList(incidents)}
      </section>
      <section class="card">
        <h3>Графики и дашборды</h3>
        ${renderDlpLinks(links)}
      </section>
    </div>
    <section class="card evidence-card">
      <h3>Доказательства</h3>
      ${renderDlpEvidence(evidence)}
    </section>
  `;
}

function renderKpiCards(items) {
  return `<div class="summary-grid kpi-grid">${(items || []).map(item => `
    <article class="card kpi-card">
      <span class="badge ${statusClass(item.status)}">${escapeHtml(item.status || "INFO")}</span>
      <h3>${escapeHtml(item.label)}</h3>
      <p class="kpi-value">${escapeHtml(item.value)}</p>
      <p class="muted">${escapeHtml(item.context || "")}</p>
    </article>
  `).join("")}</div>`;
}

function renderReportSections(sections) {
  return `<div class="grid-2">${(sections || []).map(section => `
    <section class="card report-section">
      <h3>${escapeHtml(section.title)}</h3>
      <div class="list compact-list">${(section.items || []).map(item => `
        <div class="row compact-row">
          <strong>${escapeHtml(item.label)}</strong>
          <span class="muted">${escapeHtml(item.value)}</span>
          <span class="badge ${statusClass(item.status)}">${escapeHtml(item.status || "INFO")}</span>
        </div>
      `).join("")}</div>
    </section>
  `).join("")}</div>`;
}

function renderReports(data) {
  return `
    <h2 class="section-title">Отчеты</h2>
    <div class="grid-2">
      <section class="card report-hero">
        <span class="badge ${statusClass(data.severity)}">${escapeHtml(data.severity)}</span>
        <h3>${escapeHtml(data.headline)}</h3>
        <p class="muted">${escapeHtml(data.period || "")} · обновлено ${escapeHtml(data.generated_at_utc || "")}</p>
      </section>
      <section class="card">
        <h3>Для руководителя</h3>
        <div class="list compact-list">${(data.executive_points || []).map(point => `
          <div class="row compact-row"><strong>Итог</strong><span class="muted">${escapeHtml(point)}</span><span></span></div>
        `).join("")}</div>
      </section>
    </div>
    <h3 class="section-title">Ключевые показатели</h3>
    ${renderKpiCards(data.kpis)}
    ${renderWorkforceIndexExplanation(data.workforce_policy)}
    <h3 class="section-title">Срезы отчета</h3>
    ${renderReportSections(data.sections)}
    <section class="card markdown-card">
      <h3>Markdown для отчета</h3>
      <pre>${escapeHtml(data.markdown || "")}</pre>
    </section>
  `;
}

async function refresh() {
  if (!state.links) state.links = await loadJson("/links");
  const summary = await loadJson("/summary");
  renderSummary(summary);
  const content = document.getElementById("content");
  const data = await loadJson(`/${state.tab}`);
  if (state.tab === "operator") content.innerHTML = renderOperator(data);
  if (state.tab === "manager") {
    const reports = await loadJson("/reports").catch(() => null);
    content.innerHTML = renderManager(data, reports);
  }
  if (state.tab === "owner") content.innerHTML = renderOwner(data);
  if (state.tab === "incidents") {
    const evidence = await loadJson("/dlp/evidence").catch(error => ({ ok: false, error: error.message, items: [] }));
    content.innerHTML = renderIncidents({ incidents: data, evidence });
  }
  if (state.tab === "reports") content.innerHTML = renderReports(data);
}

function setTab(tab) {
  state.tab = tab;
  document.querySelectorAll(".tab").forEach(btn => {
    btn.classList.toggle("is-active", btn.dataset.tab === tab);
  });
  document.getElementById("content").innerHTML = `<p class="muted">Загрузка...</p>`;
  refresh().catch(showError);
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

refresh().catch(showError);
setInterval(() => refresh().catch(showError), 60000);
