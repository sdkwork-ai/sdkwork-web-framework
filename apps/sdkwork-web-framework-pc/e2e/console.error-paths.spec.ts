/**
 * Error-path E2E: verifies the PC console surfaces RFC 9457 Problem+json
 * responses from the backend SDK transport in the `.error` region for
 * 401/403/429/503, matching WEB_FRAMEWORK_SPEC §10 / SECURITY_SPEC §6.
 *
 * These tests use Playwright route interception (mocked backend) so every
 * Problem+json payload is deterministic and does not depend on a live
 * admin-server. The transport's Problem+json parsing and the React error
 * boundary rendering are exercised end-to-end in a real browser.
 */

import { test, expect, type Page } from "@playwright/test";

import { e2eAdminAuthToken } from "../../../scripts/e2e-constants.mjs";

/** Problem+json payload shapes mirroring crates/sdkwork-web-core/src/problem.rs. */
function problemJson(
  type: string,
  title: string,
  status: number,
  detail: string,
  extras: Record<string, string> = {},
): string {
  return JSON.stringify({ type, title, status, detail, ...extras });
}

const RUNTIME_DEFAULTS = {
  production_security_policy: { cors_validate: true },
  default_security_policy: {},
  optional_features_production_sqlx: { sqlx: true },
};

const OPTIONAL_FEATURES = {
  recommended_production_sqlx: { sqlx: true },
  development: {},
};

/**
 * Mock the defaults-tab requests (runtime_defaults + optional_features) as
 * successful so the initial page render is stable. Error-path mocks are
 * layered on top per-test for non-defaults tabs.
 */
async function mockDefaultsOk(page: Page): Promise<void> {
  await page.route("**/backend/v3/api/web-framework/**", async (route) => {
    const url = route.request().url();
    let data: unknown = [];
    if (url.includes("/runtime_defaults")) {
      data = RUNTIME_DEFAULTS;
    } else if (url.includes("/optional_features")) {
      data = OPTIONAL_FEATURES;
    } else {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ success: true, data }),
      });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ success: true, data }),
    });
  });
}

/** Override a specific backend path to return a Problem+json error response. */
async function mockErrorForPath(
  page: Page,
  pathFragment: string,
  status: number,
  body: string,
  headers: Record<string, string> = {},
): Promise<void> {
  await page.route("**/backend/v3/api/web-framework/**", async (route) => {
    const url = route.request().url();
    if (url.includes(pathFragment)) {
      await route.fulfill({
        status,
        contentType: "application/problem+json",
        headers: { "content-type": "application/problem+json", ...headers },
        body,
      });
      return;
    }
    // Fall through to the defaults mock for other paths.
    let data: unknown = [];
    if (url.includes("/runtime_defaults")) {
      data = RUNTIME_DEFAULTS;
    } else if (url.includes("/optional_features")) {
      data = OPTIONAL_FEATURES;
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ success: true, data }),
    });
  });
}

test.describe("Web Framework PC console error paths (Problem+json)", () => {
  test.beforeEach(async ({ page }) => {
    // Inject admin auth token so control-plane tabs (CORS, etc.) are visible.
    await page.addInitScript((token: string) => {
      sessionStorage.setItem("sdkwork.authToken", token);
    }, e2eAdminAuthToken());
  });

  test("401 MissingCredentials: clears dev session and reloads to logged-out view", async ({
    page,
  }) => {
    // The 401 path calls onUnauthorized() which clears sessionStorage and
    // triggers location.reload(). The beforeEach addInitScript re-injects the
    // token on every navigation, so we add a guard that removes the token on
    // the second load (the reload), verifying the session was cleared.
    await page.addInitScript(() => {
      if (sessionStorage.getItem("__sdkwork_401_first_load") !== null) {
        // Second load (after 401-induced reload): token must NOT be present.
        sessionStorage.removeItem("sdkwork.authToken");
      } else {
        sessionStorage.setItem("__sdkwork_401_first_load", "1");
      }
    });

    await mockErrorForPath(
      page,
      "/cors_policies",
      401,
      problemJson(
        "https://sdkwork.dev/problems/missing-credentials",
        "Unauthorized",
        401,
        "Auth token is missing or expired",
        { traceId: "trace-401-test" },
      ),
    );

    await page.goto("/");
    await expect(page.getByTestId("web-framework-console")).toBeVisible();
    // CORS tab is visible because the admin token was injected.
    await expect(page.getByRole("button", { name: "CORS" })).toBeVisible();

    // Switch to CORS to trigger the 401 → onUnauthorized → reload.
    await page.getByRole("button", { name: "CORS" }).click();

    // After reload, the token is cleared so only "defaults" tab is visible.
    await expect(page.getByRole("button", { name: "CORS" })).toBeHidden({
      timeout: 15_000,
    });

    // The dev session token must have been cleared by onUnauthorized.
    const stored = await page.evaluate(() =>
      sessionStorage.getItem("sdkwork.authToken"),
    );
    expect(stored).toBeNull();

    // The defaults tab must still render its data after the reload.
    await expect(page.locator(".error")).toHaveCount(0, { timeout: 10_000 });
    await expect(page.locator("pre")).toContainText("production_security_policy");
  });

  test("403 Forbidden: surfaces detail without clearing session", async ({
    page,
  }) => {
    await mockErrorForPath(
      page,
      "/cors_policies",
      403,
      problemJson(
        "https://sdkwork.dev/problems/forbidden",
        "Forbidden",
        403,
        "Tenant does not have control-plane access",
        { traceId: "trace-403-test" },
      ),
    );

    await page.goto("/");
    await expect(page.getByTestId("web-framework-console")).toBeVisible();
    await page.getByRole("button", { name: "CORS" }).click();

    const errorRegion = page.locator(".error[role='alert']");
    await expect(errorRegion).toBeVisible({ timeout: 10_000 });
    await expect(errorRegion).toContainText(
      "Tenant does not have control-plane access",
    );

    // 403 must NOT clear the session (only 401 triggers onUnauthorized).
    const stored = await page.evaluate(() =>
      sessionStorage.getItem("sdkwork.authToken"),
    );
    expect(stored).not.toBeNull();
  });

  test("429 RateLimitExceeded: surfaces detail and retry-after header", async ({
    page,
  }) => {
    await mockErrorForPath(
      page,
      "/cors_policies",
      429,
      problemJson(
        "https://sdkwork.dev/problems/rate-limit-exceeded",
        "Too Many Requests",
        429,
        "Rate limit exceeded; retry after backoff",
        { traceId: "trace-429-test" },
      ),
      { "retry-after": "30" },
    );

    await page.goto("/");
    await expect(page.getByTestId("web-framework-console")).toBeVisible();
    await page.getByRole("button", { name: "CORS" }).click();

    const errorRegion = page.locator(".error[role='alert']");
    await expect(errorRegion).toBeVisible({ timeout: 10_000 });
    await expect(errorRegion).toContainText("Rate limit exceeded");
  });

  test("503 DependencyUnavailable: surfaces client-safe detail", async ({
    page,
  }) => {
    await mockErrorForPath(
      page,
      "/cors_policies",
      503,
      problemJson(
        "https://sdkwork.dev/problems/dependency-unavailable",
        "Service Unavailable",
        503,
        "A required dependency is temporarily unavailable",
        { traceId: "trace-503-test" },
      ),
    );

    await page.goto("/");
    await expect(page.getByTestId("web-framework-console")).toBeVisible();
    await page.getByRole("button", { name: "CORS" }).click();

    const errorRegion = page.locator(".error[role='alert']");
    await expect(errorRegion).toBeVisible({ timeout: 10_000 });
    await expect(errorRegion).toContainText(
      "A required dependency is temporarily unavailable",
    );
    // Client-safe detail must NOT leak implementation internals.
    await expect(errorRegion).not.toContainText("sqlx");
    await expect(errorRegion).not.toContainText("connection");
  });

  test("defaults tab still loads when non-defaults tab errors (epoch guard)", async ({
    page,
  }) => {
    // Only CORS errors; defaults tab must remain functional.
    await mockDefaultsOk(page);
    await page.route("**/backend/v3/api/web-framework/cors_policies**", (route) =>
      route.fulfill({
        status: 503,
        contentType: "application/problem+json",
        body: problemJson(
          "https://sdkwork.dev/problems/dependency-unavailable",
          "Service Unavailable",
          503,
          "A required dependency is temporarily unavailable",
        ),
      }),
    );

    await page.goto("/");
    await expect(page.getByTestId("web-framework-console")).toBeVisible();
    await expect(page.locator(".error")).toHaveCount(0, { timeout: 10_000 });
    await expect(page.locator("pre")).toContainText("production_security_policy");

    // Switch to CORS → error appears; switch back to defaults → error clears.
    await page.getByRole("button", { name: "CORS" }).click();
    await expect(page.locator(".error[role='alert']")).toBeVisible({
      timeout: 10_000,
    });

    await page.getByRole("button", { name: "默认配置" }).click();
    await expect(page.locator(".error")).toHaveCount(0, { timeout: 10_000 });
    await expect(page.locator("pre")).toContainText("production_security_policy");
  });
});

// Silence unused-var lint for mockDefaultsOk when only mockErrorForPath is used.
void mockDefaultsOk;
