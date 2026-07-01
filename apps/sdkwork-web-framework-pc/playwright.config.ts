import { defineConfig, devices } from "@playwright/test";

const previewPort = 4175;

export default defineConfig({
  testDir: "./e2e",
  testMatch: /console\.(smoke|error-paths)\.spec\.ts/,
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: "list",
  use: {
    baseURL: `http://127.0.0.1:${previewPort}`,
    trace: "on-first-retry",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: `npm run preview -- --port ${previewPort} --strictPort --host 127.0.0.1`,
    url: `http://127.0.0.1:${previewPort}`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
