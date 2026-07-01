#!/usr/bin/env node
/**
 * Generates PC admin `operations.ts` from `routes.manifest.json` (contract-first, SDK_SPEC aligned).
 * Usage:
 *   node scripts/generate-pc-admin-operations.mjs          # write file
 *   node scripts/generate-pc-admin-operations.mjs --check  # fail if drift
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(
  repoRoot,
  'apis/backend-api/web-framework/routes.manifest.json',
);
const pathsRsPath = path.join(
  repoRoot,
  'crates/sdkwork-routes-web-framework-backend-api/src/paths.rs',
);
const outputPath = path.join(
  repoRoot,
  'apps/sdkwork-web-framework-pc/src/sdk/backend-sdk/operations.ts',
);

const checkOnly = process.argv.includes('--check');

function readApiPrefix() {
  const pathsSource = fs.readFileSync(pathsRsPath, 'utf8');
  const match = pathsSource.match(/pub const API_PREFIX: &str = "([^"]+)"/);
  assert.ok(match, 'paths.rs must declare API_PREFIX');
  return match[1];
}

function capitalize(value) {
  return value.length === 0 ? value : value[0].toUpperCase() + value.slice(1);
}

function operationIdToFlatKey(operationId) {
  const parts = operationId.split('.');
  assert.equal(parts[0], 'webFramework', `unexpected operationId prefix: ${operationId}`);
  assert.equal(parts.length, 3, `operationId must be webFramework.<domain>.<action>: ${operationId}`);
  return `${parts[1]}${capitalize(parts[2])}`;
}

function pathToExpression(fullPath, apiPrefix) {
  assert.ok(fullPath.startsWith(apiPrefix), `path must start with API_PREFIX: ${fullPath}`);
  const suffix = fullPath.slice(apiPrefix.length);
  const paramMatch = suffix.match(/\{([^}]+)\}/g);
  if (!paramMatch) {
    return `\`\${WEB_FRAMEWORK_ADMIN_API_PREFIX}${suffix}\``;
  }
  const params = [...new Set(paramMatch.map((token) => token.slice(1, -1)))];
  assert.ok(
    params.every((name) => name === 'nodeId'),
    `only {nodeId} path params are supported in generator: ${fullPath}`,
  );
  const argName = 'nodeId';
  const template = suffix.replace(/\{nodeId\}/g, '${encodeURIComponent(nodeId)}');
  return `(${argName}: string) =>\n      \`\${WEB_FRAMEWORK_ADMIN_API_PREFIX}${template}\``;
}

function buildOperationsTree(manifest, apiPrefix) {
  const tree = {};
  const domainOrder = [];
  const actionOrder = new Map();

  for (const row of manifest) {
    const parts = row.operationId.split('.');
    const domain = parts[1];
    const action = parts[2];
    if (!tree[domain]) {
      tree[domain] = {};
      domainOrder.push(domain);
      actionOrder.set(domain, []);
    }
    tree[domain][action] = pathToExpression(row.path, apiPrefix);
    actionOrder.get(domain).push(action);
  }

  return { tree, domainOrder, actionOrder };
}

function renderNestedObject(tree, domainOrder, actionOrder, indent) {
  const lines = [];
  for (const domain of domainOrder) {
    lines.push(`${indent}${domain}: {`);
    for (const action of actionOrder.get(domain)) {
      const expr = tree[domain][action];
      if (expr.includes('\n')) {
        lines.push(`${indent}  ${action}: ${expr},`);
      } else {
        lines.push(`${indent}  ${action}: ${expr},`);
      }
    }
    lines.push(`${indent}},`);
  }
  return lines.join('\n');
}

function renderOperationIds(manifest) {
  return manifest
    .map((row) => [operationIdToFlatKey(row.operationId), row.operationId])
    .map(([key, value]) => `  ${key}: "${value}",`)
    .join('\n');
}

function renderFile(manifest, apiPrefix) {
  const { tree, domainOrder, actionOrder } = buildOperationsTree(manifest, apiPrefix);
  return `/** AUTO-GENERATED from apis/backend-api/web-framework/routes.manifest.json — do not edit manually. */
/** Regenerate: node scripts/generate-pc-admin-operations.mjs */
export const WEB_FRAMEWORK_ADMIN_API_PREFIX = "${apiPrefix}";

export const webFrameworkAdminOperations = {
${renderNestedObject(tree, domainOrder, actionOrder, '  ')}
} as const;

export const webFrameworkAdminOperationIds = {
${renderOperationIds(manifest)}
} as const;
`;
}

function normalizeNewlines(value) {
  return value.replace(/\r\n/g, '\n');
}

function main() {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const apiPrefix = readApiPrefix();
  const rendered = renderFile(manifest, apiPrefix);

  if (checkOnly) {
    const existing = normalizeNewlines(fs.readFileSync(outputPath, 'utf8'));
    assert.equal(
      existing.trimEnd(),
      normalizeNewlines(rendered).trimEnd(),
      'operations.ts is out of date; run node scripts/generate-pc-admin-operations.mjs',
    );
    process.stdout.write('generate-pc-admin-operations.mjs --check passed\n');
    return;
  }

  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, rendered, 'utf8');
  process.stdout.write(`wrote ${path.relative(repoRoot, outputPath)}\n`);
}

main();
