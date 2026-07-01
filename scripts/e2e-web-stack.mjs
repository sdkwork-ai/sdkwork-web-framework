#!/usr/bin/env node
/** Starts admin-server + vite preview for Playwright integration tests. */

import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import {
  E2E_ADMIN_BIND,
  E2E_ADMIN_HEALTH_URL,
  E2E_JWT_SECRET,
  E2E_KEY_ID,
  E2E_PREVIEW_PORT,
  E2E_PREVIEW_URL,
  E2E_TENANT_ID,
} from "./e2e-constants.mjs";
import { waitForUrl } from "./e2e-wait-for-url.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const pcApp = path.join(repoRoot, "apps", "sdkwork-web-framework-pc");
const dataDir = path.join(repoRoot, "target", "e2e");
mkdirSync(dataDir, { recursive: true });
const dbPath = path.join(dataDir, "admin-server.db").replaceAll("\\", "/");

const children = [];
let shuttingDown = false;

function spawnLogged(command, args, options) {
  const child = spawn(command, args, {
    stdio: "inherit",
    shell: process.platform === "win32",
    ...options,
  });
  children.push(child);
  child.on("exit", (code, signal) => {
    if (!shuttingDown && code !== 0) {
      console.error(`[e2e-web-stack] ${command} exited`, { code, signal });
      shutdown(code ?? 1);
    }
  });
  return child;
}

function shutdown(code = 0) {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;
  for (const child of children) {
    if (!child.killed) {
      child.kill();
    }
  }
  process.exit(code);
}

process.on("SIGINT", () => shutdown(0));
process.on("SIGTERM", () => shutdown(0));

const adminEnv = {
  ...process.env,
  SDKWORK_WEB_FRAMEWORK_ENV: "prod",
  SDKWORK_WEB_FRAMEWORK_ADMIN_BIND: E2E_ADMIN_BIND,
  SDKWORK_WEB_FRAMEWORK_STORE_URL: `sqlite:${dbPath}?mode=rwc`,
  SDKWORK_WEB_FRAMEWORK_STORE_POOL_SIZE: "2",
  SDKWORK_WEB_FRAMEWORK_JWT_HS256_SECRET: E2E_JWT_SECRET,
  SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_TENANT_ID: E2E_TENANT_ID,
  SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_KEY_ID: E2E_KEY_ID,
  RUST_LOG: process.env.RUST_LOG ?? "warn",
};

spawnLogged("cargo", ["run", "-p", "sdkwork-web-admin-server", "--quiet"], {
  cwd: repoRoot,
  env: adminEnv,
});

await waitForUrl(E2E_ADMIN_HEALTH_URL, 180_000);

spawnLogged(
  "npm",
  ["run", "preview", "--", "--port", String(E2E_PREVIEW_PORT), "--strictPort", "--host", "127.0.0.1"],
  {
    cwd: pcApp,
    env: {
      ...process.env,
      SDKWORK_E2E_ADMIN_URL: `http://${E2E_ADMIN_BIND}`,
    },
  },
);

await waitForUrl(E2E_PREVIEW_URL, 60_000);
console.log(`[e2e-web-stack] ready admin=${E2E_ADMIN_HEALTH_URL} preview=${E2E_PREVIEW_URL}`);

await new Promise(() => {});
