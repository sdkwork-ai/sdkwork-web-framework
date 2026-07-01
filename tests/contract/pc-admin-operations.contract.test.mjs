#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const manifestPath = path.join(
  repoRoot,
  'apis/backend-api/web-framework/routes.manifest.json',
);
const operationsPath = path.join(
  repoRoot,
  'apps/sdkwork-web-framework-pc/src/sdk/backend-sdk/operations.ts',
);
const pathsRsPath = path.join(
  repoRoot,
  'crates/sdkwork-routes-web-framework-backend-api/src/paths.rs',
);

const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const operationsSource = fs.readFileSync(operationsPath, 'utf8');
const pathsSource = fs.readFileSync(pathsRsPath, 'utf8');
const apiPrefixMatch = operationsSource.match(
  /export const WEB_FRAMEWORK_ADMIN_API_PREFIX = "([^"]+)"/,
);
assert.ok(apiPrefixMatch, 'WEB_FRAMEWORK_ADMIN_API_PREFIX must be declared');
const apiPrefix = apiPrefixMatch[1];
const rustApiPrefixMatch = pathsSource.match(
  /pub const API_PREFIX: &str = "([^"]+)"/,
);
assert.ok(rustApiPrefixMatch, 'paths.rs must declare API_PREFIX');
assert.equal(
  rustApiPrefixMatch[1],
  apiPrefix,
  'PC WEB_FRAMEWORK_ADMIN_API_PREFIX must match Rust paths::API_PREFIX',
);

function extractOperationIds(source) {
  const block = source.match(
    /export const webFrameworkAdminOperationIds = \{([\s\S]*?)\} as const;/,
  );
  assert.ok(block, 'webFrameworkAdminOperationIds block must exist');
  return [...block[1].matchAll(/:\s*"([^"]+)"/g)].map((match) => match[1]);
}

function extractStaticPaths(source) {
  const matches = [
    ...source.matchAll(/`\$\{WEB_FRAMEWORK_ADMIN_API_PREFIX\}([^`]+)`/g),
    ...source.matchAll(new RegExp(`"${apiPrefix.replace(/\//g, '\\/')}([^"]+)"`, 'g')),
  ];
  return matches.map((match) => `${apiPrefix}${match[1]}`);
}

function normalizeManifestPath(pathValue) {
  return pathValue;
}

function normalizeOperationsPath(pathValue) {
  return pathValue
    .replace('${encodeURIComponent(nodeId)}', '{nodeId}')
    .replace(/\$\{WEB_FRAMEWORK_ADMIN_API_PREFIX\}/g, `${apiPrefix}/`);
}

const manifestOperationIds = new Set(manifest.map((row) => row.operationId));
const manifestPaths = new Set(manifest.map((row) => normalizeManifestPath(row.path)));

for (const operationId of extractOperationIds(operationsSource)) {
  assert.ok(
    manifestOperationIds.has(operationId),
    `operations.ts operationId ${operationId} missing from routes.manifest.json`,
  );
}

for (const staticPath of extractStaticPaths(operationsSource)) {
  const normalized = normalizeOperationsPath(staticPath);
  assert.ok(
    [...manifestPaths].some((manifestPath) => manifestPath === normalized),
    `operations.ts path ${staticPath} missing from routes.manifest.json`,
  );
}

for (const row of manifest) {
  assert.ok(row.operationId, 'manifest row must declare operationId');
  assert.ok(
    row.path.startsWith(apiPrefix),
    `manifest row ${row.operationId} must use PC admin API prefix`,
  );
  assert.ok(
    row.requiredPermission && String(row.requiredPermission).startsWith('web-framework.'),
    `manifest row ${row.operationId} must declare framework-scoped requiredPermission`,
  );
}

assert.equal(
  manifest.length,
  manifestOperationIds.size,
  'routes.manifest.json must not contain duplicate operationIds',
);

const pcSrcRoot = path.join(repoRoot, 'apps/sdkwork-web-framework-pc/src');
const mainSource = fs.readFileSync(path.join(pcSrcRoot, 'main.tsx'), 'utf8');
const hookSource = fs.readFileSync(path.join(pcSrcRoot, 'hooks/useWebFrameworkAdmin.ts'), 'utf8');
const serviceSource = fs.readFileSync(
  path.join(pcSrcRoot, 'services/web-framework-admin-service.ts'),
  'utf8',
);
const transportSource = fs.readFileSync(
  path.join(pcSrcRoot, 'sdk/backend-sdk/transport.ts'),
  'utf8',
);

assert.ok(
  hookSource.includes('web-framework-admin-service'),
  'useWebFrameworkAdmin hook must delegate to web-framework-admin-service',
);
assert.ok(
  serviceSource.includes('backend-sdk'),
  'web-framework-admin-service must consume backend SDK (no raw HTTP)',
);
assert.doesNotMatch(
  mainSource,
  /from\s+["'].*backend-sdk/,
  'main.tsx must not import backend SDK directly (UI → hook → service → SDK)',
);
assert.ok(
  hookSource.includes('devAuth'),
  'useWebFrameworkAdmin hook must use devAuth for local permission gating',
);
assert.ok(
  transportSource.includes('traceId'),
  'backend SDK transport must surface traceId from Problem+json responses',
);

process.stdout.write('pc-admin-operations.contract.test.mjs passed\n');
