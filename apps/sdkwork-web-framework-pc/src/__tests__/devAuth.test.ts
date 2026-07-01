import { describe, it, expect, beforeEach } from "vitest";
import {
  readDevAuthClaims,
  hasPermission,
  PERM_CONTROL_PLANE,
  PERM_TENANT_ADMIN,
  PERM_PLATFORM_READ,
} from "../devAuth";

function makeJwt(payload: Record<string, unknown>): string {
  const header = Buffer.from(JSON.stringify({ alg: "HS256", typ: "JWT" })).toString("base64url");
  const body = Buffer.from(JSON.stringify(payload)).toString("base64url");
  return `${header}.${body}.signature`;
}

describe("readDevAuthClaims", () => {
  it("returns null for empty string", () => {
    expect(readDevAuthClaims("")).toBeNull();
  });

  it("returns null for null/undefined", () => {
    expect(readDevAuthClaims(null)).toBeNull();
    expect(readDevAuthClaims(undefined)).toBeNull();
  });

  it("returns null for whitespace-only string", () => {
    expect(readDevAuthClaims("   ")).toBeNull();
  });

  it("returns null for non-JWT string (no dots)", () => {
    expect(readDevAuthClaims("not-a-jwt")).toBeNull();
  });

  it("returns null for malformed base64 payload", () => {
    expect(readDevAuthClaims("header.!!!.signature")).toBeNull();
  });

  it("parses valid JWT with tenant_id and permission_scope", () => {
    const token = makeJwt({
      tenant_id: "tenant-123",
      permission_scope: "web-framework.control-plane,web-framework.tenant.admin",
    });
    const claims = readDevAuthClaims(token);
    expect(claims).not.toBeNull();
    expect(claims?.tenant_id).toBe("tenant-123");
    expect(claims?.permission_scope).toBe("web-framework.control-plane,web-framework.tenant.admin");
  });

  it("parses JWT with empty permission_scope", () => {
    const token = makeJwt({ permission_scope: "" });
    const claims = readDevAuthClaims(token);
    expect(claims).not.toBeNull();
    expect(claims?.permission_scope).toBe("");
  });

  it("parses JWT without permission_scope field", () => {
    const token = makeJwt({ tenant_id: "100001" });
    const claims = readDevAuthClaims(token);
    expect(claims).not.toBeNull();
    expect(claims?.tenant_id).toBe("100001");
    expect(claims?.permission_scope).toBeUndefined();
  });
});

describe("hasPermission", () => {
  it("returns false for null claims", () => {
    expect(hasPermission(null, PERM_CONTROL_PLANE)).toBe(false);
  });

  it("returns false when permission_scope is undefined", () => {
    expect(hasPermission({ tenant_id: "100001" }, PERM_CONTROL_PLANE)).toBe(false);
  });

  it("returns false when permission_scope is empty", () => {
    expect(hasPermission({ permission_scope: "" }, PERM_CONTROL_PLANE)).toBe(false);
  });

  it("returns true when permission_scope contains the exact permission", () => {
    expect(
      hasPermission({ permission_scope: "web-framework.control-plane" }, PERM_CONTROL_PLANE),
    ).toBe(true);
  });

  it("returns true when permission_scope contains permission among multiple", () => {
    const scope = `web-framework.control-plane, ${PERM_TENANT_ADMIN},other.perm`;
    expect(hasPermission({ permission_scope: scope }, PERM_TENANT_ADMIN)).toBe(true);
  });

  it("returns false when permission_scope does not contain the permission", () => {
    expect(hasPermission({ permission_scope: PERM_TENANT_ADMIN }, PERM_CONTROL_PLANE)).toBe(false);
  });

  it("returns false when permission_scope contains partial match", () => {
    expect(
      hasPermission({ permission_scope: "web-framework.control" }, PERM_CONTROL_PLANE),
    ).toBe(false);
  });

  it("handles whitespace in permission_scope", () => {
    expect(
      hasPermission(
        { permission_scope: `  ${PERM_PLATFORM_READ}  ,  other  ` },
        PERM_PLATFORM_READ,
      ),
    ).toBe(true);
  });

  it("filters empty entries from comma-separated scope", () => {
    expect(
      hasPermission(
        { permission_scope: `,,  ${PERM_CONTROL_PLANE}  ,,` },
        PERM_CONTROL_PLANE,
      ),
    ).toBe(true);
  });
});
