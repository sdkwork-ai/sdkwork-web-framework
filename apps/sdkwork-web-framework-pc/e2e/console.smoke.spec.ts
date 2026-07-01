import { test, expect, type Page } from "@playwright/test";

function fakeDevJwt(permissionScope: string): string {
  const payload = Buffer.from(JSON.stringify({ permission_scope: permissionScope }))
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
  return `e2eheader.${payload}.e2esig`;
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

async function mockWebFrameworkBackend(page: Page): Promise<void> {
  await page.route("**/backend/v3/api/web-framework/**", async (route) => {
    const url = route.request().url();
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

test.describe("Web Framework PC console", () => {
  test.beforeEach(async ({ page }) => {
    await mockWebFrameworkBackend(page);
  });

  test("loads defaults tab shell without dev auth", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("web-framework-console")).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "SDKWork Web Framework Console" }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "默认配置" })).toBeVisible();
    await expect(page.locator("pre")).not.toContainText("加载中…", { timeout: 10_000 });
    await expect(page.locator("pre")).toContainText("runtime");
  });

  test("reveals control-plane tabs when dev auth token grants permissions", async ({
    page,
  }) => {
    await page.addInitScript((token: string) => {
      sessionStorage.setItem("sdkwork.authToken", token);
    }, fakeDevJwt("web-framework.control-plane"));
    await page.goto("/");
    await expect(page.getByRole("button", { name: "CORS" })).toBeVisible();
    await expect(page.getByRole("button", { name: "控制节点" })).toBeVisible();
    await expect(page.getByRole("button", { name: "安全事件" })).toBeVisible();
  });
});
