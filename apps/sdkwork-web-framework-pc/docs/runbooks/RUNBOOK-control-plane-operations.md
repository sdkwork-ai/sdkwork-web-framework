# RUNBOOK: Control-Plane Operations

Runbook for: sdkwork-web-framework-pc + sdkwork-web-admin-server
Owner: SDKWork platform maintainers
Updated: 2026-06-26
Specs: DOCUMENTATION_SPEC.md §7, WEB_FRAMEWORK_SPEC.md, SECURITY_SPEC.md

## Signals

| Signal | Source | Healthy | Action |
|--------|--------|---------|--------|
| `GET /healthz` | admin-server | 200 OK | If non-200: check SQLite pool, JWT secret env |
| `GET /readyz` | admin-server | 200 OK | If non-200: check Redis (if enabled), SQLx migrations |
| `GET /metrics` | admin-server | Prometheus payload | Scrape into Grafana |
| Console load | PC app | Page renders defaults tab | If blank: check `window.__SDKWORK_ADMIN_CREDENTIALS__` injection |
| Backend 401 rate | admin-server audit log | < 1% of requests | If spike: rotate JWT signing key (see below) |
| Backend 403 rate | admin-server audit log | < 5% of requests | If spike: verify tenant admin permission_scope claims |

## Dashboards

- **admin-server Prometheus**: `http://<admin-host>:<port>/metrics` — request count,
  latency histogram, error rate by kind.
- **PC console**: no built-in dashboard; monitor backend `/metrics` endpoint.

## Commands

### Start admin-server (local dev)

```bash
cd e:\sdkwork-space\sdkwork-web-framework
export SDKWORK_WEB_FRAMEWORK_JWT_HS256_SECRET=dev-local-secret
cargo run -p sdkwork-web-admin-server
# Listens on 127.0.0.1:3920
```

### Start PC console (local dev)

```bash
cd e:\sdkwork-space\sdkwork-web-framework\apps\sdkwork-web-framework-pc
npm install
npm run dev
# Vite dev server on 5173, proxies /backend → 127.0.0.1:3920
```

### Run full verification

```powershell
cd e:\sdkwork-space\sdkwork-web-framework
.\scripts\verify.ps1
```

### Run integration E2E only

```bash
cd e:\sdkwork-space\sdkwork-web-framework\apps\sdkwork-web-framework-pc
npm run test:e2e:integration
```

## Token / Key Rotation

### JWT HS256 signing key rotation

The admin-server verifies dual-token JWTs with `SDKWORK_WEB_FRAMEWORK_JWT_HS256_SECRET`.

1. Generate new secret: `openssl rand -base64 48`
2. Update env on all admin-server instances simultaneously (rolling restart).
3. Update `window.__SDKWORK_ADMIN_CREDENTIALS__` injection in hosting page (IAM portal).
4. Verify: `curl -H "Authorization: Bearer <new-auth-token>" -H "Access-Token: <new-access-token>" http://<admin-host>:<port>/backend/v3/api/web-framework/runtime_defaults`
5. Monitor 401 rate for 15 minutes; if spike, verify all instances rotated.

### Dev sessionStorage token

Dev tokens in `sessionStorage['sdkwork.authToken']` are cleared by:
- `clearDevSession()` (called on 401 by transport).
- Manual: browser DevTools → Application → Session Storage → delete key.

## Tenant Isolation Incident Response

### Symptom

Tenant A sees tenant B's data in the console, or backend returns 403 Forbidden on
cross-tenant upsert.

### Triage

1. Check audit log: `GET /backend/v3/api/web-framework/audit_events?tenant_id=<A>` —
   verify the request's `tenant_id` claim matches the resource `tenant_id`.
2. Check the JWT `login_scope` claim: `ORGANIZATION` vs `TENANT`. Organization scope
   allows cross-tenant access only with `web-framework.platform.read`.
3. Check backend `EnforcePrincipalTenantIsolationPolicy` interceptor (stage 13).

### Remediation

- If JWT claims are wrong: revoke the session, rotate signing key.
- If policy is misconfigured: verify `runtime.optional_features.tenant_isolation` is
  enabled in admin-server.
- File incident in audit log with `traceId` correlation (and internal server request id in store rows when present).

## Migration Rollback

### SQLx migration rollback

Admin-server uses SQLx migrations in `crates/sdkwork-webstore-database-host/migrations/`.

1. Identify the migration version: `cargo run -p sdkwork-web-admin-server -- --list-migrations`
2. Rollback: `cargo run -p sdkwork-web-admin-server -- --rollback <version>`
3. Verify schema: `sqlite3 <db-path> ".tables" | grep web_`
4. Restart admin-server and verify `/readyz` returns 200.

### Configuration rollback

If a CORS / rate-limit / tenant-runtime upsert causes incidents:

1. PC console → relevant tab → edit JSON → revert to previous values → save.
2. Or directly: `PUT /backend/v3/api/web-framework/<resource>` with previous payload.
3. Verify via `GET` that the overlay reverted.

## Provider Outage (Redis)

### Symptom

`/readyz` returns 503 when Redis is enabled but unavailable.

### Triage

1. Check `SDKWORK_WEB_FRAMEWORK_REDIS_URL` env var points to correct Redis.
2. Check Redis: `redis-cli -u <url> ping` → expect `PONG`.
3. If Redis is down: admin-server automatically degrades to in-memory rate-limit store
   (single-instance only). Multi-instance deployments MUST restore Redis.

### Remediation

- Restart Redis cluster.
- If Redis is permanently unavailable: remove `SDKWORK_WEB_FRAMEWORK_REDIS_URL` env var
  and restart admin-server (degrades to in-memory store; not suitable for multi-instance).

## Escalation

| Severity | Escalate to | Response |
|----------|------------|----------|
| Production down | Platform on-call | Immediate |
| Tenant isolation breach | Security on-call | Immediate |
| JWT key rotation failure | Platform on-call | 15 minutes |
| Redis outage (multi-instance) | Platform on-call | 30 minutes |
| PC console blank page | Frontend on-call | 1 hour |
