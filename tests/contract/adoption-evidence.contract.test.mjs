#!/usr/bin/env node
/** Validates committed framework pathfinder adoption evidence JSON. */

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const evidencePath = path.join(repoRoot, "specs", "framework-adoption.evidence.json");

const validate = spawnSync(
  "node",
  [path.join(repoRoot, "scripts", "validate-adoption-evidence.mjs"), evidencePath],
  { cwd: repoRoot, encoding: "utf8", shell: process.platform === "win32" },
);
if (validate.status !== 0) {
  console.error(validate.stdout);
  console.error(validate.stderr);
  process.exit(validate.status ?? 1);
}

console.log("adoption-evidence.contract: OK");
