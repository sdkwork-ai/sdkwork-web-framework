import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  getWebFrameworkAdminService,
  resetWebFrameworkAdminServiceForTests,
  setWebFrameworkAdminServiceForTests,
} from "../web-framework-admin-service";
import type { WebFrameworkAdminBackendSdk } from "../../sdk/backend-sdk";

function makeFakeSdk(overrides: Partial<WebFrameworkAdminBackendSdk> = {}): WebFrameworkAdminBackendSdk {
  return {
    listCorsPolicies: vi.fn().mockResolvedValue([]),
    upsertCorsPolicy: vi.fn().mockResolvedValue({}),
    listRateLimitPolicies: vi.fn().mockResolvedValue([]),
    upsertRateLimitPolicy: vi.fn().mockResolvedValue({}),
    listTenantProfiles: vi.fn().mockResolvedValue([]),
    upsertTenantProfile: vi.fn().mockResolvedValue({}),
    listControlNodes: vi.fn().mockResolvedValue([]),
    registerControlNode: vi.fn().mockResolvedValue({}),
    heartbeatControlNode: vi.fn().mockResolvedValue({}),
    deleteControlNode: vi.fn().mockResolvedValue(undefined),
    runtimeDefaults: vi.fn().mockResolvedValue({}),
    optionalFeatures: vi.fn().mockResolvedValue({}),
    listSecurityEvents: vi.fn().mockResolvedValue([]),
    listAuditEvents: vi.fn().mockResolvedValue([]),
    ...overrides,
  } as unknown as WebFrameworkAdminBackendSdk;
}

describe("web-framework-admin-service", () => {
  afterEach(() => {
    resetWebFrameworkAdminServiceForTests();
  });

  it("returns the same instance on subsequent calls (caching)", () => {
    const sdk1 = getWebFrameworkAdminService();
    const sdk2 = getWebFrameworkAdminService();
    expect(sdk1).toBe(sdk2);
  });

  it("returns a new instance after reset", () => {
    const sdk1 = getWebFrameworkAdminService();
    resetWebFrameworkAdminServiceForTests();
    const sdk2 = getWebFrameworkAdminService();
    expect(sdk1).not.toBe(sdk2);
  });

  it("returns injected fake SDK via setWebFrameworkAdminServiceForTests", () => {
    const fake = makeFakeSdk();
    setWebFrameworkAdminServiceForTests(fake);
    expect(getWebFrameworkAdminService()).toBe(fake);
  });

  it("fake SDK listCorsPolicies returns empty array", async () => {
    const fake = makeFakeSdk();
    setWebFrameworkAdminServiceForTests(fake);
    const service = getWebFrameworkAdminService();
    const result = await service.listCorsPolicies("prod");
    expect(result).toEqual([]);
    expect(fake.listCorsPolicies).toHaveBeenCalledWith("prod");
  });

  it("fake SDK listCorsPolicies returns error when configured to reject", async () => {
    const error = new Error("permission denied");
    const fake = makeFakeSdk({
      listCorsPolicies: vi.fn().mockRejectedValue(error),
    });
    setWebFrameworkAdminServiceForTests(fake);
    const service = getWebFrameworkAdminService();
    await expect(service.listCorsPolicies("prod")).rejects.toThrow("permission denied");
  });

  it("fake SDK runtimeDefaults returns populated snapshot", async () => {
    const snapshot = {
      production_security_policy: { cors: { allowed_origins: [] } },
      default_security_policy: {},
      optional_features_production_sqlx: { tenant_isolation: true },
    };
    const fake = makeFakeSdk({
      runtimeDefaults: vi.fn().mockResolvedValue(snapshot),
    });
    setWebFrameworkAdminServiceForTests(fake);
    const service = getWebFrameworkAdminService();
    const result = await service.runtimeDefaults();
    expect(result).toEqual(snapshot);
  });
});
