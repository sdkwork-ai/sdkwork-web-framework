#!/usr/bin/env node
/** Ensures release evidence collector produces a valid QUALITY_GATE bundle. */

import { readFileSync, existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

const collect = spawnSync("node", [path.join(repoRoot, "scripts", "collect-release-evidence.mjs")], {
  cwd: repoRoot,
  encoding: "utf8",
  shell: process.platform === "win32",
});
if (collect.status !== 0) {
  console.error(collect.stdout);
  console.error(collect.stderr);
  process.exit(collect.status ?? 1);
}

const outputPath = path.join(repoRoot, "target", "release-evidence", "release-evidence.json");
if (!existsSync(outputPath)) {
  console.error("release-evidence.json was not written");
  process.exit(1);
}

const bundle = JSON.parse(readFileSync(outputPath, "utf8"));
if (bundle.kind !== "sdkwork.web-framework.release-evidence") {
  console.error("release evidence kind mismatch");
  process.exit(1);
}
if (!bundle.gitCommit || bundle.gitCommit === "unknown") {
  console.warn("release-evidence: git commit unavailable (non-git checkout?)");
}
if (!Array.isArray(bundle.verification?.commands) || bundle.verification.commands.length === 0) {
  console.error("release evidence must list verification.commands");
  process.exit(1);
}
for (const key of [
  "changelog",
  "rolloutDoc",
  "adoptionTemplate",
  "frameworkPathfinderAdoptions",
  "workflowManifest",
]) {
  if (!bundle.releaseArtifacts?.[key]) {
    console.error(`release evidence missing releaseArtifacts.${key}`);
    process.exit(1);
  }
}

console.log("release-evidence.contract: OK");
