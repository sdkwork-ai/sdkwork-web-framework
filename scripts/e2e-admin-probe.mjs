#!/usr/bin/env node
/** Probes assembled admin-server with E2E dual-token credentials (local debug / contract). */

import {
  E2E_ADMIN_BASE_URL,
  E2E_ADMIN_HEALTH_URL,
  e2eAdminAuthToken,
  e2eAccessToken,
} from "./e2e-constants.mjs";
import { waitForUrl } from "./e2e-wait-for-url.mjs";

const path = "/backend/v3/api/web-framework/runtime_defaults";

async function probe(url, label) {
  const response = await fetch(`${url}${path}`, {
    headers: {
      Authorization: `Bearer ${e2eAdminAuthToken()}`,
      "Access-Token": e2eAccessToken(),
      accept: "application/json",
    },
  });
  const body = await response.text();
  console.log(`[probe:${label}]`, response.status, body.slice(0, 300));
  if (!response.ok) {
    process.exit(1);
  }
}

try {
  await waitForUrl(E2E_ADMIN_HEALTH_URL, 5_000);
} catch {
  console.error(`admin-server not reachable at ${E2E_ADMIN_HEALTH_URL}`);
  process.exit(2);
}

await probe(E2E_ADMIN_BASE_URL, "direct");
console.log("e2e-admin-probe: OK");
