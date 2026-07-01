/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_SDKWORK_WEB_FRAMEWORK_BACKEND_API_BASE_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
