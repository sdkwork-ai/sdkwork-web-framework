# Repository Guidelines

Domain: `platform`  
Capability: `web-framework`  
Type: **基础底层框架仓库**（非业务产品）  
Status: `implementing`

SDKWork **所有带 HTTP API 能力仓库**所依赖的 Web/SaaS 基础框架：Axum/Tower 集成封装、SaaS 开发标准、通用横切能力。

## SDKWORK Soul

Read `../sdkwork-specs/SOUL.md` before executing repository tasks.

## SDKWORK Standards

Canonical entrypoint: `../sdkwork-specs/README.md`. Do not copy root standards into this repository.

## Application Identity

This repository is a platform framework workspace, not an SDKWork application root. PC demo app identity lives in `apps/sdkwork-web-framework-pc/sdkwork.app.config.json`.

## Local Dictionary Structure

- `AGENTS.md` — agent execution rules (this file).
- `.sdkwork/` — repository skills, plugins, workspace metadata.
- `specs/` — framework L1 standard and `component.spec.json`.
- `crates/` — Rust framework crates and framework-owned admin route crate.
- `apis/` — HTTP contract sources for framework-owned surfaces.
- `apps/` — optional runnable demos (PC admin UI).
- `docs/` — framework design and ADRs.
- `tests/` — cross-crate architecture and security verification.
- `scripts/` — thin verification entrypoints.
- `deployments/` — packaging and deployment handoff notes.

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

## Spec Resolution Order

1. This `AGENTS.md`.
2. `specs/component.spec.json` and `specs/WEB_FRAMEWORK_STANDARD.md`.
3. `.sdkwork/README.md` when extending local skills/plugins.
4. `../sdkwork-specs/README.md` and task-specific root specs.
5. Implementation files.

## Required Specs By Task Type

| Task | Required specs |
| --- | --- |
| Agent/workflow rules | `../sdkwork-specs/SOUL.md`, `../sdkwork-specs/AGENTS_SPEC.md`, `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md` |
| Any code change | `../sdkwork-specs/CODE_STYLE_SPEC.md`, `../sdkwork-specs/NAMING_SPEC.md`, `../sdkwork-specs/RUST_CODE_SPEC.md` |
| HTTP framework/runtime | `../sdkwork-specs/WEB_FRAMEWORK_SPEC.md`, `specs/WEB_FRAMEWORK_STANDARD.md`, `../sdkwork-specs/API_SPEC.md` §10 |
| Web backend handlers | `../sdkwork-specs/WEB_BACKEND_SPEC.md`, `../sdkwork-specs/SECURITY_SPEC.md` §5.1 |
| SQL store / migrations | `../sdkwork-specs/DATABASE_SPEC.md`, `docs/architecture/tech/TECH-06-database-design.md` |
| Release / CI | `../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`, `../sdkwork-specs/RELEASE_SPEC.md` |
| Verification | `../sdkwork-specs/TEST_SPEC.md`, `../sdkwork-specs/QUALITY_GATE_SPEC.md` |

## 定位

- **是**：`WebRequestContext`、Interceptor 链、安全策略、HTTP bootstrap、契约类型
- **不是**：IAM、电商、网关等业务；不包含 `sdkwork-routes-<业务>-*`
- **依赖**：`sdkwork-appbase` → 本仓库（单向，本仓库 **不** 依赖 appbase）

## Code Style Rules

Follow `../sdkwork-specs/RUST_CODE_SPEC.md`. Framework crates must not depend on business repositories.

Build scripts, dev runners, and `pnpm clean` must follow `CODE_STYLE_SPEC.md` §7 (Build Source Integrity And Self-Healing). Git-tracked build-critical source files must be verified before builds and self-healed from git when missing; `clean` must not delete them.

## Build, Test, and Verification

Canonical command list: `specs/component.spec.json` → `verification.commands`.

```bash
scripts/verify.ps1   # Windows
scripts/verify.sh    # Unix
```

Or run the core gates directly:

```bash
cargo test --workspace
cargo test -p sdkwork-web-architecture-tests
cargo test -p sdkwork-web-bootstrap --test integration
cargo test -p sdkwork-routes-web-framework-backend-api --test openapi_authority
cargo test -p sdkwork-routes-web-framework-backend-api --test routes_contract
cargo test -p sdkwork-web-bootstrap --features admin-api --test admin_api_readiness
node tests/contract/database-framework.contract.test.mjs
node tests/contract/pc-admin-operations.contract.test.mjs
cargo clippy --workspace -- -D warnings
```

PC admin console: `cd apps/sdkwork-web-framework-pc && npm run verify`

Architecture guard: no `cargo tree` edges to `sdkwork-appbase`, `sdkwork-iam-*`, or product routers.

## Agent Execution Rules

- Specs before memory; evidence before completion.
- Do not vendor framework pipeline source into business repos.
- Do not add business route crates without an explicit framework control-plane ADR.

## HTTP API Response Envelope

All L2+ `app-api`, `backend-api`, and SDKWork-owned business `open-api` HTTP contracts `MUST` follow `API_SPEC.md` section 4.5, section 14, and section 15:

- **Input:** typed request bodies, section 14.1 list/search/command input, `SdkWorkListQuery`, and `q` for free-text search.
- **Success output:** `SdkWorkApiResponse` with `{ "code": 0, "data": <payload>, "traceId": "<server-uuid>" }`.
- **Error output:** HTTP 4xx/5xx `application/problem+json` (`ProblemDetail`) with numeric `code` and `traceId`.
- Success `code` is numeric `int32`; HTTP 2xx JSON bodies `MUST` use `0` only. REST semantics remain on HTTP status (`201`, `202`, etc.).
- Platform error codes are numeric non-zero values per section 15.3 (`40001`, `40101`, `40401`, …).
- Single resource: `data.item`
- Lists: `data.items` + `data.pageInfo` (`PageInfo.mode` is `offset` or `cursor`)
- Commands: `data.accepted` plus optional `resourceId` / `status`
- Async accept (`202`): `data.operationId`, `data.status`, optional `pollUrl`

Vendor compatibility `open-api` routes that mirror upstream tool or provider wire (for example OpenAI `/v1/*`, Claude Code, Codex) `MAY` opt out only when every exempt operation declares `x-sdkwork-wire-protocol: external` and `x-sdkwork-external-protocol-id` per `API_SPEC.md` section 4.5.2. SDKWork-owned business `open-api` operations `MUST NOT` opt out.

Errors `MUST` use HTTP 4xx/5xx with `application/problem+json` (`ProblemDetail`) including required numeric `code` and `traceId`. Business failures `MUST NOT` use HTTP 2xx with non-zero `code`, string wire codes, `success`, or human `message`.

Forbidden legacy envelopes and fields: `PlusApiResult`, `AppbaseApiResult`, `StoreApiResult`, `SdkWorkResponse`, per-domain `*ApiResult`, wire field `requestId`, bare domain DTOs at the HTTP root, and top-level `{ items, pageInfo, traceId }` without `data`.

Handlers `MUST` serialize success and map errors through `sdkwork-web-framework` response mapping. Generated HTTP SDKs (`--standard-profile sdkwork-v3`) unwrap `data` by default and expose typed numeric `ProblemDetail.code` / `traceId` on errors; use `.raw` when the full envelope is required.

Before completing API contract, SDK generation, or frontend service work, run:

```bash
node <sdkwork-specs>/tools/check-api-response-envelope.mjs --workspace <workspace-root>
```

Authority: `sdkwork-specs/API_SPEC.md` section 4.5 and sections 14–16, `SDK_SPEC.md` section 4.2, `FRONTEND_SPEC.md`, `MIGRATION_SPEC.md` section 4.2.

## Human Review Rules

Human review is required for breaking standard changes, security exceptions, and changes to the 18-stage interceptor order or `WebRequestContext` vocabulary.
