# Pipeline 与 Interceptor 设计

> 前置阅读：[00-framework-foundation.md](./00-framework-foundation.md)  
> 实现于 `sdkwork-web-core` + `sdkwork-web-axum`。

## 1. 原则

- 链路由 **框架拥有**；业务 **注册扩展**，不绕过
- 认证/授权 **语义** 在框架；**决策实现** 通过 trait 注入
- 无业务 crate 依赖

## 2. WebCallInterceptor

```rust
pub trait WebCallInterceptor<R>: Send + Sync + 'static
where
    R: WebRequestContextResolver + Clone,
{
    fn name(&self) -> &'static str;
    fn stage(&self) -> WebCallStage;
    async fn before(...) -> Result<(), WebFrameworkError>;
    async fn after(...) -> Result<(), WebFrameworkError>;
}
```

## 3. 标准 18 阶段

顺序与 `API_SPEC.md` §10.2、`SECURITY_SPEC.md` §5.1 一致。

| # | Stage | 框架实现 | 业务扩展 |
| --- | --- | --- | --- |
| 1 | RequestIdentity | ✅ | — |
| 2 | SurfaceClassification | ✅ | Profile 配置 |
| 3 | Cors | ✅ | CorsPolicy 配置 |
| 4 | MethodGuard | ✅ | — |
| 5 | CrossSiteRequest | ✅ | — |
| 6 | SqlInjectionGuard | ✅ | header 列表配置 |
| 7 | RequestSizeLimit | ✅ | 限额配置 |
| 8 | RateLimit | ✅ 引擎 | `RateLimitStore` 可插拔 |
| 9 | Idempotency | ✅ 引擎 | `IdempotencyStore` 可插拔 |
| 10 | RequestContextResolution | ✅ | `WebRequestContextResolver`；open-api 另需 `ApiKeyLookupService`、`OAuthTokenLookupService`、`OpenApiCredentialSchemeDetector` |
| 11 | Authentication | ✅ | resolver 产出 principal |
| 12 | Authorization | ✅ 调用点 | `AuthorizationPolicy` trait |
| 13 | TenantIsolation | ✅ 调用点 | `TenantIsolationPolicy` trait |
| 14 | ContextInjection | ✅ | `DomainContextInjector` 列表 |
| 15 | Logging | ✅ | tracing 配置 |
| 16 | Audit | ✅ 调用点 | `AuditEmitter` trait |
| 17 | HeaderSecurity | ✅ after | — |
| 18 | ResponseIdentity | ✅ after | — |

**修订说明**：原 appbase 中 stub 阶段在框架中须提供 **完整调用框架 + 默认内存实现**；生产 store/policy 由业务装配。

## 4. 业务 trait（框架定义，业务实现）

```rust
pub trait AuthorizationPolicy: Send + Sync {
    fn authorize(
        &self,
        ctx: &WebRequestContext,
        operation_id: &str,
    ) -> Result<(), WebFrameworkError>;
}

pub trait TenantIsolationPolicy: Send + Sync {
    fn enforce(
        &self,
        ctx: &WebRequestContext,
        operation_id: &str,
    ) -> Result<(), WebFrameworkError>;
}

pub trait AuditEmitter: Send + Sync {
    async fn emit(&self, fact: AuditFact) -> Result<(), WebFrameworkError>;
}
```

默认 `AllowAllAuthorizationPolicy` / `PassThroughTenantIsolationPolicy` **仅用于测试**，生产必须由业务提供。

## 5. WebFrameworkRuntime 装配

```rust
WebCallInterceptorChain::standard()
    .with_runtime(WebFrameworkRuntime {
        resolver: iam_resolver,           // 业务
        authorization: Some(iam_authz),   // 业务
        domain_injectors: vec![iam_inj],  // 业务
        rate_limit_store: redis_store,    // 框架 store 或业务自定义
        ..
    })
```

## 6. 自定义 Interceptor

业务可 `with_interceptor()` 插入 **额外** 阶段，但：

- 不得删除标准阶段
- 不得排在 `RequestContextResolution` 之后却修改 credentials 解析逻辑

## 7. Domain Pipeline（不属于本框架）

Gateway 计费等 `InvocationPipeline` 留在业务仓库（claw-router）；HTTP 链必须先完成。

## 8. 测试（框架仓库）

- `chain_order_matches_api_spec`
- `open_api_oauth_bearer_resolves_authenticated_principal`
- `open_api_api_key_resolves_authenticated_principal`
- `open_api_flexible_detector_selects_oauth_when_only_bearer_present`
- 无 IAM mock 表；用 `TestResolver` / `TestAuthorizationPolicy`
