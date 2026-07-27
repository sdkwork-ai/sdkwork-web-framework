# SDKWork Web Framework Standard

- Version: 1.0
- Scope: `sdkwork-web-framework` repository — HTTP/SaaS integration framework for all SDKWork API-capable repositories
- Status: implementing
- Authority: narrows `../sdkwork-specs/WEB_FRAMEWORK_SPEC.md`, `API_SPEC.md` §10, `WEB_BACKEND_SPEC.md`, `SECURITY_SPEC.md` §5.1; does not contradict root specs
- Related: [docs/architecture/tech/TECH-00-framework-foundation.md](../docs/architecture/tech/TECH-00-framework-foundation.md), [docs/architecture/tech/TECH-14-standards-system.md](../docs/architecture/tech/TECH-14-standards-system.md)

Internationalization authority: `../sdkwork-specs/I18N_SPEC.md`. This L1 standard defines the framework runtime hooks required to enforce it.

## 1. Purpose

This standard defines how SDKWork embeds Axum/Tower for multi-tenant SaaS APIs. Business repositories implement extension traits; the framework enforces pipeline order, context vocabulary, locale negotiation, response mapping, and secure defaults.

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

- `locale: WebLocaleContext` - effective locale, fallback locale, active locales, source, and message bundle version metadata

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

### 3.4 Locale context

`WebRequestContext.locale` `MUST` follow `../sdkwork-specs/I18N_SPEC.md`:

```text
WebLocaleContext {
  requestedLocale?: LocaleTag
  effectiveLocale: LocaleTag
  fallbackLocale: LocaleTag
  supportedLocales: LocaleTag[]
  activeLocales: LocaleTag[]
  source: user-preference | tenant-preference | app-default | accept-language | sdk-header | system-default
  catalogVersion?: string
  messageBundleVersion?: string
  timezone?: string
  numberingSystem?: string
}
```

Rules:

- Locale resolution `MUST` run inside RequestContextResolution before ContextInjection.
- Public and protected routes `MUST` receive locale context.
- Resolver precedence is authenticated user preference, tenant/application preference, approved SDK/host runtime locale, `Accept-Language`, application default, explicit fallback.
- Handlers `MUST NOT` parse `Accept-Language`, `X-SdkWork-Locale`, cookies, query parameters, or user-agent language values.
- Localized responses `MUST` emit `Content-Language`; language-varying responses `MUST` emit `Vary: Accept-Language`.

### 3.5 Public routes (auth bypass)

Business APIs that do not require login `MUST` declare `RouteAuth::Public` on the matching `HttpRoute` in the route crate `manifest.rs`. Infrastructure paths (`/healthz`, `/readyz`, `/metrics`, WebSocket bootstrap prefixes, etc.) `MAY` remain in `WebRequestContextProfile::public_path_prefixes`.

| Rule | Requirement |
| --- | --- |
| P1 | Runtime `public_path` resolution `MUST` use manifest `RouteAuth` when `method + path` matches a manifest row. |
| P2 | Unmatched paths `MAY` fall back to `public_path_prefixes` (infra only). |
| P3 | Protected manifest routes `MUST NOT` be covered by a `public_path_prefix` (`HttpRouteManifest::validate_public_path_prefixes`). |
| P4 | Public routes `MUST` still run the full interceptor chain; only credential resolution, Authentication, Authorization, and TenantIsolation are skipped. |
| P5 | Public routes `MUST` receive `WebRequestContext` with `auth_mode: Public` and `principal: None`. `Authorization`, `Access-Token`, API-key, OAuth, ingress-token, agent-token, and client context projection headers are rejected; anonymous never means optional session credentials. |
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

### 3.6 Bootstrap-body, credential-entry, and refresh profiles

Pre-session operations use the auth profile declared by their owning API contract. Operations
classified as credential-entry `MUST` declare `RouteAuth::CredentialEntryBootstrap` through
`HttpRoute::credential_entry_bootstrap(...)`; operations classified as anonymous use
`RouteAuth::Public`. The framework does not classify business operations by name or path.

| Profile | Allowed proof | Runtime context | Forbidden credentials |
| --- | --- | --- | --- |
| `BootstrapBody` | Explicit typed body credential on backend-api only | `WebAuthMode::BootstrapBody`; framework credential resolution is skipped and the handler validates the body credential, operator permission, and tenant scope | Every credential/context header |
| `CredentialEntryBootstrap` | Bootstrap `Access-Token` JWT only | `WebAuthMode::CredentialEntryBootstrap`; tenant/app isolation is resolved from the verified bootstrap JWT | Session `Authorization`, refresh proof, API key, OAuth bearer, ingress/agent token, and client context projection |
| `RefreshToken` | The route-declared refresh proof only | `WebAuthMode::RefreshToken`; the IAM handler/adapter validates and projects the refresh session | `Authorization`, `Access-Token`, API key, OAuth bearer, ingress/agent token, and client context projection |

Rules:

- `BootstrapBody` is not anonymous and is valid only for backend-api operations whose owning
  contract explicitly declares `bootstrap-body`. It materializes `security: []`,
  `x-sdkwork-auth-mode: bootstrap-body`, and `x-sdkwork-forbid-credential-headers: true`.
- Bootstrap-body handlers must validate the typed body credential before any business mutation and
  must enforce operation permission and tenant scope. Browser-facing admin flows must use
  dual-token management operations instead of collecting bootstrap credentials.
- Missing bootstrap `Access-Token` fails with `40101` before handler dispatch.
- Expired, invalid, and revoked bootstrap/session credentials use `40102`, `40103`, and `40104`
  respectively when the resolver can distinguish them.
- `HttpRoute::credential_entry_public(...)` is a migration alias only. It `MUST` construct the
  first-class credential-entry profile and `MUST NOT` materialize anonymous OpenAPI metadata.
- Profile selection comes only from the matched route manifest. Header presence never changes the
  selected profile.
- The stage-2 contamination guard validates the complete profile allowlist and rejects unexpected
  credential or context headers before request-context resolution or handler execution.

### 3.7 Vendor compatibility profile

`RouteAuth::Compatibility` is reserved for operation-level external wire protocols governed by
`API_SPEC.md` section 4.5.2. It is never an anonymous escape hatch.

- A compatibility route `MUST` use `HttpRoute::compatibility(...)` and declare a lowercase
  kebab-case external protocol id, named security schemes, and explicit security requirement rows.
- Scheme names inside one requirement row are AND; multiple requirement rows are OR, matching
  OpenAPI semantics exactly.
- The route `MUST` provide the exact upstream OpenAPI operation JSON. The materializer preserves
  its request/response/status shapes and adds `x-sdkwork-wire-protocol: external`,
  `x-sdkwork-external-protocol-id`, canonical security, and framework context extensions.
- Missing or invalid compatibility metadata fails framework assembly and OpenAPI materialization.
  Compatibility `MUST NOT` materialize as `security: []`.
- The framework extracts only credentials declared by the selected requirement and calls
  `WebRequestContextResolver::resolve_compatibility(...)`. A missing adapter fails as dependency
  unavailable; handlers never parse vendor credential headers themselves.
- Undeclared standard credential headers fail the stage-2 contamination guard. Missing declared
  credentials fail with `40101` before handler dispatch.
- Compatibility is valid only on an approved open-api surface. SDKWork-owned app-api,
  backend-api, internal-api, gateway-api, and business open-api routes use canonical profiles.

## 4. Other mandatory types

| Type | Responsibility |
| --- | --- |
| `WebApiSurface` | `OpenApi` \| `AppApi` \| `BackendApi` \| `InternalApi` \| `GatewayApi` \| `Unknown` |
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

`LocaleResolution` is a required sub-stage of RequestContextResolution. It does not change the fixed 18-stage order.

## 6. Mandatory Extension Traits (business implements)

| Trait | When invoked |
| --- | --- |
| `WebRequestContextResolver` | Stage 10 |
| `AuthorizationPolicy` | Stage 12 |
| `TenantIsolationPolicy` | Stage 13 |
| `DomainContextInjector` | Stage 14 |
| `ApiKeyLookupService` | Stage 10 (open-api api-key and internal-api ingress token) |
| `OAuthTokenLookupService` | Stage 10 (open-api oauth) |
| `OpenApiCredentialSchemeDetector` | Stage 10 (open-api flexible) |
| `LocaleResolver` | Stage 10 |
| `UserLocalePreferenceProvider` | Stage 10/11 |
| `TenantLocalePreferenceProvider` | Stage 10/11 |
| `MessageBundleProvider` | response/error boundary |
| `LocalizedProblemMapper` | response/error boundary |
| `ValidationMessageResolver` | extractor/validation boundary |
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
- Handlers `MUST NOT` parse `Authorization`, `Access-Token`, `X-API-Key`, `X-SDKWork-Access-Token`, SDKWork identity projection headers, locale headers, cookies, query parameters, or user-agent language values to resolve context.
- Handlers `MUST` consume locale, timezone, numbering system, and message bundle version from `WebRequestContext.locale`.
- Services `MUST` accept `&WebRequestContext` or `TenantAppContext` for tenant/app scoping.
- Services `MUST NOT` depend on Axum request types.
- Repositories `MUST NOT` accept bare `tenant_id` without a context provenance.

## 8. API Surfaces

| Surface | Prefix | Auth |
| --- | --- | --- |
| app-api | `/app/v3/api` | Anonymous, credential-entry-bootstrap, refresh-token, or dual token (`Authorization` JWT + `Access-Token` JWT) |
| backend-api | `/backend/v3/api` | Dual token |
| internal-api | `/internal/v3/api` | `X-SDKWork-Ingress-Token`; resolved through the ingress-token resolver |
| open-api | configured prefixes | API key, OAuth bearer, or header-driven flexible (`RouteAuth::OpenApiFlexible`) |
| anonymous | manifest `RouteAuth::Public` | No credential; `principal: None` |
| credential entry | manifest `RouteAuth::CredentialEntryBootstrap` | Bootstrap `Access-Token` JWT only |
| refresh token | manifest `RouteAuth::RefreshToken` | Declared refresh proof only; no automatic credential headers |
| infra | `public_path_prefixes` (`/healthz`, `/metrics`, …) | none |

Route crates for **business** capabilities `MUST NOT` live in `sdkwork-web-framework`.

**Exception (framework control-plane):** `sdkwork-routes-web-framework-backend-api` is the explicit framework-owned backend-api route crate for web-framework admin/control-plane surfaces (`/backend/v3/api/web-framework`). It follows the same `WebRequestContext`, manifest, OpenAPI, and security rules as application route crates. See `apis/backend-api/web-framework/` and `WEB_FRAMEWORK_SPEC.md` §6.

## 9. Secure Defaults

- CORS: deny-by-default.
- Development and test runtimes use the framework private-network-origin policy to allow
  loopback, RFC 1918 IPv4, and IPv6 unique-local-address origins on arbitrary numeric dev-server
  ports. The framework evaluates each request Origin, so DHCP address changes require no copied
  application allowlist. Development origin directives are invalid in production; production
  CORS remains an exact-origin allowlist.
- Responses that echo an allowed Origin must merge `Origin` into `Vary`, preserve existing `Vary`
  values, and may emit `Access-Control-Allow-Credentials: true` only with a concrete echoed Origin.
- A CORS preflight is an `OPTIONS` request carrying both `Origin` and
  `Access-Control-Request-Method`. The standard interceptor chain validates the origin, requested
  method, and requested headers, then short-circuits an accepted preflight with `204 No Content`
  and the standard CORS/security/trace response headers. Business routers must not add ad hoc
  `OPTIONS` routes for preflight handling.
- Multi-surface gateways must apply the same environment-derived CORS policy to every merged API
  surface so router merge order cannot change preflight behavior.
- The standard Web Framework interceptor is the only CORS authority for a mounted API router.
  Process hosts must not add a second Tower/Axum CORS layer around routers already wrapped with
  `WebFrameworkLayer`; environment and exact production origins are injected into each framework
  layer before router merge.
- Request ID: server-generated UUID v4.
- Unauthenticated protected paths: 401 Problem+json.
- Framework-generated auth, authorization, tenant-isolation, routing, and dependency problems
  include `authProfile`, `failedStage`, and a stable non-secret `reason` whenever route metadata is
  known. They never expose token contents, lookup keys, subject secrets, or upstream addresses.
- Every SDKWork-owned custom 4xx/5xx response is normalized to `application/problem+json`, including
  Axum/Spring extractor or binding rejection, request-size/media-type rejection, router fallback,
  timeout, and handler errors. The framework supplies server-owned `traceId` and `instance`, and
  supplies the manifest `operationId` whenever method and path resolve to a route. Explicit external
  protocol operations preserve their declared upstream error wire.
- `with_server_request_identity` and equivalent request-id-only middleware do not satisfy Web
  Framework integration for business API routes and cannot replace a manifest-bound framework
  layer.
- Oversized body: 413.
- Rate limit exceeded: 429 with `Retry-After` when applicable.
- Rate-limit / idempotency / audit store errors: fail-closed (`503` Problem+json via `DependencyUnavailable`); applications `MUST NOT` bypass stores in production.
- Production SaaS rate limit, idempotency, and tenant concurrent admission stores `MUST` report `is_distributed_ha() == true` (Redis adapters in `sdkwork-web-store-redis`); memory and SQLx adapters are dev/single-replica only.
- B12 JSON body context-selector inspection: single bounded buffer per request (required to re-inject body for downstream handlers); limit follows tenant runtime profile / global body cap.

## 10. Observability

- Logs `MUST` redact tokens and API keys.
- Metrics and logs `SHOULD` include `request_id`, `trace_id` (when known), `api_surface`, `operation_id` when known.
- Metrics and logs `SHOULD` include locale as diagnostic context when it is available, but machine names and audit action codes remain non-localized.
- All framework Problem+json error surfaces (pipeline, extractors, handlers, contract fallback, timeouts) `MUST` include `traceId` when available via `WebRequestContext` or inbound W3C `traceparent`, and `SHOULD` include safe `i18nKey`/`locale` metadata when message mapping exists.
- All SDKWork-owned custom Problem+json surfaces `MUST` include `instance`; matched routes `MUST`
  include `operationId`, and unmatched routes `MUST NOT` fabricate one.
- Raw URL paths with identifiers `MUST NOT` be logged; use route templates.
- `HttpMetricsRegistry` uses hard framework ceilings of 4,096 labeled request series and 128 pipeline-stage series. Public construction may lower but must not raise those ceilings.
- A request-series key over 2,048 bytes or pipeline-stage label over 128 bytes is dropped and counted; metric recording must not allocate the rejected series.
- Unresolved routes use the fixed metric label `unmatched`. Redacting individual path segments is not sufficient to bound metric cardinality.
- Series saturation must increment `sdkwork_http_metric_series_dropped_total{kind=...}` without rejecting the business request or preventing updates to already registered series.
- Infrastructure paths `/health`, `/healthz`, `/livez`, `/readyz`, and `/metrics` must not inflate application request counters.

## 11. Verification

Framework repository canonical gate list: `specs/component.spec.json` → `verification.commands`.

```bash
scripts/verify.ps1   # Windows
scripts/verify.sh    # Unix
```

Business repository after integration:

- Contract test: pipeline order unchanged.
- Handler static rule: no raw credential or locale header parsing in route crates.
- Locale context test: public and protected routes receive `WebRequestContext.locale`; unsupported locales resolve through fallback.
- Locale response test: localized responses emit `Content-Language`, language-varying responses emit `Vary: Accept-Language`, and localized problem mapping preserves numeric `ProblemDetail.code` and `traceId`.
- Problem routing test: extractor, handler, timeout, and fallback errors contain `instance`; matched
  routes contain the manifest `operationId`; explicit external protocol operations retain their wire.
- Open-api auth check: protected routes declare `api-key`, `oauth`, or `open-api-flexible`; security vectors cover missing credentials, API key resolution, OAuth bearer resolution, and flexible scheme selection.

## 12. Capability Matrix

Machine-readable catalog: [web-framework-capability.matrix.json](./web-framework-capability.matrix.json).

Human catalog: [docs/architecture/tech/TECH-13-capability-catalog.md](../docs/architecture/tech/TECH-13-capability-catalog.md).
