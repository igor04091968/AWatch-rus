#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const validationDocs = [
  "docs/PILOT_VALIDATION_CHECKLIST_RU.md",
  "docs/PILOT_GAP_ANALYSIS_RU.md",
  "docs/CUSTOMER_DISCOVERY_QUESTIONS_RU.md",
  "docs/PILOT_SUCCESS_CRITERIA_RU.md",
  "docs/COMPETITIVE_POSITIONING_RU.md",
];

const demoDocs = [
  "docs/PILOT_DEMO_SCENARIO_RU.md",
  "docs/DEMO_RUNBOOK_RU.md",
  "docs/DEMO_REPORT_EXAMPLE_RU.md",
  "docs/PILOT_VALUE_PROPOSITION_RU.md",
  "docs/demo/DEMO_SCENARIO_EXECUTIVE_RU.md",
  "docs/demo/DEMO_SCENARIO_SECURITY_RU.md",
  "docs/demo/DEMO_SCENARIO_FORENSICS_RU.md",
  "docs/demo/DEMO_PACK_ACCEPTANCE_CHECKLIST_RU.md",
  "docs/fixtures/pilot-v1-demo/README_RU.md",
  "docs/fixtures/pilot-v1-demo/demo-seed-data.json",
];

const registryDocs = [
  "docs/REGISTRY_PRODUCT_PASSPORT_RU.md",
  "docs/REGISTRY_ARCHITECTURE_RU.md",
  "docs/REGISTRY_FUNCTIONAL_SCOPE_RU.md",
  "docs/REGISTRY_DEPENDENCY_STATEMENT_RU.md",
  "docs/REGISTRY_DEPLOYMENT_MODEL_RU.md",
  "docs/REGISTRY_COMMERCIAL_POSITIONING_RU.md",
  "docs/REGISTRY_READINESS_CHECKLIST_RU.md",
];

const deploymentDocs = [
  "docs/ENTERPRISE_DEPLOYMENT_GUIDE_RU.md",
  "docs/DEPLOYMENT_TOPOLOGIES_RU.md",
  "docs/SIZING_GUIDE_RU.md",
  "docs/BACKUP_AND_RECOVERY_RU.md",
  "docs/OPERATIONS_RUNBOOK_RU.md",
  "docs/SECURITY_HARDENING_RU.md",
  "docs/ENTERPRISE_ACCEPTANCE_CHECKLIST_RU.md",
];

const screenshots = [
  "docs/screenshots/01-executive-overview.png",
  "docs/screenshots/02-risk-heatmap.png",
  "docs/screenshots/03-security-view.png",
  "docs/screenshots/04-operations-view.png",
  "docs/screenshots/05-investigation-pack.png",
  "docs/screenshots/06-markdown-report.png",
  "docs/screenshots/07-product-architecture.png",
];

const roadmapDocs = [
  "docs/roadmap/README.md",
  "docs/roadmap/TASK_001_PILOT_V1_STABILIZATION.md",
  "docs/roadmap/TASK_002_PRODUCTION_HARDENING.md",
  "docs/roadmap/TASK_003_EXPLAINABLE_KPI.md",
  "docs/roadmap/TASK_003A_PORTAL_HARDENING_CLEANUP.md",
  "docs/roadmap/TASK_004_RISK_NARRATIVE.md",
  "docs/roadmap/TASK_005_RUST_AGENT_BASELINE.md",
  "docs/roadmap/TASK_006_EXECUTIVE_ACTION_CENTER.md",
  "docs/roadmap/TASK_007_CUSTOMER_DEMO_PACK.md",
  "docs/roadmap/TASK_008_REGISTRY_READINESS.md",
  "docs/roadmap/TASK_009_ENTERPRISE_DEPLOYMENT_GUIDE.md",
  "docs/roadmap/TASK_010_PILOT_VALIDATION.md",
];

const reportsAndRunbooks = [
  "docs/DEMO_REPORT_EXAMPLE_RU.md",
  "docs/PRODUCTION_INCIDENT_REPORT_2026-06-07_RU.md",
  "docs/OPERATIONS_RUNBOOK_RU.md",
  "docs/OPERATIONS_RUNBOOK_WORKTIME_RU.md",
  "docs/BACKUP_AND_RECOVERY_RU.md",
  "docs/DEMO_RUNBOOK_RU.md",
];

function readText(file) {
  return fs.readFileSync(path.join(root, file), "utf8");
}

function exists(file) {
  return fs.existsSync(path.join(root, file));
}

function checkFiles(files) {
  return files.filter((file) => !exists(file));
}

function checkScreenshots(files) {
  const pngSignature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const findings = [];
  for (const file of files) {
    const fullPath = path.join(root, file);
    if (!fs.existsSync(fullPath)) {
      findings.push({ file, reason: "missing" });
      continue;
    }
    const data = fs.readFileSync(fullPath);
    if (data.length <= 1000 || !data.subarray(0, 8).equals(pngSignature)) {
      findings.push({ file, reason: "not_png_or_too_small", size: data.length });
    }
  }
  return findings;
}

function checkMarkdownLinks(files) {
  const findings = [];
  const linkPattern = /!?\[[^\]]+\]\(([^)]+)\)/g;
  for (const file of files) {
    if (!file.endsWith(".md")) continue;
    let match = null;
    const text = readText(file);
    while ((match = linkPattern.exec(text)) !== null) {
      const raw = String(match[1] || "").trim();
      const target = raw.split(/\s+/)[0].replace(/^<|>$/g, "");
      if (
        !target
        || target.startsWith("#")
        || target.startsWith("http://")
        || target.startsWith("https://")
        || target.startsWith("mailto:")
      ) {
        continue;
      }
      const withoutAnchor = target.split("#", 1)[0];
      if (!withoutAnchor) continue;
      const resolved = path.resolve(root, path.dirname(file), withoutAnchor);
      if (!fs.existsSync(resolved)) {
        findings.push({ file, target });
      }
    }
  }
  return findings;
}

function checkRequiredContent() {
  const checks = [
    {
      file: "docs/PILOT_VALIDATION_CHECKLIST_RU.md",
      markers: ["Executive", "Workforce", "Security", "Forensics", "Agent", "Reporting"],
    },
    {
      file: "docs/PILOT_GAP_ANALYSIS_RU.md",
      markers: ["Что готово", "Что требует доработки", "Что не входит в пилот", "Что отложено на roadmap"],
    },
    {
      file: "docs/CUSTOMER_DISCOVERY_QUESTIONS_RU.md",
      markers: ["Директор", "Руководитель подразделения", "ИБ", "ИТ", "Эксплуатация"],
    },
    {
      file: "docs/PILOT_SUCCESS_CRITERIA_RU.md",
      markers: ["Критерии успеха", "Критерии провала", "KPI пилота", "30 дней"],
    },
    {
      file: "docs/COMPETITIVE_POSITIONING_RU.md",
      markers: ["ActivityWatch", "Стахановец", "StaffCop", "SearchInform", "InfoWatch"],
    },
  ];
  const findings = [];
  for (const check of checks) {
    const text = readText(check.file);
    for (const marker of check.markers) {
      if (!text.includes(marker)) {
        findings.push({ file: check.file, marker });
      }
    }
  }
  return findings;
}

function checkSensitiveStrings(files) {
  const patterns = [
    { name: "private_network_10", re: /10\.10\.10\./ },
    { name: "private_network_192", re: /192\.168\./ },
    { name: "private_network_172", re: /172\.(1[6-9]|2\d|3[0-1])\./ },
    { name: "private_operator_domain", re: /dm\.iri/i },
    { name: "customer_codename", re: new RegExp(`${["Det", "Mir"].join("")}|${["SHARKON", "2025"].join("")}`, "i") },
    { name: "local_operator_path", re: /\/home\/igor/i },
    { name: "mts_phishing_context", re: /\b(lk|l)\.mts\.ru/i },
  ];
  const findings = [];
  for (const file of files) {
    if (!file.endsWith(".md") && !file.endsWith(".json")) continue;
    const text = readText(file);
    for (const pattern of patterns) {
      if (pattern.re.test(text)) {
        findings.push({ file, pattern: pattern.name });
      }
    }
  }
  return findings;
}

function pass(checks, name, ok, details = {}) {
  checks.push({ name, ok: Boolean(ok), ...details });
}

function main() {
  const checks = [];
  const docsToScan = [
    "README.md",
    ...validationDocs,
    ...demoDocs,
    ...registryDocs,
    ...deploymentDocs,
    ...roadmapDocs,
    ...reportsAndRunbooks,
  ];
  const sensitiveScanFiles = [
    ...validationDocs,
    "docs/fixtures/pilot-v1-demo/README_RU.md",
    "docs/fixtures/pilot-v1-demo/demo-seed-data.json",
  ];

  pass(checks, "validation_docs_exist", checkFiles(validationDocs).length === 0, {
    missing: checkFiles(validationDocs),
  });
  pass(checks, "demo_docs_exist", checkFiles(demoDocs).length === 0, {
    missing: checkFiles(demoDocs),
  });
  pass(checks, "registry_docs_exist", checkFiles(registryDocs).length === 0, {
    missing: checkFiles(registryDocs),
  });
  pass(checks, "deployment_docs_exist", checkFiles(deploymentDocs).length === 0, {
    missing: checkFiles(deploymentDocs),
  });
  pass(checks, "roadmap_exists", checkFiles(roadmapDocs).length === 0, {
    missing: checkFiles(roadmapDocs),
  });
  pass(checks, "reports_and_runbooks_exist", checkFiles(reportsAndRunbooks).length === 0, {
    missing: checkFiles(reportsAndRunbooks),
  });

  const screenshotFindings = checkScreenshots(screenshots);
  pass(checks, "screenshots_exist_and_are_png", screenshotFindings.length === 0, {
    findings: screenshotFindings,
  });

  const requiredContentFindings = checkRequiredContent();
  pass(checks, "validation_docs_have_required_sections", requiredContentFindings.length === 0, {
    findings: requiredContentFindings,
  });

  const linkFindings = checkMarkdownLinks(docsToScan);
  pass(checks, "markdown_links_valid", linkFindings.length === 0, {
    findings: linkFindings,
  });

  const sensitiveFindings = checkSensitiveStrings(sensitiveScanFiles);
  pass(checks, "sensitive_string_scan", sensitiveFindings.length === 0, {
    findings: sensitiveFindings,
  });

  const ok = checks.every((check) => check.ok);
  process.stdout.write(`${JSON.stringify({
    ok,
    generated_at_utc: new Date().toISOString(),
    checks,
  }, null, 2)}\n`);
  process.exitCode = ok ? 0 : 2;
}

main();
