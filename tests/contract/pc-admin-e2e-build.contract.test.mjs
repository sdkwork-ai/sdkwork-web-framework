#!/usr/bin/env node
/** Ensures E2E build bakes dual-token access credentials into the Vite dist bundle. */

import { readFileSync, readdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const pcApp = path.join(repoRoot, "apps", "sdkwork-web-framework-pc");
const distAssets = path.join(pcApp, "dist", "assets");
const e2eConstants = readFileSync(
  path.join(repoRoot, "scripts", "e2e-constants.mjs"),
  "utf8",
);

if (!e2eConstants.includes('login_scope: "ORGANIZATION"')) {
  console.error(
    "pc-admin-e2e-build: e2e-constants must use ORGANIZATION login_scope for backend-api",
  );
  process.exit(1);
}

const build = spawnSync("node", [path.join(repoRoot, "scripts", "build-pc-admin-e2e.mjs")], {
  cwd: repoRoot,
  stdio: "inherit",
  shell: process.platform === "win32",
});
if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

const jsBundles = readdirSync(distAssets).filter((name) => name.endsWith(".js"));
if (jsBundles.length === 0) {
  console.error("pc-admin-e2e-build: no JS bundles in dist/assets");
  process.exit(1);
}

const combined = jsBundles
  .map((name) => readFileSync(path.join(distAssets, name), "utf8"))
  .join("\n");

if (!combined.includes("eyJ")) {
  console.error(
    "pc-admin-e2e-build: dist bundle must contain baked HS256 access token (eyJ prefix)",
  );
  process.exit(1);
}

console.log("pc-admin-e2e-build.contract: OK");
