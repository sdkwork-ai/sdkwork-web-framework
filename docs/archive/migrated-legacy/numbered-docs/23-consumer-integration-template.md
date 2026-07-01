# 消费者集成模板（Rust API 服务）

> 框架仓库提供的 **参考装配**；IAM 实现在 `sdkwork-appbase` adapter（跨仓库）。  
> 完整迁移：[10-migration-from-appbase.md](./10-migration-from-appbase.md)

## Cargo.toml

```toml
[dependencies]
axum = { workspace = true }
tokio = { workspace = true }
sdkwork-web-bootstrap = { path = "../sdkwork-web-framework/crates/sdkwork-web-bootstrap", features = ["redis", "sqlx"] }
sdkwork-web-core = { path = "../sdkwork-web-framework/crates/sdkwork-web-core" }
sdkwork-web-axum = { path = "../sdkwork-web-framework/crates/sdkwork-web-axum" }
sdkwork-web-store-redis = { path = "../sdkwork-web-framework/crates/sdkwork-web-store-redis" }
# 业务 adapter（appbase 示例）
sdkwork-iam-web-adapter = { path = "../sdkwork-iam/crates/sdkwork-iam-web-adapter" }
```

## main.rs（生产 SaaS，< 20 行核心装配）

```rust
use axum::Router;
use sdkwork_web_bootstrap::WebFramework;
use sdkwork_web_core::{HttpRouteManifest, JwtProductionClaimPolicy, tenant_bound_saas_verifying_web_request_resolver_with_claim_policy};
use sdkwork_web_store_redis::{shared_rate_limit_store, shared_idempotency_store, shared_concurrent_admission_store};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    sdkwork_web_bootstrap::init_tracing_from_env();
    let pool = /* business sqlx pool */;
    let redis_url = std::env::var("SDKWORK_WEB_FRAMEWORK_REDIS_URL")?;

    let resolver = tenant_bound_saas_verifying_web_request_resolver_with_claim_policy(
        IamTenantSigningKeyLookup::new(pool.clone()),
        IamJwtSessionRevocationChecker::new(pool.clone()),
        IamApiKeyLookupService::new(pool.clone()),
        JwtProductionClaimPolicy::saas_production(vec!["https://iam.example".into()], vec!["my-app".into()]),
    );

    let framework = WebFramework::builder(resolver)
        .production_defaults()
        .readiness_check(db_readiness(pool.clone()))
        .rate_limit_store(shared_rate_limit_store(&redis_url, "my-app")?)
        .idempotency_store(shared_idempotency_store(&redis_url, "my-app")?)
        .concurrent_admission_store(shared_concurrent_admission_store(&redis_url, "my-app")?)
        .authorization_policy(Arc::new(IamAuthorizationPolicy::new(pool.clone())))
        .tenant_isolation_policy(Arc::new(IamTenantIsolationPolicy::new(pool.clone())))
        .route_manifest(HttpRouteManifest::new(MY_ROUTES))
        .build();

    let app = framework.mount_service_routes(my_business_router());
    framework.run("0.0.0.0:8080".parse()?, app).await?;
    Ok(())
}
```

## Handler 示例

```rust
use axum::{routing::get, Router};
use sdkwork_web_core::WebRequestContext;

fn my_business_router() -> Router {
    Router::new().route("/app/v3/api/items", get(list_items))
}

async fn list_items(ctx: WebRequestContext) -> impl axum::response::IntoResponse {
    // ctx.principal, ctx.tenancy — 已由 18 阶段 pipeline 填充
    axum::Json(serde_json::json!({ "requestId": ctx.request_id.0 }))
}
```

## 清单

- [ ] `HttpRoute` manifest 与 OpenAPI authority 一致
- [ ] 生产使用 IAM adapter resolver，非 dev resolver
- [ ] Redis HA store + readiness probe
- [ ] backend-api 控制面调用使用 **ORGANIZATION** `login_scope`（`EnforcePrincipalTenantIsolationPolicy` 拒绝 TENANT 个人会话）
- [ ] `scripts/verify` 等价门禁在消费者 CI 中运行
- [ ] 运维 env 文档化（见 [21-operations-runbook.md](./21-operations-runbook.md)）
- [ ] Rollout / 采纳证据（见 [24-production-rollout-and-adoption.md](./24-production-rollout-and-adoption.md)）
