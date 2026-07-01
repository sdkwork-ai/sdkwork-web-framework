# sdkwork-web-framework-pc PRD

Status: active
Owner: SDKWork maintainers
Application: sdkwork-web-framework-pc
Updated: 2026-06-26
Specs: REQUIREMENTS_SPEC.md, DOCUMENTATION_SPEC.md, APP_PC_ARCHITECTURE_SPEC.md, WEB_FRAMEWORK_SPEC.md

## Document Map

- Add `PRD-<topic>.md` shards in this directory when the PRD grows beyond one reviewable screen.

## 1. Background And Problem

The SDKWork Web Framework provides an 18-stage HTTP interceptor pipeline with dynamic
per-tenant overlays for CORS, rate limiting, tenant runtime profiles, and control-node
topology. Until this PC console existed, operators managed these overlays by directly
calling the backend admin API (`/backend/v3/api/web-framework/*`) with hand-crafted JWTs
and curl. This is error-prone, lacks audit trail visibility, and blocks tenant admins from
self-serving their own CORS / rate-limit policies.

The PC console replaces that workflow with a permission-aware single-page application that
exposes the control plane through a dual-token backend SDK transport.

## 2. Target Users

| User | Permission scope | Visible tabs |
|------|-----------------|--------------|
| Platform operator | `web-framework.control-plane` or `web-framework.platform.read` | all 7 tabs |
| Tenant admin | `web-framework.tenant.admin` | defaults, cors, rateLimit, tenant, audit |
| Anonymous / no permission | (none) | defaults only (read-only runtime snapshot) |

## 3. Goals And Non-Goals

### Goals

- Browse and upsert CORS policies, rate-limit policies, tenant runtime profiles.
- Register, heartbeat, and delete distributed control-plane nodes.
- Read security events and audit events (platform operator only for security).
- Enforce permission-based tab visibility client-side; backend always re-authorizes.
- Zero baked credentials in production bundles.

### Non-Goals

- This console is NOT a business application surface; it does not serve end-users.
- This console does NOT manage IAM users, organizations, or billing.
- This console does NOT replace the backend admin API; it is a thin UX layer over it.
- This console does NOT implement its own authentication; it consumes credentials injected
  by the hosting page (IAM current-session SDK) or dev sessionStorage.

## 4. Scope

Seven tabs, each backed by a backend SDK method:

| Tab | SDK method | Operations |
|-----|-----------|------------|
| defaults | `runtimeDefaults()` + `optionalFeatures()` | read-only |
| cors | `listCorsPolicies(env)` / `upsertCorsPolicy(record)` | list + upsert |
| rateLimit | `listRateLimitPolicies(env)` / `upsertRateLimitPolicy(record)` | list + upsert |
| tenant | `listTenantProfiles(env)` / `upsertTenantProfile(record)` | list + upsert |
| nodes | `listControlNodes(env)` / `registerControlNode(record)` / `heartbeatControlNode(id)` / `deleteControlNode(id)` | list + register + heartbeat + delete |
| security | `listSecurityEvents()` | read-only (platform only) |
| audit | `listAuditEvents()` | read-only |

## 5. User Scenarios

1. **Operator rotates CORS policy**: Select environment → cors tab → edit JSON → save.
   Backend validates via 18-stage pipeline; console refreshes on success.
2. **Tenant admin self-serves rate limit**: Login with tenant admin scope → rateLimit tab
   → edit JSON → save. Backend rejects cross-tenant upsert with 403.
3. **Operator registers new control node**: nodes tab → paste node JSON → save →
   heartbeat periodically to keep node alive.
4. **Operator investigates security incident**: security tab → browse security events
   (CORS denials, CSRF rejections, rate-limit exceedances) with `traceId`
   correlation.

## 6. Success Metrics

- All 21 `component.spec.json` verification commands pass on Windows and Linux.
- Playwright smoke e2e (2 tests) and integration e2e (2 tests) pass.
- No credentials baked into production bundle (verified by build smoke test).
- Backend SDK transport correctly maps Problem+json errors to `BackendSdkError` with
  `traceId` correlation.

## 7. Phases

- **M3 (current)**: 7 tabs, dual-token transport, epoch guard, i18n catalog, Playwright
  smoke + integration e2e, production credential injection via `window.__SDKWORK_ADMIN_CREDENTIALS__`.
- **Future**: full i18n locale switching, bulk import/export, real-time node topology graph.

## 8. Linked Requirements

- WEB_FRAMEWORK_SPEC.md §8 (18-stage pipeline)
- APP_PC_ARCHITECTURE_SPEC.md §1 (credential injection contract)
- SECURITY_SPEC.md §4 (UI permission checks do not replace backend authorization)
- DOCUMENTATION_SPEC.md §7 (runbooks for L3 foundation domains)

## 9. Open Questions

- Should the console support multi-environment switching beyond dev/test/prod?
- Should tenant admins be allowed to view (not just upsert) their own rate-limit policies?
