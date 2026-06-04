const state = {
  reports: null,
};

const $ = (id) => document.getElementById(id);

function setStatus(kind, text) {
  const box = $("status");
  box.className = `status ${kind}`;
  box.textContent = text;
}

function text(value, fallback = "-") {
  if (value === null || value === undefined || value === "") return fallback;
  return String(value);
}

function badge(level) {
  const value = text(level, "UNKNOWN").toUpperCase();
  return `<span class="badge ${value.toLowerCase()}">${value}</span>`;
}

function esc(value) {
  return text(value, "").replace(/[&<>"']/g, (ch) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;",
    "'": "&#39;",
  }[ch]));
}

async function loadReports() {
  setStatus("loading", "Загрузка данных");
  const response = await fetch("api/reports", { cache: "no-store" });
  if (!response.ok) throw new Error(`API вернул ${response.status}`);
  state.reports = await response.json();
  render(state.reports);
  setStatus("ready", `Данные готовы · ${new Date().toLocaleTimeString("ru-RU")}`);
}

function render(report) {
  const summary = report.executive_dashboard?.summary || {};
  const security = report.security_events_summary || {};
  const coverage = report.agent_coverage_sla || {};
  const cases = report.executive_dashboard?.open_cases ?? 0;
  $("mainRisk").textContent = text(summary.main_risk_cause, "Связанный риск не выражен");
  $("mainRiskText").textContent = text(summary.main_data_gap, "Критичных пробелов данных не найдено");
  $("securityEvents").textContent = text(security.events_24h, "0");
  $("securityStatus").textContent = security.status === "ok"
    ? "События безопасности доступны"
    : "Источник событий безопасности не активен";
  $("casesCount").textContent = text(cases, "0");
  $("coveragePct").textContent = coverage.coverage_pct === undefined ? "-" : `${coverage.coverage_pct}%`;
  $("coverageStatus").textContent = text(coverage.sla_status, "статус не рассчитан");

  renderBusinessRisk(report.business_risk || []);
  renderCandidates(report.risk_incident_candidates || []);
  renderCorrelation(report.security_correlation || []);
}

function renderBusinessRisk(items) {
  const box = $("businessRisk");
  const rows = items.slice(0, 5);
  if (!rows.length) {
    box.innerHTML = `<p class="muted">Риски подразделений не выявлены.</p>`;
    return;
  }
  box.innerHTML = rows.map((item) => `
    <div class="row">
      <div>
        <strong>${esc(item.department)}</strong>
        <small>${esc((item.reasons || []).join("; ") || "причина не указана")}</small>
      </div>
      ${badge(item.risk_level)}
    </div>
  `).join("");
}

function renderCandidates(items) {
  const box = $("candidates");
  const rows = items.slice(0, 5);
  if (!rows.length) {
    box.innerHTML = `<p class="muted">Кандидатов на проверку сейчас нет.</p>`;
    return;
  }
  box.innerHTML = rows.map((item) => `
    <div class="row">
      <div>
        <strong>${esc(item.department || item.hostname || item.id)}</strong>
        <small>${esc(item.reason || item.recommendation || "требует проверки")}</small>
      </div>
      ${badge(item.risk_level)}
    </div>
  `).join("");
}

function renderCorrelation(items) {
  const box = $("correlation");
  const rows = items.slice(0, 8);
  if (!rows.length) {
    box.innerHTML = `<p class="muted">Связь рисков и активности пока не выражена.</p>`;
    return;
  }
  box.innerHTML = `
    <table>
      <thead>
        <tr><th>Подразделение</th><th>Уровень взаимосвязи</th><th>Причина</th><th>Риск</th></tr>
      </thead>
      <tbody>
        ${rows.map((item) => `
          <tr>
            <td>${esc(item.department)}</td>
            <td>${esc(item.correlation_score ?? 0)}/100</td>
            <td>${esc(item.correlation_reason || "нет пояснения")}</td>
            <td>${badge(item.business_risk_level)}</td>
          </tr>
        `).join("")}
      </tbody>
    </table>
  `;
}

$("refreshButton").addEventListener("click", () => {
  loadReports().catch((error) => setStatus("error", `Ошибка получения данных: ${error.message}`));
});

loadReports().catch((error) => setStatus("error", `Ошибка получения данных: ${error.message}`));
