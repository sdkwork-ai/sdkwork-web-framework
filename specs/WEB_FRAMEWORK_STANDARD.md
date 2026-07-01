# SDKWork Web Framework Standard

- Version: 1.0
- Scope: `sdkwork-web-framework` repository — HTTP/SaaS integration framework for all SDKWork API-capable repositories
- Status: implementing
- Authority: narrows `../sdkwork-specs/WEB_FRAMEWORK_SPEC.md`, `API_SPEC.md` §10, `WEB_BACKEND_SPEC.md`, `SECURITY_SPEC.md` §5.1; does not contradict root specs
- Related: [docs/architecture/tech/TECH-00-framework-foundation.md](../docs/architecture/tech/TECH-00-framework-foundation.md), [docs/architecture/tech/TECH-14-standards-system.md](../docs/architecture/tech/TECH-14-standards-system.md)

## 1. Purpose

This standard defines how SDKWork embeds Axum/Tower for multi-tenant SaaS APIs. Business repositories implement extension traits; the framework enforces pipeline order, context vocabulary, and secure defaults.

## 2. Dependency Rule

- Business repositories `MUST` depend on `sdkwork-web-framework`.
- `sdkwork-web-framework` `MUST NOT` depend on any business repository or business route crate.

## 3. WebRequestContext (mandatory)

Full specification: [docs/architecture/tech/TECH-03-web-request-context.md](../docs/architecture/tech/TECH-03-web-request-context.md).  
JSON Schema: [web-request-context.schema.json](./web-request-context.schema.json).

### 3.1 Structure

`WebRequestContext` `MUST` contain:

- `request_id` — server UUID v4
- `trace_id` — optional W3C trace id derived from inbound `traceparent` when present
- `api_surface`, `auth_mode`
- `transport` — path, method, credential presence flags
- `principal: Option<WebRequestPrincipal>` — `None` only on public routes

`WebRequestPrincipal` `MUST` be grouped as:

- `tenancy` — `tenant_id`, `organization_id`, `login_scope`
- `app` — `app_id`, `environment`, `deployment_mode`, optional `workspace_id` / `composition_id`
- `subject` — `user_id`, `session_id`, `subject_type`
- `auth` — `auth_level`, optional `api_key_id`
- `scopes` — `data_scope`, `permission_scope`

### 3.2 Tenant and app rules

- `tenant_id` and `app_id` `MUST` come from verified tokens or API key lookup, never from client path/query/body/header selectors.
- Protected app-api handlers `MUST` call `require_tenant_id()` and `require_app_id()` (or equivalent) before business logic.
- `login_scope` `MUST` be consistent with `organization_id`.

### 3.3 Automatic injection (all API handlers)

| Rule | Requirement |
| --- | --- |
| I1 | Routers under app/backend/open API prefixes `MUST` use `with_web_request_context`. |
| I2 | Every business handler `MUST` declare `WebRequestContext` (or `RequirePrincipal`) as a parameter. |
| I3 | Framework `MUST` implement `FromRequestParts<WebRequestContext>` reading Extensions. |
| I4 | `ContextInjection` stage `MUST` insert `WebRequestContext` before handler execution. |
| I5 | Public routes `MUST` still receive `WebRequestContext` with `principal: None`. |
| I6 | OpenAPI operations `MUST` declare `x-sdkwork-request-context: WebRequestContext`. |

Legacy alias `AppRequestContext` `MAY` exist for migration only.

### 3.4 Public routes (auth bypass)

Business APIs that do not require login `MUST` declare `RouteAuth::Public` on the matching `HttpRoute` in the route crate `manifest.rs`. Infrastructure paths (`/healthz`, `/readyz`, `/metrics`, WebSocket bootstrap prefixes, etc.) `MAY` remain in `WebRequestContextProfile::public_path_prefixes`.

| Rule | Requirement |
| --- | --- |
| P1 | Runtime `public_path` resolution `MUST` use manifest `RouteAuth` when `method + path` matches a manifest row. |
| P2 | Unmatched paths `MAY` fall back to `public_path_prefixes` (infra only). |
| P3 | Protected manifest routes `MUST NOT` be covered by a `public_path_prefix` (`HttpRouteManifest::validate_public_path_prefixes`). |
| P4 | Public routes `MUST` still run the full interceptor chain; only credential resolution, Authentication, Authorization, and TenantIsolation are skipped. |
| P5 | Public routes `MUST` receive `WebRequestContext` with `auth_mode: Public` and tenant-isolation principal from `Access-Token` JWT claims including required `token_version`. Semicolon claim-string `Access-Token` values `MUST` be rejected. Credential-entry routes with `forbidCredentialHeaders` `MUST` accept bootstrap `Access-Token` JWT but keep session `principal: None`. |
| P6 | Public handlers `MUST NOT` use `RequirePrincipal` or call `require_tenant_id()` / `require_app_id()`. |
| P7 | Auth-sensitive public operations (login, register, password reset) `SHOULD` set `rate_limit_tier: AuthCritical`. |
| P8 | Materialized OpenAPI for public operations `MUST` include `security: []` and `x-sdkwork-route-auth: public`. |
| P9 | Manifest path templates (`{param}`) `MUST` match concrete request paths for auth resolution. |
| P10 | Public routes `MUST NOT` fail CORS/cross-site origin checks before handler execution; cookie CSRF rules still apply. |

Assembly:

```rust
WebFramework::builder(resolver)
    .route_manifest(HttpRouteManifest::new(BUSINESS_ROUTES))
    .build();
```

No duplicate `public_path_prefixes` entry is required for manifest-declared public business routes.

## 4. Other mandatory types

| Type | Responsibility |
| --- | --- |
| `WebApiSurface` | `OpenApi` \| `AppApi` \| `BackendApi` \| `GatewayApi` \| `Unknown` |
| `TenantAppContext` | Service-layer view of tenant + app + subject ids |
| `WebFrameworkError` | Framework boundary errors → `application/problem+json` |
| `HttpRoute` | Route manifest row for OpenAPI materialization |
| `WebFrameworkRuntime` | Resolver, policies, stores, injectors assembly |

## 5. Mandatory Pipeline (18 stages, fixed order)

1. RequestIdentity  
2. SurfaceClassification  
3. Cors  
4. MethodGuard  
5. CrossSiteRequest  
6. SqlInjectionGuard  
7. RequestSizeLimit  
8. RateLimit  
9. Idempotency  
10. RequestContextResolution  
11. Authentication  
12. Authorization  
13. TenantIsolation  
14. ContextInjection  
15. Logging  
16. Audit  
17. HeaderSecurity  
18. ResponseIdentity  

Protected routers `MUST` use `WebCallInterceptorChain::standard()` or a documented strict superset.

## 6. Mandatory Extension Traits (business implements)

| Trait | When invoked |
| --- | --- |
| `WebRequestContextResolver` | Stage 10 |
| `AuthorizationPolicy` | Stage 12 |
| `TenantIsolationPolicy` | Stage 13 |
| `DomainContextInjector` | Stage 14 |
| `ApiKeyLookupService` | Stage 10 (open-api api-key) |
| `OAuthTokenLookupService` | Stage 10 (open-api oauth) |
| `OpenApiCredentialSchemeDetector` | Stage 10 (open-api flexible) |
| `TenantSigningKeyLookup` | Stage 10 JWT verify (auth/access/oauth); production SaaS `MUST` use tenant-bound keys (`HS256` secret or `RS256` SPKI via `kid`) |
| `JwtSessionRevocationChecker` | Stage 10 JWT verify after claim validation; production SaaS `MUST` wire IAM session revocation via `tenant_bound_saas_verifying_web_request_resolver()` |
| `ReadinessCheck` | `/readyz` assembly; production SaaS `MUST` wire via `WebFrameworkBuilder::readiness_check()` |

Production SaaS `MUST NOT` use dev-only claim-string resolvers or global shared HS256 secrets.

Production SaaS JWT claim policy `MUST` configure `iss`/`aud` through `JwtProductionClaimPolicy::saas_production(issuers, audiences)` via `tenant_bound_saas_verifying_web_request_resolver_with_claim_policy()`.

Production profiles `SHOULD` wire `WebFrameworkBuilder::request_timeout()` (default 30s via `production_defaults()`).

Application startup/shutdown hooks `MAY` implement `WebFrameworkLifecycle` (EP-20) and run through `WebFramework::run()` or `serve_with_lifecycle()`.

Production SaaS assembly `MUST` use `tenant_bound_saas_verifying_web_request_resolver_with_claim_policy()` with a real `JwtSessionRevocationChecker`, distributed-HA `RateLimitStore` / `IdempotencyStore` / `ConcurrentAdmissionStore` (`is_distributed_ha() == true`; typically `sdkwork-web-store-redis`), server-side `ApiKeyLookupService`, `JwtProductionClaimPolicy::saas_production()`, and `WebFrameworkBuilder::readiness_check()`. Control-plane standalone profiles `MAY` use `tenant_bound_verifying_web_request_resolver()` with `WebFrameworkOptionalFeatures::control_plane_standalone()`.

## 7. Handler and service rules

- Handlers `MUST` take `WebRequestContext` as a function parameter (auto-injected via `FromRequestParts`).
- Handlers `MUST NOT` use `Extension<WebRequestContext>` as the only pattern when `FromRequestParts` is available.
- Handlers `MUST NOT` parse `Authorization`, `Access-Token`, `X-API-Key`, or SDKWork identity projection headers.
- Services `MUST` accept `&WebRequestContext` or `TenantAppContext` for tenant/app scoping.
- Services `MUST NOT` depend on Axum request types.
- Repositories `MUST NOT` accept bare `tenant_id` without a context provenance.

## 8. API Surfaces

| Surface | Prefix | Auth |
| --- | --- | --- |
| app-api | `/app/v3/api` | Dual token (`Authorization` JWT + `Access-Token` JWT) |
| backend-api | `/backend/v3/api` | Dual token |
| open-api | configured prefixes | API key, OAuth bearer, or header-driven flexible (`RouteAuth::OpenApiFlexible`) |
| public / refresh-token | manifest `RouteAuth::Public` / `RefreshToken` | `Access-Token` JWT required for tenant isolation; session `Authorization` optional |
| infra | `public_path_prefixes` (`/healthz`, `/metrics`, …) | none |

Route crates for **business** capabilities `MUST NOT` live in `sdkwork-web-framework`.

**Exception (framework control-plane):** `sdkwork-routes-web-framework-backend-api` is the explicit framework-owned backend-api route crate for web-framework admin/control-plane surfaces (`/backend/v3/api/web-framework`). It follows the same `WebRequestContext`, manifest, OpenAPI, and security rules as application route crates. See `apis/backend-api/web-framework/` and `WEB_FRAMEWORK_SPEC.md` §6.

## 9. Secure Defaults

- CORS: deny-by-default.
- Request ID: server-generated UUID v4.
- Unauthenticated protected paths: 401 Problem+json.
- Oversized body: 413.
- Rate limit exceeded: 429 with `Retry-After` when applicable.
- Rate-limit / idempotency / audit store errors: fail-closed (`503` Problem+json via `DependencyUnavailable`); applications `MUST NOT` bypass stores in production.
- Production SaaS rate limit, idempotency, and tenant concurrent admission stores `MUST` report `is_distributed_ha() == true` (Redis adapters in `sdkwork-web-store-redis`); memory and SQLx adapters are dev/single-replica only.
- B12 JSON body context-selector inspection: single bounded buffer per request (required to re-inject body for downstream handlers); limit follows tenant runtime profile / global body cap.

## 10. Observability

- Logs `MUST` redact tokens and API keys.
- Metrics and logs `SHOULD` include `request_id`, `trace_id` (when known), `api_surface`, `operation_id` when known.
- All framework Problem+json error surfaces (pipeline, extractors, handlers, contract fallback, timeouts) `MUST` include `traceId` when available via `WebRequestContext` or inbound W3C `traceparent`.
- Raw URL paths with identifiers `MUST NOT` be logged; use route templates.

## 11. Verification

Framework repository canonical gate list: `specs/component.spec.json` → `verification.commands`.

```bash
scripts/verify.ps1   # Windows
scripts/verify.sh    # Unix
```

Business repository after integration:

- Contract test: pipeline order unchanged.
- Handler static rule: no raw credential header parsing in route crates.
- Open-api auth check: protected routes declare `api-key`, `oauth`, or `open-api-flexible`; security vectors cover missing credentials, API key resolution, OAuth bearer resolution, and flexible scheme selection.

## 12. Capability Matrix

Machine-readable catalog: [web-framework-capability.matrix.json](./web-framework-capability.matrix.json).

Human catalog: [docs/architecture/tech/TECH-13-capability-catalog.md](../docs/architecture/tech/TECH-13-capability-catalog.md).
