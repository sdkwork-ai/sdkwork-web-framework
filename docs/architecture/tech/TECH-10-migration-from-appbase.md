> Migrated from `docs/architecture/tech/TECH-10-migration-from-appbase.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 从 sdkwork-appbase 迁移计划

> 前置阅读：[TECH-00-framework-foundation.md](./TECH-00-framework-foundation.md)  
> **迁移是业务仓库侧工作**；框架仓库 **不依赖** appbase 即可完成开发与测试。

## 1. 目标

- appbase **删除** `sdkwork-platform-http-context-service`
- appbase **新增依赖** `sdkwork-web-framework`
- IAM 相关实现 **留在 appbase**，以 adapter 形式实现框架 trait

## 2. 迁出 → 迁入映射

| appbase（删除/迁出） | web-framework（新建） |
| --- | --- |
| `sdkwork-platform-http-context-service` | 拆为 `sdkwork-web-contract` … `sdkwork-web-bootstrap` |
| `AppRequestContext` 等 | `WebRequestContext`（类型别名在 `sdkwork-web-core`） |
| `IamHttpRoute` | `HttpRoute`（contract crate） |
| IAM 耦合代码 | **不迁入** |

## 3. appbase 新增（业务 adapter，非框架）

```text
sdkwork-appbase/crates/
  sdkwork-iam-web-adapter/
    IamWebRequestContextResolver
    IamApiKeyLookupService
    IamOAuthTokenLookupService
    IamOpenApiWebRequestContextResolver
    IamAuthTokenParser / IamAccessTokenParser
    IamAuthorizationPolicy
    IamTenantIsolationPolicy
    IamDomainContextInjector
    IamAuditEmitter（可选，写 iam 或 web 审计表）
    IamTenantSigningKeyLookup      # EP-05d: resolve_hs256_key / resolve_rs256_key
    IamJwtSessionRevocationChecker # EP-05e: iam_session 吊销查表
```

### 3.1 生产 SaaS JWT 装配（appbase adapter）

业务生产环境 `MUST` 使用框架提供的 SaaS 装配入口，不得手工拼装 dev resolver：

```rust
let resolver = tenant_bound_saas_verifying_web_request_resolver_with_claim_policy(
    IamTenantSigningKeyLookup::new(pool.clone()),
    IamJwtSessionRevocationChecker::new(pool.clone()),
    IamApiKeyLookupService::new(pool.clone()),
    JwtProductionClaimPolicy::saas_production(
        vec!["https://iam.example".to_owned()],
        vec!["appbase".to_owned()],
    ),
);

WebFramework::builder(resolver)
    .production_defaults()
    .readiness_check(db_readiness)
    .rate_limit_store(redis_rate_limit)           // sdkwork-web-store-redis; is_distributed_ha() = true
    .idempotency_store(redis_idempotency)         // sdkwork-web-store-redis
    .concurrent_admission_store(redis_concurrent) // sdkwork-web-store-redis; D9 tenant concurrency
    .authorization_policy(Arc::new(IamAuthorizationPolicy::new(...)))
    .tenant_isolation_policy(Arc::new(IamTenantIsolationPolicy::new(...)))
    .build();
```

生产 SaaS **禁止** 使用 `Memory*` 或 SQLx 限流/幂等存储（`validate_production_assembly` 通过 `RateLimitStore::is_distributed_ha()` 等判定）；多副本 HA 须使用 `sdkwork-web-store-redis` 提供的 `RedisRateLimitStore` / `RedisIdempotencyStore` / `RedisConcurrentAdmissionStore`。

IAM adapter `TenantSigningKeyLookup` 实现示例：

- `resolve_hs256_key(kid)` → `TenantSigningKeyMaterial::hs256(tenant_id, kid, secret)`
- `resolve_rs256_key(kid)` → `TenantSigningKeyMaterial::rs256_spki(tenant_id, kid, spki_der)`

Claim policy（iss/aud）通过 `tenant_bound_saas_verifying_web_request_resolver_with_claim_policy()` 注入 `JwtProductionClaimPolicy::saas_production(...)`；control-plane bootstrap 可使用 `tenant_bound_verifying_web_request_resolver_with_claim_policy()`。

```toml
# appbase/Cargo.toml
sdkwork-web-core = { path = "../sdkwork-web-framework/crates/sdkwork-web-core" }
sdkwork-web-axum = { path = "../sdkwork-web-framework/crates/sdkwork-web-axum" }
# ...
```

## 4. 框架侧解耦检查（先于 appbase 切换）

从迁出代码中 **删除**：

- `use sdkwork_iam_context_service::*`
- `to_iam_app_context()` / `From<IamAppContext>`
- `context_injection` 内硬编码 `IamAppContext`

替换为：

- `DomainContextInjector` 注册机制
- 框架测试用 `TestDomainContextInjector`

## 5. 分阶段

### Phase A — 框架仓库就绪（无 appbase 依赖）

1. 创建 web-framework workspace 与 crate 拆分
2. 从 appbase **复制**源码并去 IAM（不 cargo 依赖 appbase）
3. `cargo test --workspace` 全绿

### Phase B — appbase adapter

1. 新建 `sdkwork-iam-web-adapter`
2. 搬入 IAM 验签、API Key / OAuth Bearer 查表、IamAppContext 映射

### Phase C — appbase 路由接入

1. `sdkwork-routes-iam-app-api` 使用 `with_web_request_context`
2. 删除 handler 内手工 header 解析

### Phase D — appbase 清理

1. 删除 `sdkwork-platform-http-context-service`
2. 更新 import 与 CI

### Phase E — 其它消费者

1. claw-router / api-gateway 嵌入 IAM 路由时使用 router crate 自带的 `WebFramework` 层，**禁止**在 gateway merge 时二次包裹
2. claw all-in-one：gateway 同时嵌入 `sdkwork-iam-app-api` 与 `sdkwork-iam-backend-api`（`/backend/v3/api/iam/*` 优先于 claw backend 宽前缀）
3. **sdkwork-knowledgebase**：app / backend / open-api 通过各自 `web_bootstrap` + domain injector 映射 `WebRequestContext`；生产入口用 `build_*_with_web_framework()`，本地 `main` 仍可用 `dev_auth`
4. **T1 commerce repos**：各 T1 `*-standalone-gateway` 在装配层包裹 WebFramework（`/health`、`/ready` 公共）
5. aiot 等自定义 transport 栈可复用 `sdkwork-iam-web-adapter` resolver，按域注入自己的 request context（无需 axum 时可只依赖 `sdkwork-web-core` trait）

## 6. 兼容

`AppRequestContext` / `AppRequestPrincipal` 等别名已内置于 `sdkwork-web-core`；不再维护独立的 `sdkwork-web-compat` crate。

## 7. 验收

### 7.1 框架仓库（Phase A — 已完成）

- [x] web-framework：`cargo tree` 无 appbase/iam（`dependency_graph` 测试）
- [x] 18 阶段链完整可测 + 共享 Java 向量（`java_parity` / `pipeline-stage-order.json`）
- [x] Release p99 预算：`scripts/benchmark-pipeline.sh`（M4 §3.1）
- [x] OWASP API Top 10 映射：`docs/architecture/tech/TECH-18-owasp-api-top10-mapping.md`

### 7.2 业务仓库（Phase B–E — appbase / 消费者）

- [ ] appbase：依赖 web-framework，无本地 framework crate
- [ ] appbase：删除 `sdkwork-platform-http-context-service`
- [ ] IAM 测试通过且使用 `WebRequestContext` extractor（非手工 Header 解析）

## 8. 回滚

appbase 保留旧 crate 直至 Phase D 验证通过；框架版本 pin。

