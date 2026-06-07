#!/usr/bin/env node

const baseUrl = (process.env.AWATCH_PORTAL_SMOKE_URL || "http://127.0.0.1:8720").replace(/\/+$/, "");

async function request(path, options = {}) {
  const response = await fetch(`${baseUrl}${path}`, {
    ...options,
    headers: {
      "X-AWatch-Role": "executive",
      "X-Request-Id": "smoke-production-hardening",
      ...(options.headers || {}),
    },
  });
  const text = await response.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    json = null;
  }
  return { response, text, json };
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

async function main() {
  const health = await request("/healthz");
  assert(health.response.status === 200, "/healthz must return 200");
  assert(health.json?.status === "ok", "/healthz must return status=ok");
  assert(
    health.response.headers.get("x-request-id") === "smoke-production-hardening",
    "X-Request-Id must be returned",
  );
  assert(
    health.response.headers.get("x-correlation-id") === "smoke-production-hardening",
    "X-Correlation-Id must be returned",
  );

  const ready = await request("/readyz");
  assert([200, 503].includes(ready.response.status), "/readyz must return controlled status");
  assert(ready.json?.checks && typeof ready.json.checks === "object", "/readyz must return checks JSON");

  const version = await request("/version");
  assert(version.response.status === 200, "/version must return 200");
  assert(version.json?.app_version, "/version must include app_version");
  assert(version.json?.schema_version === "pilot-v1", "/version must include schema_version=pilot-v1");

  const metrics = await request("/metrics");
  assert(metrics.response.status === 200, "/metrics must return 200");
  assert(metrics.text.includes("awatch_http_requests_total"), "/metrics must include HTTP metric");
  assert(metrics.text.includes("awatch_readyz_status"), "/metrics must include readyz gauge");

  const pageTooLarge = await request("/api/reports?page_size=999999&role=executive");
  assert(pageTooLarge.response.status === 400, "too large page_size must be rejected");
  assert(pageTooLarge.json?.error_code === "invalid_page_size", "page_size reject must explain error_code");

  const rangeTooLarge = await request("/api/reports?date_from=2026-01-01&date_to=2026-12-31&role=executive");
  assert(rangeTooLarge.response.status === 400, "too wide report range must be rejected");
  assert(rangeTooLarge.json?.error_code === "report_range_too_large", "range reject must explain error_code");

  const roleDenied = await request("/api/security?role=manager", {
    headers: { "X-AWatch-Role": "manager" },
  });
  assert(roleDenied.response.status === 403, "manager must not access security scope");

  const kpiExplain = await request("/api/workforce/kpi/explain?role=executive");
  assert(kpiExplain.response.status === 200, "KPI explain must return 200");
  assert(typeof kpiExplain.json?.kpi_score === "number", "KPI explain must include kpi_score");
  assert(Array.isArray(kpiExplain.json?.factors), "KPI explain must include factors");
  assert(kpiExplain.json.factors.some((item) => item.name === "productive_activity"), "KPI explain factors must be deterministic");

  const riskNarrative = await request("/api/risk/narrative?role=executive");
  assert(riskNarrative.response.status === 200, "Risk narrative must return 200");
  assert(typeof riskNarrative.json?.risk_score === "number", "Risk narrative must include risk_score");
  assert(["low", "guarded", "medium", "high", "critical"].includes(riskNarrative.json?.risk_level), "Risk narrative must include stable risk_level");
  assert(Array.isArray(riskNarrative.json?.why), "Risk narrative must include why list");
  assert(riskNarrative.json?.model?.type === "rule_based", "Risk narrative must be rule-based");

  const actions = await request("/api/actions?role=executive");
  assert(actions.response.status === 200, "Actions API must return 200");
  assert(Array.isArray(actions.json?.actions), "Actions API must include actions array");
  assert(actions.json?.model?.type === "rule_based", "Actions model must be rule-based");
  assert(actions.json?.model?.auto_remediation === false, "Actions must not enable auto-remediation");

  console.log(JSON.stringify({
    ok: true,
    baseUrl,
    checked: [
      "/healthz",
      "/readyz",
      "/version",
      "/metrics",
      "query limits",
      "role gates",
      "/api/workforce/kpi/explain",
      "/api/risk/narrative",
      "/api/actions",
    ],
  }, null, 2));
}

main().catch((error) => {
  console.error(JSON.stringify({ ok: false, baseUrl, error: error.message }, null, 2));
  process.exit(1);
});
