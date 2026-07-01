import {
  createWebFrameworkAdminBackendSdkFromEnv,
  type WebFrameworkAdminBackendSdk,
} from "../sdk/backend-sdk";

let cachedSdk: WebFrameworkAdminBackendSdk | null = null;

/** Domain service for framework control-plane admin operations (BACKEND_UI_SPEC / FRONTEND_CODE_SPEC). */
export function getWebFrameworkAdminService(): WebFrameworkAdminBackendSdk {
  if (!cachedSdk) {
    cachedSdk = createWebFrameworkAdminBackendSdkFromEnv();
  }
  return cachedSdk;
}

export function resetWebFrameworkAdminServiceForTests(): void {
  cachedSdk = null;
}

/** Inject a fake SDK for unit tests. Call resetWebFrameworkAdminServiceForTests() in afterEach. */
export function setWebFrameworkAdminServiceForTests(sdk: WebFrameworkAdminBackendSdk): void {
  cachedSdk = sdk;
}
