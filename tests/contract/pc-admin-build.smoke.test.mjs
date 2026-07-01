#!/usr/bin/env node
/**
 * Post-build smoke check for PC admin console (BACKEND_UI_SPEC / FRONTEND_CODE_SPEC).
 * Requires `apps/sdkwork-web-framework-pc/dist` from `npm run verify` in the PC app.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const distRoot = path.join(repoRoot, 'apps/sdkwork-web-framework-pc/dist');
const indexPath = path.join(distRoot, 'index.html');
const assetsDir = path.join(distRoot, 'assets');

assert.ok(fs.existsSync(indexPath), 'PC admin dist/index.html missing; run npm run verify in apps/sdkwork-web-framework-pc');

const indexHtml = fs.readFileSync(indexPath, 'utf8');
assert.match(indexHtml, /SDKWork Web Framework Console/, 'built index.html must include console title');
assert.ok(indexHtml.includes('id="root"'), 'built index.html must mount React root');

assert.ok(fs.existsSync(assetsDir), 'PC admin dist/assets missing after vite build');
const jsBundles = fs
  .readdirSync(assetsDir)
  .filter((name) => name.endsWith('.js'));
assert.ok(jsBundles.length > 0, 'vite build must emit at least one JS bundle');

const bundleSource = jsBundles
  .map((name) => fs.readFileSync(path.join(assetsDir, name), 'utf8'))
  .join('\n');
for (const needle of [
  'SDKWork Web Framework Console',
  'web-framework-console',
  'cors_policies',
  'traceId',
  'application/problem+json',
]) {
  assert.ok(
    bundleSource.includes(needle),
    `PC admin bundle must retain ${needle} (console shell + SDK transport)`,
  );
}

process.stdout.write('pc-admin-build.smoke.test.mjs passed\n');
