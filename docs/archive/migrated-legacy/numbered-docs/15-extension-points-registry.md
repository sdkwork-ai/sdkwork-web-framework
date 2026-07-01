# 扩展点注册表（Extension Points Registry）

> 框架 **只定义接口与调用时机**；**禁止**在框架内提供业务实现。

## 1. 扩展点总表

| EP-ID | Trait / Hook | 调用阶段 | 框架默认 | 业务实现示例 |
| --- | --- | --- | --- | --- |
| EP-01 | `WebRequestContextResolver` | RequestContextResolution | `DevClaimResolver`（仅 local） | `IamWebRequestContextResolver` |
| EP-02 | `AuthTokenParser` | Resolution 前 | `BearerTrimParser` | IAM JWT parser |
| EP-03 | `AccessTokenParser` | Resolution 前 | `HeaderTrimParser` | IAM access JWT |
| EP-04 | `ApiKeyParser` | Resolution 前 | `PlainHeaderParser` | 标准化 x-api-key |
| EP-05 | `ApiKeyLookupService` | resolve_api_key | inline-claims（dev） | IAM 查 `iam_api_key` hash |
| EP-05b | `OAuthTokenLookupService` | resolve_oauth_bearer | inline-claims（dev） | IAM / OAuth token introspection |
| EP-05c | `OpenApiCredentialSchemeDetector` | RequestContextResolution (open-api) | `DefaultOpenApiCredentialSchemeDetector` | 自定义 Header 优先级 |
| EP-05d | `TenantSigningKeyLookup` | JWT verify (auth/access/oauth) | `EnvBootstrapTenantSigningKeyLookup`（control-plane bootstrap, HS256） | IAM `iam_tenant_signing_key` 查表（HS256 secret 或 RS256 SPKI） |
| EP-05e | `JwtSessionRevocationChecker` | JWT verify (auth/access) post-claim | `NoOpJwtSessionRevocationChecker`（bootstrap） | IAM `iam_session` 吊销查表 |
| EP-06 | `AuthorizationPolicy` | Authorization | `DenyAll`（非测试） | `IamAuthorizationPolicy` |
| EP-07 | `TenantIsolationPolicy` | TenantIsolation | `RequirePrincipal` 最小检查 | IAM data_scope 规则 |
| EP-08 | `DomainContextInjector` | ContextInjection | 无 | `IamDomainContextInjector` |
| EP-09 | `RateLimitStore` | RateLimit | `MemoryRateLimitStore` | `RedisRateLimitStore` |
| EP-10 | `RateLimitPolicyResolver` | RateLimit | 按 `HttpRoute.tier` + IP | 租户级策略表 |
| EP-11 | `IdempotencyStore` | Idempotency | `MemoryIdempotencyStore` | `SqlxIdempotencyStore` |
| EP-12 | `AuditEmitter` | Audit | `NoopAuditEmitter` | Sqlx / IAM 双写 |
| EP-13 | `SecurityEventEmitter` | Cors/RateLimit 失败 | `Noop` 或 Sqlx | 平台安全中心 |
| EP-14 | `WebCallInterceptor` | 链任意位置（插入） | 18 个标准实现 | 产品自定义 WAF 探测 |
| EP-15 | `ReadinessCheck` | `/readyz` | 总是 200 | DB/Redis ping |
| EP-16 | `CorsPolicySource` | Cors before | 静态 `SecurityPolicy` | DB `web_cors_policy` |
| EP-17 | `OperationIdResolver` | Logging/Audit | 从 `HttpRoute` manifest 匹配 | 自定义 fallback |
| EP-18 | `ProblemDetailRenderer` | 错误映射 | 标准 Problem+json | 白标定制 |
| EP-19 | `RequestLogRedactor` | Logging | 默认脱敏规则 | 扩展正则 |
| EP-20 | `WebFrameworkLifecycle` | 启动/关闭 | `NoOpWebFrameworkLifecycle` | 自定义 drain / 资源释放 |

## 2. 装配 API（极致集成入口）

```rust
WebFramework::builder()
    .profile(WebRequestContextProfile::default())
    .security(SecurityPolicy::production())
    .resolver(iam_open_api_resolver)           // EP-01 + EP-05/05b
    .open_api_scheme_detector(Arc::new(custom_detector)) // EP-05c
    .authorization(Arc::new(iam_authz))      // EP-06
    .tenant_isolation(Arc::new(iam_tenant))    // EP-07
    .domain_injector(Arc::new(iam_injector))   // EP-08
    .rate_limit_store(Arc::new(redis_rl))      // EP-09
    .idempotency_store(Arc::new(sqlx_idem))    // EP-11
    .audit_emitter(Arc::new(sqlx_audit))       // EP-12
    .readiness_check(db_ready)                 // EP-15
    .lifecycle(Arc::new(custom_lifecycle))     // EP-20
    .request_timeout(Duration::from_secs(30))  // A10
    .build()
```

一行挂载：

```rust
let app = business_router.layer(web_framework.layer());
```

### 2.1 生产 SaaS JWT 装配（IAM adapter）

```rust
use sdkwork_web_core::{
    tenant_bound_saas_verifying_web_request_resolver_with_claim_policy,
    JwtProductionClaimPolicy,
};

let resolver = tenant_bound_saas_verifying_web_request_resolver_with_claim_policy(
    iam_tenant_signing_key_lookup,   // EP-05d: HS256 + RS256 by kid
    iam_session_revocation_checker,  // EP-05e
    iam_api_key_lookup,              // EP-05
    JwtProductionClaimPolicy::saas_production(
        vec!["https://iam.example".to_owned()],
        vec!["appbase".to_owned()],
    ),
);

WebFramework::builder(resolver)
    .production_defaults()
    .readiness_check(db_readiness)   // EP-15
    .rate_limit_store(redis_rate_limit)
    .idempotency_store(redis_idempotency)
    .build();
```

Control-plane 单节点 bootstrap 可使用 `tenant_bound_verifying_web_request_resolver()` 并设置 `WebFrameworkOptionalFeatures::control_plane_standalone()`；生产 SaaS 不得省略 `iss`/`aud` claim policy（`production_assembly` 会在启动时拒绝）。

## 3. 扩展点契约（每个 EP 必备）

| 要求 | 说明 |
| --- | --- |
| `Send + Sync` | 跨 await 点安全 |
| 无业务类型 | trait 方法签名仅框架类型 |
| 错误类型 | `WebFrameworkError` 或 `Result<_, WebFrameworkError>` |
| 文档 | 每个 EP 在 docs 有行为说明 + 测试向量 |
| 默认实现 | 可安全用于 `cargo test` |

## 4. 禁止的反模式

| 反模式 | 正确做法 |
| --- | --- |
| Handler 内 `headers.get("Authorization")` | `Extension<WebRequestContext>` |
| 业务 crate 复制 18 阶段顺序 | `WebCallInterceptorChain::standard()` |
| 框架内 `use sdkwork_iam_*` | EP-01 业务 adapter |
| 跳过链直接 `Router::route` 保护路由 | 必须 `with_web_request_context` |
| open-api 写死 `/open/v3/api` | `open_api_prefixes` 配置 |

## 5. EP 与能力目录映射

| EP | 能力 ID |
| --- | --- |
| EP-01–05, EP-05d, EP-05e | A4, A5, C9, C14 |
| EP-06–07 | C11, B5 |
| EP-08 | A8 |
| EP-09–11 | D1–D7 |
| EP-12–13 | E7, C12 |
| EP-14 | 自定义横切 |
| EP-15 | E10 |
| EP-16 | C1 |
| EP-20 | H1 |

## 6. 版本策略

| 变更 | 版本 |
| --- | --- |
| 新增 EP（可选） | minor |
| 新增 mandatory 阶段 | **major** + ADR |
| trait 方法签名变更 | major |
| 默认安全策略收紧 | minor + 迁移说明 |
