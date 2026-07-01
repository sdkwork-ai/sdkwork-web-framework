#!/usr/bin/env bash
# SDKWork web-framework verification entrypoint.
# Runs every command listed in specs/component.spec.json -> verification.commands.
# Mandatory steps fail hard; only the live Redis integration is opt-in (SDKWORK_REDIS_TEST_URL).
set -euo pipefail
cd "$(dirname "$0")/.."

unset SDKWORK_WEB_FRAMEWORK_ENV || true

echo "Running cargo test --workspace..."
cargo test --workspace

echo "Running architecture tests..."
cargo test -p sdkwork-web-architecture-tests

echo "Running bootstrap integration tests (contract fallback, production assembly)..."
cargo test -p sdkwork-web-bootstrap --test integration

echo "Running openapi authority contract tests..."
cargo test -p sdkwork-routes-web-framework-backend-api --test openapi_authority

echo "Running admin route manifest contract tests..."
cargo test -p sdkwork-routes-web-framework-backend-api --test routes_contract

echo "Running admin-api readiness integration test..."
cargo test -p sdkwork-web-bootstrap --features admin-api --test admin_api_readiness

echo "Running admin-server control-plane assembly tests..."
cargo test -p sdkwork-web-admin-server

echo "Checking PC admin operations.ts generation drift..."
node scripts/generate-pc-admin-operations.mjs --check

echo "Running database framework contract test..."
node tests/contract/database-framework.contract.test.mjs

echo "Running PC admin operations contract test..."
node tests/contract/pc-admin-operations.contract.test.mjs

echo "Running production rollout contract test..."
node tests/contract/production-rollout.contract.test.mjs

echo "Running release evidence contract tests..."
node tests/contract/release-evidence.contract.test.mjs
node tests/contract/adoption-evidence.contract.test.mjs
node scripts/collect-release-evidence.mjs

PC_APP="apps/sdkwork-web-framework-pc"
echo "Running PC admin console verify..."
if [ ! -d "$PC_APP/node_modules" ]; then
  (cd "$PC_APP" && npm ci)
elif [ ! -d "$PC_APP/node_modules/@playwright/test" ]; then
  (cd "$PC_APP" && npm install)
fi
(cd "$PC_APP" && npm run verify)
(cd "$PC_APP" && npm test)
node tests/contract/pc-admin-build.smoke.test.mjs

echo "Running PC admin Playwright E2E smoke..."
(cd "$PC_APP" && npm run test:e2e)

echo "Building PC admin console for integration E2E..."
node tests/contract/pc-admin-e2e-build.contract.test.mjs

echo "Running PC admin Playwright integration E2E..."
(cd "$PC_APP" && npm run test:e2e:integration)

# Live Redis integration tests run as part of `cargo test --workspace` above:
# when SDKWORK_REDIS_TEST_URL is set the tests exercise a live Redis,
# otherwise they self-skip (see crates/sdkwork-web-store-redis/tests/redis_live.rs).

echo "Running release pipeline benchmark..."
"$(dirname "$0")/benchmark-pipeline.sh"

echo "Running clippy..."
cargo clippy --workspace -- -D warnings
