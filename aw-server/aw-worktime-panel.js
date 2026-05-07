(function () {
  var reportBase = "__AW_WORKTIME_REPORT_BASE__";
  var reportUrl = reportBase + "/reports/worktime/today?format=html";
  var existing = document.getElementById("aw-report-links");
  if (!existing) return;

  existing.innerHTML =
    'RDP report: ' +
    '<a href="' + reportUrl + '" style="color:#fcd34d" target="_blank">HTML</a> | ' +
    '<a href="' + reportBase + '/reports/worktime/today?format=csv" style="color:#7dd3fc" target="_blank">CSV</a> | ' +
    '<a href="' + reportBase + '/reports/worktime/today" style="color:#86efac" target="_blank">JSON</a> | ' +
    '<a href="#" id="aw-report-toggle" style="color:#f9fafb">Panel</a>';

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
    '<div>RDP Worktime Report</div>' +
    '<div style="display:flex;gap:12px;align-items:center">' +
    '<a href="' + reportUrl + '" target="_blank" style="color:#93c5fd;text-decoration:none">Open</a>' +
    '<a href="#" id="aw-report-close" style="color:#fff;text-decoration:none">Close</a>' +
    "</div></div>" +
    '<iframe src="' + reportUrl + '" title="RDP Worktime Report" style="border:0;width:100%;height:calc(100% - 42px);background:#fff"></iframe>';

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
