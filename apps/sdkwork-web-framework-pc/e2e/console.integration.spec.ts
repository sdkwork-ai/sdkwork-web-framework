import { test, expect } from "@playwright/test";

import { e2eAdminAuthToken } from "../../../scripts/e2e-constants.mjs";

test.describe("Web Framework PC console (real admin-server)", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript((token: string) => {
      sessionStorage.setItem("sdkwork.authToken", token);
    }, e2eAdminAuthToken());
  });

  test("loads runtime defaults from assembled control-plane backend", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("web-framework-console")).toBeVisible();
    await expect(page.locator(".error")).toHaveCount(0, { timeout: 20_000 });
    await expect(page.locator("pre")).not.toContainText("加载中…", { timeout: 20_000 });
    await expect(page.locator("pre")).toContainText("production_security_policy");
    await expect(page.locator("pre")).toContainText("optional_features_production_sqlx");
  });

  test("lists CORS policies through dual-token backend SDK transport", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "CORS" }).click();
    await expect(page.locator(".error")).toHaveCount(0, { timeout: 20_000 });
    await expect(page.locator("pre")).not.toContainText("加载中…", { timeout: 20_000 });
    await expect(page.locator("pre")).toContainText("[]");
  });
});