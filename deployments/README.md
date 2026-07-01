# Deployment

`sdkwork-web-framework` is primarily a **Rust library workspace**. Consumers embed framework crates in their API servers and gateways; this repository does not host product application topology.

## Shipped binaries

| Binary | Crate | Role |
| --- | --- | --- |
| `sdkwork-web-admin-server` | `crates/sdkwork-web-admin-server` | Optional standalone admin/control-plane HTTP server |

## Packaging

- CI verification: `.github/workflows/verify.yml`
- Release packaging contract: `sdkwork.workflow.json` (delegates to `sdkwork-github-workflow` on tag/release)
- Deployment profiles (`standalone` / `cloud`) follow `../sdkwork-specs/DEPLOYMENT_SPEC.md`

## Production assembly checklist

Complete before promoting a consumer service to production:

### 1. Quality gate

- [ ] Run `scripts/verify.ps1` (Windows) or `scripts/verify.sh` (Unix) from repository root — all gates in `specs/component.spec.json` must pass.
- [ ] Record release evidence per `../sdkwork-specs/QUALITY_GATE_SPEC.md` (commit SHA, verify log, benchmark output).

### 2. Framework builder

- [ ] Call `WebFrameworkBuilder::production_defaults()` for SaaS/production profiles.
- [ ] Set `SDKWORK_WEB_FRAMEWORK_ENV=prod` only after production defaults and `WebRequestContextProfile.environment = Prod` are wired.
- [ ] Use `tenant_bound_saas_verifying_web_request_resolver_with_claim_policy()` with IAM `TenantSigningKeyLookup` and `JwtSessionRevocationChecker` for multi-tenant SaaS — never dev/claim-string resolvers in production.
- [ ] Provide `route_manifest(...)` and mount handlers with `mount_service_routes(...)` so contract fallback returns 501/404 Problem+json for unimplemented manifest routes.

### 3. Durable stores (M3 SaaS)

- [ ] **Rate limiting:** Redis-backed `RateLimitStore` (`sdkwork-web-store-redis`) with HA Redis cluster.
- [ ] **Idempotency / admission:** Redis adapters with `is_distributed_ha() == true`; configure connection pools via `sdkwork-database-sqlx` for SQL paths.
- [ ] **JWT / signing:** configure tenant signing key lookup and session revocation checker — do not rely on in-memory defaults.

### 4. Observability

- [ ] Enable structured tracing via `init_tracing_from_env()` at process start.
- [ ] For distributed traces, enable the `otel` feature on bootstrap/admin-server and export per `../sdkwork-specs/OBSERVABILITY_SPEC.md`.
- [ ] Expose `/healthz` and `/readyz` through `service_router`; register custom `ReadinessCheck` implementations for Redis/SQL dependencies.

### 5. Security defaults

- [ ] CORS: production profile must not set `allow_all_origins`; use explicit origin allowlists.
- [ ] Request body size, timeout, concurrent admission, and WebSocket message limits set explicitly for the deployment profile.
- [ ] Problem+json responses must include numeric `code` and server-owned `traceId` per `API_SPEC.md` §15 — no bare error mapping bypassing `WebRequestContext`.

### 6. Handoff artifacts

- [ ] Pin framework crate version in consumer `Cargo.toml`.
- [ ] Attach `CHANGELOG.md` entry for the consumed release train.
- [ ] Document consumer-specific env keys (`SDKWORK_WEB_*`, database URLs, Redis URLs) in the consumer repo — not in this framework repo.
- [ ] Complete rollout phases and adoption evidence per [docs/architecture/tech/TECH-24-production-rollout-and-adoption.md](../docs/architecture/tech/TECH-24-production-rollout-and-adoption.md).

## Reference integration

```rust
use axum::{routing::get, Router};
use sdkwork_web_bootstrap::WebFramework;
use sdkwork_web_core::{
    jwt_claims::JwtProductionClaimPolicy, jwt_tenant::NoOpJwtSessionRevocationChecker,
    tenant_bound_saas_verifying_web_request_resolver_with_claim_policy, HttpRouteManifest,
    WebRequestContext,
};

// Wire IAM TenantSigningKeyLookup + JwtSessionRevocationChecker in production.
let lookup = /* your TenantSigningKeyLookup */;
let revocation = /* your JwtSessionRevocationChecker */;
let api_key_lookup = /* your ApiKeyLookupService */;

let resolver = tenant_bound_saas_verifying_web_request_resolver_with_claim_policy(
    lookup,
    revocation,
    api_key_lookup,
    JwtProductionClaimPolicy::saas_production(
        vec!["https://iam.example".into()],
        vec!["sdkwork-api".into()],
    ),
);

let framework = WebFramework::builder(resolver)
    .production_defaults()
    .route_manifest(HttpRouteManifest::new(ROUTES))
    .readiness_check(/* Redis/SQL ReadinessCheck */)
    .build();

let app = framework.mount_service_routes(
    Router::new().route("/app/v3/api/ping", get(|ctx: WebRequestContext| async move {
        ctx.request_id.0
    })),
);
```

See [docs/architecture/tech/TECH-22-bootstrap-and-routing.md](../docs/architecture/tech/TECH-22-bootstrap-and-routing.md) and `specs/WEB_FRAMEWORK_STANDARD.md` for full assembly options.

## Operations

- **Runbook:** [docs/architecture/tech/TECH-21-operations-runbook.md](../docs/architecture/tech/TECH-21-operations-runbook.md) — health/readiness, metrics, OTel, graceful shutdown, troubleshooting
- **Production rollout:** [docs/architecture/tech/TECH-24-production-rollout-and-adoption.md](../docs/architecture/tech/TECH-24-production-rollout-and-adoption.md) — Pre-flight → Canary → Rollback + M4 adoption evidence
- **Env template:** [configs/admin-server.env.example](../configs/admin-server.env.example)
- **Endpoints:** `/healthz`, `/readyz`, `/metrics` (via `mount_service_routes` / `WebFramework::run`)

## Admin server production notes

The bundled `sdkwork-web-admin-server` binary is a **control-plane bootstrap** profile (`control_plane_standalone`):

- Default SQLite file store is single-node; use Postgres + connection pooling for HA control planes.
- Wire `SDKWORK_WEB_FRAMEWORK_REDIS_URL` for distributed rate limiting and idempotency across replicas.
- Bind to loopback or restrict network exposure; JWT uses bootstrap tenant signing via env secret.
- Uses `DisabledApiKeyLookupService` — open-api API key auth is not enabled on this binary.
- Assembly is exposed as `sdkwork_web_admin_server::assemble_control_plane()` for integration tests and custom entrypoints.
- For multi-tenant SaaS API servers, embed framework crates in your service with the SaaS resolver checklist above — do not reuse admin-server wiring.
