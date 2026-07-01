import { defineConfig, devices } from "@playwright/test";

import { E2E_PREVIEW_URL } from "../../scripts/e2e-constants.mjs";

export default defineConfig({
  testDir: "./e2e",
  testMatch: /console\.integration\.spec\.ts/,
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: "list",
  timeout: 60_000,
  use: {
    baseURL: E2E_PREVIEW_URL,
    trace: "on-first-retry",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "node ../../scripts/e2e-web-stack.mjs",
    url: E2E_PREVIEW_URL,
    reuseExistingServer: false,
    timeout: 240_000,
  },
});
