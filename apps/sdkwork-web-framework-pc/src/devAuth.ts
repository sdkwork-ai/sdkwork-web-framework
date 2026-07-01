export type DevAuthClaims = {
  tenant_id?: string;
  permission_scope?: string;
};

/**
 * Derive UI tab-visibility hints from the session token's claims.
 *
 * SECURITY: This is NOT a security boundary. The payload is decoded WITHOUT
 * signature verification purely to choose which console tabs to render. The
 * backend ALWAYS re-verifies JWT signatures, tenant binding, token_version,
 * and authorization via the 18-stage pipeline (WEB_FRAMEWORK_SPEC §8 /
 * SECURITY_SPEC §4: "UI permission checks do not replace backend authorization").
 * A forged token may reveal tab chrome, but every control-plane call is
 * independently rejected by the backend dual-token + tenant-isolation guards.
 */
export function readDevAuthClaims(authToken: string | null | undefined): DevAuthClaims | null {
  if (!authToken?.trim()) {
    return null;
  }
  const parts = authToken.trim().split(".");
  if (parts.length < 2) {
    return null;
  }
  try {
    const normalized = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const padded = normalized.padEnd(normalized.length + ((4 - (normalized.length % 4)) % 4), "=");
    const json = atob(padded);
    return JSON.parse(json) as DevAuthClaims;
  } catch {
    return null;
  }
}

export function hasPermission(
  claims: DevAuthClaims | null,
  permission: string,
): boolean {
  if (!claims?.permission_scope) {
    return false;
  }
  return claims.permission_scope
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean)
    .includes(permission);
}

export const PERM_CONTROL_PLANE = "web-framework.control-plane";
export const PERM_TENANT_ADMIN = "web-framework.tenant.admin";
export const PERM_PLATFORM_READ = "web-framework.platform.read";
