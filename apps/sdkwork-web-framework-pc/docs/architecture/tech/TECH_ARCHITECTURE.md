# sdkwork-web-framework-pc Technical Architecture

Status: active
Owner: SDKWork maintainers
Updated: 2026-06-26
Specs: ARCHITECTURE_DECISION_SPEC.md, DOCUMENTATION_SPEC.md, FRONTEND_CODE_SPEC.md, APP_PC_ARCHITECTURE_SPEC.md

## Document Map

- Add `TECH-<topic>.md` shards in this directory when the architecture grows beyond one reviewable screen.

## 1. Architecture Overview

The PC console is a single-page React application built with Vite. It does NOT own data;
it is a thin UX layer over the backend admin API (`/backend/v3/api/web-framework/*`),
which is protected by the framework's own 18-stage interceptor pipeline.

```
┌─────────────────────────────────────────────────────────┐
│  Hosting page (IAM portal / standalone HTML)            │
│  window.__SDKWORK_ADMIN_CREDENTIALS__ = { authToken,    │
│    accessToken } | () => credentials                    │
└──────────────────────┬──────────────────────────────────┘
                       │ credential injection (prod)
                       │ sessionStorage + VITE_* (dev/E2E)
┌──────────────────────▼──────────────────────────────────┐
│  PC Console SPA (React 19 + Vite 7)                     │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐  │
│  │ main.tsx    │→ │ useWebFrame  │→ │ admin-service  │  │
│  │ epoch guard │  │ workAdmin    │  │ (cached SDK)   │  │
│  │ i18n msgs   │  │ hook         │  │                │  │
│  └─────────────┘  └──────────────┘  └───────┬────────┘  │
│                                             │            │
│  ┌──────────────────────────────────────────▼─────────┐ │
│  │ Backend SDK Transport (transport.ts)               │ │
│  │  - dualTokenHeaders(provider)                      │ │
│  │  - Problem+json → BackendSdkError                  │ │
│  │  - 401 → provider.onUnauthorized()                 │ │
│  └──────────────────────────┬─────────────────────────┘ │
└─────────────────────────────┼───────────────────────────┘
                              │ HTTPS
┌─────────────────────────────▼───────────────────────────┐
│  Backend Admin API (sdkwork-web-admin-server)           │
│  18-stage pipeline: RequestIdentity → ResponseIdentity  │
│  - Dual-token JWT verification                          │
│  - Tenant isolation enforcement                         │
│  - Problem+json (RFC 9457) error contract               │
└─────────────────────────────────────────────────────────┘
```

## 2. Technology Choices

| Concern | Choice | Rationale |
|---------|--------|-----------|
| UI framework | React 19 | Workspace standard (FRONTEND_CODE_SPEC) |
| Build tool | Vite 7 | Fast HMR, native ESM, workspace standard |
| Language | TypeScript (strict) | Type safety for SDK contracts |
| E2E test | Playwright | Cross-browser, workspace standard |
| State | React hooks (useState/useRef) | No Redux needed; epoch guard suffices for race prevention |
| i18n | Lightweight message catalog | Internal admin tool; full i18n framework is overkill |
| Auth | TokenProvider open-closed pattern | Supports dev (sessionStorage) and prod (window injection) without branching |

## 3. System Boundaries And Modules

### Boundary: Credential injection

The console does NOT authenticate users. Credentials flow in through
`BackendTokenProvider` (open-closed extension point):

- `DevSessionTokenProvider`: auth token in `sessionStorage`, access token via
  `VITE_SDKWORK_ACCESS_TOKEN` (dev / E2E only).
- `RuntimeCredentialsTokenProvider`: reads `window.__SDKWORK_ADMIN_CREDENTIALS__`
  (production; injected by hosting page).

`resolveBackendTokenProvider()` selects the active provider at module load time.

### Boundary: Backend SDK transport

`createBackendSdkTransport(baseUrl, provider)` is the single HTTP egress point. It:
- Attaches `Authorization: Bearer <authToken>` and `Access-Token: <accessToken>` headers.
- Parses `application/problem+json` error responses into `BackendSdkError` with
  `status`, `problemType`, `code`, `traceId`.
- Calls `provider.onUnauthorized()` on 401 to clear local session.

UI code MUST consume the SDK facade (`WebFrameworkAdminBackendSdk`), never the transport
directly.

### Boundary: Permission-based tab visibility

`useWebFrameworkAdmin` hook resolves visible tabs from the dev auth token's JWT claims
(`web-framework.control-plane`, `web-framework.platform.read`,
`web-framework.tenant.admin`). This is UX-only; the backend always re-authorizes via the
18-stage pipeline (SECURITY_SPEC §4).

## 4. Directory And Package Layout

```
apps/sdkwork-web-framework-pc/
├── src/
│   ├── main.tsx              # Root component: epoch guard, tab switcher, JSON editor
│   ├── devAuth.ts            # Dev JWT claim parsing + permission checks
│   ├── api/types.ts          # Backend record type contracts
│   ├── hooks/
│   │   └── useWebFrameworkAdmin.ts  # Tab loading, save, node heartbeat/delete
│   ├── i18n/
│   │   └── messages.ts       # Message catalog + tab labels
│   ├── sdk/
│   │   ├── auth/token-provider.ts   # BackendTokenProvider open-closed pattern
│   │   └── backend-sdk/             # Generated SDK facade + transport
│   │       ├── transport.ts         # dual-token HTTP egress + Problem+json parsing
│   │       ├── operations.ts        # Operation metadata
│   │       ├── web-framework-admin-sdk.ts  # SDK facade
│   │       └── index.ts             # Factory: createWebFrameworkAdminBackendSdkFromEnv
│   └── services/
│       └── web-framework-admin-service.ts  # Cached singleton SDK accessor
├── e2e/
│   ├── console.smoke.spec.ts         # Dev auth tab shell + permission gating
│   └── console.integration.spec.ts   # Live backend: runtime defaults + CORS list
├── packages/sdkwork-web-framework-pc-core/  # Shared core (composition, host, modules)
├── docs/                              # PRD, TECH_ARCHITECTURE, runbooks
├── playwright.config.ts               # Smoke e2e (port 4175)
├── playwright.integration.config.ts   # Integration e2e (port 4176, live backend)
└── sdkwork.app.config.json            # App identity + env bindings
```

## 5. API, SDK, And Data Ownership

- **API surface**: `backend` (`/backend/v3/api/web-framework`)
- **SDK**: Generated backend SDK facade (`src/sdk/backend-sdk/`); do not hand-edit.
- **Data ownership**: The console owns NO data. All data lives in the backend SQLx store
  (`web_*` tables). The console is a read/upsert/delete proxy.

### SDK operations

| Operation | HTTP | Path |
|-----------|------|------|
| List CORS policies | GET | `/backend/v3/api/web-framework/cors_policies` |
| Upsert CORS policy | PUT | `/backend/v3/api/web-framework/cors_policies` |
| List rate-limit policies | GET | `/backend/v3/api/web-framework/rate_limit_policies` |
| Upsert rate-limit policy | PUT | `/backend/v3/api/web-framework/rate_limit_policies` |
| List tenant profiles | GET | `/backend/v3/api/web-framework/tenant_runtime_profiles` |
| Upsert tenant profile | PUT | `/backend/v3/api/web-framework/tenant_runtime_profiles` |
| List control nodes | GET | `/backend/v3/api/web-framework/control_nodes` |
| Register control node | POST | `/backend/v3/api/web-framework/control_nodes` |
| Heartbeat control node | POST | `/backend/v3/api/web-framework/control_nodes/{nodeId}/heartbeat` |
| Delete control node | DELETE | `/backend/v3/api/web-framework/control_nodes/{nodeId}` |
| List security events | GET | `/backend/v3/api/web-framework/security_events` |
| List audit events | GET | `/backend/v3/api/web-framework/audit_events` |
| Runtime defaults snapshot | GET | `/backend/v3/api/web-framework/runtime_defaults` |
| Optional features snapshot | GET | `/backend/v3/api/web-framework/optional_features` |

## 6. Security, Privacy, And Observability

### Security

- **Dual-token auth**: Every backend request carries `Authorization: Bearer <authToken>`
  and `Access-Token: <accessToken>`. The backend re-verifies signatures, tenant binding,
  `token_version`, authorization, and tenant isolation (18-stage pipeline).
- **No baked credentials in production**: `VITE_SDKWORK_ACCESS_TOKEN` is dev/E2E only.
  Production deployments must inject credentials via `window.__SDKWORK_ADMIN_CREDENTIALS__`.
- **401 auto-clear**: Transport calls `provider.onUnauthorized()` on 401, which clears
  `sessionStorage` and reloads the page.
- **UI permission checks are NOT a security boundary**: Tab visibility is UX-only; the
  backend always re-authorizes (SECURITY_SPEC §4).

### Observability

- **Problem+json error contract**: All backend errors arrive as RFC 9457 Problem+json with
  numeric `code` and server-owned `traceId` (`API_SPEC.md` §15). The transport surfaces these in `BackendSdkError` for
  operator correlation.
- **Epoch guard**: `refreshEpoch` ref prevents stale responses from overwriting the
  current view during rapid tab/environment switching (FRONTEND_CODE_SPEC §4).

## 7. Deployment And Runtime Topology

### Dev / E2E

```
Vite dev server (5173) → proxy /backend → admin-server (3920)
sessionStorage: sdkwork.authToken
VITE_SDKWORK_ACCESS_TOKEN: baked into bundle
```

### Production

```
Hosting page (IAM portal)
  → injects window.__SDKWORK_ADMIN_CREDENTIALS__
  → loads static SPA bundle (vite build)
  → SPA calls backend admin API directly (no Vite proxy)
```

### Integration E2E

```
e2e-web-stack.mjs starts:
  1. admin-server (port 3921, SQLite, HS256 JWT)
  2. vite preview (port 4176)
Playwright runs console.integration.spec.ts against port 4176
```

## 8. Architecture Decision Index

| Decision | Rationale |
|----------|-----------|
| TokenProvider open-closed pattern | Supports dev (sessionStorage) and prod (window injection) without if/else branching in transport |
| Epoch guard for refresh | Prevents stale responses from overwriting current view during rapid tab switches |
| Lightweight i18n catalog | Internal admin tool; full i18n framework is overkill for ~15 strings |
| Cached singleton service | SDK is stateless; caching avoids re-creating transport per render |
| No Redux / state library | 7 tabs with independent load/save; component state + epoch guard suffices |

## 9. Verification

| Gate | Command | Coverage |
|------|---------|----------|
| Type check | `npx tsc -b` | Strict TypeScript |
| Build | `npx vite build` | Production bundle |
| Smoke E2E | `npm run test:e2e` | Tab shell, permission gating |
| Integration E2E | `npm run test:e2e:integration` | Live backend: runtime defaults, CORS list |
| Full verify | `npm run verify` | tsc + build |

All gates are listed in `specs/component.spec.json` → `verification.commands` and run by
`scripts/verify.ps1` / `scripts/verify.sh`.
