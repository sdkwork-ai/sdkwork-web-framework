# Bootstrap 与路由装配

> 业务集成必读。标准正文：[specs/WEB_FRAMEWORK_STANDARD.md](../specs/WEB_FRAMEWORK_STANDARD.md)  
> 运维：[21-operations-runbook.md](./21-operations-runbook.md) · 消费者迁移：[10-migration-from-appbase.md](./10-migration-from-appbase.md)

## 1. 三步集成（North Star）

```text
1. 依赖框架 crate（core / axum / bootstrap / 可选 store）
2. 实现 WebRequestContextResolver 等 trait adapter
3. WebFramework::builder(...).route_manifest(...).build().mount_service_routes(业务 Router)
```

目标：**≤ 20 行**装配代码，Handler 使用 `WebRequestContext` extractor，禁止手工解析鉴权头。

## 2. 最小业务服务

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

#[tokio::main]
async fn main() -> std::io::Result<()> {
    sdkwork_web_bootstrap::init_tracing_from_env();

    let framework = WebFramework::builder(DefaultWebRequestContextResolver::default())
        .route_manifest(HttpRouteManifest::new(ROUTES))
        .build();

    let app = framework.mount_service_routes(
        Router::new().route("/app/v3/api/ping", get(|ctx: WebRequestContext| async move {
            ctx.request_id.0
        })),
    );

    framework
        .run("0.0.0.0:8080".parse().expect("bind"), app)
        .await
}
```

`run` 自动挂载 `/healthz`、`/readyz`、`/metrics`，并启用 graceful shutdown（SIGTERM / Ctrl+C）。

## 3. 生产 SaaS 装配

```rust
WebFramework::builder(iam_resolver) // 业务 adapter，见 10-migration-from-appbase.md
    .production_defaults()
    .readiness_check(redis_and_db_readiness)
    .rate_limit_store(redis_rate_limit)           // sdkwork-web-store-redis
    .idempotency_store(redis_idempotency)
    .concurrent_admission_store(redis_concurrent)
    .authorization_policy(iam_authz)
    .tenant_isolation_policy(iam_isolation)
    .route_manifest(HttpRouteManifest::new(ROUTES))
    .build();
```

`production_defaults()` + `SDKWORK_WEB_FRAMEWORK_ENV=prod` 触发 `validate_production_assembly`：

- 禁止 dev/claim-string resolver
- SaaS 要求 HA Redis store（`is_distributed_ha()`）
- 要求 readiness probe、audit/security emitters、安全 CORS

## 4. HttpRoute 清单与 contract fallback（F3）

`route_manifest` 声明契约路由；未挂载 handler 的路径由 `service_router` fallback 返回：

| 情况 | 状态 | Problem type |
| --- | --- | --- |
| manifest 中存在、handler 未实现 | 501 | `not-implemented` |
| manifest 中不存在 | 404 | `not-found` |

响应含服务端 `requestId` 与 `traceId`（W3C `traceparent` 传播）。

`enable_admin_api` 时，builder 自动从 `sdkwork-routes-web-framework-backend-api::ROUTES` 推导 manifest。

## 5. service_router 基座（H1）

`mount_service_routes` / `WebFramework::run` 挂载：

| 路径 | 用途 |
| --- | --- |
| `/healthz` | Liveness |
| `/readyz` | Readiness（`ReadinessCheck`） |
| `/metrics` | Prometheus |

业务路由与基座路由 merge；请求超时在 `with_web_request_context` 内生效，保证幂等 finalize。

## 6. Handler 约定

- 使用 `WebRequestContext`、`RequirePrincipal`、`RequireTenantApp` extractor
- 错误返回 `finish_api_json` / `finish_api_response`（admin control-plane）或 `WebFrameworkRejection`
- **禁止** bare `IntoResponse for WebFrameworkError`（架构守卫）

## 7. 控制面 admin-server

独立二进制 `sdkwork-web-admin-server`：

```bash
cargo build --release -p sdkwork-web-admin-server
# env: configs/admin-server.env.example
./target/release/sdkwork-web-admin-server
```

装配见 `crates/sdkwork-web-admin-server/src/main.rs`：`enable_admin_api` + `mount_admin_routes` + `run`。

## 8. 验证

集成测试覆盖本页关键路径：

```bash
cargo test -p sdkwork-web-bootstrap --test integration
cargo test -p sdkwork-web-architecture-tests --test bootstrap_contract_fallback
```

全量门禁：`scripts/verify.ps1` / `scripts/verify.sh`。

## 9. 相关文档

- [02-architecture-design.md](./02-architecture-design.md) — crate 分层
- [05-api-surface-design.md](./05-api-surface-design.md) — API 面分类
- [15-extension-points-registry.md](./15-extension-points-registry.md) — EP-01…EP-20
