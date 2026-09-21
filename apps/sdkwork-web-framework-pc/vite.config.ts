import { resolveViteEnvironment, resolveLucideReactEntry } from '../../../sdkwork-specs/tools/vite-runtime-profile.mjs';
import { resolveBrowserDistOutDir } from '../../../sdkwork-specs/tools/browser-dist-layout.mjs';

import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { defineConfig, loadEnv } from 'vite';
import { createSdkworkCredentialEntryBootstrapVitePlugin } from '@sdkwork/iam-credential-entry/vite';
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const envDir = path.dirname(fileURLToPath(import.meta.url));
  const env = loadEnv(mode, envDir, '');
  const bootstrapAccessToken = env.SDKWORK_ACCESS_TOKEN ?? process.env.SDKWORK_ACCESS_TOKEN;
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
    plugins: [
      // The bootstrap credential reaches the renderer only through the shared IAM
      // plugin (dev-server HTML injection as
      // `globalThis.__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__`).
      // `define['process.env.SDKWORK_ACCESS_TOKEN']` is NOT a valid handoff
      // (IAM_CREDENTIAL_ENTRY_SPEC.md section 4/5).
      createSdkworkCredentialEntryBootstrapVitePlugin({
        accessToken: bootstrapAccessToken,
        environment: resolveViteEnvironment(mode, process.env),
      }),
      react(),
    ],
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
