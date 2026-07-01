# SDKWork Web Framework
repository-kind: foundation-dependency

Rust SaaS HTTP/WebSocket framework for SDKWork API surfaces. Zero business dependencies.

**Repository type:** platform framework library workspace (not a product application).  
**Standards:** `../sdkwork-specs/WEB_FRAMEWORK_SPEC.md` (L0) → `specs/WEB_FRAMEWORK_STANDARD.md` (L1).

## Project layout

| Directory | Purpose |
| --- | --- |
| `crates/` | Framework Rust crates |
| `specs/` | L1 standard, capability matrix, component spec |
| `docs/` | Design docs and ADRs |
| `tests/` | Architecture and security verification |
| `apis/` | Framework-owned HTTP contract sources |
| `apps/` | Optional PC admin demo |
| `scripts/` | Verification entrypoints |
| `deployments/` | Packaging notes |
| `.sdkwork/` | Repository agent workspace metadata |

Intentionally absent at root (narrow-purpose framework repo): `sdks/`, `jobs/`, `tools/`, `plugins/` source packages, `examples/` — add when those capabilities ship.

## Crates

| Crate | Purpose |
| --- | --- |
| `sdkwork-web-contract` | Route manifest types + OpenAPI extensions |
| `sdkwork-web-core` | `WebRequestContext`, 18-stage pipeline, policies, stores |
| `sdkwork-web-axum` | Axum middleware + extractors + WebSocket upgrade |
| `sdkwork-web-bootstrap` | `WebFramework::builder`, health/metrics, contract fallback |
| `sdkwork-web-store-redis` | Redis rate limit + idempotency adapters |
| `sdkwork-web-store-sqlx` | SQLx store adapters (`web_*` tables) via `sdkwork-database-sqlx` pools |
| `sdkwork-web-test-utils` | Test runtime helpers |
| `sdkwork-routes-web-framework-backend-api` | Framework control-plane backend-api routes |
| `sdkwork-web-admin-server` | Standalone admin server binary |

## Quick start

Recommended pattern: declare an `HttpRoute` manifest, build `WebFramework`, and mount business routes with contract fallback (501/404 Problem+json for unimplemented manifest paths).

```rust
use axum::{routing::get, Router};
use sdkwork_web_bootstrap::{HttpMethod, HttpRoute, RouteAuth, WebFramework};
use sdkwork_web_core::{DefaultWebRequestContextResolver, HttpRouteManifest, WebRequestContext};

const ROUTES: &[HttpRoute] = &[HttpRoute::new(
    HttpMethod::Get,
    "/app/v3/api/ping",
    "Ping",
    "ping",
    RouteAuth::Public,
)];

let framework = WebFramework::builder(DefaultWebRequestContextResolver::default())
    .route_manifest(HttpRouteManifest::new(ROUTES))
    .build();

let app = framework.mount_service_routes(
    Router::new().route("/app/v3/api/ping", get(|ctx: WebRequestContext| async move {
        ctx.request_id.0
    })),
);
```

Production SaaS services should also call `.production_defaults()`, wire Redis/SQL stores, and follow `deployments/README.md`.

Release evidence: `node scripts/collect-release-evidence.mjs` (see `docs/architecture/tech/TECH-24-production-rollout-and-adoption.md`).

## Verification

Canonical gate list: `specs/component.spec.json` → `verification.commands`.

```bash
scripts/verify.ps1   # Windows
scripts/verify.sh    # Unix
```

Or run individual gates (workspace tests, architecture tests, contract tests, PC verify, release benchmark, clippy) as listed in `AGENTS.md`.

## Integration expectations

| Framework | This repo |
| --- | --- |
| `sdkwork-web-framework` | **Is** this repository |
| `sdkwork-database` | Pool creation in `sdkwork-web-store-sqlx` via `sdkwork-database-sqlx` |
| `sdkwork-utils` | `SdkWorkApiResponse`, `SdkWorkResultCode`, `serde_int64`, trace header constants |
| `sdkwork-discovery` | Not required (no RPC/gRPC services) |

## Standards

- [specs/WEB_FRAMEWORK_STANDARD.md](specs/WEB_FRAMEWORK_STANDARD.md)
- [docs/architecture/tech/TECH-00-design-index.md](docs/architecture/tech/TECH-00-design-index.md)
- [AGENTS.md](AGENTS.md)

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

## Application Roots

- [apps directory index](apps/README.md)
