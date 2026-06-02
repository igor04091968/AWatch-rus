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
  return `<div class="links">
    <a class="button" href="${links.detmir_activitywatch}">DetMir ActivityWatch</a>
    <a class="button" href="${links.grafana_dashboards}">Grafana</a>
    <a class="button" href="${links.aw_ui}">AW UI</a>
    <a class="button" href="${links.worktime_report}">Worktime</a>
    <a class="button" href="${links.file1c_brief}">1C brief</a>
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

function renderManager(data) {
  return `
    <h2 class="section-title">Руководитель</h2>
    <div class="grid-2">
      <section class="card">
        <h3>Работа сегодня</h3>
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
    <div class="row">
      <strong>${escapeHtml(item.source)}</strong>
      <span class="muted">${escapeHtml(item.summary)}</span>
      <span class="badge ${statusClass(item.status)}">${escapeHtml(item.status)}</span>
    </div>
  `).join("")}</div>`;
}

function renderIncidents(data) {
  return `
    <h2 class="section-title">Инциденты</h2>
    ${renderIncidentsList(data)}
  `;
}

async function refresh() {
  const summary = await loadJson("/summary");
  renderSummary(summary);
  const content = document.getElementById("content");
  const data = await loadJson(`/${state.tab}`);
  if (state.tab === "operator") content.innerHTML = renderOperator(data);
  if (state.tab === "manager") content.innerHTML = renderManager(data);
  if (state.tab === "owner") content.innerHTML = renderOwner(data);
  if (state.tab === "incidents") content.innerHTML = renderIncidents(data);
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

refresh().catch(showError);
setInterval(() => refresh().catch(showError), 60000);
