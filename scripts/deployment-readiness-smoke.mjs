#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const requiredDocs = [
  "docs/ENTERPRISE_DEPLOYMENT_GUIDE_RU.md",
  "docs/DEPLOYMENT_TOPOLOGIES_RU.md",
  "docs/SIZING_GUIDE_RU.md",
  "docs/BACKUP_AND_RECOVERY_RU.md",
  "docs/OPERATIONS_RUNBOOK_RU.md",
  "docs/SECURITY_HARDENING_RU.md",
  "docs/ENTERPRISE_ACCEPTANCE_CHECKLIST_RU.md",
];

const requiredScreenshots = [
  "docs/screenshots/01-executive-overview.png",
  "docs/screenshots/02-risk-heatmap.png",
  "docs/screenshots/03-security-view.png",
  "docs/screenshots/04-operations-view.png",
  "docs/screenshots/05-investigation-pack.png",
  "docs/screenshots/06-markdown-report.png",
  "docs/screenshots/07-product-architecture.png",
];

const requiredRoadmap = [
  "docs/roadmap/TASK_009_ENTERPRISE_DEPLOYMENT_GUIDE.md",
];

const requiredRegistryDocs = [
  "docs/REGISTRY_PRODUCT_PASSPORT_RU.md",
  "docs/REGISTRY_ARCHITECTURE_RU.md",
  "docs/REGISTRY_FUNCTIONAL_SCOPE_RU.md",
  "docs/REGISTRY_DEPENDENCY_STATEMENT_RU.md",
  "docs/REGISTRY_DEPLOYMENT_MODEL_RU.md",
  "docs/REGISTRY_COMMERCIAL_POSITIONING_RU.md",
  "docs/REGISTRY_READINESS_CHECKLIST_RU.md",
];

const requiredDemoDocs = [
  "docs/PILOT_DEMO_SCENARIO_RU.md",
  "docs/DEMO_RUNBOOK_RU.md",
  "docs/DEMO_REPORT_EXAMPLE_RU.md",
  "docs/PILOT_VALUE_PROPOSITION_RU.md",
  "docs/demo/DEMO_SCENARIO_EXECUTIVE_RU.md",
  "docs/demo/DEMO_SCENARIO_SECURITY_RU.md",
  "docs/demo/DEMO_SCENARIO_FORENSICS_RU.md",
  "docs/fixtures/pilot-v1-demo/demo-seed-data.json",
];

function exists(file) {
  return fs.existsSync(path.join(root, file));
}

function fileText(file) {
  return fs.readFileSync(path.join(root, file), "utf8");
}

function checkFiles(files) {
  return files.filter((file) => !exists(file));
}

function checkScreenshots(files) {
  const bad = [];
  for (const file of files) {
    const fullPath = path.join(root, file);
    if (!fs.existsSync(fullPath)) {
      bad.push({ file, reason: "missing" });
      continue;
    }
    const bytes = fs.readFileSync(fullPath);
    const png = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    if (bytes.length <= 1000 || !bytes.subarray(0, 8).equals(png)) {
      bad.push({ file, reason: "not_png_or_too_small", size: bytes.length });
    }
  }
  return bad;
}

function checkMarkdownLinks(files) {
  const findings = [];
  const linkPattern = /!?\[[^\]]+\]\(([^)]+)\)/g;
  for (const file of files) {
    if (!file.endsWith(".md")) continue;
    let match = null;
    const text = fileText(file);
    while ((match = linkPattern.exec(text)) !== null) {
      const raw = String(match[1] || "").trim();
      const target = raw.split(/\s+/)[0].replace(/^<|>$/g, "");
      if (!target || target.startsWith("#") || target.startsWith("http://") || target.startsWith("https://") || target.startsWith("mailto:")) {
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

function checkSensitiveStrings(files) {
  const patterns = [
    /SHARKON2025/i,
    /10\.10\.10\./,
    /192\.168\./,
    /172\.(1[6-9]|2\d|3[0-1])\./,
    /dm\.iri/i,
    /\/home\/igor/i,
    /lk\.mts\.ru/i,
    /l\.mts\.ru/i,
    /[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i,
  ];
  const findings = [];
  for (const file of files) {
    if (!file.endsWith(".md") && !file.endsWith(".json") && !file.endsWith(".mjs")) continue;
    const text = fileText(file);
    for (const pattern of patterns) {
      if (pattern.test(text)) findings.push({ file, pattern: String(pattern) });
    }
  }
  return findings;
}

function pass(checks, name, ok, details = {}) {
  checks.push({ name, ok: Boolean(ok), ...details });
}

function main() {
  const checks = [];
  const allDocs = [
    ...requiredDocs,
    ...requiredRoadmap,
    ...requiredRegistryDocs,
    ...requiredDemoDocs,
    "README.md",
  ];

  const missingDocs = checkFiles(requiredDocs);
  pass(checks, "deployment_docs_exist", missingDocs.length === 0, { missing: missingDocs });

  const missingRoadmap = checkFiles(requiredRoadmap);
  pass(checks, "roadmap_exists", missingRoadmap.length === 0, { missing: missingRoadmap });

  const missingRegistryDocs = checkFiles(requiredRegistryDocs);
  pass(checks, "registry_docs_exist", missingRegistryDocs.length === 0, { missing: missingRegistryDocs });

  const missingDemoDocs = checkFiles(requiredDemoDocs);
  pass(checks, "demo_docs_exist", missingDemoDocs.length === 0, { missing: missingDemoDocs });

  const screenshotProblems = checkScreenshots(requiredScreenshots);
  pass(checks, "screenshots_exist_and_are_png", screenshotProblems.length === 0, { findings: screenshotProblems });

  const linkProblems = checkMarkdownLinks(allDocs);
  pass(checks, "markdown_links_valid", linkProblems.length === 0, { findings: linkProblems });

  const sensitiveFindings = checkSensitiveStrings(allDocs);
  pass(checks, "sensitive_string_scan", sensitiveFindings.length === 0, { findings: sensitiveFindings });

  const ok = checks.every((check) => check.ok);
  process.stdout.write(`${JSON.stringify({
    ok,
    generated_at_utc: new Date().toISOString(),
    checks,
  }, null, 2)}\n`);
  process.exitCode = ok ? 0 : 2;
}

main();
