(function () {
  var reportBase = "__AW_WORKTIME_REPORT_BASE__";
  function defaultDayQuery() {
    return "day=today";
  }

  var dayQuery = defaultDayQuery();
  var htmlUrl = reportBase + "/reports/worktime/today?format=html&" + dayQuery;
  var csvUrl = reportBase + "/reports/worktime/today?format=csv&" + dayQuery;
  var jsonUrl = reportBase + "/reports/worktime/today?" + dayQuery;
  var managerHtmlUrl = reportBase + "/reports/worktime/management?format=html&" + dayQuery;
  var managerJsonUrl = reportBase + "/reports/worktime/management?" + dayQuery;
  var existing = document.getElementById("aw-report-links");
  if (!existing) return;

  existing.innerHTML =
    'RDP отчёт: ' +
    '<a href="' + htmlUrl + '" style="color:#fcd34d" target="_blank">HTML</a> | ' +
    '<a href="' + csvUrl + '" style="color:#7dd3fc" target="_blank">CSV</a> | ' +
    '<a href="' + jsonUrl + '" style="color:#86efac" target="_blank">JSON</a> | ' +
    '<a href="' + managerHtmlUrl + '" style="color:#fca5a5" target="_blank">Менеджмент</a> | ' +
    '<a href="' + managerJsonUrl + '" style="color:#c4b5fd" target="_blank">Mgmt JSON</a> | ' +
    '<a href="#" id="aw-report-toggle" style="color:#f9fafb">Панель</a>';

  var panel = document.createElement("div");
  panel.id = "aw-report-panel";
  panel.style.cssText = [
    "position:fixed",
    "top:16px",
    "right:16px",
    "width:min(980px,calc(100vw - 32px))",
    "height:min(760px,calc(100vh - 32px))",
    "background:#fff",
    "border:1px solid rgba(15,23,42,.15)",
    "border-radius:12px",
    "box-shadow:0 24px 80px rgba(15,23,42,.28)",
    "overflow:hidden",
    "z-index:100000",
    "display:none"
  ].join(";");

  panel.innerHTML =
    '<div style="display:flex;align-items:center;justify-content:space-between;padding:10px 14px;background:#0f172a;color:#fff;font:600 13px/1.2 sans-serif">' +
    '<div>Отчёт по работе в RDP</div>' +
    '<div style="display:flex;gap:12px;align-items:center">' +
    '<a href="' + managerHtmlUrl + '" target="_blank" style="color:#fca5a5;text-decoration:none">Открыть менеджмент</a>' +
    '<a href="' + htmlUrl + '" target="_blank" style="color:#93c5fd;text-decoration:none">Открыть RDP</a>' +
    '<a href="#" id="aw-report-close" style="color:#fff;text-decoration:none">Закрыть</a>' +
    "</div></div>" +
    '<iframe src="' + managerHtmlUrl + '" title="Управленческий отчёт по работе в RDP" style="border:0;width:100%;height:calc(100% - 42px);background:#fff"></iframe>';

  document.body.appendChild(panel);

  function openPanel(ev) {
    if (ev) ev.preventDefault();
    panel.style.display = "block";
  }

  function closePanel(ev) {
    if (ev) ev.preventDefault();
    panel.style.display = "none";
  }

  var toggle = document.getElementById("aw-report-toggle");
  if (toggle) toggle.addEventListener("click", openPanel);
  var close = panel.querySelector("#aw-report-close");
  if (close) close.addEventListener("click", closePanel);
})();
