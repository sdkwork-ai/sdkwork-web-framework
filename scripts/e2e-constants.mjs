/** Fixed credentials for local verify / Playwright integration only — never use in production. */

import { createHmac } from "node:crypto";

export const E2E_JWT_SECRET = "e2e-bootstrap-secret-with-sufficient-length";
export const E2E_TENANT_ID = "bootstrap";
export const E2E_ORGANIZATION_ID = "org-bootstrap";
export const E2E_KEY_ID = "bootstrap";
export const E2E_ADMIN_HOST = "127.0.0.1";
export const E2E_ADMIN_PORT = 3921;
export const E2E_ADMIN_BIND = `${E2E_ADMIN_HOST}:${E2E_ADMIN_PORT}`;
export const E2E_ADMIN_BASE_URL = `http://${E2E_ADMIN_BIND}`;
export const E2E_ADMIN_HEALTH_URL = `${E2E_ADMIN_BASE_URL}/healthz`;
export const E2E_PREVIEW_PORT = 4176;
export const E2E_PREVIEW_URL = `http://127.0.0.1:${E2E_PREVIEW_PORT}`;
/** Grants both control-plane UI tabs and tenant-scoped admin API routes in integration tests. */
export const E2E_ADMIN_AUTH_PERMISSIONS =
  "web-framework.control-plane,web-framework.tenant.admin";

function base64url(value) {
  return Buffer.from(value).toString("base64url");
}

function encodeHs256Jwt(secret, keyId, payload) {
  const now = Math.floor(Date.now() / 1000);
  const body = {
    token_version: 1,
    iat: now,
    exp: now + 3600,
    ...payload,
  };
  const header = { alg: "HS256", typ: "JWT", kid: keyId };
  const headerB64 = base64url(JSON.stringify(header));
  const payloadB64 = base64url(JSON.stringify(body));
  const signingInput = `${headerB64}.${payloadB64}`;
  const signature = createHmac("sha256", secret).update(signingInput).digest("base64url");
  return `${signingInput}.${signature}`;
}

export function e2eAuthToken(permissionScope) {
  return encodeHs256Jwt(E2E_JWT_SECRET, E2E_KEY_ID, {
    token_type: "auth",
    tenant_id: E2E_TENANT_ID,
    organization_id: E2E_ORGANIZATION_ID,
    user_id: "e2e-user",
    session_id: "e2e-session",
    app_id: "appbase",
    auth_level: "password",
    login_scope: "ORGANIZATION",
    permission_scope: permissionScope,
  });
}

export function e2eAdminAuthToken() {
  return e2eAuthToken(E2E_ADMIN_AUTH_PERMISSIONS);
}

export function e2eAccessToken() {
  return encodeHs256Jwt(E2E_JWT_SECRET, E2E_KEY_ID, {
    token_type: "access",
    tenant_id: E2E_TENANT_ID,
    organization_id: E2E_ORGANIZATION_ID,
    user_id: "e2e-user",
    session_id: "e2e-session",
    app_id: "appbase",
    environment: "prod",
    deployment_mode: "saas",
    login_scope: "ORGANIZATION",
  });
}
