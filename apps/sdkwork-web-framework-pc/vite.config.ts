import { resolveBrowserDistOutDir } from '../../../sdkwork-specs/tools/browser-dist-layout.mjs';
function resolveViteEnvironment(mode: string | undefined, processEnv = process.env) {
  const profileMatch = /^(standalone|cloud)\.(development|test|staging|production)$/u.exec(mode ?? '');
  return profileMatch?.[2]
    ?? (['development', 'test', 'staging', 'production'].includes(processEnv.SDKWORK_ENVIRONMENT ?? '')
      ? (processEnv.SDKWORK_ENVIRONMENT ?? 'production')
      : 'production');
}
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { defineConfig, loadEnv } from 'vite';
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const envDir = path.dirname(fileURLToPath(import.meta.url));
  const env = loadEnv(mode, envDir, '');
  const backendUrl =
    process.env.SDKWORK_E2E_ADMIN_URL?.trim() ||
    env.SDKWORK_E2E_ADMIN_URL?.trim() ||
    env.SDKWORK_WEB_FRAMEWORK_ADMIN_URL?.trim() ||
    'http://127.0.0.1:3920';
  return {
    build: {
      outDir: resolveBrowserDistOutDir(resolveViteEnvironment(mode, process.env)),
      emptyOutDir: true,
    },
    define: {
      'process.env.SDKWORK_ACCESS_TOKEN': JSON.stringify(env.SDKWORK_ACCESS_TOKEN ?? ''),
    },
    plugins: [react()],
    server: {
      port: 5175,
      proxy: {
        "/backend": {
          target: backendUrl,
          changeOrigin: true,
        },
      },
    },
    preview: {
      proxy: {
        "/backend": {
          target: backendUrl,
          changeOrigin: true,
        },
      },
    },
  };
});
