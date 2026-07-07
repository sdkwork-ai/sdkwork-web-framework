#!/usr/bin/env node
/**
 * Align web-framework OpenAPI list operation query params with PAGINATION_SPEC / API_SPEC §14.1.
 */
import fs from 'node:fs';
import path from 'node:path';

const openapiPath = path.join(
  process.cwd(),
  'apis/backend-api/web-framework/openapi.json',
);
const doc = JSON.parse(fs.readFileSync(openapiPath, 'utf8'));

const offsetListParams = [
  {
    in: 'query',
    name: 'environment',
    required: false,
    schema: { type: 'string' },
  },
  {
    in: 'query',
    name: 'tenant_id',
    required: false,
    schema: { type: 'string' },
  },
  {
    in: 'query',
    name: 'page',
    required: false,
    schema: { minimum: 1, type: 'integer' },
  },
  {
    in: 'query',
    name: 'page_size',
    required: false,
    schema: { maximum: 200, minimum: 1, type: 'integer' },
  },
  {
    in: 'query',
    name: 'limit',
    required: false,
    deprecated: true,
    description: 'Legacy alias for page_size (offset mode).',
    schema: { maximum: 200, minimum: 1, type: 'integer' },
  },
];

const offsetListParamsNoTenant = offsetListParams.filter(
  (param) => param.name !== 'tenant_id',
);

const keysetListParams = [
  {
    in: 'query',
    name: 'tenant_id',
    required: false,
    schema: { type: 'string' },
  },
  {
    in: 'query',
    name: 'page_size',
    required: false,
    schema: { maximum: 200, minimum: 1, type: 'integer' },
  },
  {
    in: 'query',
    name: 'limit',
    required: false,
    deprecated: true,
    description: 'Legacy alias for page_size (cursor mode).',
    schema: { maximum: 200, minimum: 1, type: 'integer' },
  },
  {
    in: 'query',
    name: 'cursor',
    required: false,
    schema: { type: 'string' },
    description: 'Opaque keyset cursor (audit/security event id).',
  },
];

const keysetListParamsSecurity = keysetListParams.filter(
  (param) => param.name !== 'tenant_id',
);

const assignments = {
  '/backend/v3/api/web-framework/audit_events': keysetListParams,
  '/backend/v3/api/web-framework/security_events': keysetListParamsSecurity,
  '/backend/v3/api/web-framework/control_nodes': offsetListParamsNoTenant,
  '/backend/v3/api/web-framework/cors_policies': offsetListParams,
  '/backend/v3/api/web-framework/rate_limit_policies': offsetListParams,
  '/backend/v3/api/web-framework/tenant_runtime_profiles': offsetListParams,
};

for (const [routePath, parameters] of Object.entries(assignments)) {
  const getOp = doc.paths?.[routePath]?.get;
  if (!getOp) {
    throw new Error(`missing GET ${routePath}`);
  }
  getOp.parameters = parameters;
}

fs.writeFileSync(openapiPath, `${JSON.stringify(doc, null, 2)}\n`);
console.log('aligned openapi list pagination parameters');
