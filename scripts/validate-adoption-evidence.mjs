#!/usr/bin/env node
/** Validates sdkwork.web-framework.adoption-evidence JSON (QUALITY_GATE_SPEC release evidence). */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const fileArg = process.argv[2];
if (!fileArg) {
  console.error("usage: node scripts/validate-adoption-evidence.mjs <path-to-evidence.json>");
  process.exit(2);
}

const evidencePath = path.isAbsolute(fileArg)
  ? fileArg
  : path.join(process.cwd(), fileArg);

let evidence;
try {
  evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
} catch (error) {
  console.error(`adoption evidence parse failed: ${error instanceof Error ? error.message : error}`);
  process.exit(1);
}

if (evidence.schemaVersion !== 1) {
  console.error("adoption evidence schemaVersion must be 1");
  process.exit(1);
}
if (evidence.kind !== "sdkwork.web-framework.adoption-evidence") {
  console.error("adoption evidence kind must be sdkwork.web-framework.adoption-evidence");
  process.exit(1);
}
if (!evidence.framework?.name || !evidence.framework?.version) {
  console.error("adoption evidence.framework must include name and version");
  process.exit(1);
}
if (!Array.isArray(evidence.adoptions) || evidence.adoptions.length === 0) {
  console.error("adoption evidence must include at least one adoption entry");
  process.exit(1);
}

const requiredFields = [
  "productId",
  "frameworkVersion",
  "integrationProfile",
  "resolver",
  "stores",
  "verifyEvidence",
  "productionSince",
  "owner",
];

for (const [index, adoption] of evidence.adoptions.entries()) {
  for (const field of requiredFields) {
    if (adoption[field] === undefined || adoption[field] === null || adoption[field] === "") {
      console.error(`adoption[${index}] missing required field: ${field}`);
      process.exit(1);
    }
  }
  if (typeof adoption.stores !== "object" || adoption.stores === null) {
    console.error(`adoption[${index}] stores must be an object`);
    process.exit(1);
  }
}

console.log(`adoption-evidence: OK (${evidence.adoptions.length} adoption(s))`);
