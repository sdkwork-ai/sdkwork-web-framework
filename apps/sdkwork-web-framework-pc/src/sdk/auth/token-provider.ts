/**
 * Backend control-plane credential provider (open-closed extension point).
 *
 * SECURITY: this module only supplies credentials to the backend SDK transport.
 * The backend ALWAYS re-verifies JWT signatures, tenant binding, token_version,
 * authorization, and tenant isolation via the 18-stage pipeline
 * (WEB_FRAMEWORK_SPEC §8 / SECURITY_SPEC §5.1). Client-side credential storage is
 * a UX concern, NOT a security boundary (SECURITY_SPEC §4: "UI permission checks
 * do not replace backend authorization").
 */

export type BackendAuthCredentials = {
  authToken: string;
  accessToken: string;
};

export interface BackendTokenProvider {
  /** Returns current credentials, or null when no session is established. */
  getCredentials(): BackendAuthCredentials | null;
  /** Called by the transport on 401 Unauthorized; clears local session state. */
  onUnauthorized(): void;
}

/** sessionStorage key for the auth (session) token used in dev/E2E only. */
export const DEV_AUTH_TOKEN_STORAGE_KEY = "sdkwork.authToken";

/** Reads the dev auth token from sessionStorage (empty when absent or SSR). */
export function readDevAuthToken(): string {
  if (typeof sessionStorage === "undefined") {
    return "";
  }
  return sessionStorage.getItem(DEV_AUTH_TOKEN_STORAGE_KEY)?.trim() ?? "";
}

/** Clears the dev auth token from sessionStorage. */
export function clearDevSession(): void {
  if (typeof sessionStorage === "undefined") {
    return;
  }
  sessionStorage.removeItem(DEV_AUTH_TOKEN_STORAGE_KEY);
}

function reloadAfterSignOut(): void {
  if (typeof location !== "undefined") {
    location.reload();
  }
}

/**
 * Development / E2E provider: auth token in sessionStorage, access token injected
 * via the Vite `process.env.SDKWORK_ACCESS_TOKEN` define. Used only when the Vite
 * dev server is running, or when an E2E build explicitly bakes the access token.
 */
export class DevSessionTokenProvider implements BackendTokenProvider {
  getCredentials(): BackendAuthCredentials | null {
    const authToken = readDevAuthToken();
    const accessToken = (process.env.SDKWORK_ACCESS_TOKEN ?? "").trim();
    if (!authToken && !accessToken) {
      return null;
    }
    return { authToken, accessToken };
  }

  onUnauthorized(): void {
    clearDevSession();
    reloadAfterSignOut();
  }
}

/**
 * Production provider: credentials are injected at runtime by the hosting page
 * (e.g. the IAM current-session SDK) via `window.__SDKWORK_ADMIN_CREDENTIALS__`.
 * Tokens MUST NOT be baked into production bundles (SECURITY_SPEC §1/§4).
 */
export class RuntimeCredentialsTokenProvider implements BackendTokenProvider {
  getCredentials(): BackendAuthCredentials | null {
    const injected = readInjectedCredentials();
    if (!injected && import.meta.env.PROD) {
      // eslint-disable-next-line no-console
      console.error(
        "[sdkwork-web-framework-pc] No backend credentials injected. " +
          "Production deployment must provide window.__SDKWORK_ADMIN_CREDENTIALS__ " +
          "via the IAM current-session SDK (APP_PC_ARCHITECTURE_SPEC §1).",
      );
    }
    return injected;
  }

  onUnauthorized(): void {
    clearDevSession();
    reloadAfterSignOut();
  }
}

declare global {
  interface Window {
    __SDKWORK_ADMIN_CREDENTIALS__?:
      | BackendAuthCredentials
      | (() => BackendAuthCredentials | null);
  }
}

function readInjectedCredentials(): BackendAuthCredentials | null {
  if (typeof window === "undefined") {
    return null;
  }
  const source = window.__SDKWORK_ADMIN_CREDENTIALS__;
  if (typeof source === "function") {
    return source();
  }
  return source ?? null;
}

/**
 * Resolve the active token provider.
 *
 * - Vite dev server, or an E2E build that bakes `process.env.SDKWORK_ACCESS_TOKEN`
 *   → {@link DevSessionTokenProvider}
 * - Otherwise (production) → {@link RuntimeCredentialsTokenProvider}
 */
export function resolveBackendTokenProvider(): BackendTokenProvider {
  const bakedAccessToken = (process.env.SDKWORK_ACCESS_TOKEN ?? "").trim();
  if (import.meta.env.DEV || bakedAccessToken.length > 0) {
    return new DevSessionTokenProvider();
  }
  return new RuntimeCredentialsTokenProvider();
}
