# Changelog

All notable changes to `sdkwork-web-framework` follow [Keep a Changelog](https://keepachangelog.com/) conventions and align with `../sdkwork-specs/RELEASE_SPEC.md`.

## 0.1.0 — 2026-06-23

M3 production-readiness alignment for the platform web framework (Phase I exit).

### Added

- **Problem correlation (E/G):** `ProblemCorrelation`, `WebRequestContext::trace_id`, numeric `code` and server-owned `traceId` on every Problem+json response; W3C `traceparent` propagation into `traceId`.
- **Correlation-safe Axum surface:** `WebFrameworkRejection` for extractors; handler helpers `finish_api_json` / `finish_api_response`; removed bare `IntoResponse` for `WebFrameworkError` and `ApiProblem`.
- **Contract fallback (F3):** `ContractFallbackConfig`, auto-wiring from `WebFrameworkBuilder::route_manifest`, Axum `.fallback()` via `service_router`; `NotImplemented` (501) and manifest-aware 404 with standard Problem type URIs.
- **Observability bootstrap:** `init_tracing_from_env()`; admin-server `otel` feature gate.
- **Architecture guards:** `problem_correlation_rules`, `bootstrap_contract_fallback`, extended `handler_static_rules` and `problem_snapshot` coverage.
- **PC admin layering:** `useWebFrameworkAdmin` → `web-framework-admin-service` → composed backend SDK; contract test for operations and `traceId` parsing.
- **Release evidence:** `pipeline_benchmark` in default `scripts/verify.*`; `CHANGELOG.md` and deployment handoff checklist.

### Changed

- Admin API handlers return correlated `Response` via shared finish helpers.
- Timeout middleware preserves `OwnedProblemCorrelation` across cancellation.
- `specs/web-request-context.schema.json` documents optional `traceId`.
- Capability matrix and maturity docs reflect M3 framework GA threshold for core domains.

### Security

- Production Problem responses never use `about:blank` or leak stack traces to clients.
- Header fuzz and security vector suites enforce forged-header rejection and CORS secure defaults.

### Verification

Run `scripts/verify.ps1` or `scripts/verify.sh` — commands mirror `specs/component.spec.json` → `verification.commands`.

## Unreleased

### Added

- **Manifest-driven PC Admin SDK (K14):** `scripts/generate-pc-admin-operations.mjs` generates `operations.ts` from `routes.manifest.json`; verify gate runs `--check`.
- **PC admin build smoke (K15):** `tests/contract/pc-admin-build.smoke.test.mjs` validates Vite dist shell and SDK transport markers.
- **PC admin Playwright E2E (K16):** `apps/sdkwork-web-framework-pc/e2e/console.smoke.spec.ts` loads preview shell with mocked backend and permission-gated tabs.
- **PC admin real-backend Playwright E2E (K17):** `scripts/e2e-web-stack.mjs` boots `assemble_control_plane` admin-server; integration spec exercises dual-token SDK against live HTTP.
- **E2E JWT alignment:** integration credentials use `login_scope: ORGANIZATION` (backend-api rejects TENANT sessions per `EnforcePrincipalTenantIsolationPolicy`); integration preview uses port `4176` and never reuses smoke preview.
- **Production rollout / adoption (K18):** `docs/architecture/tech/TECH-24-production-rollout-and-adoption.md`, `specs/production-adoption.evidence.template.json`, and `production-rollout.contract.test.mjs` for M4 commercial handoff.
- **Release evidence bundle (K19):** `scripts/collect-release-evidence.mjs`, `scripts/validate-adoption-evidence.mjs`, and `specs/framework-adoption.evidence.json` (admin-server + PC console pathfinder adoptions).
- **Test env isolation:** `sdkwork-web-test-utils::IsolatedDeploymentEnv` stabilizes dev-builder tests when `SDKWORK_WEB_FRAMEWORK_ENV=prod` is set.
- **Optional live Redis tests:** `crates/sdkwork-web-store-redis/tests/redis_live.rs` runs when `SDKWORK_REDIS_TEST_URL` is set.
- **Admin server HTTP smoke test:** `assemble_control_plane()` serves `/healthz` and `/readyz` in unit tests.
- **WebSocket security evidence (K13/W5):** architecture `ws_security_vectors` tracked in capability matrix and capability catalog.
- **Redis store key namespace test** for idempotency records.
- **CI gate:** bootstrap integration tests (`contract fallback`, production assembly) in verify scripts and `component.spec.json`
- **Architecture guards:** K12 commercial-GA-readiness; admin-api auto `route_manifest` static check

### Added (Round 9)

- **COMPONENT_SPEC alignment:** `specs/README.md`, expanded `canonicalSpecs`, `runtimeEntrypoints`
- **Integration docs:** `docs/architecture/tech/TECH-22-bootstrap-and-routing.md`, `docs/architecture/tech/TECH-23-consumer-integration-template.md`（修复 deployments 断链）
- **Admin-api E2E:** `enable_admin_api` contract fallback 501 集成测试
