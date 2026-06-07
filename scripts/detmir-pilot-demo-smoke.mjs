#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

if (["1", "true", "yes"].includes(String(process.env.DETMIR_PORTAL_SMOKE_INSECURE_TLS || "").toLowerCase())) {
  process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
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

function normalizeBaseUrl(raw) {
  const value = raw.endsWith("/") ? raw : `${raw}/`;
  return new URL(value);
}

function contains(text, marker) {
  return String(text || "").toLowerCase().includes(String(marker || "").toLowerCase());
}

function hasOwn(object, key) {
  return Boolean(object && Object.prototype.hasOwnProperty.call(object, key));
}

async function fetchText(baseUrl, relativePath, role = "executive") {
  const url = new URL(relativePath, baseUrl);
  const response = await fetch(url, {
    headers: {
      ...authHeaders(),
      "X-AWatch-Role": role,
      Accept: "text/html,application/json",
    },
  });
  const text = await response.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    // HTML and text error bodies are expected for some checks.
  }
  return { url: url.toString(), status: response.status, ok: response.ok, text, json };
}

function readDemoFiles() {
  const files = [
    "docs/DEMO_RUNBOOK_RU.md",
    "docs/CUSTOMER_DEMO_SCENARIO_RU.md",
    "docs/PILOT_DEMO_SCENARIO_RU.md",
    "docs/DEMO_REPORT_EXAMPLE_RU.md",
    "docs/PILOT_VALUE_PROPOSITION_RU.md",
    "docs/demo/DEMO_SCENARIO_EXECUTIVE_RU.md",
    "docs/demo/DEMO_SCENARIO_SECURITY_RU.md",
    "docs/demo/DEMO_SCENARIO_FORENSICS_RU.md",
    "docs/demo/DEMO_PACK_ACCEPTANCE_CHECKLIST_RU.md",
    "docs/fixtures/pilot-v1-demo/README_RU.md",
    "docs/fixtures/pilot-v1-demo/demo-seed-data.json",
    "docs/fixtures/pilot-v1-demo/evidence-pack/executive-summary.md",
    "docs/fixtures/pilot-v1-demo/evidence-pack/security-technical-summary.md",
    "docs/fixtures/pilot-v1-demo/evidence-pack/investigation-report.md",
    "docs/fixtures/pilot-v1-demo/evidence-pack/investigation-contract.json",
  ];
  return files.map((file) => ({
    file,
    text: fs.readFileSync(path.join(root, file), "utf8"),
  }));
}

function validateDemoDataset() {
  const file = "docs/fixtures/pilot-v1-demo/demo-seed-data.json";
  const data = JSON.parse(fs.readFileSync(path.join(root, file), "utf8"));
  const scenarios = new Set((data.scenario_coverage || []).map((item) => item.scenario));
  const required = [
    "normal_work",
    "activity_drop",
    "remote_session_growth",
    "elevated_ueba",
    "incident_candidate",
    "low_agent_coverage",
  ];
  const missing = required.filter((name) => !scenarios.has(name));
  return {
    ok: data.demo_only === true
      && data.privacy?.real_personal_data === false
      && data.privacy?.real_customer_hosts === false
      && data.privacy?.real_customer_networks === false
      && data.privacy?.real_domains === false
      && data.privacy?.real_logins === false
      && data.privacy?.personal_data === false
      && missing.length === 0,
    missing,
    scenarios: [...scenarios].sort(),
  };
}

function validateDemoScreenshots() {
  const files = [
    "docs/screenshots/01-executive-overview.png",
    "docs/screenshots/02-risk-heatmap.png",
    "docs/screenshots/03-security-view.png",
    "docs/screenshots/04-operations-view.png",
    "docs/screenshots/05-investigation-pack.png",
    "docs/screenshots/06-markdown-report.png",
    "docs/screenshots/07-product-architecture.png",
  ];
  const missing = [];
  for (const file of files) {
    const fullPath = path.join(root, file);
    if (!fs.existsSync(fullPath)) {
      missing.push({ file, reason: "missing" });
      continue;
    }
    const bytes = fs.readFileSync(fullPath);
    if (bytes.length <= 1000 || !bytes.subarray(0, 8).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))) {
      missing.push({ file, reason: "not_png_or_too_small", size: bytes.length });
    }
  }
  return { ok: missing.length === 0, missing, count: files.length };
}

function validateDemoMarkdownLinks() {
  const findings = [];
  const linkPattern = /!?\[[^\]]+\]\(([^)]+)\)/g;
  for (const item of readDemoFiles()) {
    let match = null;
    while ((match = linkPattern.exec(item.text)) !== null) {
      const raw = String(match[1] || "").trim();
      const target = raw.split(/\s+/)[0].replace(/^<|>$/g, "");
      if (!target || target.startsWith("#") || target.startsWith("http://") || target.startsWith("https://") || target.startsWith("mailto:")) {
        continue;
      }
      const withoutAnchor = target.split("#", 1)[0];
      if (!withoutAnchor) continue;
      const resolved = path.resolve(root, path.dirname(item.file), withoutAnchor);
      if (!fs.existsSync(resolved)) {
        findings.push({ file: item.file, target });
      }
    }
  }
  return { ok: findings.length === 0, findings };
}

function scanDemoFiles() {
  const forbidden = [
    /10\.10\.\d+\.\d+/,
    /192\.168\.\d+\.\d+/,
    /172\.(1[6-9]|2\d|3[0-1])\.\d+\.\d+/,
    new RegExp(["dm", "\\.iri"].join(""), "i"),
    new RegExp(["shar", "kon"].join(""), "i"),
    new RegExp(["/home/", "igor"].join(""), "i"),
    /[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i,
    new RegExp(["Det", "Mir"].join("")),
  ];
  const findings = [];
  for (const item of readDemoFiles()) {
    for (const pattern of forbidden) {
      if (pattern.test(item.text)) {
        findings.push({ file: item.file, pattern: String(pattern) });
      }
    }
  }
  return findings;
}

function pass(checks, name, ok, details = {}) {
  checks.push({ name, ok: Boolean(ok), ...details });
}

async function main() {
  const baseUrl = normalizeBaseUrl(env("DETMIR_PORTAL_SMOKE_URL", "http://127.0.0.1:8720/portal/"));
  const checks = [];

  const dataset = validateDemoDataset();
  pass(checks, "demo_dataset_loads_and_covers_required_scenarios", dataset.ok, dataset);
  const screenshots = validateDemoScreenshots();
  pass(checks, "demo_screenshots_exist_and_are_png", screenshots.ok, screenshots);
  const markdownLinks = validateDemoMarkdownLinks();
  pass(checks, "demo_markdown_links_valid", markdownLinks.ok, markdownLinks);

  const html = await fetchText(baseUrl, "", "executive");
  pass(checks, "portal_html_available", html.ok && contains(html.text, "AWatch-rus"), {
    status: html.status,
    url: html.url,
  });
  for (const marker of [
    "Pilot v1 demo",
    "Executive demo",
    "Manager demo",
    "Security demo",
    "Forensics demo",
    "Admin demo",
  ]) {
    pass(checks, `demo_navigation:${marker}`, contains(html.text, marker), { marker });
  }

  const executiveReports = await fetchText(baseUrl, "api/reports?role=executive", "executive");
  pass(checks, "executive_reports_ok", executiveReports.ok, { status: executiveReports.status });
  pass(
    checks,
    "executive_no_forensics_only_data",
    executiveReports.ok
      && !hasOwn(executiveReports.json, "forensics")
      && !hasOwn(executiveReports.json, "risk_incident_candidates")
      && !hasOwn(executiveReports.json, "incident_review_audit_summary")
      && !hasOwn(executiveReports.json, "security_correlation"),
  );
  const executiveForensics = await fetchText(baseUrl, "api/forensics?role=executive", "executive");
  pass(checks, "executive_forensics_forbidden", executiveForensics.status === 403, {
    status: executiveForensics.status,
  });

  const managerReports = await fetchText(baseUrl, "api/reports?role=manager", "manager");
  pass(
    checks,
    "manager_no_security_only_data",
    managerReports.ok
      && !hasOwn(managerReports.json, "ueba_risk")
      && !hasOwn(managerReports.json, "security_events_summary")
      && !hasOwn(managerReports.json, "risk_incident_candidates")
      && !hasOwn(managerReports.json, "forensics"),
    { status: managerReports.status },
  );
  const managerSecurity = await fetchText(baseUrl, "api/security?role=manager", "manager");
  pass(checks, "manager_security_forbidden", managerSecurity.status === 403, {
    status: managerSecurity.status,
  });

  const securityWorkforce = await fetchText(baseUrl, "api/workforce?role=security", "security");
  pass(checks, "security_workforce_forbidden", securityWorkforce.status === 403, {
    status: securityWorkforce.status,
  });
  const securityPayload = await fetchText(baseUrl, "api/security?role=security", "security");
  pass(
    checks,
    "security_payload_ok",
    securityPayload.ok
      && hasOwn(securityPayload.json, "ueba_risk")
      && hasOwn(securityPayload.json, "security_events_summary")
      && !hasOwn(securityPayload.json, "workforce"),
    { status: securityPayload.status },
  );
  const uebaPayload = await fetchText(baseUrl, "api/ueba?role=security", "security");
  pass(
    checks,
    "security_ueba_contract_ok",
    uebaPayload.ok
      && hasOwn(uebaPayload.json, "score")
      && hasOwn(uebaPayload.json, "severity")
      && uebaPayload.json?.model?.type === "rule_based"
      && uebaPayload.json?.model?.ml_used === false
      && uebaPayload.json?.model?.llm_used === false,
    { status: uebaPayload.status },
  );
  const pfsensePayload = await fetchText(baseUrl, "api/pfsense?role=security", "security");
  pass(
    checks,
    "security_pfsense_readiness_contract_only",
    pfsensePayload.ok
      && pfsensePayload.json?.status === "contract_only"
      && pfsensePayload.json?.siem === false
      && pfsensePayload.json?.ingestion_available === false,
    { status: pfsensePayload.status },
  );

  const forensicsPayload = await fetchText(baseUrl, "api/forensics?role=forensics", "forensics");
  pass(
    checks,
    "forensics_contract_ok",
    forensicsPayload.ok
      && forensicsPayload.json?.forensics?.contract_version === "forensics-v1"
      && Array.isArray(forensicsPayload.json?.forensics?.investigations),
    { status: forensicsPayload.status },
  );
  const evidencePayload = await fetchText(baseUrl, "api/dlp/evidence?role=forensics", "forensics");
  pass(checks, "forensics_evidence_endpoint_ok", evidencePayload.ok && Boolean(evidencePayload.json), {
    status: evidencePayload.status,
  });

  const adminReports = await fetchText(baseUrl, "api/reports?role=admin", "admin");
  pass(
    checks,
    "admin_technical_readiness_visible",
    adminReports.ok
      && adminReports.json?.role_context?.role === "admin"
      && (
        hasOwn(adminReports.json, "agent_quality")
        || hasOwn(adminReports.json, "agent_coverage_sla")
        || hasOwn(adminReports.json, "security_events_summary")
        || hasOwn(adminReports.json, "operator_ok")
      ),
    { status: adminReports.status },
  );
  const readinessPayload = await fetchText(baseUrl, "api/readiness/latest?role=admin", "admin");
  pass(checks, "admin_readiness_endpoint_ok", readinessPayload.ok && Boolean(readinessPayload.json), {
    status: readinessPayload.status,
  });

  const sensitivityFindings = scanDemoFiles();
  pass(checks, "demo_pack_sensitive_string_scan", sensitivityFindings.length === 0, {
    findings: sensitivityFindings,
  });

  const ok = checks.every((check) => check.ok);
  process.stdout.write(`${JSON.stringify({
    ok,
    generated_at_utc: new Date().toISOString(),
    url: baseUrl.toString(),
    checks,
  }, null, 2)}\n`);
  process.exitCode = ok ? 0 : 2;
}

main().catch((error) => {
  process.stdout.write(`${JSON.stringify({
    ok: false,
    error: error.message,
    stack: error.stack,
  }, null, 2)}\n`);
  process.exitCode = 2;
});
