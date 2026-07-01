#!/usr/bin/env node
/** Guards production rollout doc + adoption evidence template for M4 commercial handoff. */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function read(relative) {
  return readFileSync(path.join(repoRoot, relative), "utf8");
}

const rollout = read("docs/architecture/tech/TECH-24-production-rollout-and-adoption.md");
for (const required of [
  "Pre-flight",
  "Canary",
  "Rollback",
  "scripts/verify",
  "21-operations-runbook",
  "18-owasp-api-top10-mapping",
  "production-adoption.evidence.template.json",
  "login_scope",
  "ORGANIZATION",
  "M4",
]) {
  if (!rollout.includes(required)) {
    console.error(`production-rollout doc must mention: ${required}`);
    process.exit(1);
  }
}

const templatePath = path.join(repoRoot, "specs", "production-adoption.evidence.template.json");
const template = JSON.parse(readFileSync(templatePath, "utf8"));
if (template.kind !== "sdkwork.web-framework.adoption-evidence") {
  console.error("adoption evidence template kind mismatch");
  process.exit(1);
}
if (!Array.isArray(template.adoptions) || template.adoptions.length === 0) {
  console.error("adoption evidence template must include at least one example adoption");
  process.exit(1);
}

const deployments = read("deployments/README.md");
if (!deployments.includes("24-production-rollout-and-adoption")) {
  console.error("deployments/README.md must link production rollout doc");
  process.exit(1);
}

const pathfinderPath = path.join(repoRoot, "specs", "framework-adoption.evidence.json");
const pathfinder = JSON.parse(readFileSync(pathfinderPath, "utf8"));
if (pathfinder.adoptions.length < 2) {
  console.error("framework-adoption.evidence.json must list at least two pathfinder adoptions");
  process.exit(1);
}

console.log("production-rollout.contract: OK");
