#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const contractPath = path.join(
  root,
  "adk-rust/crates/detmir-portal/src/contracts/openapi.json",
);

const requiredPublicPaths = [
  "/api/contracts",
  "/api/contracts/openapi.json",
  "/api/contracts/typescript.d.ts",
  "/api/reports",
  "/api/executive",
  "/api/workforce",
  "/api/security",
  "/api/forensics",
  "/api/ueba",
  "/api/pfsense",
  "/api/incidents",
  "/api/cases",
  "/api/readiness/latest",
  "/api/readiness/bundle",
  "/api/readiness/verify",
];

function fail(message, details = []) {
  console.error(message);
  for (const detail of details) {
    console.error(`- ${detail}`);
  }
  process.exit(1);
}

let contract;
try {
  contract = JSON.parse(fs.readFileSync(contractPath, "utf8"));
} catch (error) {
  fail(`failed to read OpenAPI contract: ${contractPath}`, [error.message]);
}

if (!contract || typeof contract !== "object" || !contract.paths || typeof contract.paths !== "object") {
  fail("OpenAPI contract has no object 'paths' section.");
}

const contractPaths = Object.keys(contract.paths);
const forbiddenPaths = contractPaths.filter((contractPathName) =>
  /dioxus|prototype-mirror|mirror/i.test(contractPathName),
);
if (forbiddenPaths.length > 0) {
  fail("OpenAPI contract contains legacy/prototype paths.", forbiddenPaths);
}

const effectivePublicPaths = new Set();
for (const contractPathName of contractPaths) {
  effectivePublicPaths.add(contractPathName);
  if (contractPathName.startsWith("/") && !contractPathName.startsWith("/api/")) {
    effectivePublicPaths.add(`/api${contractPathName}`);
  }
}

const missingPaths = requiredPublicPaths.filter((requiredPath) => !effectivePublicPaths.has(requiredPath));
if (missingPaths.length > 0) {
  fail("OpenAPI contract is missing required public API paths.", missingPaths);
}

console.log("portal contract sync guard: OK");
